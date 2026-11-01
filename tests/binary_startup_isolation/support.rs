//! Shared child-process harness for the startup-isolation binary tests.
//!
//! These helpers build an isolated record-mode invocation, hold probe
//! listeners, and assert that a failed startup touched neither the cassette
//! directory nor the network. Keeping them in a submodule leaves the test file
//! focused on the cases it specifies.

use std::{
    net::{SocketAddr, TcpListener},
    process::{Command, Output},
};

#[cfg(unix)]
use cap_std::fs::PermissionsExt;
use cap_std::{ambient_authority, fs::Dir};
use eyre::{Result, WrapErr as _, ensure};
use spycatcher_harness::cli::localizer::DISABLE_LOCALIZATION_ENV;
use tempfile::TempDir;

use super::coverage_profile;

/// Secret placed in the child environment to prove diagnostics redact it.
const SECRET: &str = "startup-failure-secret-sentinel";

/// One startup failure and the configuration that provokes it.
pub(crate) struct StartupFailureScenario {
    /// Primary `.spycatcher_harness.toml` content, when one is written.
    pub(crate) config: Option<&'static str>,
    /// Extra command-line arguments appended after the record defaults.
    pub(crate) extra_args: &'static [&'static str],
    /// Text the startup diagnostic must contain.
    pub(crate) error_fragment: &'static str,
    /// Whether a valid XDG candidate exists beneath the unreadable primary.
    pub(crate) has_valid_xdg_candidate: bool,
    /// Whether the primary candidate should be made unreadable.
    pub(crate) unreadable_config: bool,
}

/// Writes the scenario's configuration and asserts the isolated failure.
pub(crate) fn assert_record_startup_failure(scenario: &StartupFailureScenario) -> Result<()> {
    let temp_dir = tempfile::tempdir()?;
    write_startup_config_candidates(&temp_dir, scenario)?;
    let resources = RecordStartupResources::new()?;
    let output = run_record_startup(&temp_dir, scenario, &resources)?;
    assert_startup_failure(&output, &temp_dir, scenario, &resources)
}

/// Writes the primary candidate and, when requested, a valid XDG candidate.
pub(crate) fn write_startup_config_candidates(
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

/// Writes the primary candidate and applies the requested mode change.
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

/// Removes read permission from the primary candidate when requested.
fn make_primary_candidate_unreadable(directory: &Dir, should_be_unreadable: bool) -> Result<()> {
    if should_be_unreadable {
        remove_read_permissions(directory)?;
    }
    Ok(())
}

/// Sets mode `0o000` on the primary candidate.
#[cfg(unix)]
fn remove_read_permissions(directory: &Dir) -> Result<()> {
    let mut permissions = directory
        .metadata(".spycatcher_harness.toml")?
        .permissions();
    permissions.set_mode(0);
    directory.set_permissions(".spycatcher_harness.toml", permissions)?;
    Ok(())
}

/// Leaves permissions untouched on platforms without POSIX mode bits.
#[cfg(not(unix))]
fn remove_read_permissions(_directory: &Dir) -> Result<()> { Ok(()) }

/// Writes the XDG candidate that must never be reached past a failed primary.
fn write_valid_xdg_candidate(directory: &Dir) -> Result<()> {
    directory.create_dir_all("spycatcher_harness")?;
    let xdg_config = directory.open_dir("spycatcher_harness")?;
    xdg_config.write(
        "config.toml",
        b"[cmds.record]\ncassette_name = \"later-valid-candidate\"\n",
    )?;
    Ok(())
}

/// Reports whether the running process can still read the mode-`0o000` file.
pub(crate) fn primary_candidate_remains_readable(temp_dir: &TempDir) -> Result<bool> {
    let directory = Dir::open_ambient_dir(temp_dir.path(), ambient_authority())?;
    match directory.open(".spycatcher_harness.toml") {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Listeners that must remain untouched when startup fails.
pub(crate) struct RecordStartupResources {
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
    /// Binds both probe listeners so their addresses can be handed to the child.
    pub(crate) fn new() -> Result<Self> {
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

/// Binds a loopback listener that never accepts a connection.
fn bind_nonblocking_listener() -> Result<TcpListener> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

/// Runs record mode against an isolated home, XDG root, and probe listeners.
pub(crate) fn run_record_startup(
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

/// Builds the child command with every ambient configuration source removed.
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

/// Asserts a startup failure left no cassette and no listener traffic.
pub(crate) fn assert_startup_failure(
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
    assert_no_listener_traffic(resources)
}

/// Runs record mode with an unknown flag, keeping localization active.
pub(crate) fn run_record_parse_failure(
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

/// Asserts a parse failure created no cassette and contacted no listener.
pub(crate) fn assert_parse_failure_has_no_side_effects(
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

/// Asserts neither probe listener received a connection.
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
