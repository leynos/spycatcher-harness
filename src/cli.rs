//! CLI adapter for layered configuration loading.
//!
//! This module keeps command-line parsing and layered configuration loading
//! separate from domain logic. It uses `OrthoConfig` subcommand merging to
//! apply precedence `CLI > env > config files > defaults` within each
//! subcommand namespace (`cmds.record`, `cmds.replay`, `cmds.verify`).

use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser, Subcommand};
use ortho_config::load_and_merge_subcommand;
use ortho_config::subcommand::Prefix;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{HarnessConfig, config};

const CLI_MERGE_HELP: &str = concat!(
    "Configuration precedence: CLI > env > config files > defaults.\n",
    "Subcommand defaults merge from the `cmds` namespace.\n\n",
    "Example:\n",
    "  [cmds.record]\n",
    "  cassette_name = \"session_a\"\n\n",
    "  [cmds.record.upstream]\n",
    "  kind = \"openrouter\"\n",
    "  base_url = \"https://openrouter.ai/api/v1\"\n",
    "  api_key_env = \"OPENROUTER_API_KEY\"\n\n",
    "Environment prefix: SPYCATCHER_HARNESS_CMDS_<SUBCOMMAND>_...\n",
    "Nested keys use double underscores, e.g.\n",
    "SPYCATCHER_HARNESS_CMDS_RECORD_UPSTREAM__BASE_URL."
);

/// Errors returned while loading merged command configuration.
#[derive(Debug, Error)]
pub enum CliConfigError {
    /// CLI parsing failed.
    #[error("failed to parse CLI arguments: {0}")]
    CliParse(#[from] clap::Error),
    /// Subcommand merge failed.
    #[error("failed to merge layered config for `{subcommand}`: {message}")]
    Merge {
        /// The subcommand whose merge failed.
        subcommand: &'static str,
        /// Merge failure message.
        message: String,
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
    load_subcommand_config_from_iter(std::env::args_os())
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
    let cli = parse_cli_from_iter(iter)?;
    let prefix = Prefix::new("SPYCATCHER_HARNESS_");
    cli.command.load_config(&prefix)
}

fn parse_cli_from_iter<I, T>(iter: I) -> Result<Cli, CliConfigError>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    match Cli::try_parse_from(iter) {
        Ok(cli) => Ok(cli),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            Err(CliConfigError::DisplayRequested {
                output: error.to_string(),
            })
        }
        Err(error) => Err(CliConfigError::CliParse(error)),
    }
}

fn merge_subcommand_config<T>(
    prefix: &Prefix,
    args: &T,
    subcommand: &'static str,
) -> Result<T, CliConfigError>
where
    T: serde::de::DeserializeOwned + Serialize + Default + CommandFactory,
{
    load_and_merge_subcommand(prefix, args).map_err(|error| CliConfigError::Merge {
        subcommand,
        message: error.to_string(),
    })
}

#[derive(Debug, Parser)]
#[command(name = "spycatcher-harness")]
#[command(about = "Deterministic record/replay harness for LLM API testing")]
#[command(after_long_help = CLI_MERGE_HELP)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

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
    fn load_config(self, prefix: &Prefix) -> Result<HarnessConfig, CliConfigError> {
        load_command_config(prefix, &self)
    }
}

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

fn load_record_config(prefix: &Prefix, args: &RecordArgs) -> Result<HarnessConfig, CliConfigError> {
    let merged_args: RecordArgs = merge_subcommand_config(prefix, args, "record")?;
    Ok(to_record_config(&merged_args))
}

fn load_replay_config(prefix: &Prefix, args: &ReplayArgs) -> Result<HarnessConfig, CliConfigError> {
    let merged_args: ReplayArgs = merge_subcommand_config(prefix, args, "replay")?;
    Ok(to_replay_config(&merged_args))
}

