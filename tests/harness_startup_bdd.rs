//! BDD scenarios for harness startup and shutdown.
//!
//! Step definitions and scenario bindings for the feature file at
//! `tests/features/harness_startup.feature`.
use std::net::SocketAddr;
use std::sync::Arc;

use camino::Utf8PathBuf;
use eyre::WrapErr;
use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then, when};

use spycatcher_harness::config::ListenAddr;
use spycatcher_harness::{
    HarnessConfig, HarnessError, HarnessResult, RunningHarness, start_harness,
};

#[path = "support/bdd_fixtures.rs"]
mod bdd_fixtures;
#[path = "support/test_utils.rs"]
mod test_utils;

use bdd_fixtures::unique_cassette_name;
use test_utils::build_runtime;

// -- Helpers ----------------------------------------------------------------

fn make_record_config(prefix: &str, listen: Option<ListenAddr>) -> (HarnessConfig, Utf8PathBuf) {
    let cassette_name = unique_cassette_name(prefix);
    let cassette_dir = Utf8PathBuf::from("target/test-harness-bdd");
    let expected_cassette_path = cassette_dir.join(&cassette_name);
    let cfg = HarnessConfig {
        listen: listen.unwrap_or_else(|| ListenAddr::from(SocketAddr::from(([127, 0, 0, 1], 0)))),
        mode: spycatcher_harness::config::Mode::Record,
        cassette_dir,
        cassette_name,
        upstream: Some(spycatcher_harness::config::UpstreamConfig::default()),
        ..HarnessConfig::default()
    };
    (cfg, expected_cassette_path)
}

fn start_and_store_harness(harness_world: &HarnessWorld) -> eyre::Result<()> {
    let cfg = harness_world
        .config
        .take()
        .ok_or_else(|| eyre::eyre!("config must be set before starting"))?;
    let runtime = Arc::new(build_runtime().wrap_err("test runtime should build")?);
    harness_world.runtime.set(Arc::clone(&runtime));
    let result = runtime.block_on(start_harness(cfg));
    harness_world.start_result.set(result);
    Ok(())
}

// -- World fixture ----------------------------------------------------------

#[derive(Default, ScenarioState)]
struct HarnessWorld {
    runtime: Slot<Arc<tokio::runtime::Runtime>>,
    config: Slot<HarnessConfig>,
    start_result: Slot<HarnessResult<RunningHarness>>,
    shutdown_result: Slot<HarnessResult<()>>,
    expected_cassette_path: Slot<Utf8PathBuf>,
}

#[fixture]
fn harness_world() -> HarnessWorld {
    HarnessWorld::default()
}

// -- Given steps ------------------------------------------------------------

#[given("a valid harness configuration")]
fn a_valid_harness_configuration(harness_world: &HarnessWorld) {
    let (cfg, expected_cassette_path) = make_record_config("valid", None);
    harness_world
        .expected_cassette_path
        .set(expected_cassette_path);
    harness_world.config.set(cfg);
}

#[given("a harness configuration with an empty cassette name")]
fn a_harness_configuration_with_an_empty_cassette_name(harness_world: &HarnessWorld) {
    let cfg = HarnessConfig {
        cassette_name: String::new(),
        ..HarnessConfig::default()
    };
    harness_world.config.set(cfg);
}

#[given("the harness has been started")]
fn the_harness_has_been_started(harness_world: &HarnessWorld) -> eyre::Result<()> {
    start_and_store_harness(harness_world)
}

#[given("a harness configuration with listen address {addr}")]
fn a_harness_configuration_with_listen_address(harness_world: &HarnessWorld, addr: SocketAddr) {
    let (cfg, expected_cassette_path) = make_record_config("listen", Some(ListenAddr::from(addr)));
    harness_world
        .expected_cassette_path
        .set(expected_cassette_path);
    harness_world.config.set(cfg);
}

// -- When steps -------------------------------------------------------------

#[when("the harness is started")]
fn when_the_harness_is_started(harness_world: &HarnessWorld) -> eyre::Result<()> {
    start_and_store_harness(harness_world)
}

#[when("the harness is shut down")]
fn when_the_harness_is_shut_down(harness_world: &HarnessWorld) -> eyre::Result<()> {
    let harness = harness_world
        .start_result
        .take()
        .ok_or_else(|| eyre::eyre!("harness must be started before shutdown"))?
        .wrap_err("start_result must be Ok to shut down")?;
    let runtime = harness_world
        .runtime
        .with_ref(Arc::clone)
        .ok_or_else(|| eyre::eyre!("runtime must be set"))?;
    let result = runtime.block_on(harness.shutdown());
    harness_world.shutdown_result.set(result);
    Ok(())
}

// -- Then steps -------------------------------------------------------------

