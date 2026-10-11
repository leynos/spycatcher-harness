//! Unit tests for CLI layered configuration loading.
//!
//! This suite exercises [`spycatcher_harness::cli::load_subcommand_config_from_iter`]
//! in isolated child processes to control configuration discovery and environment.
//! It covers:
//!
//! - Cassette-name precedence (CLI > env > config file > default) for `replay`.
//! - Localization-field precedence for `locale` and `fallback_locale` across all
//!   three subcommands (`record`, `replay`, `verify`).
//! - Property-based validation that arbitrary syntactically valid BCP 47 language
//!   identifiers are accepted without error.
//! - Snapshot tests for `InvalidLocale` error messages produced when
//!   `--locale` or `--fallback-locale` receives a non-BCP 47 value.
//!
//! Related suites:
//! - `tests/harness_cli_layering_bdd.rs` — BDD scenarios for the same surface.
//! - `src/bin/spycatcher_harness.rs` (inline tests) — startup locale-plan and
//!   loader construction unit tests.

use rstest::rstest;

#[path = "support/cli_config_snapshot.rs"]
mod cli_config_snapshot;
#[path = "support/isolated_cli_process.rs"]
mod isolated_cli_process;

use cli_config_snapshot::load_config_snapshot as load_with_child;
use isolated_cli_process::ModeSnapshot;

const REPLAY_FILE_CONFIG: &str = "[cmds.replay]\ncassette_name = \"from_file\"\n";
const REPLAY_LOCALIZATION_FILE_CONFIG: &str = concat!(
    "[cmds.replay.localization]\n",
    "locale = \"en-GB\"\n",
    "fallback_locale = \"en-US\"\n",
);

#[derive(Debug)]
struct LayeringScenario<'a, Expected> {
    argv: &'a [&'a str],
    config_file: Option<&'a str>,
    env_vars: &'a [(&'a str, &'a str)],
    expected: Expected,
}

#[rstest]
#[case(
    LayeringScenario {
        argv: &["spycatcher-harness", "replay"],
        config_file: None,
        env_vars: &[],
        expected: "default",
    },
)]
#[case(
    LayeringScenario {
        argv: &["spycatcher-harness", "replay"],
        config_file: Some(REPLAY_FILE_CONFIG),
        env_vars: &[],
        expected: "from_file",
    },
)]
#[case(
    LayeringScenario {
        argv: &["spycatcher-harness", "replay"],
        config_file: Some(REPLAY_FILE_CONFIG),
        env_vars: &[("SPYCATCHER_HARNESS_CMDS_REPLAY_CASSETTE_NAME", "from_env")],
        expected: "from_env",
    },
)]
#[case(
    LayeringScenario {
        argv: &["spycatcher-harness", "replay", "--cassette-name", "from_cli"],
        config_file: Some(REPLAY_FILE_CONFIG),
        env_vars: &[("SPYCATCHER_HARNESS_CMDS_REPLAY_CASSETTE_NAME", "from_env")],
        expected: "from_cli",
    },
)]
fn replay_cassette_name_precedence(#[case] scenario: LayeringScenario<'static, &'static str>) {
    let loaded = load_with_child(scenario.argv, scenario.config_file, scenario.env_vars)
        .expect("config should load");
    assert_eq!(loaded.cassette_name, scenario.expected);
    assert_eq!(loaded.mode, ModeSnapshot::Replay);
}

#[rstest]
#[case("record", ModeSnapshot::Record)]
#[case("replay", ModeSnapshot::Replay)]
#[case("verify", ModeSnapshot::Verify)]
fn cassette_name_precedence_is_preserved_for_each_command(
    #[case] subcommand: &str,
    #[case] expected_mode: ModeSnapshot,
) {
    let config = format!("[cmds.{subcommand}]\ncassette_name = \"from_file\"\n");
    let env_name = format!(
        "SPYCATCHER_HARNESS_CMDS_{}_CASSETTE_NAME",
        subcommand.to_uppercase()
    );
    let loaded = load_with_child(
        &[
            "spycatcher-harness",
            subcommand,
            "--cassette-name",
            "from_cli",
        ],
        Some(&config),
        &[(env_name.as_str(), "from_env")],
    )
    .expect("config should load");

    assert_eq!(loaded.mode, expected_mode);
    assert_eq!(loaded.cassette_name, "from_cli");
}

