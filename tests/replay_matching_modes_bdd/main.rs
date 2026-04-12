//! BDD scenarios for replay matching modes.
//!
//! Step definitions and scenario bindings for the feature file at
//! `tests/features/replay_matching_modes.feature`.
#![expect(
    clippy::expect_used,
    reason = "BDD step functions use expect for step precondition enforcement"
)]

use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, scenario};
use spycatcher_harness::cassette::{Cassette, ReplayMatchEngine};
use spycatcher_harness::config::MatchMode;

mod helpers;
mod steps;

#[derive(Default, ScenarioState)]
struct MatchingWorld {
    /// Temporary storage for cassette before engine is created.
    /// Once engine is created, the cassette is owned by the engine.
    cassette: Slot<Cassette>,
    engine: Slot<ReplayMatchEngine>,
    mode: Slot<MatchMode>,
    matched_count: Slot<usize>,
    mismatch_count: Slot<usize>,
    mismatch_interaction_id: Slot<usize>,
    mismatch_expected_hash: Slot<String>,
    mismatch_observed_hash: Slot<String>,
    mismatch_diff_summary: Slot<String>,
    first_response_id: Slot<String>,
    second_response_id: Slot<String>,
}

#[fixture]
fn matching_world() -> MatchingWorld {
    MatchingWorld::default()
}

#[scenario(
    path = "tests/features/replay_matching_modes.feature",
    name = "Sequential strict mode serves interactions in order"
)]
fn sequential_strict_mode_serves_interactions_in_order(matching_world: MatchingWorld) {
    let _ = matching_world;
}

#[scenario(
    path = "tests/features/replay_matching_modes.feature",
    name = "Sequential strict mode rejects a mismatched request"
)]
fn sequential_strict_mode_rejects_a_mismatched_request(matching_world: MatchingWorld) {
    let _ = matching_world;
}

#[scenario(
    path = "tests/features/replay_matching_modes.feature",
    name = "Keyed mode permits out-of-order requests"
)]
fn keyed_mode_permits_out_of_order_requests(matching_world: MatchingWorld) {
    let _ = matching_world;
}

#[scenario(
    path = "tests/features/replay_matching_modes.feature",
    name = "Keyed mode consumes duplicate hashes in recorded order"
)]
fn keyed_mode_consumes_duplicate_hashes_in_recorded_order(matching_world: MatchingWorld) {
    let _ = matching_world;
}

#[scenario(
    path = "tests/features/replay_matching_modes.feature",
    name = "Replay engine rejects requests after cassette exhaustion"
)]
fn replay_engine_rejects_requests_after_cassette_exhaustion(matching_world: MatchingWorld) {
    let _ = matching_world;
}
