//! `spycatcher-harness` CLI binary entry point.
//!
//! Delegates all startup and shutdown behaviour to the
//! [`spycatcher_harness`] library.

use eyre::WrapErr;
use spycatcher_harness::cli::{CliConfigError, load_subcommand_config};
use spycatcher_harness::start_harness;
use std::io::Write;

/// Application entry point.
///
/// # Errors
///
/// Returns an error if configuration loading, startup, or shutdown fails.
fn main() -> eyre::Result<()> {
    let config = load_config_or_display_output()?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .wrap_err("failed to build tokio runtime")?;
    rt.block_on(run_harness(config))
}

fn load_config_or_display_output() -> eyre::Result<spycatcher_harness::HarnessConfig> {
    match load_subcommand_config() {
        Ok(config) => Ok(config),
        Err(CliConfigError::DisplayRequested { output }) => {
            write_display_output(&output).wrap_err("failed to write CLI output")?;
            std::process::exit(0);
        }
        Err(error) => Err(error).wrap_err("failed to load merged command config"),
    }
}

fn write_display_output(output: &str) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()
}

async fn run_harness(config: spycatcher_harness::HarnessConfig) -> eyre::Result<()> {
    let harness = start_harness(config)
        .await
        .wrap_err("failed to start harness")?;
    harness
        .shutdown()
        .await
        .wrap_err("failed to shut down harness")
}
