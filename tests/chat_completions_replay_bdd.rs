//! BDD scenarios for chat completions replay.

use rstest::fixture;
use rstest_bdd_macros::scenario;

mod chat_completions_replay;
#[path = "chat_completions_replay/record_helpers.rs"]
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
    name = "Replay emits a recorded OpenRouter stream including comment frames"
)]
fn replay_emits_a_recorded_open_router_stream_including_comment_frames(replay_world: ReplayWorld) {
    let _ = replay_world;
}

#[scenario(
    path = "tests/features/chat_completions_replay.feature",
    name = "Replay rejects streaming requests when the cassette has no recording"
)]
fn replay_rejects_streaming_requests_when_the_cassette_has_no_recording(replay_world: ReplayWorld) {
    let _ = replay_world;
}

#[scenario(
    path = "tests/features/chat_completions_replay.feature",
    name = "Canonical-stream matching ignores comment-only drift"
)]
fn canonical_stream_matching_ignores_comment_only_drift(replay_world: ReplayWorld) {
    let _ = replay_world;
}

#[scenario(
    path = "tests/features/chat_completions_replay.feature",
    name = "Replay rejects malformed JSON requests before matching"
)]
fn replay_rejects_malformed_json_requests_before_matching(replay_world: ReplayWorld) {
    let _ = replay_world;
}
