//! Layering error-visibility tests.
//!
//! These cases keep invalid values, malformed discovered files, and unreadable
//! discovered files visible to the user instead of silently falling back to
//! defaults.

use rstest::rstest;

use super::support::load_with_sources;

#[rstest]
fn invalid_cli_locale_fails_loading() {
    let error = load_with_sources(
        &["spycatcher-harness", "replay", "--locale", "not_a_locale"],
        None,
        &[],
    )
    .expect_err("invalid locale should fail loading");
    let message = error.to_string();

    insta::assert_snapshot!(
        message,
        @"invalid localization field `locale` value `not_a_locale`: Parser error: Invalid subtag"
    );
    assert!(
        message.contains("locale") && message.contains("not_a_locale"),
        "expected error about invalid locale, got: {message}",
    );
}

/// Rejects an invalid explicit fallback with its field, value, and parser error.
#[rstest]
fn invalid_cli_fallback_locale_fails_loading() {
    let error = load_with_sources(
        &[
            "spycatcher-harness",
            "replay",
            "--fallback-locale",
            "not_a_locale",
        ],
        None,
        &[],
    )
    .expect_err("invalid fallback locale should fail loading");

    assert_eq!(
        error.to_string(),
        concat!(
            "invalid localization field `fallback_locale` value `not_a_locale`: ",
            "Parser error: Invalid subtag"
        ),
        "expected the fallback field, rejected value, and established error format",
    );
}

#[rstest]
fn invalid_values_fail_loading() {
    let error = load_with_sources(
        &["spycatcher-harness", "replay"],
        None,
        &[("SPYCATCHER_HARNESS_CMDS_REPLAY_LISTEN", "not-an-address")],
    )
    .expect_err("invalid listen should fail loading");
    let message = error.to_string();
    assert!(
        (message.contains("invalid") && message.contains("address"))
            || (message.contains("socket") && message.contains("parse")),
        "expected error about invalid listen address, got: {message}",
    );
}

#[rstest]
fn malformed_discovered_file_remains_visible() {
    let error = load_with_sources(
        &["spycatcher-harness", "replay"],
        Some("[cmds.replay\n"),
        &[],
    )
    .expect_err("malformed discovered file should fail loading");
    let message = error.to_string();

    assert!(
        message.contains(".spycatcher_harness.toml"),
        "error should identify the discovered file: {message}",
    );
    assert!(
        message.to_ascii_lowercase().contains("parse")
            || message.to_ascii_lowercase().contains("toml"),
        "error should identify the parse failure: {message}",
    );
}

#[cfg(unix)]
#[rstest]
fn unreadable_discovered_file_remains_visible() -> eyre::Result<()> {
    let Some(load_result) =
        super::unreadable_file::load_with_unreadable_file(&["spycatcher-harness", "replay"])?
    else {
        return Ok(());
    };
    let Err(error) = load_result else {
        eyre::bail!("unreadable discovered file unexpectedly loaded");
    };
    let message = error.to_string();
    if !message.contains(".spycatcher_harness.toml") {
        eyre::bail!("error should identify the discovered file: {message}");
    }
    let message_lower = message.to_ascii_lowercase();
    if !message_lower.contains("permission denied") && !message_lower.contains("permissiondenied") {
        eyre::bail!("error should identify the read failure: {message}");
    }

    Ok(())
}
