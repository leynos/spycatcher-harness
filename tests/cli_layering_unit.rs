//! Unit tests for CLI layered configuration loading.

use std::cell::RefCell;

use eyre::{Result, eyre};
use ortho_config::figment;
use rstest::rstest;

use spycatcher_harness::HarnessConfig;
use spycatcher_harness::cli::{LoadedSubcommandConfig, load_subcommand_config_from_iter};

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
        .expect("record config should contain upstream values");
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
