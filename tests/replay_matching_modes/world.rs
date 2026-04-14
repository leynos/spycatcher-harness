//! Test world state for BDD scenarios.

use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;
use spycatcher_harness::cassette::{Cassette, InteractionPosition, ReplayMatchEngine};
use spycatcher_harness::config::MatchMode;

#[derive(Default, ScenarioState)]
pub struct MatchingWorld {
    /// Temporary storage for cassette before engine is created.
    /// Once engine is created, the cassette is owned by the engine.
    pub(super) cassette: Slot<Cassette>,
    pub(super) engine: Slot<ReplayMatchEngine>,
    pub(super) mode: Slot<MatchMode>,
    pub(super) matched_count: Slot<usize>,
    pub(super) matched_response_ids: Slot<Vec<String>>,
    pub(super) mismatch_count: Slot<usize>,
    pub(super) mismatch_position: Slot<InteractionPosition>,
    pub(super) mismatch_expected_hash: Slot<String>,
    pub(super) mismatch_observed_hash: Slot<String>,
    pub(super) mismatch_diff_summary: Slot<String>,
    pub(super) first_response_id: Slot<String>,
    pub(super) second_response_id: Slot<String>,
}

#[fixture]
pub fn matching_world() -> MatchingWorld {
    MatchingWorld::default()
}
