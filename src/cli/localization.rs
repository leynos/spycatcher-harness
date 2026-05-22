//! Localization helpers for the CLI configuration adapter.
//!
//! This module owns the small amount of policy needed while translating merged
//! command arguments into `HarnessConfig` localization fields.

use i18n_embed::unic_langid::LanguageIdentifier;

use super::CliConfigError;

pub(super) fn select_localization_override<'a>(
    field: &'static str,
    cli_value: Option<&'a str>,
    merged_value: Option<&'a str>,
) -> Option<&'a str> {
    if let Some(value) = cli_value {
        tracing::debug!(
            field,
            source = "cli",
            value,
            "selected localization override"
        );
        return Some(value);
    }
    let value = merged_value?;
    tracing::debug!(
        field,
        source = "merged",
        value,
        "selected localization override"
    );
    Some(value)
}

pub(super) fn validate_language_identifier(
    field: &'static str,
    value: &str,
) -> Result<(), CliConfigError> {
    value
        .parse::<LanguageIdentifier>()
        .map(|_| ())
        .map_err(|source| {
            tracing::warn!(field, value, %source, "invalid CLI localization value");
            CliConfigError::InvalidLocale {
                field,
                value: value.to_owned(),
                source,
            }
        })
}
