//! Build harness configuration from layered CLI arguments.

use camino::Utf8PathBuf;

use super::localization::{CommonOverrides, validate_language_identifier};
use super::{CliConfigError, RecordArgs, ReplayArgs, VerifyArgs};
use crate::{HarnessConfig, config};

/// Builds [`HarnessConfig`] from common overrides, `mode`, and optional upstream.
fn build_config(
    overrides: CommonOverrides<'_>,
    mode: config::Mode,
    upstream: Option<config::UpstreamConfig>,
) -> Result<HarnessConfig, CliConfigError> {
    let mut config = HarnessConfig::default();
    apply_overrides(&mut config, overrides)?;
    config.mode = mode;
    config.upstream = upstream;
    tracing::debug!(
        mode = ?config.mode,
        locale = config.localization.locale.as_deref(),
        fallback_locale = config.localization.fallback_locale.as_str(),
        "built harness configuration from layered CLI input"
    );
    Ok(config)
}

/// Converts record CLI and merged arguments into record-mode [`HarnessConfig`].
pub(super) fn to_record_config(
    cli_args: &RecordArgs,
    merged_args: &RecordArgs,
) -> Result<HarnessConfig, CliConfigError> {
    build_config(
        (cli_args, merged_args).into(),
        config::Mode::Record,
        merged_args.upstream.clone().map(Into::into),
    )
}

/// Converts replay CLI and merged arguments into replay-mode [`HarnessConfig`].
pub(super) fn to_replay_config(
    cli_args: &ReplayArgs,
    merged_args: &ReplayArgs,
) -> Result<HarnessConfig, CliConfigError> {
    build_config((cli_args, merged_args).into(), config::Mode::Replay, None)
}

/// Converts verify CLI and merged arguments into verify-mode [`HarnessConfig`].
pub(super) fn to_verify_config(
    cli_args: &VerifyArgs,
    merged_args: &VerifyArgs,
) -> Result<HarnessConfig, CliConfigError> {
    build_config((cli_args, merged_args).into(), config::Mode::Verify, None)
}

/// Applies `overrides` to `config`, validating locale values before mutation.
fn apply_overrides(
    config: &mut HarnessConfig,
    overrides: CommonOverrides<'_>,
) -> Result<(), CliConfigError> {
    apply_shared_overrides(config, overrides);
    apply_locale_override(config, overrides.locale)?;
    apply_fallback_locale_override(config, overrides.fallback_locale)
}

/// Applies fields shared by all command modes.
/// For example, a cassette-name override of `Some("sample")` replaces the name
/// with `"sample"`; an absent listen override leaves the address unchanged.
fn apply_shared_overrides(config: &mut HarnessConfig, overrides: CommonOverrides<'_>) {
    if let Some(listen_override) = overrides.listen {
        config.listen = listen_override.into();
    }
    if let Some(cassette_dir_override) = overrides.cassette_dir {
        config.cassette_dir = Utf8PathBuf::from(cassette_dir_override);
    }
    if let Some(cassette_name_override) = overrides.cassette_name {
        cassette_name_override.clone_into(&mut config.cassette_name);
    }
}

/// Applies and validates a requested locale before mutating the configuration.
/// For example, `apply_locale_override(config, Some("en-GB"))` sets the locale
/// to `Some("en-GB")` and returns `Ok(())`; invalid text returns `InvalidLocale`
/// without replacing the existing locale.
fn apply_locale_override(
    config: &mut HarnessConfig,
    locale_override: Option<&str>,
) -> Result<(), CliConfigError> {
    if let Some(locale_value) = locale_override {
        tracing::debug!(
            field = "locale",
            value = locale_value,
            "applying locale override"
        );
        validate_language_identifier("locale", locale_value)?;
        config.localization.locale = Some(locale_value.to_owned());
    }
    Ok(())
}

/// Applies the fallback locale or retains the default, then validates it.
/// For example, with a valid configuration, an override of `Some("en-GB")` sets
/// the fallback to `"en-GB"` and returns `Ok(())`; `None` validates and retains
/// the existing fallback.
fn apply_fallback_locale_override(
    config: &mut HarnessConfig,
    fallback_locale_override: Option<&str>,
) -> Result<(), CliConfigError> {
    if let Some(fallback_locale_value) = fallback_locale_override {
        apply_supplied_fallback_locale(config, fallback_locale_value)?;
    } else {
        log_default_fallback_locale(&config.localization.fallback_locale);
    }
    validate_language_identifier("fallback_locale", &config.localization.fallback_locale)?;
    Ok(())
}

/// Logs and validates a supplied fallback before replacing the configured value.
/// For example, `"en-GB"` replaces the fallback after validation; `"not_a_locale"`
/// returns `InvalidLocale` while retaining the existing fallback.
fn apply_supplied_fallback_locale(
    config: &mut HarnessConfig,
    fallback_locale_value: &str,
) -> Result<(), CliConfigError> {
    tracing::debug!(
        field = "fallback_locale",
        value = fallback_locale_value,
        "applying locale override"
    );
    validate_language_identifier("fallback_locale", fallback_locale_value)?;
    fallback_locale_value.clone_into(&mut config.localization.fallback_locale);
    Ok(())
}

/// Logs retention of the configured fallback without changing or validating it.
/// For example, `log_default_fallback_locale("en-US")` records `"en-US"` as the
/// default fallback; the caller still validates that value afterwards.
fn log_default_fallback_locale(fallback_locale_value: &str) {
    tracing::debug!(
        field = "fallback_locale",
        value = fallback_locale_value,
        "using default fallback locale"
    );
}
