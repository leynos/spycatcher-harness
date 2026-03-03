//! `spycatcher-harness` CLI binary entry point.
//!
//! Delegates all startup and shutdown behaviour to the
//! [`spycatcher_harness`] library. CLI argument parsing and `OrthoConfig`
//! integration are introduced in task 1.1.2.

use eyre::WrapErr;
use spycatcher_harness::{HarnessConfig, start_harness};

/// Application entry point.
///
/// Constructs a default configuration, starts the harness, and shuts
/// it down.  This placeholder will be replaced with CLI argument
/// parsing in task 1.1.2.
///
/// # Errors
///
/// Exits with a non-zero status if harness startup or shutdown fails.
#[tokio::main(flavor = "current_thread")]
async fn main() -> eyre::Result<()> {
    let cfg = HarnessConfig::default();
    let harness = start_harness(cfg)
        .await
        .wrap_err("failed to start harness")?;
    harness
        .shutdown()
        .await
        .wrap_err("failed to shut down harness")?;
    Ok(())
}
