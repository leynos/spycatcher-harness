//! `spycatcher_harness` — library for recording and replaying LLM API
//! interactions.
//!
//! This crate provides the core library API for the Spycatcher harness,
//! enabling deterministic capture and replay of LLM API sessions for
//! regression testing. The binary crate `spycatcher-harness` delegates
//! all startup and shutdown behaviour to the entry points defined here.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use spycatcher_harness::{start_harness, HarnessConfig};
//!
//! # async fn example() -> spycatcher_harness::HarnessResult<()> {
//! let cfg = HarnessConfig::default();
//! let harness = start_harness(cfg).await?;
//! // … use the harness …
//! harness.shutdown().await?;
//! # Ok(())
//! # }
//! ```

pub mod cassette;
pub mod config;
pub mod error;
pub mod i18n;
pub mod protocol;
pub mod replay;
pub mod server;
pub mod upstream;

pub use config::HarnessConfig;
pub use error::{HarnessError, HarnessResult};

use std::net::SocketAddr;

use camino::Utf8PathBuf;

/// A running harness instance.
///
/// Returned by [`start_harness`] upon successful startup. Holds the
/// address the harness is listening on and the path to the active
/// cassette file.
#[derive(Debug)]
#[must_use]
pub struct RunningHarness {
    /// The address the harness is listening on.
    pub addr: SocketAddr,
    /// The path to the cassette file in use.
    pub cassette_path: Utf8PathBuf,
}

impl RunningHarness {
    /// Gracefully shuts down the running harness.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError`] if shutdown fails.
    #[expect(
        clippy::unused_async,
        reason = "async is part of the public API contract; \
                  server teardown will require async in task 1.3.1"
    )]
    pub async fn shutdown(self) -> HarnessResult<()> {
        Ok(())
    }
}

/// Starts the harness with the given configuration.
///
/// Validates the configuration and prepares the harness for operation.
/// In the current skeleton no HTTP server is bound; the returned
/// address reflects the configured listen address.
///
/// # Errors
///
/// Returns [`HarnessError::InvalidConfig`] if the configuration is
/// invalid (e.g. empty cassette name).
///
/// # Examples
///
/// ```rust,no_run
/// use spycatcher_harness::{start_harness, HarnessConfig};
///
/// # async fn example() -> spycatcher_harness::HarnessResult<()> {
/// let cfg = HarnessConfig::default();
/// let harness = start_harness(cfg).await?;
/// harness.shutdown().await?;
/// # Ok(())
/// # }
/// ```
#[expect(
    clippy::unused_async,
    reason = "async is part of the public API contract; \
              server binding will require async in task 1.3.1"
)]
pub async fn start_harness(cfg: HarnessConfig) -> HarnessResult<RunningHarness> {
    validate_config(&cfg)?;

    let cassette_path = cfg.cassette_dir.join(&cfg.cassette_name);

    Ok(RunningHarness {
        addr: cfg.listen.as_socket_addr(),
        cassette_path,
    })
}

/// Validates the harness configuration.
fn validate_config(cfg: &HarnessConfig) -> HarnessResult<()> {
    if cfg.cassette_name.is_empty() {
        return Err(HarnessError::InvalidConfig {
            message: "cassette name must not be empty".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    use rstest::rstest;

    #[rstest]
    #[tokio::test]
    async fn start_harness_with_valid_config_succeeds() {
        let cfg = HarnessConfig::default();
        let result = start_harness(cfg).await;
        assert!(result.is_ok());
    }

    #[rstest]
    #[tokio::test]
    async fn start_harness_with_empty_cassette_name_fails() {
        let cfg = HarnessConfig {
            cassette_name: String::new(),
            ..HarnessConfig::default()
        };
        let result = start_harness(cfg).await;
        assert!(matches!(result, Err(HarnessError::InvalidConfig { .. })));
    }

    #[rstest]
    #[tokio::test]
    async fn start_harness_cassette_path_joins_dir_and_name() {
        let cfg = HarnessConfig {
            cassette_dir: Utf8PathBuf::from("test/cassettes"),
            cassette_name: "smoke_001".to_owned(),
            ..HarnessConfig::default()
        };
        let harness = start_harness(cfg).await.expect("startup should succeed");
        assert_eq!(
            harness.cassette_path,
            Utf8PathBuf::from("test/cassettes/smoke_001"),
        );
    }

    #[rstest]
    #[tokio::test]
    async fn shutdown_succeeds() {
        let cfg = HarnessConfig::default();
        let harness = start_harness(cfg).await.expect("startup should succeed");
        let result = harness.shutdown().await;
        assert!(result.is_ok());
    }

    #[rstest]
    #[tokio::test]
    async fn start_harness_returns_configured_listen_address() {
        let expected = SocketAddr::from(([10, 0, 0, 1], 9090));
        let cfg = HarnessConfig {
            listen: expected.into(),
            ..HarnessConfig::default()
        };
        let harness = start_harness(cfg).await.expect("startup should succeed");
        assert_eq!(harness.addr, expected);
    }
}
