//! CLI adapter for layered configuration loading.
//!
//! This module keeps command-line parsing and layered configuration loading
//! separate from domain logic. It uses `OrthoConfig` subcommand merging to
//! apply precedence `CLI > env > config files > defaults` within each
//! subcommand namespace (`cmds.record`, `cmds.replay`, `cmds.verify`).

use clap::{CommandFactory, Parser, Subcommand};
use ortho_config::load_and_merge_subcommand;
use ortho_config::subcommand::Prefix;
use ortho_config::{Localizer, NoOpLocalizer};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::HarnessConfig;

#[path = "../cli_args.rs"]
mod cli_args;
#[path = "../cli_help.rs"]
mod cli_help;
mod config_build;
#[path = "localization.rs"]
pub mod localization;
#[path = "localize_cmd.rs"]
mod localize_cmd;
#[path = "localizer.rs"]
pub mod localizer;

use cli_args::{LocalizationArgs, RecordUpstreamArgs};
use cli_help::CLI_MERGE_HELP;
use config_build::{to_record_config, to_replay_config, to_verify_config};
use localize_cmd::try_parse_localized_from_iter;

/// Errors returned while loading merged command configuration.
#[derive(Debug, Error)]
pub enum CliConfigError {
    /// CLI parsing failed.
    #[error("{0}")]
    CliParse(#[from] clap::Error),
    /// Subcommand merge failed.
    #[error("failed to merge layered config for `{subcommand}`: {message}")]
    Merge {
        /// The subcommand whose merge failed.
        subcommand: &'static str,
        /// Merge failure message.
        message: String,
    },
    /// A merged localization field did not contain a valid language
    /// identifier.
    #[error("invalid localization field `{field}` value `{value}`: {source}")]
    InvalidLocale {
        /// Field containing the invalid value.
        field: &'static str,
        /// Invalid language identifier text.
        value: String,
        /// Parser failure from `unic-langid`.
        source: i18n_embed::unic_langid::LanguageIdentifierError,
    },
    /// Help/version output was requested and should be printed before a clean
    /// process exit.
    #[error("{output}")]
    DisplayRequested {
        /// Rendered clap output for help/version.
        output: String,
    },
}

/// Loads merged configuration for the selected subcommand using process args.
///
/// # Examples
///
/// ```rust,no_run
/// use spycatcher_harness::cli::load_subcommand_config;
///
/// let config = load_subcommand_config()?;
/// // Use `config` to start the harness.
/// # let _ = config;
/// # Ok::<(), spycatcher_harness::cli::CliConfigError>(())
/// ```
///
/// # Errors
///
/// Returns [`CliConfigError`] if argument parsing fails or if layered loading
/// from files/environment fails.
pub fn load_subcommand_config() -> Result<HarnessConfig, CliConfigError> {
    load_subcommand_config_with_localizer(&NoOpLocalizer::new())
}

/// Loads merged configuration for the selected subcommand using `localizer`.
///
/// # Errors
///
/// Returns [`CliConfigError`] if argument parsing fails or if layered loading
/// from files/environment fails.
pub fn load_subcommand_config_with_localizer(
    localizer: &dyn Localizer,
) -> Result<HarnessConfig, CliConfigError> {
    load_subcommand_config_from_iter_with_localizer(std::env::args_os(), localizer)
}

/// Loads merged configuration for the selected subcommand from `iter`.
///
/// # Examples
///
/// ```rust,no_run
/// use spycatcher_harness::cli::load_subcommand_config_from_iter;
///
/// let config = load_subcommand_config_from_iter(["spycatcher-harness", "replay"])?;
/// assert_eq!(config.cassette_name, "default");
/// # Ok::<(), spycatcher_harness::cli::CliConfigError>(())
/// ```
///
/// # Errors
///
/// Returns [`CliConfigError`] if argument parsing fails or if layered loading
/// from files/environment fails.
pub fn load_subcommand_config_from_iter<I, T>(iter: I) -> Result<HarnessConfig, CliConfigError>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    load_subcommand_config_from_iter_with_localizer(iter, &NoOpLocalizer::new())
}