fn load_verify_config(prefix: &Prefix, args: &VerifyArgs) -> Result<HarnessConfig, CliConfigError> {
    let merged_args: VerifyArgs = merge_subcommand_config(prefix, args, "verify")?;
    Ok(to_verify_config(&merged_args))
}

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
    #[serde(default)]
    #[arg(skip)]
    upstream: Option<RecordUpstreamArgs>,
}

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
}

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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
struct RecordUpstreamArgs {
    #[serde(default)]
    kind: RecordUpstreamKind,
    #[serde(default = "default_record_base_url")]
    base_url: String,
    #[serde(default = "default_record_api_key_env")]
    api_key_env: String,
    #[serde(default)]
    extra_headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
enum RecordUpstreamKind {
    #[serde(alias = "openrouter")]
    #[default]
    OpenRouter,
}

fn default_record_base_url() -> String {
    String::from("https://openrouter.ai/api/v1")
}

fn default_record_api_key_env() -> String {
    String::from("OPENROUTER_API_KEY")
}

/// Common field overrides shared by every subcommand.
#[derive(Clone, Copy)]
struct CommonOverrides<'a> {
    listen: Option<std::net::SocketAddr>,
    cassette_dir: Option<&'a str>,
    cassette_name: Option<&'a str>,
}

impl<'a> From<&'a RecordArgs> for CommonOverrides<'a> {
    fn from(args: &'a RecordArgs) -> Self {
        Self {
            listen: args.listen,
            cassette_dir: args.cassette_dir.as_deref(),
            cassette_name: args.cassette_name.as_deref(),
        }
    }
}

impl<'a> From<&'a ReplayArgs> for CommonOverrides<'a> {
    fn from(args: &'a ReplayArgs) -> Self {
        Self {
            listen: args.listen,
            cassette_dir: args.cassette_dir.as_deref(),
            cassette_name: args.cassette_name.as_deref(),
        }
    }
}

impl<'a> From<&'a VerifyArgs> for CommonOverrides<'a> {
    fn from(args: &'a VerifyArgs) -> Self {
        Self {
            listen: args.listen,
            cassette_dir: args.cassette_dir.as_deref(),
            cassette_name: args.cassette_name.as_deref(),
        }
    }
}

fn build_config(
    overrides: CommonOverrides<'_>,
    mode: config::Mode,
    upstream: Option<config::UpstreamConfig>,
) -> HarnessConfig {
    let mut config = HarnessConfig::default();
    apply_overrides(
        &mut config,
        overrides.listen,
        overrides.cassette_dir,
        overrides.cassette_name,
    );
    config.mode = mode;
    config.upstream = upstream;
    config
}

fn to_record_config(args: &RecordArgs) -> HarnessConfig {
    build_config(
        args.into(),
        config::Mode::Record,
        args.upstream.clone().map(Into::into),
    )
}

fn to_replay_config(args: &ReplayArgs) -> HarnessConfig {
    build_config(args.into(), config::Mode::Replay, None)
}

fn to_verify_config(args: &VerifyArgs) -> HarnessConfig {
    build_config(args.into(), config::Mode::Verify, None)
}

fn apply_overrides(
    config: &mut HarnessConfig,
    listen: Option<std::net::SocketAddr>,
    cassette_dir: Option<&str>,
    cassette_name: Option<&str>,
) {
    if let Some(listen_override) = listen {
        config.listen = listen_override.into();
    }
    if let Some(cassette_dir_override) = cassette_dir {
        config.cassette_dir = Utf8PathBuf::from(cassette_dir_override);
    }
    if let Some(cassette_name_override) = cassette_name {
        cassette_name_override.clone_into(&mut config.cassette_name);
    }
}

impl From<RecordUpstreamArgs> for config::UpstreamConfig {
    fn from(value: RecordUpstreamArgs) -> Self {
        Self {
            kind: value.kind.into(),
            base_url: value.base_url,
            api_key_env: value.api_key_env,
            extra_headers: value.extra_headers,
        }
    }
}

impl From<RecordUpstreamKind> for config::UpstreamKind {
    fn from(value: RecordUpstreamKind) -> Self {
        match value {
            RecordUpstreamKind::OpenRouter => Self::OpenRouter,
        }
    }
}
