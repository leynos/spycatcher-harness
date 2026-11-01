//! BDD scenarios for replay cassette schema validation.
//!
//! Step definitions and scenario bindings for the feature file at
//! `tests/features/cassette_schema_versioning.feature`.
use camino::Utf8PathBuf;
use cap_std::{ambient_authority, fs_utf8::Dir};
use eyre::WrapErr;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then, when};
use spycatcher_harness::cassette::Cassette;
use spycatcher_harness::{
    HarnessConfig, HarnessError, HarnessResult, RunningHarness, start_harness,
};

#[path = "support/bdd_fixtures.rs"]
mod bdd_fixtures;
#[path = "support/test_utils.rs"]
mod test_utils;

use bdd_fixtures::unique_cassette_name;
use test_utils::build_runtime;

#[derive(Default, ScenarioState)]
struct CassetteWorld {
    config: Slot<HarnessConfig>,
    start_result: Slot<HarnessResult<RunningHarness>>,
    expected_cassette_path: Slot<Utf8PathBuf>,
}

#[fixture]
fn cassette_world() -> CassetteWorld {
    // Keep the body multi-line: rstest re-emits it as a block expression,
    // so a collapsed one-line body trips `unused_braces`.
    CassetteWorld::default()
}

fn setup_replay_config(
    cassette_world: &CassetteWorld,
    name_prefix: &str,
    cassette_json: &serde_json::Value,
) -> eyre::Result<()> {
    let cassette_name = unique_cassette_name(name_prefix);
    let cassette_dir = Utf8PathBuf::from("target/test-replay-bdd");
    let cassette_path = cassette_dir.join(&cassette_name);
    write_cassette(&cassette_path, cassette_json)?;
    cassette_world.expected_cassette_path.set(cassette_path);
    cassette_world.config.set(HarnessConfig {
        mode: spycatcher_harness::config::Mode::Replay,
        cassette_dir,
        cassette_name,
        ..HarnessConfig::default()
    });
    Ok(())
}

#[given("a replay configuration with a supported cassette")]
fn a_replay_configuration_with_a_supported_cassette(
    cassette_world: &CassetteWorld,
) -> eyre::Result<()> {
    let cassette_json =
        serde_json::to_value(Cassette::new()).wrap_err("empty cassette should serialize")?;
    setup_replay_config(cassette_world, "supported", &cassette_json)
}

#[given("a replay configuration with cassette format version {version:u32}")]
fn a_replay_configuration_with_cassette_format_version(
    cassette_world: &CassetteWorld,
    version: u32,
) -> eyre::Result<()> {
    setup_replay_config(
        cassette_world,
        "unsupported",
        &serde_json::json!({
            "format_version": version,
            "interactions": [],
        }),
    )
}

#[when("the replay harness is started")]
fn the_replay_harness_is_started(cassette_world: &CassetteWorld) -> eyre::Result<()> {
    let cfg = cassette_world
        .config
        .take()
        .ok_or_else(|| eyre::eyre!("config must be set before starting"))?;
    let rt = build_runtime().wrap_err("test runtime should build")?;
    let result = rt.block_on(start_harness(cfg));
    cassette_world.start_result.set(result);
    Ok(())
}

#[then("the replay harness is running")]
fn the_replay_harness_is_running(cassette_world: &CassetteWorld) -> eyre::Result<()> {
    let is_ok = cassette_world
        .start_result
        .with_ref(Result::is_ok)
        .ok_or_else(|| eyre::eyre!("start_result must be set"))?;
    eyre::ensure!(is_ok, "expected replay harness to be running");
    Ok(())
}

#[then("the replay cassette path matches the configured directory and name")]
fn the_replay_cassette_path_matches_the_configured_directory_and_name(
    cassette_world: &CassetteWorld,
) -> eyre::Result<()> {
    let path = cassette_world
        .start_result
        .with_ref(|result| {
            result
                .as_ref()
                .map(|harness| harness.cassette_path.clone())
                .map_err(ToString::to_string)
        })
        .ok_or_else(|| eyre::eyre!("start_result must be set"))?
        .map_err(|error| eyre::eyre!("replay harness should be running: {error}"))?;
    let expected = cassette_world
        .expected_cassette_path
        .with_ref(Utf8PathBuf::clone)
        .ok_or_else(|| eyre::eyre!("expected_cassette_path must be set"))?;
    eyre::ensure!(
        path == expected,
        "cassette path mismatch: actual {path}, expected {expected}",
    );
    Ok(())
}

#[then("startup fails with an unsupported cassette format error")]
fn startup_fails_with_an_unsupported_cassette_format_error(
    cassette_world: &CassetteWorld,
) -> eyre::Result<()> {
    let is_unsupported = cassette_world
        .start_result
        .with_ref(|result| {
            matches!(
                result,
                Err(HarnessError::UnsupportedCassetteFormatVersion { .. })
            )
        })
        .ok_or_else(|| eyre::eyre!("start_result must be set"))?;
    eyre::ensure!(is_unsupported, "expected unsupported cassette format error");
    Ok(())
}

#[then("the error mentions format version {version:u32}")]
fn the_error_mentions_format_version(
    cassette_world: &CassetteWorld,
    version: u32,
) -> eyre::Result<()> {
    let message = cassette_world
        .start_result
        .with_ref(|result| result.as_ref().err().map(ToString::to_string))
        .ok_or_else(|| eyre::eyre!("start_result must be set"))?
        .ok_or_else(|| eyre::eyre!("expected an error"))?;
    eyre::ensure!(
        message.contains(&version.to_string()),
        "error should mention version {version}, got: {message}",
    );
    Ok(())
}

#[scenario(
    path = "tests/features/cassette_schema_versioning.feature",
    name = "Replay startup succeeds with a supported cassette"
)]
fn replay_startup_succeeds_with_a_supported_cassette(cassette_world: CassetteWorld) {
    let _ = cassette_world;
}

#[scenario(
    path = "tests/features/cassette_schema_versioning.feature",
    name = "Replay startup rejects an unsupported cassette version"
)]
fn replay_startup_rejects_an_unsupported_cassette_version(cassette_world: CassetteWorld) {
    let _ = cassette_world;
}

fn write_cassette(cassette_path: &Utf8PathBuf, value: &serde_json::Value) -> eyre::Result<()> {
    let root_dir =
        Dir::open_ambient_dir(".", ambient_authority()).wrap_err("ambient root should open")?;
    let parent_dir = match cassette_path.parent() {
        Some(parent_path) => {
            root_dir
                .create_dir_all(parent_path)
                .wrap_err("cassette parent directory should be created")?;
            root_dir
                .open_dir(parent_path)
                .wrap_err("cassette parent directory should open")?
        }
        None => root_dir,
    };
    let file_name = cassette_path
        .file_name()
        .ok_or_else(|| eyre::eyre!("cassette path should contain a file name"))?;
    let file = parent_dir
        .create(file_name)
        .wrap_err("cassette file should open")?;
    serde_json::to_writer_pretty(file, value).wrap_err("cassette JSON should write")?;
    Ok(())
}