#[then("the harness is running")]
fn the_harness_is_running(harness_world: &HarnessWorld) -> eyre::Result<()> {
    let is_ok = harness_world
        .start_result
        .with_ref(Result::is_ok)
        .ok_or_else(|| eyre::eyre!("start_result must be set"))?;
    eyre::ensure!(is_ok, "expected harness to be running");
    Ok(())
}

#[then("the cassette path matches the configured directory and name")]
fn the_cassette_path_matches_the_configured_directory_and_name(
    harness_world: &HarnessWorld,
) -> eyre::Result<()> {
    let path = harness_world
        .start_result
        .with_ref(|result| {
            result
                .as_ref()
                .map(|harness| harness.cassette_path.clone())
                .map_err(ToString::to_string)
        })
        .ok_or_else(|| eyre::eyre!("start_result must be set"))?
        .map_err(|error| eyre::eyre!("harness should be running: {error}"))?;
    let expected = harness_world
        .expected_cassette_path
        .with_ref(Utf8PathBuf::clone)
        .ok_or_else(|| eyre::eyre!("expected_cassette_path must be set"))?;
    eyre::ensure!(
        path == expected,
        "cassette path should match the configured test directory and cassette name: got {path}, expected {expected}",
    );
    Ok(())
}

#[then("the startup fails with an invalid configuration error")]
fn the_startup_fails_with_an_invalid_configuration_error(
    harness_world: &HarnessWorld,
) -> eyre::Result<()> {
    let is_invalid_config = harness_world
        .start_result
        .with_ref(|r| matches!(r, Err(HarnessError::InvalidConfig { .. })))
        .ok_or_else(|| eyre::eyre!("start_result must be set"))?;
    eyre::ensure!(is_invalid_config, "expected InvalidConfig error");
    Ok(())
}

#[then("the error message mentions the cassette name")]
fn the_error_message_mentions_the_cassette_name(harness_world: &HarnessWorld) -> eyre::Result<()> {
    let msg = harness_world
        .start_result
        .with_ref(|result| result.as_ref().err().map(ToString::to_string))
        .ok_or_else(|| eyre::eyre!("start_result must be set"))?
        .ok_or_else(|| eyre::eyre!("expected an error"))?;
    eyre::ensure!(
        msg.contains("cassette name"),
        "error message should mention 'cassette name', got: {msg}",
    );
    Ok(())
}

#[then("the shutdown succeeds")]
fn the_shutdown_succeeds(harness_world: &HarnessWorld) -> eyre::Result<()> {
    let is_ok = harness_world
        .shutdown_result
        .with_ref(Result::is_ok)
        .ok_or_else(|| eyre::eyre!("shutdown_result must be set"))?;
    eyre::ensure!(is_ok, "expected shutdown to succeed");
    Ok(())
}

#[then("the harness address is bound on {ip}")]
fn the_harness_address_is_bound_on(
    harness_world: &HarnessWorld,
    ip: std::net::IpAddr,
) -> eyre::Result<()> {
    let actual = harness_world
        .start_result
        .with_ref(|result| {
            result
                .as_ref()
                .map(|harness| harness.addr)
                .map_err(ToString::to_string)
        })
        .ok_or_else(|| eyre::eyre!("start_result must be set"))?
        .map_err(|error| eyre::eyre!("harness should be running: {error}"))?;
    eyre::ensure!(
        actual.ip() == ip,
        "expected address {ip}, got {}",
        actual.ip()
    );
    Ok(())
}

#[then("the harness port is non-zero")]
fn the_harness_port_is_non_zero(harness_world: &HarnessWorld) -> eyre::Result<()> {
    let actual = harness_world
        .start_result
        .with_ref(|result| {
            result
                .as_ref()
                .map(|harness| harness.addr)
                .map_err(ToString::to_string)
        })
        .ok_or_else(|| eyre::eyre!("start_result must be set"))?
        .map_err(|error| eyre::eyre!("harness should be running: {error}"))?;
    eyre::ensure!(actual.port() != 0, "expected OS-assigned port");
    Ok(())
}

// -- Scenario bindings ------------------------------------------------------

#[scenario(
    path = "tests/features/harness_startup.feature",
    name = "Start harness with valid configuration"
)]
fn start_harness_with_valid_configuration(harness_world: HarnessWorld) {
    let _ = harness_world;
}

#[scenario(
    path = "tests/features/harness_startup.feature",
    name = "Start harness with empty cassette name fails"
)]
fn start_harness_with_empty_cassette_name_fails(harness_world: HarnessWorld) {
    let _ = harness_world;
}

#[scenario(
    path = "tests/features/harness_startup.feature",
    name = "Shutdown a running harness"
)]
fn shutdown_a_running_harness(harness_world: HarnessWorld) {
    let _ = harness_world;
}

#[scenario(
    path = "tests/features/harness_startup.feature",
    name = "Start harness binds an OS-selected localhost port"
)]
fn start_harness_binds_an_os_selected_localhost_port(harness_world: HarnessWorld) {
    let _ = harness_world;
}
