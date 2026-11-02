//! Explicit-source entry point for loading selected CLI configuration.
//!
//! This application adapter passes v0.10.0's independent file-discovery and
//! environment-scan sources through the same subcommand conversion path used
//! by the process-backed loader.

use clap::CommandFactory;
use ortho_config::{
    Localizer,
    SharedScanEnvSource,
    SubcommandFileContext,
    load_and_merge_subcommand_with_sources_at,
    subcommand::Prefix,
};
use serde::{Serialize, de::DeserializeOwned};

use super::{Cli, CliConfigError, Commands, RecordArgs, ReplayArgs, VerifyArgs};

const CONFIG_PREFIX: &str = "SPYCATCHER_HARNESS_";

/// Loads a selected subcommand using explicit file and environment sources.
///
/// The `files` context supplies the base directory and named discovery
/// lookups. `merge_source` supplies environment values for the prefixed merge
/// layer. Passing a `MapEnv` for both keeps the call independent of ambient
/// process state while preserving the standard `CLI > environment > files >
/// defaults` precedence.
///
/// # Examples
///
/// ```rust,no_run
/// use std::{path::Path, sync::Arc};
///
/// use ortho_config::{
///     MapEnv,
///     NoOpLocalizer,
///     SharedEnvSource,
///     SharedScanEnvSource,
///     SubcommandFileContext,
/// };
/// use spycatcher_harness::cli::load_subcommand_config_from_iter_with_sources;
///
/// let source = Arc::new(MapEnv::new());
/// let discovery: SharedEnvSource = source.clone();
/// let merge: SharedScanEnvSource = source;
/// let files = SubcommandFileContext::new(Path::new("."), discovery.as_ref());
/// let localizer = NoOpLocalizer::new();
/// let config = load_subcommand_config_from_iter_with_sources(
///     ["spycatcher-harness", "replay"],
///     &localizer,
///     files,
///     merge,
/// )?;
/// # let _ = config;
/// # Ok::<(), spycatcher_harness::cli::CliConfigError>(())
/// ```
///
/// # Errors
///
/// Returns [`CliConfigError`] when argument parsing, file discovery, source
/// merging, or application-level configuration validation fails.
pub fn load_subcommand_config_from_iter_with_sources<I, T>(
    iter: I,
    localizer: &dyn Localizer,
    files: SubcommandFileContext<'_>,
    merge_source: SharedScanEnvSource,
) -> Result<crate::HarnessConfig, CliConfigError>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = super::parse_cli_from_iter(iter, localizer)?;
    let sources = ConfigSources {
        prefix: Prefix::new(CONFIG_PREFIX),
        files,
        merge_source,
    };
    load_selected_subcommand(cli, sources)
}

/// Carries the independent capabilities needed for one configuration merge.
struct ConfigSources<'a> {
    /// Prefix used to select this application's configuration namespace.
    prefix: Prefix,
    /// Named environment lookups and base directory for file discovery.
    files: SubcommandFileContext<'a>,
    /// Scanned environment values merged beneath explicit CLI values.
    merge_source: SharedScanEnvSource,
}

/// Merges the selected command's configuration, preserving its own namespace.
fn load_selected_subcommand(
    cli: Cli,
    sources: ConfigSources<'_>,
) -> Result<crate::HarnessConfig, CliConfigError> {
    match cli.command {
        Commands::Record(args) => {
            let merged: RecordArgs = merge_subcommand_config(&args, "record", sources)?;
            super::config_build::to_record_config(&args, &merged)
        }
        Commands::Replay(args) => {
            let merged: ReplayArgs = merge_subcommand_config(&args, "replay", sources)?;
            super::config_build::to_replay_config(&args, &merged)
        }
        Commands::Verify(args) => {
            let merged: VerifyArgs = merge_subcommand_config(&args, "verify", sources)?;
            super::config_build::to_verify_config(&args, &merged)
        }
    }
}

/// Loads one command's file and environment layers before its explicit CLI values.
fn merge_subcommand_config<T>(
    args: &T,
    subcommand: &'static str,
    sources: ConfigSources<'_>,
) -> Result<T, CliConfigError>
where
    T: Serialize + DeserializeOwned + Default + CommandFactory,
{
    super::trace_subcommand_merge_start(subcommand);
    let merged = load_and_merge_subcommand_with_sources_at(
        &sources.prefix,
        args,
        sources.files,
        sources.merge_source,
    )
    .map_err(|error| super::merge_subcommand_failure(subcommand, error))?;
    super::trace_subcommand_merge_success(subcommand);
    Ok(merged)
}
