//! End-to-end tests for binary-owned startup localization.
//!
//! These tests execute the compiled `spycatcher-harness` binary so coverage
//! includes `main`, Clap argument parsing, layered configuration loading,
//! language-loader construction, and user-facing error rendering.

use std::process::Command;

#[test]
fn binary_uses_locale_flags_for_startup_error_rendering() {
    let temp_dir = tempfile::tempdir().expect("temporary directory should be created");
    let output = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"))
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
