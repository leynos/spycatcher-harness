//! BDD scenarios for harness startup and shutdown.
//!
//! Step definitions and scenario bindings for the feature file at
//! `tests/features/harness_startup.feature`.
#![expect(
    clippy::expect_used,
    reason = "BDD step functions use expect for step precondition enforcement"
)]
#![allow(
    unused_variables,
    reason = "rstest-bdd scenario macro introduces variables consumed by fixture resolution"
)]

use std::net::SocketAddr;

use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then, when};

use spycatcher_harness::config::ListenAddr;
use spycatcher_harness::{
    HarnessConfig, HarnessError, HarnessResult, RunningHarness, start_harness,
};

// -- Helpers ----------------------------------------------------------------

/// Builds a single-threaded Tokio runtime for use in synchronous BDD
/// step functions.
fn build_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
}

// -- World fixture ----------------------------------------------------------

#[derive(Default, ScenarioState)]
struct HarnessWorld {
    config: Slot<HarnessConfig>,
    start_result: Slot<HarnessResult<RunningHarness>>,
    shutdown_result: Slot<HarnessResult<()>>,
}

#[fixture]
fn harness_world() -> HarnessWorld {
    HarnessWorld::default()
}

// -- Given steps ------------------------------------------------------------

#[given("a valid harness configuration")]
fn a_valid_harness_configuration(harness_world: &HarnessWorld) {
    harness_world.config.set(HarnessConfig::default());
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
fn the_harness_has_been_started(harness_world: &HarnessWorld) {
    let cfg = harness_world
        .config
        .take()
        .expect("config must be set before starting");
    let rt = build_runtime();
    let result = rt.block_on(start_harness(cfg));
    harness_world.start_result.set(result);
}

#[given("a harness configuration with listen address {addr}")]
fn a_harness_configuration_with_listen_address(harness_world: &HarnessWorld, addr: SocketAddr) {
    let cfg = HarnessConfig {
        listen: ListenAddr::from(addr),
        ..HarnessConfig::default()
    };
    harness_world.config.set(cfg);
}

// -- When steps -------------------------------------------------------------

#[when("the harness is started")]
fn when_the_harness_is_started(harness_world: &HarnessWorld) {
    let cfg = harness_world
        .config
        .take()
        .expect("config must be set before starting");
    let rt = build_runtime();
    let result = rt.block_on(start_harness(cfg));
    harness_world.start_result.set(result);
}

#[when("the harness is shut down")]
fn when_the_harness_is_shut_down(harness_world: &HarnessWorld) {
    let harness = harness_world
        .start_result
        .take()
        .expect("harness must be started before shutdown")
        .expect("start_result must be Ok to shut down");
    let rt = build_runtime();
    let result = rt.block_on(harness.shutdown());
    harness_world.shutdown_result.set(result);
}

// -- Then steps -------------------------------------------------------------

#[then("the harness is running")]
fn the_harness_is_running(harness_world: &HarnessWorld) {
    let is_ok = harness_world
        .start_result
        .with_ref(Result::is_ok)
        .expect("start_result must be set");
    assert!(is_ok, "expected harness to be running");
}

#[then("the cassette path matches the configured directory and name")]
fn the_cassette_path_matches_the_configured_directory_and_name(harness_world: &HarnessWorld) {
    let path = harness_world
        .start_result
        .with_ref(|r| {
            r.as_ref()
                .expect("harness should be running")
                .cassette_path
                .clone()
        })
        .expect("start_result must be set");
    assert_eq!(
        path.as_str(),
        "fixtures/llm/default",
        "cassette path should join default dir and name",
    );
}

#[then("the startup fails with an invalid configuration error")]
fn the_startup_fails_with_an_invalid_configuration_error(harness_world: &HarnessWorld) {
    let is_invalid_config = harness_world
        .start_result
        .with_ref(|r| matches!(r, Err(HarnessError::InvalidConfig { .. })))
        .expect("start_result must be set");
    assert!(is_invalid_config, "expected InvalidConfig error");
}

#[then("the error message mentions the cassette name")]
fn the_error_message_mentions_the_cassette_name(harness_world: &HarnessWorld) {
    let msg = harness_world
        .start_result
        .with_ref(|r| format!("{}", r.as_ref().expect_err("expected an error")))
        .expect("start_result must be set");
    assert!(
        msg.contains("cassette name"),
        "error message should mention 'cassette name', got: {msg}",
    );
}

#[then("the shutdown succeeds")]
fn the_shutdown_succeeds(harness_world: &HarnessWorld) {
    let is_ok = harness_world
        .shutdown_result
        .with_ref(Result::is_ok)
        .expect("shutdown_result must be set");
    assert!(is_ok, "expected shutdown to succeed");
}

#[then("the harness address is {addr}")]
fn the_harness_address_is(harness_world: &HarnessWorld, addr: SocketAddr) {
    let actual = harness_world
        .start_result
        .with_ref(|r| r.as_ref().expect("harness should be running").addr)
        .expect("start_result must be set");
    assert_eq!(actual, addr);
}

// -- Scenario bindings ------------------------------------------------------

#[scenario(
    path = "tests/features/harness_startup.feature",
    name = "Start harness with valid configuration"
)]
fn start_harness_with_valid_configuration(harness_world: HarnessWorld) {}

#[scenario(
    path = "tests/features/harness_startup.feature",
    name = "Start harness with empty cassette name fails"
)]
fn start_harness_with_empty_cassette_name_fails(harness_world: HarnessWorld) {}

#[scenario(
    path = "tests/features/harness_startup.feature",
    name = "Shutdown a running harness"
)]
fn shutdown_a_running_harness(harness_world: HarnessWorld) {}

#[scenario(
    path = "tests/features/harness_startup.feature",
    name = "Start harness preserves listen address"
)]
fn start_harness_preserves_listen_address(harness_world: HarnessWorld) {}
