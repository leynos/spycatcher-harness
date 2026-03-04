//! CLI adapter for layered configuration loading.
//!
//! This module keeps command-line parsing and layered configuration loading
//! separate from domain logic. It uses `OrthoConfig` subcommand merging to
//! apply precedence `CLI > env > config files > defaults` within each
//! subcommand namespace (`cmds.record`, `cmds.replay`, `cmds.verify`).

use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
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
        Commands::Record(args) => {
            let merged = load_and_merge_subcommand(&prefix, &args).map_err(|error| {
                CliConfigError::Merge {
                    subcommand: "record",
                    message: error.to_string(),
                }
            })?;
            Ok(LoadedSubcommandConfig::Record(to_record_config(merged)))
        }
        Commands::Replay(args) => {
            let merged = load_and_merge_subcommand(&prefix, &args).map_err(|error| {
                CliConfigError::Merge {
                    subcommand: "replay",
                    message: error.to_string(),
                }
            })?;
            Ok(LoadedSubcommandConfig::Replay(to_replay_config(&merged)))
        }
        Commands::Verify(args) => {
            let merged = load_and_merge_subcommand(&prefix, &args).map_err(|error| {
                CliConfigError::Merge {
                    subcommand: "verify",
                    message: error.to_string(),
                }
            })?;
            Ok(LoadedSubcommandConfig::Verify(to_verify_config(&merged)))
        }
    }
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

fn to_record_config(args: RecordArgs) -> HarnessConfig {
    let mut config = HarnessConfig::default();
    apply_overrides(
        &mut config,
        args.listen,
        args.cassette_dir.as_deref(),
        args.cassette_name.as_deref(),
    );
    config.mode = config::Mode::Record;
    config.upstream = args.upstream.map(Into::into);
    config
}

fn to_replay_config(args: &ReplayArgs) -> HarnessConfig {
    let mut config = HarnessConfig::default();
    apply_overrides(
        &mut config,
        args.listen,
        args.cassette_dir.as_deref(),
        args.cassette_name.as_deref(),
    );
    config.mode = config::Mode::Replay;
    config.upstream = None;
    config
}

fn to_verify_config(args: &VerifyArgs) -> HarnessConfig {
    let mut config = HarnessConfig::default();
    apply_overrides(
        &mut config,
        args.listen,
        args.cassette_dir.as_deref(),
        args.cassette_name.as_deref(),
    );
    config.mode = config::Mode::Replay;
    config.upstream = None;
    config
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

#[cfg(test)]
mod tests {
    //! Unit tests for CLI layered configuration loading.

    use std::cell::RefCell;

    use eyre::{Result, eyre};
    use ortho_config::figment;
    use rstest::rstest;

    use super::*;

    #[expect(
        clippy::result_large_err,
        reason = "figment::Jail callback requires figment::error::Result"
    )]
    fn load_with_jail(
        argv: &[&str],
        config_file: Option<&str>,
        env_vars: &[(&str, &str)],
    ) -> Result<LoadedSubcommandConfig> {
        let loaded = RefCell::new(None);

        figment::Jail::try_with(|jail| {
            if let Some(content) = config_file {
                jail.create_file(".spycatcher_harness.toml", content)?;
            }
            for (key, value) in env_vars {
                jail.set_env(key, value);
            }
            let cfg = load_subcommand_config_from_iter(argv)
                .map_err(|error| figment::Error::from(error.to_string()))?;
            loaded.replace(Some(cfg));
            Ok(())
        })
        .map_err(|error| eyre!(error.to_string()))?;

        loaded
            .into_inner()
            .ok_or_else(|| eyre!("configuration load did not execute"))
    }

    fn replay_config(loaded: LoadedSubcommandConfig) -> HarnessConfig {
        match loaded {
            LoadedSubcommandConfig::Replay(cfg) => cfg,
            other => panic!("expected replay config, got {other:?}"),
        }
    }

    #[rstest]
    fn precedence_uses_defaults_when_no_sources_present() {
        let loaded = load_with_jail(&["spycatcher-harness", "replay"], None, &[])
            .expect("defaults-only replay config should load");
        let cfg = replay_config(loaded);
        assert_eq!(cfg.cassette_name, "default");
    }

    #[rstest]
    fn precedence_uses_file_over_defaults() {
        let config = "[cmds.replay]\ncassette_name = \"from_file\"\n";
        let loaded = load_with_jail(&["spycatcher-harness", "replay"], Some(config), &[])
            .expect("file-backed replay config should load");
        let cfg = replay_config(loaded);
        assert_eq!(cfg.cassette_name, "from_file");
    }

    #[rstest]
    fn precedence_uses_env_over_file() {
        let config = "[cmds.replay]\ncassette_name = \"from_file\"\n";
        let loaded = load_with_jail(
            &["spycatcher-harness", "replay"],
            Some(config),
            &[("SPYCATCHER_HARNESS_CMDS_REPLAY_CASSETTE_NAME", "from_env")],
        )
        .expect("env override should merge for replay");
        let cfg = replay_config(loaded);
        assert_eq!(cfg.cassette_name, "from_env");
    }

    #[rstest]
    fn precedence_uses_cli_over_env() {
        let config = "[cmds.replay]\ncassette_name = \"from_file\"\n";
        let loaded = load_with_jail(
            &[
                "spycatcher-harness",
                "replay",
                "--cassette-name",
                "from_cli",
            ],
            Some(config),
            &[("SPYCATCHER_HARNESS_CMDS_REPLAY_CASSETTE_NAME", "from_env")],
        )
        .expect("CLI override should win over env for replay");
        let cfg = replay_config(loaded);
        assert_eq!(cfg.cassette_name, "from_cli");
    }

    #[rstest]
    fn record_supports_cmds_namespace_for_nested_upstream_values() {
        let config = concat!(
            "[cmds.record]\n",
            "cassette_name = \"cassette_a\"\n",
            "[cmds.record.upstream]\n",
            "kind = \"openrouter\"\n",
            "base_url = \"https://example.invalid/api\"\n",
            "api_key_env = \"TEST_API_KEY\"\n"
        );

        let loaded = load_with_jail(&["spycatcher-harness", "record"], Some(config), &[])
            .expect("record config should load");
        let LoadedSubcommandConfig::Record(cfg) = loaded else {
            panic!("expected record config");
        };

        let upstream = cfg
            .upstream
            .unwrap_or_else(|| panic!("record config should contain upstream values"));
        assert_eq!(cfg.cassette_name, "cassette_a");
        assert_eq!(upstream.base_url, "https://example.invalid/api");
        assert_eq!(upstream.api_key_env, "TEST_API_KEY");
    }

    #[rstest]
    fn verify_supports_cmds_namespace_overrides() {
        let config = "[cmds.verify]\ncassette_name = \"verify_cassette\"\n";
        let loaded = load_with_jail(&["spycatcher-harness", "verify"], Some(config), &[])
            .expect("verify config should load");

        let LoadedSubcommandConfig::Verify(cfg) = loaded else {
            panic!("expected verify config");
        };

        assert_eq!(cfg.cassette_name, "verify_cassette");
    }

    #[rstest]
    fn invalid_values_fail_loading() {
        let loaded = load_with_jail(
            &["spycatcher-harness", "replay"],
            None,
            &[("SPYCATCHER_HARNESS_CMDS_REPLAY_LISTEN", "not-an-address")],
        );
        assert!(loaded.is_err(), "invalid listen should fail loading");
    }
}