#[rstest]
fn replay_localization_defaults_to_fallback_locale() {
    let loaded =
        load_with_child(&["spycatcher-harness", "replay"], None, &[]).expect("config should load");

    assert_eq!(loaded.localization.locale, None);
    assert_eq!(loaded.localization.fallback_locale, "en-US");
}

#[rstest]
#[case(
    LayeringScenario {
        argv: &["spycatcher-harness", "replay"],
        config_file: Some(REPLAY_LOCALIZATION_FILE_CONFIG),
        env_vars: &[],
        expected: (Some("en-GB"), "en-US"),
    },
)]
#[case(
    LayeringScenario {
        argv: &["spycatcher-harness", "replay"],
        config_file: Some(REPLAY_LOCALIZATION_FILE_CONFIG),
        env_vars: &[
            ("SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__LOCALE", "en-AU"),
            (
                "SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__FALLBACK_LOCALE",
                "en-US",
            ),
        ],
        expected: (Some("en-AU"), "en-US"),
    },
)]
#[case(
    LayeringScenario {
        argv: &[
            "spycatcher-harness",
            "replay",
            "--locale",
            "en-CA",
            "--fallback-locale",
            "en-US",
        ],
        config_file: Some(REPLAY_LOCALIZATION_FILE_CONFIG),
        env_vars: &[
            ("SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__LOCALE", "en-AU"),
            (
                "SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__FALLBACK_LOCALE",
                "en-US",
            ),
        ],
        expected: (Some("en-CA"), "en-US"),
    },
)]
fn replay_localization_precedence(
    #[case] scenario: LayeringScenario<'static, (Option<&'static str>, &'static str)>,
) {
    let loaded = load_with_child(scenario.argv, scenario.config_file, scenario.env_vars)
        .expect("config should load");

    let (expected_locale, expected_fallback_locale) = scenario.expected;
    assert_eq!(loaded.localization.locale.as_deref(), expected_locale);
    assert_eq!(
        loaded.localization.fallback_locale,
        expected_fallback_locale
    );
}

#[rstest]
fn env_nested_locale_wins_over_file_only_cli_alias() {
    let config = concat!(
        "[cmds.replay]\n",
        "locale = \"en-GB\"\n",
        "fallback_locale = \"en-GB\"\n",
    );
    let loaded = load_with_child(
        &["spycatcher-harness", "replay"],
        Some(config),
        &[
            (
                "SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__LOCALE",
                "en-AU",
            ),
            (
                "SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__FALLBACK_LOCALE",
                "en-US",
            ),
        ],
    )
    .expect("config should load");

    assert_eq!(loaded.localization.locale.as_deref(), Some("en-AU"));
    assert_eq!(loaded.localization.fallback_locale, "en-US");
}

#[rstest]
fn cli_locale_alias_wins_over_env_nested_locale() {
    let loaded = load_with_child(
        &[
            "spycatcher-harness",
            "replay",
            "--locale",
            "en-CA",
            "--fallback-locale",
            "en-US",
        ],
        None,
        &[
            (
                "SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__LOCALE",
                "en-AU",
            ),
            (
                "SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__FALLBACK_LOCALE",
                "en-GB",
            ),
        ],
    )
    .expect("config should load");

    assert_eq!(loaded.localization.locale.as_deref(), Some("en-CA"));
    assert_eq!(loaded.localization.fallback_locale, "en-US");
}

