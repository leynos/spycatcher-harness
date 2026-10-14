//! Configured startup locale planning and embedded Fluent catalogue loading.
//!
//! Configured fallback validation precedes primary validation, while the resulting
//! selection plan tries the primary locale before its fallback.

use i18n_embed::fluent::FluentLanguageLoader;
use i18n_embed::unic_langid::LanguageIdentifier;
use spycatcher_harness::config::LocalizationConfig;
use spycatcher_harness::i18n::HarnessLocalizations;
use thiserror::Error;

/// Errors returned while preparing startup localization.
#[derive(Debug, Error)]
pub(super) enum StartupLocalizationError {
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

/// Constructs a language loader from the configured ordered locale plan.
/// For example, primary `"en-GB"` with fallback `"en-US"` selects in that order
/// and uses `"en-US"` as the Fluent fallback.
///
/// # Errors
/// Returns locale parsing, empty-plan, or embedded catalogue loading failures.
pub(super) fn build_language_loader(
    localization: &LocalizationConfig,
) -> Result<FluentLanguageLoader, StartupLocalizationError> {
    log_startup_locale_selection(localization);
    let locale_plan = plan_locale_selection(localization)?;
    log_selected_locale_plan(&locale_plan);
    load_selected_catalogue(&locale_plan)
}

/// Logs configured locale values before validation or catalogue selection.
/// For example, an unset primary is recorded alongside the configured fallback.
fn log_startup_locale_selection(localization: &LocalizationConfig) {
    tracing::debug!(
        target: "spycatcher_harness",
        locale = localization.locale.as_deref(),
        fallback_locale = %localization.fallback_locale,
        "planning startup locale selection"
    );
}

/// Records the validated ordered plan before building the Fluent loader.
/// For example, `[en-GB, en-US]` retains the requested-before-fallback order.
fn log_selected_locale_plan(locale_plan: &[LanguageIdentifier]) {
    tracing::debug!(target: "spycatcher_harness", ?locale_plan, "selected startup locale plan");
}

/// Loads embedded localizations with the plan's final entry as Fluent fallback.
/// For example, `[en-GB, en-US]` uses `en-US` as fallback and selects both in order.
///
/// # Errors
/// Returns `EmptyLocalePlan` for an empty plan or an embedded catalogue failure.
fn load_selected_catalogue(
    locale_plan: &[LanguageIdentifier],
) -> Result<FluentLanguageLoader, StartupLocalizationError> {
    let fallback_locale = locale_plan
        .last()
        .cloned()
        .ok_or(StartupLocalizationError::EmptyLocalePlan)?;
    let loader = FluentLanguageLoader::new("spycatcher-harness", fallback_locale);
    i18n_embed::select(&loader, &HarnessLocalizations, locale_plan)?;
    tracing::info!(target: "spycatcher_harness", "constructed startup language loader");
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
pub(super) fn plan_locale_selection(
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
        tracing::warn!(target: "spycatcher_harness", field, value, %source, "invalid startup locale value");
        StartupLocalizationError::InvalidLocale {
            field,
            value: value.to_owned(),
            source,
        }
    })
}
