//! End-to-end tests for binary-owned startup localization.
//!
//! These tests execute the compiled `spycatcher-harness` binary so coverage
//! includes `main`, Clap argument parsing, layered configuration loading,
//! language-loader construction, and user-facing error rendering.

use std::process::{Command, ExitStatus, Output};

use eyre::WrapErr as _;
use rstest::rstest;

use spycatcher_harness::cli::localizer::DISABLE_LOCALIZATION_ENV;

/// Executes the binary with an empty child environment.
/// For example, `localized_cli_output(&["--help"])` captures localized help on
/// stdout without changing the test process's environment or working directory.
///
/// # Errors
///
/// Propagates an I/O error if launching or collecting the child process fails.
fn localized_cli_output(args: &[&str]) -> std::io::Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"));
    command.env_clear().args(args).output()
}

/// Runs `args` and returns the localized stdout, rejecting diagnostics.
fn localized_stdout(args: &[&str], subject: &str) -> String {
    let output = localized_cli_output(args).expect("binary should execute");

    assert!(
        output.status.success(),
        "{subject} should exit successfully"
    );
    assert!(
        output.stderr.is_empty(),
        "{subject} should not write diagnostics"
    );
    String::from_utf8(output.stdout).expect("stdout should be UTF-8")
}

#[test]
fn binary_emits_localized_help() {
    insta::assert_snapshot!(localized_stdout(&["--help"], "help"));
}

#[test]
fn binary_emits_localized_version() {
    insta::assert_snapshot!(localized_stdout(&["--version"], "version"));
}

#[test]
fn binary_emits_localized_unknown_argument_error() {
    let output = localized_cli_output(&["replay", "--not-a-flag"]).expect("binary should execute");

    assert_eq!(
        output.status.code(),
        Some(2),
        "Clap parse errors exit with 2"
    );
    assert!(output.stdout.is_empty(), "parse errors belong on stderr");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    insta::assert_snapshot!(stderr);
}

#[test]
fn binary_can_disable_cli_localization_for_diagnostics() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"));
    let output = command
        .env_clear()
        .env(DISABLE_LOCALIZATION_ENV, "1")
        .arg("--help")
        .output()
        .expect("binary should execute");

    assert!(output.status.success(), "help should exit successfully");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("Usage: spycatcher-harness <COMMAND>"));
    assert!(!stdout.contains("records upstream LLM API traffic into cassettes"));
}

#[test]
fn binary_uses_locale_flags_for_startup_error_rendering() {
    let temp_dir = tempfile::tempdir().expect("temporary directory should be created");
    let mut command = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"));
    let output = command
        .env_clear()
        .current_dir(temp_dir.path())
        .args(["record", "--locale", "en-GB", "--fallback-locale", "en-US"])
        .output()
        .expect("binary should execute");

    assert_eq!(
        output.status.code(),
        Some(1),
        "record mode without upstream should exit one"
    );
    assert!(output.stdout.is_empty(), "startup errors belong on stderr");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    let error_line = stderr
        .lines()
        .find(|line| line.starts_with("Error:"))
        .expect("stderr should contain an Error: line");
    insta::assert_snapshot!(
        error_line,
        @"Error: failed to start harness: invalid configuration: \u{2068}upstream configuration is required for record mode\u{2069}"
    );
}

/// Captured streams and exit status from one isolated help subprocess.
struct DiagnosticHelpOutput {
    /// Exit status used to prove diagnostics do not turn help into an error.
    status: ExitStatus,
    /// Help text, kept separate from the warning stream.
    stdout: String,
    /// Application diagnostics emitted before CLI parsing.
    stderr: String,
}

/// Runs help with explicitly selected child-only localization settings.
/// For example, `diagnostic_help(Some("1"), None, None)` captures stock help
/// and its diagnostic stderr stream without changing the test process's environment.
fn diagnostic_help(
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
    let output = command
        .output()
        .wrap_err("execute diagnostic help subprocess")?;
    Ok(DiagnosticHelpOutput {
        status: output.status,
        stdout: String::from_utf8(output.stdout).wrap_err("help stdout is not UTF-8")?,
        stderr: String::from_utf8(output.stderr).wrap_err("help stderr is not UTF-8")?,
    })
}

/// Truthy diagnostic spellings retain stock help and expose the bypass warning.
#[rstest]
#[case::numeric("1")]
#[case::uppercase("TRUE")]
#[case::trimmed_yes(" YES ")]
#[case::on("on")]
fn binary_diagnostic_truthy_switch_warns_on_stderr(#[case] value: &str) -> eyre::Result<()> {
    let output = diagnostic_help(Some(value), None, None)?;
    let control = diagnostic_help(Some("1"), None, None)?;
    assert_truthy_diagnostic_help(&output, &control);
    Ok(())
}

/// Absent and falsy diagnostic settings preserve localized help without warnings.
#[rstest]
#[case::absent(None)]
#[case::empty(Some(""))]
#[case::zero(Some("0"))]
#[case::false_value(Some("false"))]
#[case::unsupported(Some("enabled"))]
fn binary_diagnostic_falsy_switch_keeps_localized_help(
    #[case] value: Option<&str>,
) -> eyre::Result<()> {
    let output = diagnostic_help(value, None, None)?;
    let control = diagnostic_help(None, None, None)?;
    assert_falsy_diagnostic_help(&output, &control);
    Ok(())
}

/// Invalid primary locale diagnostics remain separate from valid fallback help.
#[test]
fn binary_diagnostic_invalid_primary_warns_and_preserves_fallback_help() -> eyre::Result<()> {
    let output = diagnostic_help(None, Some("not_a_locale"), Some("en-GB"))?;
    let control = diagnostic_help(None, Some("en-GB"), Some("en-GB"))?;
    assert_primary_locale_fallback_help(&output, &control);
    Ok(())
}

/// Both invalid locale warnings accompany unchanged en-US fallback help.
#[test]
fn binary_diagnostic_invalid_both_warns_and_preserves_default_help() -> eyre::Result<()> {
    let output = diagnostic_help(None, Some("not_a_locale"), Some("also_invalid"))?;
    let control = diagnostic_help(None, Some("en-US"), Some("en-US"))?;
    assert_default_locale_fallback_help(&output, &control);
    Ok(())
}

/// Checks stock help and visible bypass diagnostics after successful subprocess setup.
#[track_caller]
fn assert_truthy_diagnostic_help(output: &DiagnosticHelpOutput, control: &DiagnosticHelpOutput) {
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
fn assert_falsy_diagnostic_help(output: &DiagnosticHelpOutput, control: &DiagnosticHelpOutput) {
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
fn assert_primary_locale_fallback_help(
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
fn assert_default_locale_fallback_help(
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
