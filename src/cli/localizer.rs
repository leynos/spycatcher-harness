//! CLI localizer construction and early locale selection.
//!
//! The binary needs a localizer before full CLI parsing because `--locale` is
//! itself parsed by `clap`. This module keeps that best-effort startup policy
//! at the application edge.

use std::sync::Arc;

use i18n_embed::unic_langid::{LanguageIdentifier, langid};
use ortho_config::{FluentLocalizer, Localizer, NoOpLocalizer};

/// Environment variable that forces stock `clap` output for diagnostics.
pub const DISABLE_LOCALIZATION_ENV: &str = "SPYCATCHER_HARNESS_DISABLE_LOCALIZATION";
/// Primary locale setting read before full command-line parsing.
const LOCALE_ENV: &str = "SPYCATCHER_HARNESS_LOCALE";
/// Fallback locale setting read when the primary setting is absent or invalid.
const FALLBACK_LOCALE_ENV: &str = "SPYCATCHER_HARNESS_FALLBACK_LOCALE";
/// Embedded Fluent messages used to localize early CLI output.
const CLI_FTL: &str = include_str!("../../i18n/en-US/spycatcher-harness.ftl");

/// Constructs a CLI localizer from the embedded Fluent catalogue.
///
/// # Examples
///
/// ```rust
/// use i18n_embed::unic_langid::langid;
/// use spycatcher_harness::cli::localizer::build_cli_localizer;
///
/// let localizer = build_cli_localizer(langid!("en-US"));
/// assert!(localizer.lookup("cli-about", None).is_some());
/// ```
#[must_use]
pub fn build_cli_localizer(locale: LanguageIdentifier) -> Arc<dyn Localizer> {
    build_cli_localizer_from_resources(locale, [CLI_FTL])
}

/// Constructs a CLI localizer from caller-supplied Fluent resources.
///
/// # Examples
///
/// ```rust
/// use i18n_embed::unic_langid::langid;
/// use spycatcher_harness::cli::localizer::build_cli_localizer_from_resources;
///
/// let localizer = build_cli_localizer_from_resources(langid!("en-US"), ["broken = {"]);
/// assert!(localizer.lookup("cli-about", None).is_none());
/// ```
pub fn build_cli_localizer_from_resources(
    locale: LanguageIdentifier,
    resources: impl IntoIterator<Item = &'static str>,
) -> Arc<dyn Localizer> {
    match FluentLocalizer::builder(locale)
        .with_consumer_resources(resources)
        .try_build()
    {
        Ok(localizer) => Arc::new(localizer),
        Err(error) => {
            tracing::warn!(?error, "falling back to NoOpLocalizer for CLI localization");
            Arc::new(NoOpLocalizer::new())
        }
    }
}

/// Selects the locale used before full CLI parsing.
///
/// Logs a warning and returns `en-US` if `candidate` is not a valid BCP 47
/// language identifier.
///
/// # Examples
///
/// ```rust
/// use spycatcher_harness::cli::localizer::parse_early_locale;
///
/// assert_eq!(parse_early_locale(Some("en-GB")).to_string(), "en-GB");
/// assert_eq!(parse_early_locale(Some("not_a_locale")).to_string(), "en-US");
/// ```
#[must_use]
pub fn parse_early_locale(candidate: Option<&str>) -> LanguageIdentifier {
    parse_locale(candidate.unwrap_or("en-US")).unwrap_or_else(|error| {
        tracing::warn!(?error, "invalid early CLI locale; using en-US");
        langid!("en-US")
    })
}

/// Reads and parses one early locale setting, warning when its value is invalid.
/// For example, an environment value of `"en-GB"` yields that identifier;
/// `"not_a_locale"` emits the supplied warning and returns `None`.
fn try_locale_from_env(var: &str, warn_msg: &str) -> Option<LanguageIdentifier> {
    let Ok(locale) = std::env::var(var) else {
        return None;
    };
    match parse_locale(&locale) {
        Ok(parsed) => Some(trace_selected_early_locale(var, parsed)),
        Err(error) => {
            warn_invalid_early_locale(&error, warn_msg);
            None
        }
    }
}

/// Records a valid early locale and returns it for selection.
/// For example, passing an `en-GB` identifier emits its selected-locale debug
/// event and returns the same identifier.
fn trace_selected_early_locale(var: &str, parsed: LanguageIdentifier) -> LanguageIdentifier {
    tracing::debug!(env_var = var, locale = %parsed, "selected early CLI locale");
    parsed
}

/// Reports an invalid environment locale before trying the next candidate.
/// For example, passing a parse error and `"trying fallback locale"` emits a
/// warning with that message and the error; locale selection remains the caller's job.
fn warn_invalid_early_locale(
    error: &i18n_embed::unic_langid::LanguageIdentifierError,
    warn_msg: &str,
) {
    tracing::warn!(?error, "{warn_msg}");
}

/// Returns the locale to use before full CLI parsing.
///
/// Reads `SPYCATCHER_HARNESS_LOCALE`, then
/// `SPYCATCHER_HARNESS_FALLBACK_LOCALE`, and falls back to `en-US`. Invalid
/// environment values are ignored after emitting a warning.
#[must_use]
pub fn early_locale_plan() -> LanguageIdentifier {
    try_locale_from_env(
        LOCALE_ENV,
        "invalid primary early CLI locale; trying fallback locale",
    )
    .or_else(|| {
        try_locale_from_env(
            FALLBACK_LOCALE_ENV,
            "invalid fallback early CLI locale; using en-US",
        )
    })
    .unwrap_or_else(|| langid!("en-US"))
}

/// Returns true when CLI localization should be bypassed for diagnostics.
///
/// Reads `SPYCATCHER_HARNESS_DISABLE_LOCALIZATION` and emits a warning when the
/// diagnostic switch is set to a truthy value.
#[must_use]
pub fn is_cli_localization_disabled() -> bool {
    let Ok(value) = std::env::var(DISABLE_LOCALIZATION_ENV) else {
        return false;
    };
    let is_disabled = is_truthy_localization_switch(&value);
    if is_disabled {
        warn_localization_disabled();
    }
    is_disabled
}

/// Recognizes the diagnostic switch's supported truthy spellings.
/// For example, `is_truthy_localization_switch(" YES ")` returns `true`, while
/// `is_truthy_localization_switch("false")` returns `false`.
fn is_truthy_localization_switch(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Reports that the diagnostic switch bypassed CLI localization.
/// For example, after a truthy switch is selected, calling this helper emits a
/// warning labelled with `SPYCATCHER_HARNESS_DISABLE_LOCALIZATION`.
fn warn_localization_disabled() {
    tracing::warn!(
        env_var = DISABLE_LOCALIZATION_ENV,
        "CLI localization disabled by diagnostic environment switch"
    );
}

/// Parses a BCP 47 language identifier without applying a fallback.
/// For example, `parse_locale("en-GB")` returns that identifier in `Ok`, while
/// `parse_locale("not_a_locale")` returns the parsing error.
fn parse_locale(
    value: &str,
) -> Result<LanguageIdentifier, i18n_embed::unic_langid::LanguageIdentifierError> {
    value.parse()
}
