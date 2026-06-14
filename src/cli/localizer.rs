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
const LOCALE_ENV: &str = "SPYCATCHER_HARNESS_LOCALE";
const FALLBACK_LOCALE_ENV: &str = "SPYCATCHER_HARNESS_FALLBACK_LOCALE";
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

/// Returns the locale to use before full CLI parsing.
#[must_use]
pub fn early_locale_plan() -> LanguageIdentifier {
    if let Ok(locale) = std::env::var(LOCALE_ENV) {
        match parse_locale(&locale) {
            Ok(parsed) => return parsed,
            Err(error) => tracing::warn!(
                ?error,
                "invalid primary early CLI locale; trying fallback locale"
            ),
        }
    }

    if let Ok(locale) = std::env::var(FALLBACK_LOCALE_ENV) {
        match parse_locale(&locale) {
            Ok(parsed) => return parsed,
            Err(error) => tracing::warn!(?error, "invalid fallback early CLI locale; using en-US"),
        }
    }

    langid!("en-US")
}

/// Returns true when CLI localization should be bypassed for diagnostics.
#[must_use]
pub fn is_cli_localization_disabled() -> bool {
    std::env::var(DISABLE_LOCALIZATION_ENV).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes"
        )
    })
}

fn parse_locale(
    value: &str,
) -> Result<LanguageIdentifier, i18n_embed::unic_langid::LanguageIdentifierError> {
    value.parse()
}
