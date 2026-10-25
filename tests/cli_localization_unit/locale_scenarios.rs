//! Property and environment-probe tests for early locale handling.

use i18n_embed::unic_langid::LanguageIdentifier;
use proptest::prelude::*;
use rstest::rstest;
use spycatcher_harness::cli::localizer::{DISABLE_LOCALIZATION_ENV, parse_early_locale};

use super::isolated_cli_process::{self, ProbeOutcome, ProbeRequest};
use super::{CLI_FTL, LocalizationArgs, build_cli_localizer_from_resources};

#[derive(Clone, Copy, Debug)]
struct LocaleParseCase {
    candidate: Option<&'static str>,
    expected: &'static str,
}

#[derive(Clone, Copy, Debug)]
struct LocalePlanCase {
    primary: &'static str,
    fallback: &'static str,
    expected: &'static str,
}

#[derive(Clone, Copy, Debug)]
struct LocalizationSwitchCase {
    env_value: Option<&'static str>,
    expected: bool,
}

fn language_subtag() -> impl Strategy<Value = String> {
    proptest::collection::vec(b'a'..=b'z', 2..=3)
        .prop_map(|bytes| bytes.into_iter().map(char::from).collect())
}

fn region_subtag() -> impl Strategy<Value = String> {
    proptest::collection::vec(b'A'..=b'Z', 2)
        .prop_map(|bytes| bytes.into_iter().map(char::from).collect())
}

fn script_subtag() -> impl Strategy<Value = String> {
    (b'A'..=b'Z', proptest::collection::vec(b'a'..=b'z', 3))
        .prop_map(|(first, rest)| std::iter::once(first).chain(rest).map(char::from).collect())
}

fn variant_subtag() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::from("valencia")),
        proptest::collection::vec(b'a'..=b'z', 5..=8)
            .prop_map(|bytes| bytes.into_iter().map(char::from).collect()),
    ]
}

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

proptest! {
    #[test]
    fn parse_early_locale_accepts_generated_language_identifiers(locale in valid_locale_text()) {
        let parsed = locale
            .parse::<LanguageIdentifier>()
            .map_err(|error| TestCaseError::fail(error.to_string()))?;

        prop_assert_eq!(parse_early_locale(Some(locale.as_str())), parsed);
    }

    #[test]
    fn generated_language_identifiers_resolve_to_a_cli_localizer(
        locale in valid_locale_text(),
        use_broken_resource in any::<bool>(),
    ) {
        let parsed = locale
            .parse::<LanguageIdentifier>()
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let resources = if use_broken_resource {
            vec!["cli-about = {"]
        } else {
            vec![CLI_FTL]
        };
        let localizer = build_cli_localizer_from_resources(parsed, resources);
        let mut args = LocalizationArgs::new();
        args.insert("binary", "spycatcher-harness".into());
        args.insert("version", "0.1.0".into());

        let rendered = localizer.lookup("cli-about", Some(&args));

        prop_assert!(rendered.is_none_or(|text| !text.trim().is_empty()));
    }
}

#[rstest]
#[case::valid_primary(LocaleParseCase {
    candidate: Some("en-GB"),
    expected: "en-GB",
})]
#[case::invalid_primary(LocaleParseCase {
    candidate: Some("not_a_locale"),
    expected: "en-US",
})]
#[case::missing(LocaleParseCase {
    candidate: None,
    expected: "en-US",
})]
fn parse_early_locale_is_deterministic(#[case] scenario: LocaleParseCase) {
    assert_eq!(
        parse_early_locale(scenario.candidate).to_string(),
        scenario.expected
    );
}

#[rstest]
#[case::invalid_primary_uses_fallback(LocalePlanCase {
    primary: "not_a_locale",
    fallback: "en-GB",
    expected: "en-GB",
})]
#[case::valid_primary_takes_precedence(LocalePlanCase {
    primary: "en-GB",
    fallback: "fr-FR",
    expected: "en-GB",
})]
#[case::invalid_fallback_uses_default(LocalePlanCase {
    primary: "not_a_locale",
    fallback: "not_a_locale",
    expected: "en-US",
})]
fn early_locale_plan_respects_environment_precedence(#[case] scenario: LocalePlanCase) {
    let env_vars = [
        ("SPYCATCHER_HARNESS_LOCALE", scenario.primary),
        ("SPYCATCHER_HARNESS_FALLBACK_LOCALE", scenario.fallback),
    ];
    let probe = isolated_cli_process::run_probe(&ProbeRequest::EarlyLocalePlan, None, &env_vars)
        .expect("early locale probe should run");

    assert_eq!(
        probe.outcome,
        ProbeOutcome::LocalePlan(String::from(scenario.expected))
    );
}

#[rstest]
#[case::absent(LocalizationSwitchCase {
    env_value: None,
    expected: false,
})]
#[case::empty(LocalizationSwitchCase {
    env_value: Some(""),
    expected: false,
})]
#[case::zero(LocalizationSwitchCase {
    env_value: Some("0"),
    expected: false,
})]
#[case::false_value(LocalizationSwitchCase {
    env_value: Some("false"),
    expected: false,
})]
#[case::one(LocalizationSwitchCase {
    env_value: Some("1"),
    expected: true,
})]
#[case::true_value(LocalizationSwitchCase {
    env_value: Some("true"),
    expected: true,
})]
#[case::yes(LocalizationSwitchCase {
    env_value: Some("yes"),
    expected: true,
})]
#[case::on(LocalizationSwitchCase {
    env_value: Some("on"),
    expected: true,
})]
fn cli_localization_disable_switch_requires_truthy_value(#[case] scenario: LocalizationSwitchCase) {
    let env_vars = scenario
        .env_value
        .map_or_else(Vec::new, |value| vec![(DISABLE_LOCALIZATION_ENV, value)]);
    let probe =
        isolated_cli_process::run_probe(&ProbeRequest::LocalizationDisabled, None, &env_vars)
            .expect("localization-disable probe should run");

    assert_eq!(
        probe.outcome,
        ProbeOutcome::LocalizationDisabled(scenario.expected)
    );
}
