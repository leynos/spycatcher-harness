//! Localization-field layering tests for `locale` and `fallback_locale`.
//!
//! The suite covers the documented CLI > environment > config-file > default
//! precedence for every subcommand, plus property-based validation that
//! arbitrary well-formed BCP 47 identifiers survive the load unchanged.

use proptest::prelude::*;
use rstest::rstest;

use super::cases::{LocalizationPrecedenceCase, Subcommand};
use super::support::{
    REPLAY_LOCALIZATION_FILE_CONFIG, load_with_sources, optional_locale_text, valid_locale_text,
};

proptest! {
    #[test]
    fn cli_locale_validation_accepts_generated_valid_language_identifiers(
        locale in valid_locale_text(),
        fallback_locale in valid_locale_text(),
    ) {
        let loaded = load_with_sources(
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

    #[test]
    fn replay_localization_layering_preserves_precedence_and_fallback(
        file_locale in optional_locale_text(),
        env_locale in optional_locale_text(),
        cli_locale in optional_locale_text(),
        file_fallback_locale in valid_locale_text(),
        env_fallback_locale in optional_locale_text(),
        cli_fallback_locale in optional_locale_text(),
    ) {
        let config_file = format!(
            "[cmds.replay.localization]\nlocale = \"{}\"\nfallback_locale = \"{}\"\n",
            file_locale.as_deref().unwrap_or("en-GB"),
            file_fallback_locale
        );
        let mut argv = vec!["spycatcher-harness", "replay"];
        if let Some(locale) = cli_locale.as_deref() {
            argv.extend(["--locale", locale]);
        }
        if let Some(fallback_locale) = cli_fallback_locale.as_deref() {
            argv.extend(["--fallback-locale", fallback_locale]);
        }

        let mut env_vars = Vec::new();
        if let Some(locale) = env_locale.as_deref() {
            env_vars.push(("SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__LOCALE", locale));
        }
        if let Some(fallback_locale) = env_fallback_locale.as_deref() {
            env_vars.push((
                "SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__FALLBACK_LOCALE",
                fallback_locale,
            ));
        }

        let loaded = load_with_sources(&argv, Some(config_file.as_str()), &env_vars)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let expected_locale = cli_locale
            .as_deref()
            .or(env_locale.as_deref())
            .or(file_locale.as_deref())
            .or(Some("en-GB"));
        let expected_fallback_locale = cli_fallback_locale
            .as_deref()
            .or(env_fallback_locale.as_deref())
            .unwrap_or(file_fallback_locale.as_str());

        prop_assert_eq!(loaded.localization.locale.as_deref(), expected_locale);
        prop_assert_eq!(
            loaded.localization.fallback_locale.as_str(),
            expected_fallback_locale
        );
        prop_assert!(!loaded.localization.fallback_locale.is_empty());
    }
}

#[rstest]
fn replay_localization_defaults_to_fallback_locale() {
    let loaded = load_with_sources(&["spycatcher-harness", "replay"], None, &[])
        .expect("config should load");

    assert_eq!(loaded.localization.locale, None);
    assert_eq!(loaded.localization.fallback_locale, "en-US");
}

#[rstest]
#[case(LocalizationPrecedenceCase {
    argv: &["spycatcher-harness", "replay"],
    config_file: Some(REPLAY_LOCALIZATION_FILE_CONFIG),
    env_vars: &[],
    expected_locale: Some("en-GB"),
    expected_fallback_locale: "en-US",
})]
#[case(LocalizationPrecedenceCase {
    argv: &["spycatcher-harness", "replay"],
    config_file: Some(REPLAY_LOCALIZATION_FILE_CONFIG),
    env_vars: &[
        ("SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__LOCALE", "en-AU"),
        (
            "SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__FALLBACK_LOCALE",
            "en-US"
        ),
    ],
    expected_locale: Some("en-AU"),
    expected_fallback_locale: "en-US",
})]
#[case(LocalizationPrecedenceCase {
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
            "en-US"
        ),
    ],
    expected_locale: Some("en-CA"),
    expected_fallback_locale: "en-US",
})]
fn replay_localization_precedence(#[case] case: LocalizationPrecedenceCase) {
    let loaded =
        load_with_sources(case.argv, case.config_file, case.env_vars).expect("config should load");

    assert_eq!(loaded.localization.locale.as_deref(), case.expected_locale);
    assert_eq!(
        loaded.localization.fallback_locale,
        case.expected_fallback_locale
    );
}

#[rstest]
fn env_nested_locale_wins_over_file_only_cli_alias() {
    let config = concat!(
        "[cmds.replay]\n",
        "locale = \"en-GB\"\n",
        "fallback_locale = \"en-GB\"\n",
    );
    let loaded = load_with_sources(
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
    let loaded = load_with_sources(
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
#[case(Subcommand::Record)]
#[case(Subcommand::Replay)]
#[case(Subcommand::Verify)]
fn localization_overrides_work_for_each_subcommand(#[case] subcommand: Subcommand) {
    let name = subcommand.as_str();
    let config = format!(
        "[cmds.{name}.localization]\n\
         locale = \"en-GB\"\n\
         fallback_locale = \"en-US\"\n"
    );
    let argv = ["spycatcher-harness", name];
    let env_locale = format!(
        "SPYCATCHER_HARNESS_CMDS_{}_LOCALIZATION__LOCALE",
        name.to_uppercase()
    );
    let env_vars = [(env_locale.as_str(), "en-CA")];
    let loaded = load_with_sources(&argv, Some(&config), &env_vars).expect("config should load");

    assert_eq!(loaded.mode, subcommand.mode());
    assert_eq!(loaded.localization.locale.as_deref(), Some("en-CA"));
    assert_eq!(loaded.localization.fallback_locale, "en-US");
}
