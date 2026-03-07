//! Unit tests for CLI layered configuration loading.

use std::cell::RefCell;

use eyre::{Result, eyre};
use ortho_config::figment;
use rstest::rstest;

use spycatcher_harness::cli::load_subcommand_config_from_iter;
use spycatcher_harness::{HarnessConfig, config};

#[expect(
    clippy::result_large_err,
    reason = "figment::Jail callback requires figment::error::Result"
)]
fn load_with_jail(
    argv: &[&str],
    config_file: Option<&str>,
    env_vars: &[(&str, &str)],
) -> Result<HarnessConfig> {
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

const REPLAY_FILE_CONFIG: &str = "[cmds.replay]\ncassette_name = \"from_file\"\n";

#[rstest]
#[case(
    &["spycatcher-harness", "replay"],
    None,
    &[],
    "default",
)]
#[case(
    &["spycatcher-harness", "replay"],
    Some(REPLAY_FILE_CONFIG),
    &[],
    "from_file",
)]
#[case(
    &["spycatcher-harness", "replay"],
    Some(REPLAY_FILE_CONFIG),
    &[("SPYCATCHER_HARNESS_CMDS_REPLAY_CASSETTE_NAME", "from_env")],
    "from_env",
)]
#[case(
    &["spycatcher-harness", "replay", "--cassette-name", "from_cli"],
    Some(REPLAY_FILE_CONFIG),
    &[("SPYCATCHER_HARNESS_CMDS_REPLAY_CASSETTE_NAME", "from_env")],
    "from_cli",
)]
fn replay_cassette_name_precedence(
    #[case] argv: &[&str],
    #[case] config_file: Option<&str>,
    #[case] env_vars: &[(&str, &str)],
    #[case] expected_cassette_name: &str,
) {
    let loaded = load_with_jail(argv, config_file, env_vars).expect("config should load");
    assert_eq!(loaded.cassette_name, expected_cassette_name);
    assert_eq!(loaded.mode, config::Mode::Replay);
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
    let upstream = loaded
        .upstream
        .expect("record config should contain upstream values");
    assert_eq!(loaded.cassette_name, "cassette_a");
    assert_eq!(loaded.mode, config::Mode::Record);
    assert_eq!(upstream.base_url, "https://example.invalid/api");
    assert_eq!(upstream.api_key_env, "TEST_API_KEY");
}

#[rstest]
fn verify_supports_cmds_namespace_overrides() {
    let config = "[cmds.verify]\ncassette_name = \"verify_cassette\"\n";
    let loaded = load_with_jail(&["spycatcher-harness", "verify"], Some(config), &[])
        .expect("verify config should load");
    assert_eq!(loaded.cassette_name, "verify_cassette");
    assert_eq!(loaded.mode, config::Mode::Verify);
}

#[rstest]
fn invalid_values_fail_loading() {
    let error = load_with_jail(
        &["spycatcher-harness", "replay"],
        None,
        &[("SPYCATCHER_HARNESS_CMDS_REPLAY_LISTEN", "not-an-address")],
    )
    .expect_err("invalid listen should fail loading");
    let message = error.to_string();
    assert!(
        (message.contains("invalid") && message.contains("address"))
            || (message.contains("socket") && message.contains("parse")),
        "expected error about invalid listen address, got: {message}",
    );
}
