//! Unit tests for CLI layered configuration loading.
//!
//! This suite exercises [`spycatcher_harness::cli::load_subcommand_config_from_iter`]
//! in isolation using `figment::Jail` to control the filesystem and environment.
//! It covers:
//!
//! - Cassette-name precedence (CLI > env > config file > default) for `replay`.
//! - Localisation-field precedence for `locale` and `fallback_locale` across all
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

use std::cell::RefCell;

use eyre::{Result, eyre};
use ortho_config::figment;
use proptest::prelude::*;
use rstest::rstest;

use spycatcher_harness::cli::load_subcommand_config_from_iter;
use spycatcher_harness::{HarnessConfig, config};

/// Loads a [`HarnessConfig`] under `figment::Jail` using `argv`, optional
/// `config_file` contents, and `env_vars`, returning the loaded config or
/// conversion error.
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
const REPLAY_LOCALIZATION_FILE_CONFIG: &str = concat!(
    "[cmds.replay.localization]\n",
    "locale = \"en-GB\"\n",
    "fallback_locale = \"en-US\"\n",
);

/// Generates lowercase two- or three-letter language subtags as a strategy.
fn language_subtag() -> impl Strategy<Value = String> {
    proptest::collection::vec(b'a'..=b'z', 2..=3)
        .prop_map(|bytes| bytes.into_iter().map(char::from).collect())
}

/// Generates uppercase two-letter region subtags as a strategy.
fn region_subtag() -> impl Strategy<Value = String> {
    proptest::collection::vec(b'A'..=b'Z', 2)
        .prop_map(|bytes| bytes.into_iter().map(char::from).collect())
}

/// Generates valid locale text such as `xx` or `xx-YY` as a strategy.
fn valid_locale_text() -> impl Strategy<Value = String> {
    (language_subtag(), proptest::option::of(region_subtag())).prop_map(|(language, region)| {
        match region {
            Some(region_text) => format!("{language}-{region_text}"),
            None => language,
        }
    })
}

proptest! {
    #[test]
    fn cli_locale_validation_accepts_generated_valid_language_identifiers(
        locale in valid_locale_text(),
        fallback_locale in valid_locale_text(),
    ) {
        let loaded = load_with_jail(
            &[
                "spycatcher-harness",
                "replay",
                "--locale",
                locale.as_str(),
                "--fallback-locale",
                fallback_locale.as_str(),
            ],
            None,
            &[],
        )
        .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(loaded.localization.locale.as_deref(), Some(locale.as_str()));
        prop_assert_eq!(loaded.localization.fallback_locale, fallback_locale);
    }
}

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
fn replay_localization_defaults_to_fallback_locale() {
    let loaded =
        load_with_jail(&["spycatcher-harness", "replay"], None, &[]).expect("config should load");

    assert_eq!(loaded.localization.locale, None);
    assert_eq!(loaded.localization.fallback_locale, "en-US");
}

#[rstest]
#[case(
    &["spycatcher-harness", "replay"],
    Some(REPLAY_LOCALIZATION_FILE_CONFIG),
    &[],
    (Some("en-GB"), "en-US"),
)]
#[case(
    &["spycatcher-harness", "replay"],
    Some(REPLAY_LOCALIZATION_FILE_CONFIG),
    &[
        ("SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__LOCALE", "en-AU"),
        (
            "SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__FALLBACK_LOCALE",
            "en-US"
        ),
    ],
    (Some("en-AU"), "en-US"),
)]
#[case(
    &[
        "spycatcher-harness",
        "replay",
        "--locale",
        "en-CA",
        "--fallback-locale",
        "en-US",
    ],
    Some(REPLAY_LOCALIZATION_FILE_CONFIG),
    &[
        ("SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__LOCALE", "en-AU"),
        (
            "SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__FALLBACK_LOCALE",
            "en-US"
        ),
    ],
    (Some("en-CA"), "en-US"),
)]
fn replay_localization_precedence(
    #[case] argv: &[&str],
    #[case] config_file: Option<&str>,
    #[case] env_vars: &[(&str, &str)],
    #[case] expected: (Option<&str>, &str),
) {
    let loaded = load_with_jail(argv, config_file, env_vars).expect("config should load");

    let (expected_locale, expected_fallback_locale) = expected;
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
    let loaded = load_with_jail(
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
    let loaded = load_with_jail(
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
#[case("record", config::Mode::Record)]
#[case("replay", config::Mode::Replay)]
#[case("verify", config::Mode::Verify)]
fn localization_overrides_work_for_each_subcommand(
    #[case] subcommand: &str,
    #[case] expected_mode: config::Mode,
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
    let env_vars = [(env_locale.as_str(), "en-CA")];
    let loaded = load_with_jail(&argv, Some(&config), &env_vars).expect("config should load");

    assert_eq!(loaded.mode, expected_mode);
    assert_eq!(loaded.localization.locale.as_deref(), Some("en-CA"));
    assert_eq!(loaded.localization.fallback_locale, "en-US");
}

#[rstest]
fn invalid_cli_locale_fails_loading() {
    let error = load_with_jail(
        &["spycatcher-harness", "replay", "--locale", "not_a_locale"],
        None,
        &[],
    )
    .expect_err("invalid locale should fail loading");
    let message = error.to_string();

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