#[rstest]
#[case("record", ModeSnapshot::Record)]
#[case("replay", ModeSnapshot::Replay)]
#[case("verify", ModeSnapshot::Verify)]
fn localization_overrides_work_for_each_subcommand(
    #[case] subcommand: &str,
    #[case] expected_mode: ModeSnapshot,
) {
    let config = format!(
        "[cmds.{subcommand}.localization]\n\
         locale = \"en-GB\"\n\
         fallback_locale = \"en-US\"\n"
    );
    let argv = ["spycatcher-harness", subcommand];
    let env_locale = format!(
        "SPYCATCHER_HARNESS_CMDS_{}_LOCALIZATION__LOCALE",
        subcommand.to_uppercase()
    );
    let env_fallback_locale = format!(
        "SPYCATCHER_HARNESS_CMDS_{}_LOCALIZATION__FALLBACK_LOCALE",
        subcommand.to_uppercase()
    );
    let env_vars = [
        (env_locale.as_str(), "en-CA"),
        (env_fallback_locale.as_str(), "en-AU"),
    ];
    let loaded = load_with_child(&argv, Some(&config), &env_vars).expect("config should load");

    assert_eq!(loaded.mode, expected_mode);
    assert_eq!(loaded.localization.locale.as_deref(), Some("en-CA"));
    assert_eq!(loaded.localization.fallback_locale, "en-AU");
}

#[rstest]
#[case("record", ModeSnapshot::Record)]
#[case("replay", ModeSnapshot::Replay)]
#[case("verify", ModeSnapshot::Verify)]
fn explicit_locale_flags_override_nested_values_for_each_subcommand(
    #[case] subcommand: &str,
    #[case] expected_mode: ModeSnapshot,
) {
    let config = format!(
        "[cmds.{subcommand}.localization]\nlocale = \"en-GB\"\nfallback_locale = \"en-GB\"\n"
    );
    let env_locale = format!(
        "SPYCATCHER_HARNESS_CMDS_{}_LOCALIZATION__LOCALE",
        subcommand.to_uppercase()
    );
    let env_fallback_locale = format!(
        "SPYCATCHER_HARNESS_CMDS_{}_LOCALIZATION__FALLBACK_LOCALE",
        subcommand.to_uppercase()
    );
    let loaded = load_with_child(
        &[
            "spycatcher-harness",
            subcommand,
            "--locale",
            "en-CA",
            "--fallback-locale",
            "en-US",
        ],
        Some(&config),
        &[
            (env_locale.as_str(), "en-AU"),
            (env_fallback_locale.as_str(), "en-AU"),
        ],
    )
    .expect("config should load");

    assert_eq!(loaded.mode, expected_mode);
    assert_eq!(loaded.localization.locale.as_deref(), Some("en-CA"));
    assert_eq!(loaded.localization.fallback_locale, "en-US");
}

#[rstest]
fn invalid_cli_locale_fails_loading() {
    let error = load_with_child(
        &["spycatcher-harness", "replay", "--locale", "not_a_locale"],
        None,
        &[],
    )
    .expect_err("invalid locale should fail loading");
    let message = error.clone();

    insta::assert_snapshot!(
        message,
        @"invalid localization field `locale` value `not_a_locale`: Parser error: Invalid subtag"
    );
    assert!(
        message.contains("locale") && message.contains("not_a_locale"),
        "expected error about invalid locale, got: {message}",
    );
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

    let loaded = load_with_child(&["spycatcher-harness", "record"], Some(config), &[])
        .expect("record config should load");
    let upstream = loaded
        .upstream
        .expect("record config should contain upstream values");
    assert_eq!(loaded.cassette_name, "cassette_a");
    assert_eq!(loaded.mode, ModeSnapshot::Record);
    assert_eq!(upstream.base_url.as_str(), "https://example.invalid/api");
    assert_eq!(upstream.api_key_env, "TEST_API_KEY");
}

#[rstest]
fn verify_supports_cmds_namespace_overrides() {
    let config = "[cmds.verify]\ncassette_name = \"verify_cassette\"\n";
    let loaded = load_with_child(&["spycatcher-harness", "verify"], Some(config), &[])
        .expect("verify config should load");
    assert_eq!(loaded.cassette_name, "verify_cassette");
    assert_eq!(loaded.mode, ModeSnapshot::Verify);
}

#[rstest]
fn invalid_values_fail_loading() {
    let error = load_with_child(
        &["spycatcher-harness", "replay"],
        None,
        &[("SPYCATCHER_HARNESS_CMDS_REPLAY_LISTEN", "not-an-address")],
    )
    .expect_err("invalid listen should fail loading");
    let message = error.clone();
    assert!(
        (message.contains("invalid") && message.contains("address"))
            || (message.contains("socket") && message.contains("parse")),
        "expected error about invalid listen address, got: {message}",
    );
}
