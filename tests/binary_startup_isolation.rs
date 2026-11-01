//! Binary tests for parse and configuration failures before record startup.
//!
//! Each case runs the compiled binary in an isolated child environment and
//! proves that a failure leaves record-mode side effects untouched: no cassette
//! directory, no listener traffic, and no upstream request.

use std::process::Command;

use eyre::{Result, WrapErr as _, ensure};
use rstest::rstest;

#[path = "common/coverage_profile.rs"]
mod coverage_profile;
#[path = "common/process_env.rs"]
mod process_env;
#[path = "binary_startup_isolation/support.rs"]
mod support;

use support::{
    RecordStartupResources,
    StartupFailureScenario,
    assert_parse_failure_has_no_side_effects,
    assert_record_startup_failure,
    assert_startup_failure,
    primary_candidate_remains_readable,
    run_record_parse_failure,
    run_record_startup,
    write_startup_config_candidates,
};

/// Proves parse failures report on stderr and leave stdout empty.
#[rstest]
#[case::unknown_subcommand(&["not-a-subcommand"])]
#[case::missing_option_value(&["record", "--listen"])]
fn binary_parse_failures_write_only_to_stderr(#[case] args: &[&str]) -> Result<()> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"));
    command.args(args).env_clear();
    coverage_profile::preserve_coverage_profile(&mut command);
    let output = command.output().wrap_err("run binary")?;

    ensure!(
        output.status.code() == Some(2),
        "parse failures should use Clap's parse-failure status, got {:?}",
        output.status.code()
    );
    ensure!(
        output.stdout.is_empty(),
        "parse errors should not write to stdout"
    );
    ensure!(
        !output.stderr.is_empty(),
        "parse errors should write to stderr"
    );
    Ok(())
}

/// Proves each startup failure aborts before record-mode resources exist.
#[rstest]
#[case::malformed_config(StartupFailureScenario {
    config: Some("[cmds.record\ncassette_name = \"broken\"\n"),
    extra_args: &[],
    error_fragment: "failed to load merged command config",
    has_valid_xdg_candidate: false,
    unreadable_config: false,
})]
#[case::missing_inheritance(StartupFailureScenario {
    config: Some("extends = \"missing-parent.toml\"\n[cmds.record]\n"),
    extra_args: &[],
    error_fragment: "missing-parent.toml",
    has_valid_xdg_candidate: false,
    unreadable_config: false,
})]
#[case::invalid_locale(StartupFailureScenario {
    config: None,
    extra_args: &["--locale", "not_a_locale"],
    error_fragment: "invalid localization field `locale`",
    has_valid_xdg_candidate: false,
    unreadable_config: false,
})]
#[case::invalid_fallback_locale(StartupFailureScenario {
    config: None,
    extra_args: &["--fallback-locale", "not_a_locale"],
    error_fragment: "invalid localization field `fallback_locale`",
    has_valid_xdg_candidate: false,
    unreadable_config: false,
})]
fn binary_startup_failures_leave_record_resources_untouched(
    #[case] scenario: StartupFailureScenario,
) -> Result<()> {
    assert_record_startup_failure(&scenario)
}

/// Proves an unreadable discovered file aborts before a valid XDG candidate.
///
/// The case is skipped when the runner can still read a mode-`0o000` file,
/// because the discovery error cannot then be provoked.
#[cfg(unix)]
#[test]
fn binary_unreadable_discovered_file_aborts_before_valid_xdg_candidate() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let scenario = StartupFailureScenario {
        config: Some("[cmds.record]\ncassette_name = \"unreadable\"\n"),
        extra_args: &[],
        error_fragment: "Permission denied",
        has_valid_xdg_candidate: true,
        unreadable_config: true,
    };
    write_startup_config_candidates(&temp_dir, &scenario)?;
    if primary_candidate_remains_readable(&temp_dir)? {
        tracing::warn!(
            reason = "the test process can read a mode-000 file",
            "skipping unreadable-config assertion"
        );
        return Ok(());
    }

    let resources = RecordStartupResources::new()?;
    let output = run_record_startup(&temp_dir, &scenario, &resources)?;
    assert_startup_failure(&output, &temp_dir, &scenario, &resources)
}

/// Proves a localized parse failure creates no record-mode side effects.
#[test]
fn binary_parse_failure_has_no_record_startup_side_effects() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let resources = RecordStartupResources::new()?;
    let output = run_record_parse_failure(&temp_dir, &resources)?;
    assert_parse_failure_has_no_side_effects(&output, &temp_dir, &resources)
}
