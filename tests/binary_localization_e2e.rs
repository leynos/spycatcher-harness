//! End-to-end tests for binary-owned startup localization.
//!
//! These tests execute the compiled `spycatcher-harness` binary so coverage
//! includes `main`, Clap argument parsing, layered configuration loading,
//! language-loader construction, and user-facing error rendering.

use std::process::{Command, Output};

#[path = "common/coverage_profile.rs"]
mod coverage_profile;
#[path = "common/process_env.rs"]
mod process_env;
#[path = "binary_localization_e2e/support.rs"]
mod support;

use eyre::WrapErr as _;
use rstest::rstest;
use spycatcher_harness::cli::localizer::DISABLE_LOCALIZATION_ENV;
use support::{
    assert_default_locale_fallback_help,
    assert_falsy_diagnostic_help,
    assert_primary_locale_fallback_help,
    assert_truthy_diagnostic_help,
    diagnostic_help,
};

/// Executes the binary with an empty child environment.
/// For example, `localized_cli_output(&["--help"])` captures localized help on
/// stdout without changing the test process's environment or working directory.
///
/// # Errors
///
/// Propagates an I/O error if launching or collecting the child process fails.
fn localized_cli_output(args: &[&str]) -> std::io::Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"));
    command.env_clear().args(args);
    coverage_profile::preserve_coverage_profile(&mut command);
    command.output()
}

/// Runs `args` and returns the localized stdout, rejecting diagnostics.
///
/// The fallible result keeps this helper free of `expect`, which the
/// repository reserves for code inside `#[test]` functions.
///
/// # Errors
///
/// Returns an error when the subprocess cannot start, exits unsuccessfully,
/// writes diagnostics, or emits non-UTF-8 stdout.
fn localized_stdout(args: &[&str], subject: &str) -> eyre::Result<String> {
    let output = localized_cli_output(args).wrap_err("execute localized subprocess")?;

    if !output.status.success() {
        eyre::bail!("{subject} should exit successfully");
    }
    if !output.stderr.is_empty() {
        eyre::bail!(
            "{subject} should not write diagnostics: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8(output.stdout).wrap_err("localized stdout is not UTF-8")
}

/// Proves `--help` renders the bundled en-US catalogue copy.
#[test]
fn binary_emits_localized_help() -> eyre::Result<()> {
    insta::assert_snapshot!(localized_stdout(&["--help"], "help")?);
    Ok(())
}

/// Proves `--version` renders the localized version string, including the
/// interpolated binary name.
#[test]
fn binary_emits_localized_version() -> eyre::Result<()> {
    insta::assert_snapshot!(localized_stdout(&["--version"], "version")?);
    Ok(())
}

/// Proves a parse failure keeps Clap's exit status and localized wording on
/// stderr, with nothing on stdout.
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

/// Proves the diagnostic switch restores stock Clap help text.
///
/// Support workflows rely on this to read help output that matches upstream
/// Clap documentation rather than the translated catalogue.
#[test]
fn binary_can_disable_cli_localization_for_diagnostics() {
    let mut command = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"));
    command
        .env_clear()
        .env(DISABLE_LOCALIZATION_ENV, "1")
        .arg("--help");
    coverage_profile::preserve_coverage_profile(&mut command);
    let output = command.output().expect("binary should execute");

    assert!(output.status.success(), "help should exit successfully");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("Usage: spycatcher-harness <COMMAND>"));
    assert!(!stdout.contains("records upstream LLM API traffic into cassettes"));
}

/// Proves startup diagnostics honour the early locale flags.
///
/// The child gets its own home and XDG roots so a developer's real
/// configuration cannot change the expected error.
#[test]
fn binary_uses_locale_flags_for_startup_error_rendering() {
    let temp_dir = tempfile::tempdir().expect("temporary directory should be created");
    let mut command = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"));
    command
        .env_clear()
        .current_dir(temp_dir.path())
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .env("XDG_CONFIG_DIRS", temp_dir.path())
        .args(["record", "--locale", "en-GB", "--fallback-locale", "en-US"]);
    coverage_profile::preserve_coverage_profile(&mut command);
    let output = command.output().expect("binary should execute");

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
