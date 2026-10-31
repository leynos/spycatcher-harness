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
/// Couples an early locale variable with its invalid-value diagnostic.
/// For example, `PRIMARY_LOCALE_SETTING` selects the primary variable and the
/// warning that explains why selection proceeds to the fallback.
struct LocaleEnvSetting {
    /// Variable to read; `PRIMARY_LOCALE_SETTING.env_var` is `LOCALE_ENV`.
    env_var: &'static str,
    /// Warning for parse failures; the fallback setting explains the `en-US` default.
    invalid_value_warning: &'static str,
}

/// Primary lookup and warning; an invalid value permits the fallback lookup.
/// For example, `try_locale_from_env(&PRIMARY_LOCALE_SETTING)` returns `en-GB`
/// when the primary variable contains `"en-GB"`.
const PRIMARY_LOCALE_SETTING: LocaleEnvSetting = LocaleEnvSetting {
    env_var: LOCALE_ENV,
    invalid_value_warning: "invalid primary early CLI locale; trying fallback locale",
};

/// Fallback lookup and warning; an invalid value permits the `en-US` default.
/// For example, `try_locale_from_env(&FALLBACK_LOCALE_SETTING)` returns `None`
/// after warning when the fallback variable contains `"not_a_locale"`.
const FALLBACK_LOCALE_SETTING: LocaleEnvSetting = LocaleEnvSetting {
    env_var: FALLBACK_LOCALE_ENV,
    invalid_value_warning: "invalid fallback early CLI locale; using en-US",
};

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

/// Parses one supplied locale value, logging the same warning as environment lookup.
fn parse_locale_from_value(
    setting: &LocaleEnvSetting,
    candidate_value: Option<&str>,
) -> Option<LanguageIdentifier> {
    let candidate = candidate_value?;
    match parse_locale(candidate) {
        Ok(parsed) => Some(trace_selected_early_locale(setting.env_var, parsed)),
        Err(error) => {
            warn_invalid_early_locale(&error, setting.invalid_value_warning);
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
    early_locale_plan_with(
        || std::env::var(LOCALE_ENV).ok(),
        || std::env::var(FALLBACK_LOCALE_ENV).ok(),
    )
}

/// Selects a locale from lazy source values so unit tests need no env mutation.
fn early_locale_plan_with(
    primary: impl FnOnce() -> Option<String>,
    fallback: impl FnOnce() -> Option<String>,
) -> LanguageIdentifier {
    parse_locale_from_value(&PRIMARY_LOCALE_SETTING, primary().as_deref())
        .or_else(|| parse_locale_from_value(&FALLBACK_LOCALE_SETTING, fallback().as_deref()))
        .unwrap_or_else(|| langid!("en-US"))
}

/// Returns true when CLI localization should be bypassed for diagnostics.
///
/// Reads `SPYCATCHER_HARNESS_DISABLE_LOCALIZATION` and emits a warning when the
/// diagnostic switch is set to a truthy value.
#[must_use]
pub fn is_cli_localization_disabled() -> bool {
    let value = std::env::var(DISABLE_LOCALIZATION_ENV).ok();
    let is_disabled = is_localization_disabled_for_value(value.as_deref());
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

/// Interprets the optional diagnostic switch without reading process state.
fn is_localization_disabled_for_value(value: Option<&str>) -> bool {
    value.is_some_and(is_truthy_localization_switch)
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

#[cfg(test)]
mod tests {
    //! Isolated tests for early localization environment-value selection.

    use std::cell::Cell;

    use rstest::rstest;

    use super::{early_locale_plan_with, is_localization_disabled_for_value};

    /// Keeps the early locale plan on primary, then fallback, then default.
    ///
    /// An invalid value counts as absent, so the next source is consulted
    /// instead of failing startup.
    #[rstest]
    #[case::invalid_primary_uses_fallback("not_a_locale", "en-GB", "en-GB")]
    #[case::valid_primary_takes_precedence("en-GB", "fr-FR", "en-GB")]
    #[case::invalid_fallback_uses_default("not_a_locale", "not_a_locale", "en-US")]
    fn early_locale_plan_respects_value_precedence(
        #[case] primary: &str,
        #[case] fallback: &str,
        #[case] expected: &str,
    ) {
        let selected =
            early_locale_plan_with(|| Some(primary.to_owned()), || Some(fallback.to_owned()));

        assert_eq!(selected.to_string(), expected);
    }

    /// Proves the fallback source stays unread when the primary value parses.
    ///
    /// Deferring the read keeps locale selection lazy for callers that supply
    /// an expensive fallback lookup.
    #[test]
    fn valid_primary_does_not_read_fallback_source() {
        let fallback_read = Cell::new(false);
        let selected = early_locale_plan_with(
            || Some(String::from("en-GB")),
            || {
                fallback_read.set(true);
                Some(String::from("fr-FR"))
            },
        );

        assert_eq!(selected.to_string(), "en-GB");
        assert!(!fallback_read.get());
    }

    /// Accepts only recognized truthy spellings for the switch.
    ///
    /// Empty, zero, and `false` values keep localization enabled so a
    /// misconfigured value cannot silently change diagnostics.
    #[rstest]
    #[case(None, false)]
    #[case(Some(""), false)]
    #[case(Some("0"), false)]
    #[case(Some("false"), false)]
    #[case(Some("1"), true)]
    #[case(Some("true"), true)]
    #[case(Some("yes"), true)]
    #[case(Some("on"), true)]
    fn localization_switch_requires_truthy_value(
        #[case] value: Option<&str>,
        #[case] expected: bool,
    ) {
        assert_eq!(is_localization_disabled_for_value(value), expected);
    }
}