/// Loads merged configuration for the selected subcommand from `iter` using
/// `localizer`.
///
/// # Errors
///
/// Returns [`CliConfigError`] if argument parsing fails or if layered loading
/// from files/environment fails.
pub fn load_subcommand_config_from_iter_with_localizer<I, T>(
    iter: I,
    localizer: &dyn Localizer,
) -> Result<HarnessConfig, CliConfigError>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = parse_cli_from_iter(iter, localizer)?;
    let prefix = Prefix::new("SPYCATCHER_HARNESS_");
    cli.command.load_config(&prefix)
}

/// Parses [`Cli`] from `iter`, mapping help/version requests to display output.
fn parse_cli_from_iter<I, T>(iter: I, localizer: &dyn Localizer) -> Result<Cli, CliConfigError>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    match try_parse_localized_from_iter::<Cli, _, _>(iter, localizer) {
        Ok(cli) => Ok(cli),
        Err(error) if error.use_stderr() => Err(CliConfigError::CliParse(error)),
        Err(error) => Err(CliConfigError::DisplayRequested {
            output: error.to_string(),
        }),
    }
}

/// Merges env and config-file layers into subcommand `args` using `prefix`.
fn merge_subcommand_config<T>(
    prefix: &Prefix,
    args: &T,
    subcommand: &'static str,
) -> Result<T, CliConfigError>
where
    T: serde::de::DeserializeOwned + Serialize + Default + CommandFactory,
{
    trace_subcommand_merge_start(subcommand);
    let merged_args = load_and_merge_subcommand(prefix, args)
        .map_err(|error| merge_subcommand_failure(subcommand, error))?;
    trace_subcommand_merge_success(subcommand);
    Ok(merged_args)
}

/// Records the start of layered merging for one subcommand.
/// For example, passing `"replay"` emits a merge-start debug event with a
/// `subcommand="replay"` label.
fn trace_subcommand_merge_start(subcommand: &'static str) {
    tracing::debug!(subcommand, "merging layered subcommand configuration");
}

/// Records successful completion of layered merging.
/// For example, passing `"replay"` emits a merge-complete debug event with a
/// `subcommand="replay"` label.
fn trace_subcommand_merge_success(subcommand: &'static str) {
    tracing::debug!(subcommand, "merged layered subcommand configuration");
}

/// Retains the subcommand name and the original merge failure text.
/// For example, `merge_subcommand_failure("record", "invalid value")` returns
/// `CliConfigError::Merge` with subcommand `"record"` and message `"invalid value"`.
fn merge_subcommand_failure(
    subcommand: &'static str,
    error: impl std::fmt::Display,
) -> CliConfigError {
    tracing::warn!(subcommand, %error, "failed to merge layered subcommand configuration");
    CliConfigError::Merge {
        subcommand,
        message: error.to_string(),
    }
}

/// Root command parsed before loading its selected subcommand's configuration.
#[derive(Debug, Parser)]
#[command(name = "spycatcher-harness")]
#[command(version)]
#[command(about = "Deterministic record/replay harness for LLM API testing")]
#[command(after_long_help = CLI_MERGE_HELP)]
struct Cli {
    /// Subcommand selected by the caller.
    #[command(subcommand)]
    command: Commands,
}

/// Supported commands with their command-line arguments.
#[derive(Debug, Subcommand)]
enum Commands {
    /// Proxy to upstream and record interactions.
    Record(RecordArgs),
    /// Replay interactions from a cassette.
    Replay(ReplayArgs),
    /// Verify cassette and configuration integrity.
    Verify(VerifyArgs),
}

impl Commands {
    /// Loads the layered configuration for this command.
    /// For example, selecting `Replay` loads replay settings through the supplied
    /// prefix and returns the validated replay-mode `HarnessConfig`.
    fn load_config(self, prefix: &Prefix) -> Result<HarnessConfig, CliConfigError> {
        load_command_config(prefix, &self)
    }
}

/// Dispatches configuration loading to the subcommand-specific loader.
fn load_command_config(
    prefix: &Prefix,
    command: &Commands,
) -> Result<HarnessConfig, CliConfigError> {
    match command {
        Commands::Record(args) => load_record_config(prefix, args),
        Commands::Replay(args) => load_replay_config(prefix, args),
        Commands::Verify(args) => load_verify_config(prefix, args),
    }
}

