//! Binary tests for parse and configuration failures before record startup.

use std::net::TcpListener;
use std::process::Command;

use cap_std::ambient_authority;
use cap_std::fs::Dir;
#[cfg(unix)]
use cap_std::fs::PermissionsExt;
use rstest::rstest;
use spycatcher_harness::cli::localizer::DISABLE_LOCALIZATION_ENV;
use tempfile::TempDir;

type TestResult<T> = std::io::Result<T>;

#[rstest]
#[case::unknown_subcommand(&["not-a-subcommand"])]
#[case::missing_option_value(&["record", "--listen"])]
fn binary_parse_failures_write_only_to_stderr(#[case] args: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"))
        .args(args)
        .env_remove(DISABLE_LOCALIZATION_ENV)
        .env_remove("SPYCATCHER_HARNESS_LOCALE")
        .env_remove("SPYCATCHER_HARNESS_FALLBACK_LOCALE")
        .output()
        .expect("binary should execute");

    assert_eq!(
        output.status.code(),
        Some(2),
        "parse failures should use Clap's parse-failure status"
    );
    assert!(
        output.stdout.is_empty(),
        "parse errors should not write to stdout"
    );
    assert!(
        !output.stderr.is_empty(),
        "parse errors should write to stderr"
    );
}

struct StartupFailureScenario {
    config: Option<&'static str>,
    extra_args: &'static [&'static str],
    error_fragment: &'static str,
    has_valid_xdg_candidate: bool,
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
) -> TestResult<()> {
    assert_record_startup_failure(&scenario)
}

#[cfg(unix)]
#[test]
fn binary_unreadable_discovered_file_aborts_before_valid_xdg_candidate() -> TestResult<()> {
    assert_record_startup_failure(&StartupFailureScenario {
        config: Some("[cmds.record]\ncassette_name = \"unreadable\"\n"),
        extra_args: &[],
        error_fragment: "Permission denied",
        has_valid_xdg_candidate: true,
        unreadable_config: true,
    })
}

fn assert_record_startup_failure(scenario: &StartupFailureScenario) -> TestResult<()> {
    let temp_dir = tempfile::tempdir()?;
    write_startup_config_candidates(&temp_dir, scenario)?;
    let resources = RecordStartupResources::new()?;
    let output = run_record_startup(&temp_dir, scenario, &resources)?;
    assert_startup_failure(&output, &temp_dir, scenario, &resources)
}

fn write_startup_config_candidates(
    temp_dir: &TempDir,
    scenario: &StartupFailureScenario,
) -> TestResult<()> {
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
) -> TestResult<()> {
    if let Some(config) = scenario.config {
        directory.write(".spycatcher_harness.toml", config.as_bytes())?;
        make_primary_candidate_unreadable(directory, scenario.unreadable_config)?;
    }
    Ok(())
}

fn make_primary_candidate_unreadable(
    directory: &Dir,
    should_be_unreadable: bool,
) -> TestResult<()> {
    if should_be_unreadable {
        remove_read_permissions(directory)?;
    }
    Ok(())
}

#[cfg(unix)]
fn remove_read_permissions(directory: &Dir) -> TestResult<()> {
    let mut permissions = directory
        .metadata(".spycatcher_harness.toml")?
        .permissions();
    permissions.set_mode(0);
    directory.set_permissions(".spycatcher_harness.toml", permissions)
}

#[cfg(not(unix))]
fn remove_read_permissions(_directory: &Dir) -> TestResult<()> {
    Ok(())
}

fn write_valid_xdg_candidate(directory: &Dir) -> TestResult<()> {
    directory.create_dir_all("spycatcher_harness")?;
    let xdg_config = directory.open_dir("spycatcher_harness")?;
    xdg_config.write(
        "config.toml",
        b"[cmds.record]\ncassette_name = \"later-valid-candidate\"\n",
    )
}

