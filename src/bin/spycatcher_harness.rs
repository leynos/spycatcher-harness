//! `spycatcher-harness` CLI binary entry point.
//!
//! The binary owns process-level concerns: layered configuration loading,
//! startup locale negotiation, construction of the Fluent language loader, and
//! rendering localized harness errors before handing semantic work to the
//! [`spycatcher_harness`] library.

use eyre::{WrapErr, eyre};
use i18n_embed::fluent::FluentLanguageLoader;
use spycatcher_harness::i18n::localize_harness_error;
use spycatcher_harness::start_harness;

#[path = "spycatcher_harness/cli_startup.rs"]
mod cli_startup;
#[path = "spycatcher_harness/process_output.rs"]
mod process_output;
#[path = "spycatcher_harness/startup_localization.rs"]
mod startup_localization;

use cli_startup::load_config_or_display_output;
use process_output::initialize_tracing;
use startup_localization::build_language_loader;

#[cfg(test)]
use spycatcher_harness::config::LocalizationConfig;
#[cfg(test)]
use startup_localization::plan_locale_selection;

/// Application entry point.
fn main() -> eyre::Result<()> {
    initialize_tracing()?;
    let config = load_config_or_display_output()?;
    let language_loader =
        build_language_loader(&config.localization).wrap_err("failed to prepare localization")?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .wrap_err("failed to build tokio runtime")?;
    rt.block_on(run_harness(config, &language_loader))
}

/// Starts the harness with `config`, awaits shutdown, and renders any
/// [`spycatcher_harness::HarnessError`] through `language_loader` before
/// wrapping it in an [`eyre::Report`].
///
/// # Errors
///
/// Returns an error if `start_harness` or `shutdown` fails.
async fn run_harness(
    config: spycatcher_harness::HarnessConfig,
    language_loader: &FluentLanguageLoader,
) -> eyre::Result<()> {
    let harness = start_harness(config).await.map_err(|error| {
        eyre!(
            "failed to start harness: {}",
            localize_harness_error(language_loader, &error)
        )
    })?;
    harness.shutdown().await.map_err(|error| {
        eyre!(
            "failed to shut down harness: {}",
            localize_harness_error(language_loader, &error)
        )
    })
}

#[cfg(test)]
mod tests {
    //! Tests for binary-owned startup localization.

    use proptest::prelude::*;
    use rstest::rstest;
    use spycatcher_harness::HarnessError;
    use spycatcher_harness::i18n::localize_harness_error;

    use super::*;

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
        fn plan_locale_selection_accepts_generated_valid_language_identifiers(
            locale in valid_locale_text(),
            fallback_locale in valid_locale_text(),
        ) {
            let localization = LocalizationConfig {
                locale: Some(locale.clone()),
                fallback_locale: fallback_locale.clone(),
            };

            let locale_plan = plan_locale_selection(&localization)?;
            let rendered: Vec<String> = locale_plan.iter().map(ToString::to_string).collect();

            prop_assert_eq!(rendered, vec![locale, fallback_locale]);
        }

        #[test]
        fn plan_locale_selection_accepts_generated_valid_fallback_identifiers(
            fallback_locale in valid_locale_text(),
        ) {
            let localization = LocalizationConfig {
                locale: None,
                fallback_locale: fallback_locale.clone(),
            };

            let locale_plan = plan_locale_selection(&localization)?;
            let rendered: Vec<String> = locale_plan.iter().map(ToString::to_string).collect();

            prop_assert_eq!(rendered, vec![fallback_locale]);
        }
    }

    #[rstest]
    fn plan_locale_selection_uses_fallback_when_locale_is_unset() {
        let localization = LocalizationConfig {
            locale: None,
            fallback_locale: String::from("en-US"),
        };

        let locale_plan = plan_locale_selection(&localization).expect("locale plan should parse");

        assert_eq!(locale_plan.len(), 1);
        assert_eq!(
            locale_plan.first().map(ToString::to_string),
            Some(String::from("en-US"))
        );
    }

    #[rstest]
    fn plan_locale_selection_preserves_requested_locale_order() {
        let localization = LocalizationConfig {
            locale: Some(String::from("en-GB")),
            fallback_locale: String::from("en-US"),
        };

        let locale_plan = plan_locale_selection(&localization).expect("locale plan should parse");

        let rendered: Vec<String> = locale_plan.iter().map(ToString::to_string).collect();
        assert_eq!(rendered, vec!["en-GB", "en-US"]);
    }

    #[rstest]
    #[case(
        Some(String::from("not_a_locale")),
        String::from("en-US"),
        "locale",
        "invalid localization field `locale` value `not_a_locale`: Parser error: Invalid subtag"
    )]
    #[case(
        Some(String::from("en-GB")),
        String::from("not_a_locale"),
        "fallback_locale",
        "invalid localization field `fallback_locale` value `not_a_locale`: Parser error: Invalid subtag"
    )]
    fn plan_locale_selection_rejects_invalid_field(
        #[case] locale: Option<String>,
        #[case] fallback_locale: String,
        #[case] expected_field: &str,
        #[case] expected_message: &str,
    ) {
        let localization = LocalizationConfig {
            locale,
            fallback_locale,
        };
        let error = plan_locale_selection(&localization).expect_err("invalid locale should fail");
        let message = error.to_string();
        assert_eq!(message, expected_message);
        assert!(
            message.contains(expected_field),
            "expected field name `{expected_field}` in error: {message}"
        );
        assert!(
            message.contains("not_a_locale"),
            "expected invalid value `not_a_locale` in error: {message}"
        );
    }

    #[rstest]
    fn build_language_loader_falls_back_to_embedded_english_catalogue() {
        let localization = LocalizationConfig {
            locale: Some(String::from("en-GB")),
            fallback_locale: String::from("en-US"),
        };

        let loader = build_language_loader(&localization).expect("loader should build");
        let rendered = localize_harness_error(
            &loader,
            &HarnessError::InvalidConfig {
                message: String::from("missing upstream"),
            },
        );

        assert_eq!(
            rendered,
            "invalid configuration: \u{2068}missing upstream\u{2069}"
        );
    }

    #[tokio::test]
    async fn run_harness_renders_startup_errors_with_language_loader() {
        let localization = LocalizationConfig::default();
        let loader = build_language_loader(&localization).expect("loader should build");
        let mut config = spycatcher_harness::HarnessConfig::default();
        config.cassette_name.clear();

        let error = run_harness(config, &loader)
            .await
            .expect_err("invalid config should fail startup");

        let message = error.to_string();
        insta::assert_snapshot!(
            message,
            @"failed to start harness: invalid configuration: \u{2068}cassette name must not be empty\u{2069}"
        );
    }
}
