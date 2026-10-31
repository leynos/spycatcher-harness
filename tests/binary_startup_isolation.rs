//! Binary tests for parse and configuration failures before record startup.
//!
//! Each case runs the compiled binary in an isolated child environment and
//! proves that a failure leaves record-mode side effects untouched: no cassette
//! directory, no listener traffic, and no upstream request.

use std::net::{SocketAddr, TcpListener};
use std::process::{Command, Output};

use cap_std::ambient_authority;
use cap_std::fs::Dir;
#[cfg(unix)]
use cap_std::fs::PermissionsExt;
use eyre::{Result, WrapErr as _, ensure};
use rstest::rstest;
use spycatcher_harness::cli::localizer::DISABLE_LOCALIZATION_ENV;
use tempfile::TempDir;

#[path = "common/coverage_profile.rs"]
mod coverage_profile;

/// Secret placed in the child environment to prove diagnostics redact it.
const SECRET: &str = "startup-failure-secret-sentinel";

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

/// One startup failure and the configuration that provokes it.
struct StartupFailureScenario {
    /// Primary `.spycatcher_harness.toml` content, when one is written.
    config: Option<&'static str>,
    /// Extra command-line arguments appended after the record defaults.
    extra_args: &'static [&'static str],
    /// Text the startup diagnostic must contain.
    error_fragment: &'static str,
    /// Whether a valid XDG candidate exists beneath the unreadable primary.
    has_valid_xdg_candidate: bool,
    /// Whether the primary candidate should be made unreadable.
    unreadable_config: bool,
}

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

#[cfg(unix)]
fn primary_candidate_remains_readable(temp_dir: &TempDir) -> Result<bool> {
    let directory = Dir::open_ambient_dir(temp_dir.path(), ambient_authority())?;
    match directory.open(".spycatcher_harness.toml") {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn assert_record_startup_failure(scenario: &StartupFailureScenario) -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    write_startup_config_candidates(&temp_dir, scenario)?;
    let resources = RecordStartupResources::new()?;
    let output = run_record_startup(&temp_dir, scenario, &resources)?;
    assert_startup_failure(&output, &temp_dir, scenario, &resources)
}

fn write_startup_config_candidates(
    temp_dir: &TempDir,
    scenario: &StartupFailureScenario,
) -> Result<()> {
    let directory = Dir::open_ambient_dir(temp_dir.path(), ambient_authority())?;
    write_primary_config_candidate(&directory, scenario)?;
    if scenario.has_valid_xdg_candidate {
        write_valid_xdg_candidate(&directory)?;
    }
    Ok(())
}

fn write_primary_config_candidate(
    directory: &Dir,
    scenario: &StartupFailureScenario,
) -> Result<()> {
    if let Some(config) = scenario.config {
        directory.write(".spycatcher_harness.toml", config.as_bytes())?;
        make_primary_candidate_unreadable(directory, scenario.unreadable_config)?;
    }
    Ok(())
}

fn make_primary_candidate_unreadable(directory: &Dir, should_be_unreadable: bool) -> Result<()> {
    if should_be_unreadable {
        remove_read_permissions(directory)?;
    }
    Ok(())
}

#[cfg(unix)]
fn remove_read_permissions(directory: &Dir) -> Result<()> {
    let mut permissions = directory
        .metadata(".spycatcher_harness.toml")?
        .permissions();
    permissions.set_mode(0);
    directory.set_permissions(".spycatcher_harness.toml", permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn remove_read_permissions(_directory: &Dir) -> Result<()> {
    Ok(())
}

fn write_valid_xdg_candidate(directory: &Dir) -> Result<()> {
    directory.create_dir_all("spycatcher_harness")?;
    let xdg_config = directory.open_dir("spycatcher_harness")?;
    xdg_config.write(
        "config.toml",
        b"[cmds.record]\ncassette_name = \"later-valid-candidate\"\n",
    )?;
    Ok(())
}

/// Listeners that must remain untouched when startup fails.
struct RecordStartupResources {
    /// Listener reserved for the configured record address.
    held_listener: TcpListener,
    /// Address the child is told to bind.
    listen_address: SocketAddr,
    /// Listener reserved for the configured upstream URL.
    upstream_listener: TcpListener,
    /// URL the child is told to call for upstream traffic.
    upstream_url: String,
}

impl RecordStartupResources {
    fn new() -> Result<Self> {
        let held_listener = bind_nonblocking_listener()?;
        let listen_address = held_listener.local_addr()?;
        let upstream_listener = bind_nonblocking_listener()?;
        let upstream_url = format!("http://{}", upstream_listener.local_addr()?);
        Ok(Self {
            held_listener,
            listen_address,
            upstream_listener,
            upstream_url,
        })
    }
}

fn bind_nonblocking_listener() -> Result<TcpListener> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

fn run_record_startup(
    temp_dir: &TempDir,
    scenario: &StartupFailureScenario,
    resources: &RecordStartupResources,
) -> Result<Output> {
    let mut args = vec![
        String::from("record"),
        String::from("--listen"),
        resources.listen_address.to_string(),
        String::from("--cassette-dir"),
        String::from("cassettes"),
        String::from("--cassette-name"),
        String::from("must-not-be-written"),
    ];
    args.extend(scenario.extra_args.iter().map(|arg| (*arg).to_owned()));

    let mut command = isolated_record_command(temp_dir, resources);
    command.args(&args);
    command.output().wrap_err("run record startup")
}

fn isolated_record_command(temp_dir: &TempDir, resources: &RecordStartupResources) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"));
    command
        .env_clear()
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path())
        .env("XDG_CONFIG_DIRS", temp_dir.path())
        .env(DISABLE_LOCALIZATION_ENV, "1")
        .env(
            "SPYCATCHER_HARNESS_CMDS_RECORD_UPSTREAM__BASE_URL",
            &resources.upstream_url,
        )
        .env("OPENROUTER_API_KEY", SECRET)
        .current_dir(temp_dir.path());
    coverage_profile::preserve_coverage_profile(&mut command);
    command
}

fn assert_startup_failure(
    output: &Output,
    temp_dir: &TempDir,
    scenario: &StartupFailureScenario,
    resources: &RecordStartupResources,
) -> Result<()> {
    ensure!(
        output.status.code() == Some(1),
        "configuration and startup failures should exit with status 1, got {:?}",
        output.status.code()
    );
    ensure!(
        output.stdout.is_empty(),
        "startup errors should not write to stdout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    ensure!(
        stderr.contains(scenario.error_fragment),
        "expected {:?} in startup error, got: {stderr}",
        scenario.error_fragment
    );
    ensure!(
        !stderr.contains(SECRET),
        "startup errors must redact secrets"
    );

    let cassette_dir = temp_dir.path().join("cassettes");
    ensure!(
        !cassette_dir.exists(),
        "failed startup must not create cassettes"
    );
    ensure!(
        resources.held_listener.local_addr()? == resources.listen_address,
        "failed startup must leave the configured listener untouched"
    );
    assert_no_listener_traffic(resources)
}

#[test]
fn binary_parse_failure_has_no_record_startup_side_effects() -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    let resources = RecordStartupResources::new()?;
    let output = run_record_parse_failure(&temp_dir, &resources)?;
    assert_parse_failure_has_no_side_effects(&output, &temp_dir, &resources)
}

