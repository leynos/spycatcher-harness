//! `spycatcher-harness` CLI binary entry point.
//!
//! Delegates all startup and shutdown behaviour to the
//! [`spycatcher_harness`] library.

use eyre::WrapErr;
use i18n_embed::fluent::FluentLanguageLoader;
use i18n_embed::unic_langid::LanguageIdentifier;
use spycatcher_harness::cli::{CliConfigError, load_subcommand_config};
use spycatcher_harness::config::LocalizationConfig;
use spycatcher_harness::i18n::HarnessLocalizations;
use spycatcher_harness::start_harness;
use std::io::Write;
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
///
/// # Errors
///
/// Returns an error if configuration loading, startup, or shutdown fails.
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

fn load_config_or_display_output() -> eyre::Result<spycatcher_harness::HarnessConfig> {
    match load_subcommand_config() {
        Ok(config) => Ok(config),
        Err(CliConfigError::DisplayRequested { output }) => {
            write_display_output(&output).wrap_err("failed to write CLI output")?;
            std::process::exit(0);
        }
        Err(error) => Err(error).wrap_err("failed to load merged command config"),
    }
}

fn write_display_output(output: &str) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()
}

fn build_language_loader(
    localization: &LocalizationConfig,
) -> Result<FluentLanguageLoader, StartupLocalizationError> {
    let locale_plan = plan_locale_selection(localization)?;
    let fallback_locale = locale_plan
        .last()
        .cloned()
        .ok_or(StartupLocalizationError::EmptyLocalePlan)?;
    let loader = FluentLanguageLoader::new("spycatcher-harness", fallback_locale);
    i18n_embed::select(&loader, &HarnessLocalizations, &locale_plan)?;
    Ok(loader)
}

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

fn parse_language_identifier(
    field: &'static str,
    value: &str,
) -> Result<LanguageIdentifier, StartupLocalizationError> {
    value
        .parse()
        .map_err(|source| StartupLocalizationError::InvalidLocale {
            field,
            value: value.to_owned(),
            source,
        })
}

async fn run_harness(
    config: spycatcher_harness::HarnessConfig,
    _language_loader: &FluentLanguageLoader,
) -> eyre::Result<()> {
    let harness = start_harness(config)
        .await
        .wrap_err("failed to start harness")?;
    harness
        .shutdown()
        .await
        .wrap_err("failed to shut down harness")
}

#[cfg(test)]
mod tests {
    //! Tests for binary-owned startup localization.

    use rstest::rstest;
    use spycatcher_harness::HarnessError;
    use spycatcher_harness::i18n::localize_harness_error;

    use super::*;

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
    fn plan_locale_selection_rejects_invalid_locale_without_fallback() {
        let localization = LocalizationConfig {
            locale: Some(String::from("not_a_locale")),
            fallback_locale: String::from("en-US"),
        };

        let error = plan_locale_selection(&localization).expect_err("invalid locale should fail");

        assert!(error.to_string().contains("locale"));
        assert!(error.to_string().contains("not_a_locale"));
    }

    #[rstest]
    fn plan_locale_selection_rejects_invalid_fallback_locale() {
        let localization = LocalizationConfig {
            locale: Some(String::from("en-GB")),
            fallback_locale: String::from("not_a_locale"),
        };

        let error =
            plan_locale_selection(&localization).expect_err("invalid fallback locale should fail");

        assert!(error.to_string().contains("fallback_locale"));
        assert!(error.to_string().contains("not_a_locale"));
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
}
