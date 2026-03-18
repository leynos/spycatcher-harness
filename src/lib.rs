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
//! // The default replay config assumes a compatible cassette already exists.
//! harness.shutdown().await?;
//! # Ok(())
//! # }
//! ```

pub mod cassette;
pub mod cli;
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

use crate::cassette::{
    CassetteReader, filesystem::FilesystemCassetteStore, filesystem::probe_record_write_access,
};

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
/// Validates the configuration, prepares cassette access for the selected
/// mode, and prepares the harness for operation. In the current skeleton no
/// HTTP server is bound; the returned address reflects the configured listen
/// address.
///
/// # Errors
///
/// Returns [`HarnessError::InvalidConfig`] if the configuration is invalid
/// (for example empty cassette name, path traversal in cassette name, or
/// record mode without upstream configuration).
///
/// Returns [`HarnessError::CassetteNotFound`],
/// [`HarnessError::InvalidCassette`], or
/// [`HarnessError::UnsupportedCassetteFormatVersion`] when replay or verify
/// mode cannot load a compatible cassette from disk. In record mode, these
/// errors can also occur when an existing cassette file is found but is
/// malformed or uses an unsupported format version.
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
    prepare_cassette(&cfg, &cassette_path)?;

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
    if cfg.cassette_name.starts_with('/') || cfg.cassette_name.contains("..") {
        return Err(HarnessError::InvalidConfig {
            message: "cassette name must not contain path traversal sequences".to_owned(),
        });
    }
    if cfg.mode == config::Mode::Record && cfg.upstream.is_none() {
        return Err(HarnessError::InvalidConfig {
            message: "upstream configuration is required for record mode".to_owned(),
        });
    }
    Ok(())
}

