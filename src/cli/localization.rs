//! Localization helpers for the CLI configuration adapter.
//!
//! This module owns the small amount of policy needed while translating merged
//! command arguments into `HarnessConfig` localization fields. It also
//! re-exports the project-owned [`LocalizeCmd`] extension trait and
//! [`try_parse_localized_from_iter`] helper so callers can apply the same
//! `ortho_config::Localizer` pipeline to `clap` help, version, and parse-error
//! rendering before those merged fields are available.

use i18n_embed::unic_langid::LanguageIdentifier;

pub use super::localize_cmd::{LocalizeCmd, try_parse_localized_from_iter};
use super::{CliConfigError, RecordArgs, ReplayArgs, VerifyArgs};

/// Common localization and shared field overrides selected for a subcommand.
#[derive(Clone, Copy)]
pub(super) struct CommonOverrides<'a> {
    /// Merged listen address, if supplied.
    pub(super) listen: Option<std::net::SocketAddr>,
    /// Merged cassette directory, if supplied.
    pub(super) cassette_dir: Option<&'a str>,
    /// Merged cassette name, if supplied.
    pub(super) cassette_name: Option<&'a str>,
    /// Locale selected from the command line before merged localization.
    pub(super) locale: Option<&'a str>,
    /// Fallback locale selected from the command line before merged localization.
    pub(super) fallback_locale: Option<&'a str>,
}

/// Implement shared override extraction for each subcommand argument type.
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

/// Prefers an explicit CLI locale over its merged environment or file value.
/// For example, `select_localization_override("locale", Some("en-GB"),
/// Some("en-US"))` returns `Some("en-GB")`; with no CLI value it selects `en-US`.
pub(super) fn select_localization_override<'a>(
    field: &'static str,
    cli_value: Option<&'a str>,
    merged_value: Option<&'a str>,
) -> Option<&'a str> {
    if let Some(value) = cli_value {
        trace_selected_override(field, "cli", value);
        return Some(value);
    }
    let value = merged_value?;
    trace_selected_override(field, "merged_env_or_config", value);
    Some(value)
}

/// Records which input supplied a localization override.
/// For example, `trace_selected_override("locale", "cli", "en-GB")` emits a
/// debug event with those field, source and value labels.
fn trace_selected_override(field: &'static str, source: &'static str, value: &str) {
    tracing::debug!(field, source, value, "selected localization override");
}

/// Rejects invalid locale text before it changes the harness configuration.
/// For example, `validate_language_identifier("locale", "en-GB")` returns
/// `Ok(())`, while `"not_a_locale"` returns `InvalidLocale` with the field and
/// rejected value retained.
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
