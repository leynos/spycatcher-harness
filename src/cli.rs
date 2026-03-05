//! CLI adapter for layered configuration loading.
//!
//! This module keeps command-line parsing and layered configuration loading
//! separate from domain logic. It uses `OrthoConfig` subcommand merging to
//! apply precedence `CLI > env > config files > defaults` within each
//! subcommand namespace (`cmds.record`, `cmds.replay`, `cmds.verify`).

use std::collections::BTreeMap;

use camino::Utf8PathBuf;
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

/// Parsed command with effective layered configuration.
#[derive(Debug, Clone)]
pub enum LoadedSubcommandConfig {
    /// Effective config for the `record` subcommand.
    Record(HarnessConfig),
    /// Effective config for the `replay` subcommand.
    Replay(HarnessConfig),
    /// Effective config for the `verify` subcommand.
    Verify(HarnessConfig),
}

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
}

/// Loads merged configuration for the selected subcommand using process args.
///
/// # Errors
///
/// Returns [`CliConfigError`] if argument parsing fails or if layered loading
/// from files/environment fails.
pub fn load_subcommand_config() -> Result<LoadedSubcommandConfig, CliConfigError> {
    load_subcommand_config_from_iter(std::env::args_os())
}

/// Loads merged configuration for the selected subcommand from `iter`.
///
/// # Errors
///
/// Returns [`CliConfigError`] if argument parsing fails or if layered loading
/// from files/environment fails.
pub fn load_subcommand_config_from_iter<I, T>(
    iter: I,
) -> Result<LoadedSubcommandConfig, CliConfigError>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::try_parse_from(iter)?;
    let prefix = Prefix::new("SPYCATCHER_HARNESS_");

    match cli.command {
        Commands::Record(args) => load_record_config(&prefix, &args),
        Commands::Replay(args) => load_replay_config(&prefix, &args),
        Commands::Verify(args) => load_verify_config(&prefix, &args),
    }
}

fn load_record_config(
    prefix: &Prefix,
    args: &RecordArgs,
) -> Result<LoadedSubcommandConfig, CliConfigError> {
    let merged_args = merge_subcommand_config(prefix, args, "record")?;
    Ok(LoadedSubcommandConfig::Record(to_record_config(
        &merged_args,
    )))
}

fn load_replay_config(
    prefix: &Prefix,
    args: &ReplayArgs,
) -> Result<LoadedSubcommandConfig, CliConfigError> {
    let merged_args = merge_subcommand_config(prefix, args, "replay")?;
    Ok(LoadedSubcommandConfig::Replay(to_replay_config(
        &merged_args,
    )))
}

fn load_verify_config(
    prefix: &Prefix,
    args: &VerifyArgs,
) -> Result<LoadedSubcommandConfig, CliConfigError> {
    let merged_args = merge_subcommand_config(prefix, args, "verify")?;
    Ok(LoadedSubcommandConfig::Verify(to_verify_config(
        &merged_args,
    )))
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

trait CommonArgs {
    fn listen(&self) -> Option<std::net::SocketAddr>;
    fn cassette_dir(&self) -> Option<&str>;
    fn cassette_name(&self) -> Option<&str>;
}

impl CommonArgs for RecordArgs {
    fn listen(&self) -> Option<std::net::SocketAddr> {
        self.listen
    }
    fn cassette_dir(&self) -> Option<&str> {
        self.cassette_dir.as_deref()
    }
    fn cassette_name(&self) -> Option<&str> {
        self.cassette_name.as_deref()
    }
}

impl CommonArgs for ReplayArgs {
    fn listen(&self) -> Option<std::net::SocketAddr> {
        self.listen
    }
    fn cassette_dir(&self) -> Option<&str> {
        self.cassette_dir.as_deref()
    }
    fn cassette_name(&self) -> Option<&str> {
        self.cassette_name.as_deref()
    }
}

impl CommonArgs for VerifyArgs {
    fn listen(&self) -> Option<std::net::SocketAddr> {
        self.listen
    }
    fn cassette_dir(&self) -> Option<&str> {
        self.cassette_dir.as_deref()
    }
    fn cassette_name(&self) -> Option<&str> {
        self.cassette_name.as_deref()
    }
}

fn build_config(
    args: &impl CommonArgs,
    mode: config::Mode,
    upstream: Option<config::UpstreamConfig>,
) -> HarnessConfig {
    let mut config = HarnessConfig::default();
    apply_overrides(
        &mut config,
        args.listen(),
        args.cassette_dir(),
        args.cassette_name(),
    );
    config.mode = mode;
    config.upstream = upstream;
    config
}

fn to_record_config(args: &RecordArgs) -> HarnessConfig {
    let upstream = args.upstream.clone().map(Into::into);
    build_config(args, config::Mode::Record, upstream)
}

fn to_replay_config(args: &ReplayArgs) -> HarnessConfig {
    build_config(args, config::Mode::Replay, None)
}

fn to_verify_config(args: &VerifyArgs) -> HarnessConfig {
    build_config(args, config::Mode::Replay, None)
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
