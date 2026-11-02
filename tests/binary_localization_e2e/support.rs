//! Child-process harness for the binary diagnostic-localization tests.
//!
//! Each helper runs `--help` in a fresh temporary directory, so the developer's
//! real configuration and locale cannot influence the assertion. Captured
//! streams stay separate so a diagnostic can never be mistaken for help text.

use std::process::{Command, ExitStatus};

use eyre::WrapErr as _;
use spycatcher_harness::cli::localizer::DISABLE_LOCALIZATION_ENV;

use super::coverage_profile;

/// Captured streams and exit status from one isolated help subprocess.
pub(crate) struct DiagnosticHelpOutput {
    /// Exit status used to prove diagnostics do not turn help into an error.
    pub(crate) status: ExitStatus,
    /// Help text, kept separate from the warning stream.
    pub(crate) stdout: String,
    /// Application diagnostics emitted before CLI parsing.
    pub(crate) stderr: String,
}

/// Runs help with explicitly selected child-only localization settings.
/// For example, `diagnostic_help(Some("1"), None, None)` captures stock help
/// and its diagnostic stderr stream without changing the test process's
/// environment.
///
/// # Errors
///
/// Returns an error when the temporary directory cannot be created, the child
/// cannot run, or either stream is not UTF-8.
pub(crate) fn diagnostic_help(
    disable_localization: Option<&str>,
    primary_locale: Option<&str>,
    fallback_locale: Option<&str>,
) -> eyre::Result<DiagnosticHelpOutput> {
    let directory = tempfile::tempdir().wrap_err("create isolated help directory")?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"));
    command
        .env_clear()
        .current_dir(directory.path())
        .arg("--help");
    for (key, value) in [
        (DISABLE_LOCALIZATION_ENV, disable_localization),
        ("SPYCATCHER_HARNESS_LOCALE", primary_locale),
        ("SPYCATCHER_HARNESS_FALLBACK_LOCALE", fallback_locale),
    ] {
        if let Some(configured_value) = value {
            command.env(key, configured_value);
        }
    }
    coverage_profile::preserve_coverage_profile(&mut command);
    let output = command
        .output()
        .wrap_err("execute diagnostic help subprocess")?;
    Ok(DiagnosticHelpOutput {
        status: output.status,
        stdout: String::from_utf8(output.stdout).wrap_err("help stdout is not UTF-8")?,
        stderr: String::from_utf8(output.stderr).wrap_err("help stderr is not UTF-8")?,
    })
}

/// Checks stock help and visible bypass diagnostics after successful subprocess setup.
#[track_caller]
pub(crate) fn assert_truthy_diagnostic_help(
    output: &DiagnosticHelpOutput,
    control: &DiagnosticHelpOutput,
) {
    assert_truthy_help_stdout(output, control);
    assert_plain_bypass_warning(output);
}

/// A stock-help control avoids pinning the full upstream help text.
#[track_caller]
fn assert_truthy_help_stdout(output: &DiagnosticHelpOutput, control: &DiagnosticHelpOutput) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "diagnostic help must exit zero"
    );
    assert_eq!(
        control.status.code(),
        Some(0),
        "stock help control must exit zero"
    );
    assert_eq!(
        output.stdout, control.stdout,
        "truthy spellings must retain stock help"
    );
    assert!(
        output
            .stdout
            .contains("Usage: spycatcher-harness <COMMAND>"),
        "stock usage must remain",
    );
    assert!(
        !output
            .stdout
            .contains("records upstream LLM API traffic into cassettes"),
        "localization must be bypassed",
    );
    assert!(
        !output.stdout.contains("CLI localization disabled"),
        "warnings must not contaminate help stdout",
    );
}

/// A visible, uncoloured warning makes the diagnostic bypass observable in captured output.
#[track_caller]
fn assert_plain_bypass_warning(output: &DiagnosticHelpOutput) {
    assert!(
        output
            .stderr
            .contains("CLI localization disabled by diagnostic environment switch"),
        "diagnostic switch warning missing: {}",
        output.stderr,
    );
    assert!(
        output.stderr.contains(DISABLE_LOCALIZATION_ENV),
        "warning must identify the diagnostic environment switch: {}",
        output.stderr,
    );
    assert!(
        !output.stderr.contains('\u{1b}'),
        "captured diagnostics must not contain ANSI escapes",
    );
}

/// Checks localized help preservation and silence for absent or falsy switches.
#[track_caller]
pub(crate) fn assert_falsy_diagnostic_help(
    output: &DiagnosticHelpOutput,
    control: &DiagnosticHelpOutput,
) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "localized help must exit zero"
    );
    assert_eq!(
        control.status.code(),
        Some(0),
        "localized control must exit zero"
    );
    assert_eq!(
        output.stdout, control.stdout,
        "falsy switches must preserve localized help"
    );
    assert!(
        output
            .stdout
            .contains("records upstream LLM API traffic into cassettes"),
        "localized copy must remain",
    );
    assert!(
        !output.stderr.contains("CLI localization disabled"),
        "falsy switches must not emit disabled warnings: {}",
        output.stderr,
    );
    assert!(
        control.stderr.is_empty(),
        "clean help control must be quiet: {}",
        control.stderr
    );
}

/// Checks a primary-locale warning without changing valid fallback help.
#[track_caller]
pub(crate) fn assert_primary_locale_fallback_help(
    output: &DiagnosticHelpOutput,
    control: &DiagnosticHelpOutput,
) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "fallback help must exit zero"
    );
    assert_eq!(
        control.status.code(),
        Some(0),
        "valid fallback control must exit zero"
    );
    assert_eq!(
        output.stdout, control.stdout,
        "invalid primary must retain fallback help"
    );
    assert!(
        output
            .stderr
            .contains("invalid primary early CLI locale; trying fallback locale"),
        "invalid primary warning missing: {}",
        output.stderr,
    );
    assert!(
        !output.stderr.contains("invalid fallback early CLI locale"),
        "valid fallback must not trigger a warning: {}",
        output.stderr,
    );
    assert!(
        control.stderr.is_empty(),
        "valid locale control must be quiet: {}",
        control.stderr
    );
    assert!(
        !output.stdout.contains("invalid primary early CLI locale"),
        "locale warnings belong on stderr",
    );
    assert!(
        !output.stderr.contains('\u{1b}'),
        "captured diagnostics must not contain ANSI escapes",
    );
}

/// Checks both invalid-locale warnings while preserving default help.
#[track_caller]
pub(crate) fn assert_default_locale_fallback_help(
    output: &DiagnosticHelpOutput,
    control: &DiagnosticHelpOutput,
) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "default help after invalid locales must exit zero"
    );
    assert_eq!(
        control.status.code(),
        Some(0),
        "en-US control must exit zero"
    );
    assert_eq!(
        output.stdout, control.stdout,
        "invalid locale settings must retain en-US help"
    );
    for warning in [
        "invalid primary early CLI locale; trying fallback locale",
        "invalid fallback early CLI locale; using en-US",
    ] {
        assert!(
            output.stderr.contains(warning),
            "locale warning missing ({warning}): {}",
            output.stderr,
        );
        assert!(
            !output.stdout.contains(warning),
            "locale warnings belong on stderr"
        );
    }
    assert!(
        control.stderr.is_empty(),
        "en-US control must be quiet: {}",
        control.stderr
    );
    assert!(
        !output.stderr.contains('\u{1b}'),
        "captured diagnostics must not contain ANSI escapes",
    );
}
