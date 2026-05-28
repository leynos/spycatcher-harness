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
mod http_exchange;
pub mod i18n;
pub mod protocol;
pub mod replay;
pub mod server;
mod sse;
pub mod upstream;

pub use config::HarnessConfig;
pub use error::{HarnessError, HarnessResult};

use std::net::SocketAddr;

use camino::Utf8PathBuf;

use crate::cassette::{filesystem::FilesystemCassetteStore, filesystem::probe_record_write_access};
use crate::server::ServerHandle;

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
    runtime: Option<ServerHandle>,
}

impl RunningHarness {
    /// Gracefully shuts down the running harness.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError`] if shutdown fails.
    pub async fn shutdown(self) -> HarnessResult<()> {
        match self.runtime {
            Some(runtime) => runtime.shutdown().await,
            None => Ok(()),
        }
    }
}

/// Starts the harness with the given configuration.
///
/// Validates the configuration, prepares cassette access for the selected
/// mode, and starts the selected HTTP server when recording or replaying is
/// enabled.
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
pub async fn start_harness(cfg: HarnessConfig) -> HarnessResult<RunningHarness> {
    validate_config(&cfg)?;

    let cassette_path = cfg.cassette_dir.join(&cfg.cassette_name);
    prepare_cassette(&cfg, &cassette_path)?;
    let (addr, runtime) = match cfg.mode {
        config::Mode::Record => {
            let (addr, runtime) = server::start_record_server(&cfg, &cassette_path).await?;
            (addr, Some(runtime))
        }
        config::Mode::Replay => {
            let (addr, runtime) = server::start_replay_server(&cfg, &cassette_path).await?;
            (addr, Some(runtime))
        }
        config::Mode::Verify => {
            return Err(HarnessError::ModeNotYetImplemented {
                mode: format!("{:?}", cfg.mode),
            });
        }
    };

    Ok(RunningHarness {
        addr,
        cassette_path,
        runtime,
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
            FilesystemCassetteStore::open_or_create_for_record(cassette_path)?;
            probe_record_write_access(cassette_path)?;
            Ok(())
        }
        config::Mode::Replay | config::Mode::Verify => {
            FilesystemCassetteStore::open_for_replay(cassette_path)?;
            Ok(())
        }
    }
}
