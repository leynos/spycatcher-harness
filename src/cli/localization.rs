//! Localization helpers for the CLI configuration adapter.
//!
//! This module owns the small amount of policy needed while translating merged
//! command arguments into `HarnessConfig` localization fields.

use i18n_embed::unic_langid::LanguageIdentifier;

pub use super::localize_cmd::{LocalizeCmd, try_parse_localized_from_iter};
use super::{CliConfigError, RecordArgs, ReplayArgs, VerifyArgs};

/// Common localization and shared field overrides selected for a subcommand.
#[derive(Clone, Copy)]
pub(super) struct CommonOverrides<'a> {
    pub(super) listen: Option<std::net::SocketAddr>,
    pub(super) cassette_dir: Option<&'a str>,
    pub(super) cassette_name: Option<&'a str>,
    pub(super) locale: Option<&'a str>,
    pub(super) fallback_locale: Option<&'a str>,
}

macro_rules! impl_common_overrides {
    ($($T:ty),+ $(,)?) => {
        $(
            impl<'a> From<(&'a $T, &'a $T)> for CommonOverrides<'a> {
                fn from((cli_args, merged_args): (&'a $T, &'a $T)) -> Self {
                    Self {
                        listen: merged_args.listen,
                        cassette_dir: merged_args.cassette_dir.as_deref(),
                        cassette_name: merged_args.cassette_name.as_deref(),
                        locale: select_localization_override(
                            "locale",
                            cli_args.locale.as_deref(),
                            merged_args.localization.locale.as_deref(),
                        ),
                        fallback_locale: select_localization_override(
                            "fallback_locale",
                            cli_args.fallback_locale.as_deref(),
                            merged_args.localization.fallback_locale.as_deref(),
                        ),
                    }
                }
            }
        )+
    };
}

impl_common_overrides!(RecordArgs, ReplayArgs, VerifyArgs);

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
        source = "merged_env_or_config",
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