/// Prepares the cassette store for the selected operating mode.
fn prepare_cassette(cfg: &HarnessConfig, cassette_path: &Utf8PathBuf) -> HarnessResult<()> {
    match cfg.mode {
        config::Mode::Record => {
            let store = FilesystemCassetteStore::open_or_create_for_record(cassette_path)?;
            let _cassette = store.load()?;
            probe_record_write_access(cassette_path)?;
            Ok(())
        }
        config::Mode::Replay | config::Mode::Verify => {
            let store = FilesystemCassetteStore::open_for_replay(cassette_path)?;
            let _cassette = store.load()?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for harness lifecycle (startup, shutdown, address binding).

    use super::*;
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rstest::rstest;
    use uuid::Uuid;

    static NEXT_TEST_CASSETTE: AtomicUsize = AtomicUsize::new(1);

    #[rstest]
    #[tokio::test]
    async fn start_harness_with_record_config_succeeds() {
        let cfg = record_config(unique_cassette_name("record-valid"));
        let _harness = start_harness(cfg)
            .await
            .expect("startup with valid record config should succeed");
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
    async fn start_harness_with_traversal_cassette_name_fails() {
        let cfg = HarnessConfig {
            cassette_name: "../escape".to_owned(),
            ..HarnessConfig::default()
        };
        let result = start_harness(cfg).await;
        assert!(matches!(result, Err(HarnessError::InvalidConfig { .. })));
    }

    #[rstest]
    #[tokio::test]
    async fn start_harness_with_absolute_cassette_name_fails() {
        let cfg = HarnessConfig {
            cassette_name: "/tmp/out".to_owned(),
            ..HarnessConfig::default()
        };
        let result = start_harness(cfg).await;
        assert!(matches!(result, Err(HarnessError::InvalidConfig { .. })));
    }

    #[rstest]
    #[tokio::test]
    async fn start_harness_record_mode_without_upstream_fails() {
        let cfg = HarnessConfig {
            mode: config::Mode::Record,
            upstream: None,
            ..HarnessConfig::default()
        };
        let result = start_harness(cfg).await;
        assert!(matches!(result, Err(HarnessError::InvalidConfig { .. })));
    }

    #[rstest]
    #[tokio::test]
    async fn start_harness_record_mode_with_upstream_succeeds() {
        let cassette_name = unique_cassette_name("record-upstream");
        let cfg = record_config(cassette_name.clone());
        let _harness = start_harness(cfg)
            .await
            .expect("startup should succeed with upstream");
        let cassette_path = Utf8PathBuf::from("target/test-harness").join(cassette_name);
        assert!(
            cassette_path.is_file(),
            "record startup should create cassette file"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn start_harness_cassette_path_joins_dir_and_name() {
        let cassette_name = unique_cassette_name("path-join");
        let cassette_dir = Utf8PathBuf::from("target/test-harness");
        seed_replay_cassette(&cassette_name);
        let cfg = HarnessConfig {
            cassette_dir: cassette_dir.clone(),
            cassette_name: cassette_name.clone(),
            mode: config::Mode::Replay,
            ..HarnessConfig::default()
        };
        let harness = start_harness(cfg).await.expect("startup should succeed");
        assert_eq!(harness.cassette_path, cassette_dir.join(cassette_name));
    }

    #[rstest]
    #[tokio::test]
    async fn shutdown_succeeds() {
        let cfg = record_config(unique_cassette_name("shutdown"));
        let harness = start_harness(cfg).await.expect("startup should succeed");
        harness.shutdown().await.expect("shutdown should succeed");
    }

    #[rstest]
    #[tokio::test]
    async fn start_harness_returns_configured_listen_address() {
        let expected = SocketAddr::from(([10, 0, 0, 1], 9090));
        let cfg = HarnessConfig {
            listen: expected.into(),
            ..record_config(unique_cassette_name("listen"))
        };
        let harness = start_harness(cfg).await.expect("startup should succeed");
        assert_eq!(harness.addr, expected);
    }

    #[rstest]
    #[tokio::test]
    async fn start_harness_with_supported_replay_cassette_succeeds() {
        let cassette_name = unique_cassette_name("replay-supported");
        seed_replay_cassette(&cassette_name);

        let harness = start_harness(replay_config(cassette_name.clone()))
            .await
            .expect("supported replay cassette should start");

        assert_eq!(
            harness.cassette_path,
            Utf8PathBuf::from("target/test-harness").join(cassette_name),
        );
    }

    #[rstest]
    #[tokio::test]
    async fn start_harness_replay_missing_cassette_fails() {
        let cassette_name = unique_cassette_name("replay-missing");

        let error = start_harness(replay_config(cassette_name.clone()))
            .await
            .expect_err("missing replay cassette should fail");

        assert!(matches!(
            error,
            HarnessError::CassetteNotFound { cassette_name: found }
                if found == cassette_name
        ));
    }

    #[rstest]
    #[tokio::test]
    async fn start_harness_replay_unsupported_cassette_version_fails() {
        let supported = crate::cassette::CassetteFormatVersion::SUPPORTED.as_u32();
        let cassette_name = unique_cassette_name("replay-unsupported");
        seed_replay_cassette(&cassette_name);
        let file = cap_std::fs_utf8::Dir::open_ambient_dir(".", cap_std::ambient_authority())
            .expect("ambient root should open")
            .open_dir("target/test-harness")
            .expect("test harness directory should open")
            .create(&cassette_name)
            .expect("cassette file should open for overwrite");
        serde_json::to_writer_pretty(
            file,
            &serde_json::json!({
                "format_version": 9,
                "interactions": [],
            }),
        )
        .expect("invalid cassette should write");

        let error = start_harness(replay_config(cassette_name))
            .await
            .expect_err("unsupported replay cassette should fail");

        assert!(matches!(
            error,
            HarnessError::UnsupportedCassetteFormatVersion {
                found: 9,
                supported: found_supported,
            }
            if found_supported == supported
        ));
    }

    fn record_config(cassette_name: String) -> HarnessConfig {
        HarnessConfig {
            mode: config::Mode::Record,
            cassette_dir: Utf8PathBuf::from("target/test-harness"),
            cassette_name,
            upstream: Some(config::UpstreamConfig::default()),
            ..HarnessConfig::default()
        }
    }

    fn replay_config(cassette_name: String) -> HarnessConfig {
        HarnessConfig {
            mode: config::Mode::Replay,
            cassette_dir: Utf8PathBuf::from("target/test-harness"),
            cassette_name,
            ..HarnessConfig::default()
        }
    }

    fn unique_cassette_name(prefix: &str) -> String {
        let index = NEXT_TEST_CASSETTE.fetch_add(1, Ordering::Relaxed);
        let uuid = Uuid::new_v4();
        format!("{prefix}-{index}-{uuid}")
    }

    fn seed_replay_cassette(cassette_name: &str) -> Utf8PathBuf {
        let cassette_path = Utf8PathBuf::from("target/test-harness").join(cassette_name);
        let _store = FilesystemCassetteStore::open_or_create_for_record(&cassette_path)
            .expect("seeding replay cassette should succeed");
        cassette_path
    }
}
