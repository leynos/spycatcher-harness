//! End-to-end tests for binary-owned startup localization.
//!
//! These tests execute the compiled `spycatcher-harness` binary so coverage
//! includes `main`, Clap argument parsing, layered configuration loading,
//! language-loader construction, and user-facing error rendering.

use std::process::Command;

use spycatcher_harness::cli::localizer::DISABLE_LOCALIZATION_ENV;

#[test]
fn binary_emits_localized_help() {
    let output = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"))
        .env_remove(DISABLE_LOCALIZATION_ENV)
        .env_remove("SPYCATCHER_HARNESS_LOCALE")
        .env_remove("SPYCATCHER_HARNESS_FALLBACK_LOCALE")
        .arg("--help")
        .output()
        .expect("binary should execute");

    assert!(output.status.success(), "help should exit successfully");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    insta::assert_snapshot!(stdout);
}

#[test]
fn binary_emits_localized_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"))
        .env_remove(DISABLE_LOCALIZATION_ENV)
        .env_remove("SPYCATCHER_HARNESS_LOCALE")
        .env_remove("SPYCATCHER_HARNESS_FALLBACK_LOCALE")
        .arg("--version")
        .output()
        .expect("binary should execute");

    assert!(output.status.success(), "version should exit successfully");
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    insta::assert_snapshot!(stdout);
}

#[test]
fn binary_emits_localized_unknown_argument_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"))
        .env_remove(DISABLE_LOCALIZATION_ENV)
        .env_remove("SPYCATCHER_HARNESS_LOCALE")
        .env_remove("SPYCATCHER_HARNESS_FALLBACK_LOCALE")
        .args(["replay", "--not-a-flag"])
        .output()
        .expect("binary should execute");

    assert!(
        !output.status.success(),
        "unknown arguments should fail parsing"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    insta::assert_snapshot!(stderr);
}

#[test]
fn binary_can_disable_cli_localization_for_diagnostics() {
    let output = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"))
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
    let output = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"))
        .env_remove(DISABLE_LOCALIZATION_ENV)
        .current_dir(temp_dir.path())
        .args(["record", "--locale", "en-GB", "--fallback-locale", "en-US"])
        .output()
        .expect("binary should execute");

    assert!(
        !output.status.success(),
        "record mode without upstream should fail startup"
    );
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
