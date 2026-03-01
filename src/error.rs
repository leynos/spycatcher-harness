//! Harness error types and result aliases.
//!
//! This module provides the semantic error enum [`HarnessError`] and the
//! convenience alias [`HarnessResult`] used throughout the
//! `spycatcher_harness` library API.

use thiserror::Error;

/// Convenience alias for results returned by harness operations.
pub type HarnessResult<T> = std::result::Result<T, HarnessError>;

/// Semantic error type for harness operations.
///
/// Each variant represents a distinct failure mode that callers can
/// inspect, retry, or map to an appropriate response.
///
/// # Examples
///
/// ```
/// use spycatcher_harness::HarnessError;
///
/// let err = HarnessError::InvalidConfig {
///     message: "missing field".to_owned(),
/// };
/// assert!(format!("{err}").contains("missing field"));
/// ```
#[derive(Debug, Error)]
pub enum HarnessError {
    /// Configuration validation failed.
    #[error("invalid configuration: {message}")]
    InvalidConfig {
        /// Description of the configuration problem.
        message: String,
    },

    /// The named cassette could not be found on disk.
    #[error("cassette not found: {cassette_name}")]
    CassetteNotFound {
        /// Name of the missing cassette.
        cassette_name: String,
    },

    /// A replayed request did not match the expected interaction.
    #[error("request mismatch at interaction {interaction_id}")]
    RequestMismatch {
        /// Zero-based index of the mismatched interaction.
        interaction_id: usize,
    },

    /// A request to the upstream provider failed.
    #[error("upstream request failed")]
    UpstreamRequestFailed,

    /// An I/O operation failed.
    #[error("io failure")]
    Io,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::invalid_config(
        HarnessError::InvalidConfig { message: "bad value".to_owned() },
        "invalid configuration: bad value",
    )]
    #[case::cassette_not_found(
        HarnessError::CassetteNotFound { cassette_name: "smoke_001".to_owned() },
        "cassette not found: smoke_001",
    )]
    #[case::request_mismatch(
        HarnessError::RequestMismatch { interaction_id: 42 },
        "request mismatch at interaction 42",
    )]
    #[case::upstream_failed(HarnessError::UpstreamRequestFailed, "upstream request failed")]
    #[case::io(HarnessError::Io, "io failure")]
    fn error_display_matches_expected(#[case] error: HarnessError, #[case] expected: &str) {
        assert_eq!(format!("{error}"), expected);
    }

    #[test]
    fn harness_error_implements_std_error() {
        let err = HarnessError::InvalidConfig {
            message: "test".to_owned(),
        };
        let std_err: &dyn std::error::Error = &err;
        // Verify the trait object can be used.
        assert!(!std_err.to_string().is_empty());
    }
}