struct RecordStartupResources {
    held_listener: TcpListener,
    listen_address: std::net::SocketAddr,
    upstream_listener: TcpListener,
    upstream_url: String,
}

impl RecordStartupResources {
    fn new() -> TestResult<Self> {
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

fn bind_nonblocking_listener() -> TestResult<TcpListener> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

fn run_record_startup(
    temp_dir: &TempDir,
    scenario: &StartupFailureScenario,
    resources: &RecordStartupResources,
) -> TestResult<std::process::Output> {
    const SECRET: &str = "startup-failure-secret-sentinel";
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
        .current_dir(temp_dir.path())
        .args(&args);
    preserve_coverage_profile(&mut command);
    command.output()
}

fn assert_startup_failure(
    output: &std::process::Output,
    temp_dir: &TempDir,
    scenario: &StartupFailureScenario,
    resources: &RecordStartupResources,
) -> TestResult<()> {
    assert_startup_error(output, scenario);
    let bound_address = resources.held_listener.local_addr()?;
    assert_record_resources_untouched(temp_dir, resources, bound_address);
    Ok(())
}

fn assert_startup_error(output: &std::process::Output, scenario: &StartupFailureScenario) {
    const SECRET: &str = "startup-failure-secret-sentinel";
    assert_eq!(
        output.status.code(),
        Some(1),
        "configuration and startup failures should exit with status 1"
    );
    assert!(
        output.stdout.is_empty(),
        "startup errors should not write to stdout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(scenario.error_fragment),
        "expected {:?} in startup error, got: {stderr}",
        scenario.error_fragment
    );
    assert!(
        !stderr.contains(SECRET),
        "startup errors must redact secrets"
    );
}

fn assert_record_resources_untouched(
    temp_dir: &TempDir,
    resources: &RecordStartupResources,
    bound_address: std::net::SocketAddr,
) {
    let cassette_dir = temp_dir.path().join("cassettes");
    assert!(
        !cassette_dir.exists(),
        "failed startup must not create cassettes"
    );
    assert_eq!(
        bound_address, resources.listen_address,
        "failed startup must leave the configured listener untouched"
    );
    assert!(
        matches!(resources.held_listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "failed startup must not send traffic to the listener"
    );
    assert!(
        matches!(resources.upstream_listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "failed startup must not contact the configured upstream"
    );
}

#[test]
fn binary_parse_failure_has_no_record_startup_side_effects() -> TestResult<()> {
    let temp_dir = tempfile::tempdir()?;
    let resources = RecordStartupResources::new()?;
    let output = run_record_parse_failure(&temp_dir, &resources)?;
    assert_parse_failure_has_no_side_effects(&output, &temp_dir, &resources);
    Ok(())
}

fn run_record_parse_failure(
    temp_dir: &TempDir,
    resources: &RecordStartupResources,
) -> TestResult<std::process::Output> {
    let listen_address = resources.listen_address.to_string();
    let listen_address_arg = listen_address.as_str();
    let mut command = Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"));
    command
        .env_clear()
        .env(
            "SPYCATCHER_HARNESS_CMDS_RECORD_UPSTREAM__BASE_URL",
            &resources.upstream_url,
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
        ]);
    preserve_coverage_profile(&mut command);
    command.output()
}

fn assert_parse_failure_has_no_side_effects(
    output: &std::process::Output,
    temp_dir: &TempDir,
    resources: &RecordStartupResources,
) {
    let cassette_dir = temp_dir.path().join("cassettes");
    assert_eq!(
        output.status.code(),
        Some(2),
        "unknown argument should fail parsing"
    );
    assert!(
        output.stdout.is_empty(),
        "parse errors should not write to stdout"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
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
        matches!(resources.upstream_listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "parse failure must not contact the configured upstream"
    );
    assert!(
        matches!(resources.held_listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
        "parse failure must not send traffic to the listener"
    );
}

fn preserve_coverage_profile(command: &mut Command) {
    if let Some(profile_file) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile_file);
    }
}
