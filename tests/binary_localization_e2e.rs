//! End-to-end tests for binary-owned startup localization.
//!
//! These tests execute the compiled `spycatcher-harness` binary so coverage
//! includes `main`, Clap argument parsing, layered configuration loading,
//! language-loader construction, and user-facing error rendering.

use std::process::{Command, Output};

use rstest::rstest;

use spycatcher_harness::cli::localizer::DISABLE_LOCALIZATION_ENV;

#[path = "binary_localization_e2e/diagnostics.rs"]
mod diagnostics;

#[path = "binary_localization_e2e/startup_isolation.rs"]
mod startup_isolation;

/// Executes the binary with CLI localization variables removed from the child.
/// For example, `localized_cli_output(&["--help"])` captures localized help on
/// stdout without changing the test process's environment or working directory.
///
/// # Errors
///
/// Propagates an I/O error if launching or collecting the child process fails.
fn localized_cli_output(args: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"))
        .env_remove(DISABLE_LOCALIZATION_ENV)
        .env_remove("SPYCATCHER_HARNESS_LOCALE")
        .env_remove("SPYCATCHER_HARNESS_FALLBACK_LOCALE")
        .args(args)
        .output()
}

#[test]
fn binary_emits_localized_help() {
    let output = localized_cli_output(&["--help"]).expect("binary should execute");

    assert_eq!(
        output.status.code(),
        Some(0),
        "help should exit successfully"
    );
    assert!(output.stderr.is_empty(), "help should not write to stderr");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    insta::assert_snapshot!(stdout);
}

#[rstest]
#[case::root_short(&["-h"])]
#[case::record_short(&["record", "-h"])]
#[case::record_long(&["record", "--help"])]
#[case::replay_short(&["replay", "-h"])]
#[case::replay_long(&["replay", "--help"])]
#[case::verify_short(&["verify", "-h"])]
#[case::verify_long(&["verify", "--help"])]
fn binary_help_requests_write_only_to_stdout(#[case] args: &[&str]) {
    let output = localized_cli_output(args).expect("binary should execute");

    assert_eq!(
        output.status.code(),
        Some(0),
        "help should exit successfully"
    );
    assert!(!output.stdout.is_empty(), "help should write to stdout");
    assert!(output.stderr.is_empty(), "help should not write to stderr");
}

#[test]
fn binary_emits_localized_version() {
    let output = localized_cli_output(&["--version"]).expect("binary should execute");

    assert_eq!(
        output.status.code(),
        Some(0),
        "version should exit successfully"
    );
    assert!(
        output.stderr.is_empty(),
        "version should not write to stderr"
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    insta::assert_snapshot!(stdout);
}

#[test]
fn binary_emits_localized_unknown_argument_error() {
    let output = localized_cli_output(&["replay", "--not-a-flag"]).expect("binary should execute");

    assert_eq!(
        output.status.code(),
        Some(2),
        "unknown arguments should use Clap's parse-failure status"
    );
    assert!(
        output.stdout.is_empty(),
        "parse errors should not write to stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    insta::assert_snapshot!(stderr);
}