/// Merges record subcommand configuration into [`HarnessConfig`].
fn load_record_config(prefix: &Prefix, args: &RecordArgs) -> Result<HarnessConfig, CliConfigError> {
    let merged_args: RecordArgs = merge_subcommand_config(prefix, args, "record")?;
    to_record_config(args, &merged_args)
}

/// Merges replay subcommand configuration into [`HarnessConfig`].
fn load_replay_config(prefix: &Prefix, args: &ReplayArgs) -> Result<HarnessConfig, CliConfigError> {
    let merged_args: ReplayArgs = merge_subcommand_config(prefix, args, "replay")?;
    to_replay_config(args, &merged_args)
}

/// Merges verify subcommand configuration into [`HarnessConfig`].
/// Errors if merging or locale validation fails.
fn load_verify_config(prefix: &Prefix, args: &VerifyArgs) -> Result<HarnessConfig, CliConfigError> {
    let merged_args: VerifyArgs = merge_subcommand_config(prefix, args, "verify")?;
    to_verify_config(args, &merged_args)
}

/// Record-mode arguments merged from the command line and configuration.
#[derive(Debug, Clone, Parser, Serialize, Deserialize, Default, PartialEq)]
#[command(name = "record")]
struct RecordArgs {
    /// Listen address for the harness server (e.g. 127.0.0.1:8787).
    #[arg(long)]
    listen: Option<std::net::SocketAddr>,
    /// Directory containing cassette files.
    #[arg(long)]
    cassette_dir: Option<String>,
    /// Cassette name for this subcommand invocation.
    #[arg(long)]
    cassette_name: Option<String>,
    /// Locale for localized application messages.
    #[arg(long)]
    locale: Option<String>,
    /// Fallback locale for localized application messages.
    #[arg(long)]
    fallback_locale: Option<String>,
    /// Localization fields supplied by layered configuration.
    #[serde(default)]
    #[arg(skip)]
    localization: LocalizationArgs,
    /// Upstream settings supplied by layered configuration.
    #[serde(default)]
    #[arg(skip)]
    upstream: Option<RecordUpstreamArgs>,
}

/// Replay-mode arguments merged from the command line and configuration.
#[derive(Debug, Clone, Parser, Serialize, Deserialize, Default, PartialEq)]
#[command(name = "replay")]
struct ReplayArgs {
    /// Listen address for the harness server (e.g. 127.0.0.1:8787).
    #[arg(long)]
    listen: Option<std::net::SocketAddr>,
    /// Directory containing cassette files.
    #[arg(long)]
    cassette_dir: Option<String>,
    /// Cassette name for this subcommand invocation.
    #[arg(long)]
    cassette_name: Option<String>,
    /// Locale for localized application messages.
    #[arg(long)]
    locale: Option<String>,
    /// Fallback locale for localized application messages.
    #[arg(long)]
    fallback_locale: Option<String>,
    /// Localization fields supplied by layered configuration.
    #[serde(default)]
    #[arg(skip)]
    localization: LocalizationArgs,
}
/// Verify-mode arguments merged from the command line and configuration.
#[derive(Debug, Clone, Parser, Serialize, Deserialize, Default, PartialEq)]
#[command(name = "verify")]
struct VerifyArgs {
    /// Listen address for the harness server (e.g. 127.0.0.1:8787).
    #[arg(long)]
    listen: Option<std::net::SocketAddr>,
    /// Directory containing cassette files.
    #[arg(long)]
    cassette_dir: Option<String>,
    /// Cassette name for this subcommand invocation.
    #[arg(long)]
    cassette_name: Option<String>,
    /// Locale for localized application messages.
    #[arg(long)]
    locale: Option<String>,
    /// Fallback locale for localized application messages.
    #[arg(long)]
    fallback_locale: Option<String>,
    /// Localization fields supplied by layered configuration.
    #[serde(default)]
    #[arg(skip)]
    localization: LocalizationArgs,
}