fn run_record_parse_failure(
    temp_dir: &TempDir,
    resources: &RecordStartupResources,
) -> Result<Output> {
    let listen_address = resources.listen_address.to_string();
    let mut command = isolated_record_command(temp_dir, resources);
    command
        // Keep localization active so the localized parse-error path is the
        // one exercised; stock Clap would otherwise say "unexpected argument".
        .env_remove(DISABLE_LOCALIZATION_ENV)
        .env("OPENROUTER_API_KEY", "parse-failure-secret-sentinel")
        .args([
            "record",
            "--listen",
            listen_address.as_str(),
            "--cassette-dir",
            "cassettes",
            "--cassette-name",
            "must-not-be-written",
            "--not-a-flag",
        ]);
    command.output().wrap_err("run record parse failure")
}

fn assert_parse_failure_has_no_side_effects(
    output: &Output,
    temp_dir: &TempDir,
    resources: &RecordStartupResources,
) -> Result<()> {
    ensure!(
        output.status.code() == Some(2),
        "unknown argument should fail parsing, got {:?}",
        output.status.code()
    );
    ensure!(
        output.stdout.is_empty(),
        "parse errors should not write to stdout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    ensure!(
        stderr.contains("unknown argument"),
        "parse failure should reach the CLI error path: {stderr}"
    );
    ensure!(
        !stderr.contains("parse-failure-secret-sentinel"),
        "parse failure diagnostics must not disclose the configured secret"
    );
    ensure!(
        !temp_dir.path().join("cassettes").exists(),
        "parse failure must not create the cassette directory"
    );
    assert_no_listener_traffic(resources)
}

fn assert_no_listener_traffic(resources: &RecordStartupResources) -> Result<()> {
    ensure!(
        matches!(
            resources.held_listener.accept(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ),
        "failed startup must not send traffic to the listener"
    );
    ensure!(
        matches!(
            resources.upstream_listener.accept(),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock
        ),
        "failed startup must not contact the configured upstream"
    );
    Ok(())
}
