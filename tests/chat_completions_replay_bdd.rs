//! BDD scenarios for non-stream chat completions replay.

use rstest::fixture;
use rstest_bdd_macros::scenario;

mod chat_completions_replay;
#[expect(
    dead_code,
    reason = "the replay suite reuses only part of the record-mode integration helper module"
)]
#[path = "record_mode_proxying/helpers.rs"]
mod record_helpers;

use chat_completions_replay::world::ReplayWorld;

#[fixture]
fn replay_world() -> ReplayWorld {
    ReplayWorld::default()
}

#[scenario(
    path = "tests/features/chat_completions_replay.feature",
    name = "Non-stream replay returns the recorded response without upstream access"
)]
fn non_stream_replay_returns_recorded_response_without_upstream_access(replay_world: ReplayWorld) {
    let _ = replay_world;
}

#[scenario(
    path = "tests/features/chat_completions_replay.feature",
    name = "Replay mismatch returns a conflict diagnostic"
)]
fn replay_mismatch_returns_conflict_diagnostic(replay_world: ReplayWorld) {
    let _ = replay_world;
}

#[scenario(
    path = "tests/features/chat_completions_replay.feature",
    name = "Replay rejects streaming requests"
)]
fn replay_rejects_streaming_requests(replay_world: ReplayWorld) {
    let _ = replay_world;
}
