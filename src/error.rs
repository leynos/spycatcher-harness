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
    #[error(
        "request mismatch at interaction {interaction_id}: expected {expected_hash}, observed {observed_hash}"
    )]
    RequestMismatch {
        /// Zero-based index of the expected interaction.
        interaction_id: usize,
        /// Stable hash of the expected canonical request.
        expected_hash: String,
        /// Stable hash of the observed incoming request.
        observed_hash: String,
        /// Field-level diff summary of expected vs observed canonical JSON.
        diff_summary: String,
    },

    /// The cassette could not be parsed or was missing required fields.
    #[error("invalid cassette: {message}")]
    InvalidCassette {
        /// Description of the cassette validation problem.
        message: String,
    },

    /// The cassette uses a schema version this build does not support.
    #[error("unsupported cassette format version {found}; supported version is {supported}")]
    UnsupportedCassetteFormatVersion {
        /// The version value found on disk.
        found: u32,
        /// The format version supported by this build.
        supported: u32,
    },

    /// A request to the upstream provider failed.
    #[error("upstream request failed")]
    UpstreamRequestFailed,

    /// An I/O operation failed.
    #[error("io failure")]
    Io {
        /// The underlying I/O error.
        #[from]
        #[source]
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    //! Unit tests for harness error display formatting.

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
        HarnessError::RequestMismatch {
            interaction_id: 42,
            expected_hash: "abc123".to_owned(),
            observed_hash: "def456".to_owned(),
            diff_summary: "changed: method".to_owned(),
        },
        "request mismatch at interaction 42: expected abc123, observed def456",
    )]
    #[case::invalid_cassette(
        HarnessError::InvalidCassette { message: "missing format_version".to_owned() },
        "invalid cassette: missing format_version",
    )]
    #[case::unsupported_cassette_format(
        HarnessError::UnsupportedCassetteFormatVersion { found: 2, supported: 1 },
        "unsupported cassette format version 2; supported version is 1",
    )]
    #[case::upstream_failed(HarnessError::UpstreamRequestFailed, "upstream request failed")]
    #[case::io(
        HarnessError::Io { source: std::io::Error::new(std::io::ErrorKind::NotFound, "gone") },
        "io failure",
    )]
    fn error_display_matches_expected(#[case] error: HarnessError, #[case] expected: &str) {
        assert_eq!(format!("{error}"), expected);
    }

    #[test]
    fn io_variant_preserves_source_chain() {
        let inner = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = HarnessError::Io { source: inner };
        let std_err: &dyn std::error::Error = &err;
        assert!(
            std_err.source().is_some(),
            "Io variant must expose the underlying io::Error via source()",
        );
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
