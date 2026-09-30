//! Configuration discovery and command-scope regression checks.

use rstest::rstest;

#[path = "support/isolated_cli_process.rs"]
mod isolated_cli_process;

use isolated_cli_process::{ModeSnapshot, ProbeEnvelope, ProbeOutcome, ProbeRequest};

fn load_config(
    args: &[&str],
    config_file: Option<&str>,
    env_vars: &[(&str, &str)],
) -> Result<ProbeEnvelope, String> {
    let request = ProbeRequest::LoadConfig {
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
    };
    isolated_cli_process::run_probe(&request, config_file, env_vars)
}

#[rstest]
fn child_stderr_tail_is_bounded_and_redacts_environment_values() {
    const SECRET: &str = "test-only-sensitive-sentinel";
    let stderr = format!("{}🙂{SECRET}{}", "x".repeat(2071), "y".repeat(2022));
    let tail =
        isolated_cli_process::bounded_stderr_tail(stderr.as_bytes(), &[("TEST_API_KEY", SECRET)]);

    assert!(tail.len() <= 2048, "stderr context exceeded its byte limit");
    assert!(
        tail.starts_with("…"),
        "truncated stderr should keep its marker"
    );
    assert!(
        !tail.contains("sentinel"),
        "secret appeared in child stderr context"
    );
}

#[rstest]
fn malformed_configuration_is_reported_instead_of_using_defaults() {
    let envelope = load_config(
        &["spycatcher-harness", "replay"],
        Some("[cmds.replay\ncassette_name = \"broken\"\n"),
        &[],
    )
    .expect("probe should run");

    let ProbeOutcome::ConfigError { kind, message } = envelope.outcome else {
        panic!("malformed configuration must fail instead of using defaults");
    };
    assert_eq!(kind, "merge");
    assert!(
        message.contains("config"),
        "expected actionable parse error"
    );
}

#[rstest]
fn missing_inheritance_target_is_reported_instead_of_using_defaults() {
    let envelope = load_config(
        &["spycatcher-harness", "replay"],
        Some("extends = \"missing-parent.toml\"\n[cmds.replay]\n"),
        &[],
    )
    .expect("probe should run");

    let ProbeOutcome::ConfigError { kind, message } = envelope.outcome else {
        panic!("missing inheritance target must fail instead of using defaults");
    };
    assert_eq!(kind, "merge");
    assert!(
        message.contains("missing-parent.toml"),
        "expected error to identify the missing parent configuration"
    );
}

#[rstest]
#[case("replay", ModeSnapshot::Replay)]
#[case("verify", ModeSnapshot::Verify)]
fn record_upstream_settings_do_not_leak_into_other_commands(
    #[case] subcommand: &str,
    #[case] expected_mode: ModeSnapshot,
) {
    let config = concat!(
        "[cmds.record.upstream]\n",
        "kind = \"openrouter\"\n",
        "base_url = \"https://example.invalid/api\"\n",
        "api_key_env = \"TEST_API_KEY\"\n",
    );
    let envelope = load_config(&["spycatcher-harness", subcommand], Some(config), &[])
        .expect("probe should run");

    let ProbeOutcome::Loaded(loaded) = envelope.outcome else {
        panic!("config should load for the selected command");
    };
    assert_eq!(loaded.mode, expected_mode);
    assert!(loaded.upstream.is_none());
}

#[rstest]
fn secret_environment_values_are_absent_from_config_diagnostics_and_tracing() {
    const SECRET: &str = "test-only-sensitive-sentinel";
    let config = concat!(
        "[cmds.record.upstream]\n",
        "kind = \"openrouter\"\n",
        "base_url = \"https://example.invalid/api\"\n",
        "api_key_env = \"TEST_API_KEY\"\n",
    );
    let envelope = load_config(
        &["spycatcher-harness", "record"],
        Some(config),
        &[("TEST_API_KEY", SECRET)],
    )
    .expect("probe should run");

    let ProbeOutcome::Loaded(loaded_config) = &envelope.outcome else {
        panic!("record config should load with an API-key environment name");
    };
    let Some(upstream) = &loaded_config.upstream else {
        panic!("record config should retain its upstream settings");
    };
    assert_eq!(upstream.api_key_env, "TEST_API_KEY");

    let outcome = format!("{:?}", envelope.outcome);
    assert!(
        envelope.tracing_output.contains("handle_probe"),
        "expected captured tracing to include the probe span"
    );
    assert!(
        !outcome.contains(SECRET),
        "secret appeared in config output"
    );
    assert!(
        !envelope.tracing_output.contains(SECRET),
        "secret appeared in captured tracing"
    );
}
