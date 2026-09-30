//! Property checks for locale validation and layered CLI overrides.

use proptest::prelude::*;

#[path = "support/cli_config_snapshot.rs"]
mod cli_config_snapshot;
#[path = "support/isolated_cli_process.rs"]
mod isolated_cli_process;

use cli_config_snapshot::load_config_snapshot as load_with_child;

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

/// Generates titlecase four-letter script subtags as a strategy.
fn script_subtag() -> impl Strategy<Value = String> {
    (b'A'..=b'Z', proptest::collection::vec(b'a'..=b'z', 3))
        .prop_map(|(first, rest)| std::iter::once(first).chain(rest).map(char::from).collect())
}

/// Generates lowercase five- to eight-letter variant subtags as a strategy.
fn variant_subtag() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::from("valencia")),
        proptest::collection::vec(b'a'..=b'z', 5..=8)
            .prop_map(|bytes| bytes.into_iter().map(char::from).collect()),
    ]
}

/// Generates valid locale text such as `xx`, `xx-Xxxx`, `xx-YY`, or
/// `xx-valencia` as a strategy.
fn valid_locale_text() -> impl Strategy<Value = String> {
    (
        language_subtag(),
        proptest::option::of(script_subtag()),
        proptest::option::of(region_subtag()),
        proptest::option::of(variant_subtag()),
    )
        .prop_map(|(language, script, region, variant)| {
            let mut locale = language;
            for subtag in [script, region, variant].into_iter().flatten() {
                locale.push('-');
                locale.push_str(&subtag);
            }
            locale
        })
}

fn optional_locale_text() -> impl Strategy<Value = Option<String>> {
    proptest::option::of(valid_locale_text())
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 24,
        ..ProptestConfig::default()
    })]

    #[test]
    fn cli_locale_validation_accepts_generated_valid_language_identifiers(
        locale in valid_locale_text(),
        fallback_locale in valid_locale_text(),
    ) {
        let loaded = load_with_child(
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
        .map_err(|error| TestCaseError::fail(error.clone()))?;

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

        let loaded = load_with_child(&argv, Some(config_file.as_str()), &env_vars)
            .map_err(|error| TestCaseError::fail(error.clone()))?;
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
