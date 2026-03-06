//! Behavioural tests for layered CLI configuration loading.

use std::cell::RefCell;

use ortho_config::figment;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then, when};

use spycatcher_harness::cli::load_subcommand_config_from_iter;
use spycatcher_harness::{HarnessConfig, config};

#[derive(Default, ScenarioState)]
struct CliLayeringWorld {
    argv: Slot<Vec<String>>,
    config_file: Slot<String>,
    env_vars: Slot<Vec<(String, String)>>,
    result: Slot<Result<HarnessConfig, String>>,
}

#[fixture]
fn cli_layering_world() -> CliLayeringWorld {
    CliLayeringWorld::default()
}

fn set_command(cli_layering_world: &CliLayeringWorld, args: Vec<String>) {
    cli_layering_world.argv.set(args);
}

fn append_config(cli_layering_world: &CliLayeringWorld, fragment: &str) {
    let mut current = cli_layering_world.config_file.take().unwrap_or_default();
    current.push_str(fragment);
    cli_layering_world.config_file.set(current);
}

fn push_env(cli_layering_world: &CliLayeringWorld, key: &str, value: &str) {
    let mut vars = cli_layering_world.env_vars.take().unwrap_or_default();
    vars.push((String::from(key), String::from(value)));
    cli_layering_world.env_vars.set(vars);
}

fn trim_surrounding_quotes(value: &str) -> String {
    value.trim_matches('"').to_owned()
}

#[given("a replay command with cassette name {cassette_name}")]
fn replay_command_with_cassette_name(cli_layering_world: &CliLayeringWorld, cassette_name: String) {
    let cassette_name_value = trim_surrounding_quotes(&cassette_name);
    set_command(
        cli_layering_world,
        vec![
            String::from("spycatcher-harness"),
            String::from("replay"),
            String::from("--cassette-name"),
            cassette_name_value,
        ],
    );
}

#[given("a replay command with no CLI overrides")]
fn replay_command_with_no_cli_overrides(cli_layering_world: &CliLayeringWorld) {
    set_command(
        cli_layering_world,
        vec![String::from("spycatcher-harness"), String::from("replay")],
    );
}

#[given("a record command with no CLI overrides")]
fn record_command_with_no_cli_overrides(cli_layering_world: &CliLayeringWorld) {
    set_command(
        cli_layering_world,
        vec![String::from("spycatcher-harness"), String::from("record")],
    );
}

#[given("a verify command with no CLI overrides")]
fn verify_command_with_no_cli_overrides(cli_layering_world: &CliLayeringWorld) {
    set_command(
        cli_layering_world,
        vec![String::from("spycatcher-harness"), String::from("verify")],
    );
}

#[given("config file sets replay cassette name to {cassette_name}")]
fn config_sets_replay_cassette_name(cli_layering_world: &CliLayeringWorld, cassette_name: String) {
    let cassette_name_value = trim_surrounding_quotes(&cassette_name);
    append_config(
        cli_layering_world,
        &format!("[cmds.replay]\ncassette_name = \"{cassette_name_value}\"\n"),
    );
}

#[given("config file sets verify cassette name to {cassette_name}")]
fn config_sets_verify_cassette_name(cli_layering_world: &CliLayeringWorld, cassette_name: String) {
    let cassette_name_value = trim_surrounding_quotes(&cassette_name);
    append_config(
        cli_layering_world,
        &format!("[cmds.verify]\ncassette_name = \"{cassette_name_value}\"\n"),
    );
}

#[given("config file sets record upstream base URL to {base_url}")]
fn config_sets_record_upstream_base_url(cli_layering_world: &CliLayeringWorld, base_url: String) {
    let base_url_value = trim_surrounding_quotes(&base_url);
    append_config(
        cli_layering_world,
        &format!(
            "[cmds.record]\n\
             cassette_name = \"record_cfg\"\n\
             [cmds.record.upstream]\n\
             kind = \"openrouter\"\n\
             base_url = \"{base_url_value}\"\n\
             api_key_env = \"TEST_KEY\"\n"
        ),
    );
}

#[given("environment sets replay cassette name to {cassette_name}")]
fn env_sets_replay_cassette_name(cli_layering_world: &CliLayeringWorld, cassette_name: String) {
    let cassette_name_value = trim_surrounding_quotes(&cassette_name);
    push_env(
        cli_layering_world,
        "SPYCATCHER_HARNESS_CMDS_REPLAY_CASSETTE_NAME",
        &cassette_name_value,
    );
}

#[given("environment sets replay listen to invalid value {listen}")]
fn env_sets_invalid_replay_listen(cli_layering_world: &CliLayeringWorld, listen: String) {
    let listen_value = trim_surrounding_quotes(&listen);
    push_env(
        cli_layering_world,
        "SPYCATCHER_HARNESS_CMDS_REPLAY_LISTEN",
        &listen_value,
    );
}

