//! Cassette-name layering tests for the `replay` subcommand.
//!
//! These cases pin the CLI > environment > config-file > default precedence
//! for `cassette_name`, and prove that record-only values never leak into the
//! other subcommands.

use rstest::rstest;

use spycatcher_harness::config;

use super::cases::{CassetteNameCase, Subcommand};
use super::support::{REPLAY_FILE_CONFIG, load_with_sources};

#[rstest]
#[case(CassetteNameCase {
    argv: &["spycatcher-harness", "replay"],
    config_file: None,
    env_vars: &[],
    expected_cassette_name: "default",
})]
#[case(CassetteNameCase {
    argv: &["spycatcher-harness", "replay"],
    config_file: Some(REPLAY_FILE_CONFIG),
    env_vars: &[],
    expected_cassette_name: "from_file",
})]
#[case(CassetteNameCase {
    argv: &["spycatcher-harness", "replay"],
    config_file: Some(REPLAY_FILE_CONFIG),
    env_vars: &[("SPYCATCHER_HARNESS_CMDS_REPLAY_CASSETTE_NAME", "from_env")],
    expected_cassette_name: "from_env",
})]
#[case(CassetteNameCase {
    argv: &["spycatcher-harness", "replay", "--cassette-name", "from_cli"],
    config_file: Some(REPLAY_FILE_CONFIG),
    env_vars: &[("SPYCATCHER_HARNESS_CMDS_REPLAY_CASSETTE_NAME", "from_env")],
    expected_cassette_name: "from_cli",
})]
fn replay_cassette_name_precedence(#[case] case: CassetteNameCase) {
    let loaded =
        load_with_sources(case.argv, case.config_file, case.env_vars).expect("config should load");
    assert_eq!(loaded.cassette_name, case.expected_cassette_name);
    assert_eq!(loaded.mode, config::Mode::Replay);
}

#[rstest]
fn explicit_cli_value_equal_to_application_default_wins() {
    let loaded = load_with_sources(
        &["spycatcher-harness", "replay", "--cassette-name", "default"],
        Some(REPLAY_FILE_CONFIG),
        &[("SPYCATCHER_HARNESS_CMDS_REPLAY_CASSETTE_NAME", "from_env")],
    )
    .expect("explicit CLI default should load");

    assert_eq!(loaded.cassette_name, "default");
}

#[rstest]
#[case(Subcommand::Replay)]
#[case(Subcommand::Verify)]
fn record_only_values_do_not_leak_to_other_subcommands(#[case] subcommand: Subcommand) {
    let config = "[cmds.record]\ncassette_name = \"record_only_file\"\n";
    let loaded = load_with_sources(
        &["spycatcher-harness", subcommand.as_str()],
        Some(config),
        &[(
            "SPYCATCHER_HARNESS_CMDS_RECORD_CASSETTE_NAME",
            "record_only_env",
        )],
    )
    .expect("other subcommand should ignore record-only values");

    assert_eq!(loaded.cassette_name, "default");
    assert_eq!(loaded.mode, subcommand.mode());
}
