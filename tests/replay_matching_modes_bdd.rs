//! BDD scenarios for replay matching modes.
//!
//! Step definitions and scenario bindings for the feature file at
//! `tests/features/replay_matching_modes.feature`.

mod replay_matching_modes {
    //! Internal modules for BDD test implementation.

    pub(super) mod fixtures;
    pub(super) mod helpers;
    pub(super) mod steps;
    pub(super) mod world;
}

use replay_matching_modes::world::{MatchingWorld, matching_world};
use rstest_bdd_macros::scenario;

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
