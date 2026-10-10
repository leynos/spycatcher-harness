//! End-to-end tests for binary-owned startup localization.
//!
//! These tests execute the compiled `spycatcher-harness` binary so coverage
//! includes `main`, Clap argument parsing, layered configuration loading,
//! language-loader construction, and user-facing error rendering.

use std::net::TcpListener;
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
fn binary_parse_failure_has_no_record_startup_side_effects() {
    let temp_dir = tempfile::tempdir().expect("temporary directory should be created");
    let held_listener = TcpListener::bind("127.0.0.1:0").expect("listener port should bind");
    let listen_address = held_listener
        .local_addr()
        .expect("held listener should have a local address")
        .to_string();
    let upstream_listener = TcpListener::bind("127.0.0.1:0").expect("upstream port should bind");
    upstream_listener
        .set_nonblocking(true)
        .expect("upstream listener should be non-blocking");
    let upstream_url = format!(
        "http://{}",
        upstream_listener
            .local_addr()
            .expect("upstream listener should have a local address")
    );
    let cassette_dir = temp_dir.path().join("cassettes");
    let listen_address_arg = listen_address.as_str();

    let output = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"))
        .env_clear()
        .env(
            "SPYCATCHER_HARNESS_CMDS_RECORD_UPSTREAM__BASE_URL",
            upstream_url,
        )
        .env("OPENROUTER_API_KEY", "parse-failure-secret-sentinel")
        .current_dir(temp_dir.path())
        .args([
            "record",
            "--listen",
            listen_address_arg,
            "--cassette-dir",
            "cassettes",
            "--cassette-name",
            "must-not-be-written",
            "--not-a-flag",
        ])
        .output()
        .expect("binary should execute");

    assert!(
        !output.status.success(),
        "unknown argument should fail parsing"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(
        stderr.contains("unknown argument"),
        "parse failure should reach the CLI error path: {stderr}"
    );
    assert!(
        !stderr.contains("parse-failure-secret-sentinel"),
        "parse failure diagnostics must not disclose the configured secret"
    );
    assert!(
        !cassette_dir.exists(),
        "parse failure must not create the cassette directory"
    );
    assert!(
        matches!(upstream_listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "parse failure must not contact the configured upstream"
    );
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
