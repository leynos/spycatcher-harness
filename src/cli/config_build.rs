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
    if let Some(listen_override) = overrides.listen {
        config.listen = listen_override.into();
    }
    if let Some(cassette_dir_override) = overrides.cassette_dir {
        config.cassette_dir = Utf8PathBuf::from(cassette_dir_override);
    }
    if let Some(cassette_name_override) = overrides.cassette_name {
        cassette_name_override.clone_into(&mut config.cassette_name);
    }
    if let Some(locale_override) = overrides.locale {
        tracing::debug!(
            field = "locale",
            value = locale_override,
            "applying locale override"
        );
        validate_language_identifier("locale", locale_override)?;
        config.localization.locale = Some(locale_override.to_owned());
    }
    if let Some(fallback_locale_override) = overrides.fallback_locale {
        tracing::debug!(
            field = "fallback_locale",
            value = fallback_locale_override,
            "applying locale override"
        );
        validate_language_identifier("fallback_locale", fallback_locale_override)?;
        fallback_locale_override.clone_into(&mut config.localization.fallback_locale);
    } else {
        tracing::debug!(
            field = "fallback_locale",
            value = config.localization.fallback_locale.as_str(),
            "using default fallback locale"
        );
    }
    validate_language_identifier("fallback_locale", &config.localization.fallback_locale)?;
    Ok(())
}
