//! `spycatcher-harness` CLI binary entry point.
//!
//! Delegates all startup and shutdown behaviour to the
//! [`spycatcher_harness`] library.

use eyre::WrapErr;
use spycatcher_harness::cli::{LoadedSubcommandConfig, load_subcommand_config};
use spycatcher_harness::start_harness;

/// Application entry point.
///
/// # Errors
///
/// Returns an error if configuration loading, startup, or shutdown fails.
fn main() -> eyre::Result<()> {
    let loaded = load_subcommand_config().wrap_err("failed to load merged command config")?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .wrap_err("failed to build tokio runtime")?;
    rt.block_on(run_loaded_command(loaded))
}

async fn run_loaded_command(loaded: LoadedSubcommandConfig) -> eyre::Result<()> {
    let config = match loaded {
        LoadedSubcommandConfig::Record(config)
        | LoadedSubcommandConfig::Replay(config)
        | LoadedSubcommandConfig::Verify(config) => config,
    };
    let harness = start_harness(config)
        .await
        .wrap_err("failed to start harness")?;
    harness
        .shutdown()
        .await
        .wrap_err("failed to shut down harness")
}
