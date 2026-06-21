//! `spycatcher-harness` CLI binary entry point.
//!
//! The binary owns process-level concerns: layered configuration loading,
//! startup locale negotiation, construction of the Fluent language loader, and
//! rendering localized harness errors before handing semantic work to the
//! [`spycatcher_harness`] library.

use eyre::{WrapErr, eyre};
use i18n_embed::fluent::FluentLanguageLoader;
use i18n_embed::unic_langid::LanguageIdentifier;
use ortho_config::{Localizer, NoOpLocalizer};
use spycatcher_harness::cli::localizer::{
    build_cli_localizer, early_locale_plan, is_cli_localization_disabled,
};
use spycatcher_harness::cli::{CliConfigError, load_subcommand_config_with_localizer};
use spycatcher_harness::config::LocalizationConfig;
use spycatcher_harness::i18n::{HarnessLocalizations, localize_harness_error};
use spycatcher_harness::start_harness;
use std::{io::Write, sync::Arc};
use thiserror::Error;

/// Errors returned while preparing startup localization.
#[derive(Debug, Error)]
enum StartupLocalizationError {
    /// A configured locale could not be parsed.
    #[error("invalid localization field `{field}` value `{value}`: {source}")]
    InvalidLocale {
        /// Field containing the invalid value.
        field: &'static str,
        /// Invalid language identifier text.
        value: String,
        /// Parser failure from `unic-langid`.
        source: i18n_embed::unic_langid::LanguageIdentifierError,
    },
    /// Embedded localization assets could not be loaded.
    #[error("failed to load embedded localization assets: {0}")]
    Load(#[from] i18n_embed::I18nEmbedError),
    /// Locale planning returned no fallback language.
    #[error("locale selection plan did not contain a fallback locale")]
    EmptyLocalePlan,
}

/// Application entry point.
fn main() -> eyre::Result<()> {
    let config = load_config_or_display_output()?;
    let language_loader =
        build_language_loader(&config.localization).wrap_err("failed to prepare localization")?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .wrap_err("failed to build tokio runtime")?;
    rt.block_on(run_harness(config, &language_loader))
}

/// Loads merged subcommand configuration, or writes help/version output to
/// stdout and exits cleanly if clap requests it.
fn load_config_or_display_output() -> eyre::Result<spycatcher_harness::HarnessConfig> {
    let (localizer_kind, localizer) = select_cli_localizer();
    tracing::debug!(localizer_kind, "selected CLI localizer");
    let span = tracing::debug_span!("parse_cli", localizer_kind);
    let _guard = span.enter();
    handle_cli_config_result(
        load_subcommand_config_with_localizer(localizer.as_ref()),
        localizer_kind,
    )
}

fn handle_cli_config_result(
    result: Result<spycatcher_harness::HarnessConfig, CliConfigError>,
    localizer_kind: &str,
) -> eyre::Result<spycatcher_harness::HarnessConfig> {
    match result {
        Ok(config) => {
            tracing::debug!(localizer_kind, "parsed CLI configuration");
            Ok(config)
        }
        Err(CliConfigError::DisplayRequested { output }) => {
            tracing::debug!(localizer_kind, "parsed CLI display request");
            write_display_output(&output).wrap_err("failed to write CLI output")?;
            std::process::exit(0);
        }
        Err(CliConfigError::CliParse(error)) => {
            tracing::debug!(localizer_kind, error_kind = ?error.kind(), "parsed CLI error");
            write_error_output(&error.to_string()).wrap_err("failed to write CLI error output")?;
            std::process::exit(2);
        }
        Err(error) => Err(error).wrap_err("failed to load merged command config"),
    }
}

fn write_display_output(output: &str) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()
}

fn write_error_output(output: &str) -> std::io::Result<()> {
    let mut stderr = std::io::stderr().lock();
    stderr.write_all(output.as_bytes())?;
    stderr.flush()
}

fn select_cli_localizer() -> (&'static str, Arc<dyn Localizer>) {
    if is_cli_localization_disabled() {
        ("noop", Arc::new(NoOpLocalizer::new()))
    } else {
        ("fluent", build_cli_localizer(early_locale_plan()))
    }
}

/// Constructs a [`FluentLanguageLoader`] from layered localisation
/// configuration.
///
/// Computes a locale plan (requested locale then fallback, or fallback-only
/// when `locale` is unset), selects embedded harness localisations via
/// [`i18n_embed::select`], and uses the last entry in the plan as the Fluent
/// fallback locale.
///
/// # Errors
///
/// Returns [`StartupLocalizationError`] if any locale value fails BCP 47
/// parsing or if the embedded catalogue cannot be loaded.
fn build_language_loader(
    localization: &LocalizationConfig,
) -> Result<FluentLanguageLoader, StartupLocalizationError> {
    tracing::debug!(
        locale = localization.locale.as_deref(),
        fallback_locale = %localization.fallback_locale,
        "planning startup locale selection"
    );
    let locale_plan = plan_locale_selection(localization)?;
    tracing::debug!(?locale_plan, "selected startup locale plan");
    let fallback_locale = locale_plan
        .last()
        .cloned()
        .ok_or(StartupLocalizationError::EmptyLocalePlan)?;
    let loader = FluentLanguageLoader::new("spycatcher-harness", fallback_locale);
    i18n_embed::select(&loader, &HarnessLocalizations, &locale_plan)?;
    tracing::info!("constructed startup language loader");
    Ok(loader)
}

/// Produces an ordered list of [`LanguageIdentifier`]s to attempt during
/// locale selection.
///
/// When `localization.locale` is set the plan is
/// `[requested_locale, fallback_locale]`; otherwise it is `[fallback_locale]`.
///
/// # Errors
///
/// Returns [`StartupLocalizationError::InvalidLocale`] if either identifier
/// fails BCP 47 parsing.
fn plan_locale_selection(
    localization: &LocalizationConfig,
) -> Result<Vec<LanguageIdentifier>, StartupLocalizationError> {
    let fallback_locale =
        parse_language_identifier("fallback_locale", &localization.fallback_locale)?;
    let Some(locale) = localization.locale.as_deref() else {
        return Ok(vec![fallback_locale]);
    };
    let requested_locale = parse_language_identifier("locale", locale)?;
    Ok(vec![requested_locale, fallback_locale])
}

/// Parses `value` as a BCP 47 [`LanguageIdentifier`].
///
/// Emits a `tracing::warn!` and returns
/// [`StartupLocalizationError::InvalidLocale`] if parsing fails, including
/// `field` and `value` in both the log event and the error.
///
/// # Errors
///
/// Returns [`StartupLocalizationError::InvalidLocale`] on parse failure.
fn parse_language_identifier(
    field: &'static str,
    value: &str,
) -> Result<LanguageIdentifier, StartupLocalizationError> {
    value.parse().map_err(|source| {
        tracing::warn!(field, value, %source, "invalid startup locale value");
        StartupLocalizationError::InvalidLocale {
            field,
            value: value.to_owned(),
            source,
        }
    })
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