#[when("the layered command configuration is loaded")]
fn load_layered_config(cli_layering_world: &CliLayeringWorld) {
    let argv = cli_layering_world.argv.take().unwrap_or_default();
    let config_file = cli_layering_world.config_file.take().unwrap_or_default();
    let env_vars = cli_layering_world.env_vars.take().unwrap_or_default();

    let loaded = RefCell::new(None);
    #[expect(
        clippy::result_large_err,
        reason = "figment::Jail callback requires figment::error::Result"
    )]
    let jail_result = figment::Jail::try_with(|jail| {
        if !config_file.is_empty() {
            jail.create_file(".spycatcher_harness.toml", &config_file)?;
        }

        for (key, value) in &env_vars {
            jail.set_env(key, value);
        }

        let result =
            load_subcommand_config_from_iter(argv.clone()).map_err(|error| error.to_string());
        loaded.replace(Some(result));
        Ok(())
    });

    let result = match jail_result {
        Ok(()) => loaded
            .into_inner()
            .unwrap_or_else(|| Err(String::from("loader result missing"))),
        Err(error) => Err(error.to_string()),
    };

    cli_layering_world.result.set(result);
}

#[then("replay cassette name is {cassette_name}")]
fn replay_cassette_name_is(cli_layering_world: &CliLayeringWorld, cassette_name: String) {
    let cassette_name_value = trim_surrounding_quotes(&cassette_name);
    let outcome = cli_layering_world
        .result
        .with_ref(Clone::clone)
        .unwrap_or_else(|| Err(String::from("result slot missing")));
    let loaded_config = match outcome {
        Ok(config) => config,
        Err(error) => panic!("expected replay configuration, load failed: {error}"),
    };
    assert_eq!(loaded_config.cassette_name, cassette_name_value);
    assert_eq!(loaded_config.mode, config::Mode::Replay);
}

#[then("verify cassette name is {cassette_name}")]
fn verify_cassette_name_is(cli_layering_world: &CliLayeringWorld, cassette_name: String) {
    let cassette_name_value = trim_surrounding_quotes(&cassette_name);
    let outcome = cli_layering_world
        .result
        .with_ref(Clone::clone)
        .unwrap_or_else(|| Err(String::from("result slot missing")));
    let loaded_config = match outcome {
        Ok(config) => config,
        Err(error) => panic!("expected verify configuration, load failed: {error}"),
    };
    assert_eq!(loaded_config.cassette_name, cassette_name_value);
    assert_eq!(loaded_config.mode, config::Mode::Verify);
}

#[then("record upstream base URL is {base_url}")]
fn record_upstream_base_url_is(cli_layering_world: &CliLayeringWorld, base_url: String) {
    let base_url_value = trim_surrounding_quotes(&base_url);
    let outcome = cli_layering_world
        .result
        .with_ref(Clone::clone)
        .unwrap_or_else(|| Err(String::from("result slot missing")));
    let loaded_config = match outcome {
        Ok(config) => config,
        Err(error) => panic!("expected record configuration, load failed: {error}"),
    };
    let Some(upstream) = loaded_config.upstream else {
        panic!("expected record upstream");
    };
    assert_eq!(loaded_config.mode, config::Mode::Record);
    assert_eq!(upstream.base_url, base_url_value);
}

#[then("command configuration loading fails with error containing {error_marker}")]
fn command_loading_fails(cli_layering_world: &CliLayeringWorld, error_marker: String) {
    let outcome = cli_layering_world
        .result
        .with_ref(Clone::clone)
        .unwrap_or_else(|| Err(String::from("result slot missing")));
    match outcome {
        Ok(config) => {
            panic!("expected command configuration loading to fail, but succeeded with: {config:?}")
        }
        Err(error) => assert!(
            error.contains(&error_marker),
            "expected error to contain marker {error_marker:?}, but was: {error}",
        ),
    }
}

#[scenario(
    path = "tests/features/harness_cli_layering.feature",
    name = "Replay precedence favours CLI over env and file"
)]
fn replay_precedence_favours_cli_over_env_and_file(cli_layering_world: CliLayeringWorld) {
    let _ = cli_layering_world;
}

#[scenario(
    path = "tests/features/harness_cli_layering.feature",
    name = "Record command merges cmds.record upstream values"
)]
fn record_command_merges_cmds_record_upstream_values(cli_layering_world: CliLayeringWorld) {
    let _ = cli_layering_world;
}

#[scenario(
    path = "tests/features/harness_cli_layering.feature",
    name = "Verify command merges cmds.verify cassette value"
)]
fn verify_command_merges_cmds_verify_cassette_value(cli_layering_world: CliLayeringWorld) {
    let _ = cli_layering_world;
}

#[scenario(
    path = "tests/features/harness_cli_layering.feature",
    name = "Invalid environment value fails loading"
)]
fn invalid_environment_value_fails_loading(cli_layering_world: CliLayeringWorld) {
    let _ = cli_layering_world;
}
