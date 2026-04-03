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
use rstest_bdd_macros::{ScenarioState, given, scenario, then, when};
use serde_json::{Value, json};
use spycatcher_harness::cassette::{
    Cassette, Interaction, InteractionMetadata, MatchOutcome, RecordedRequest, RecordedResponse,
    ReplayMatchEngine,
};
use spycatcher_harness::config::MatchMode;

#[derive(Default, ScenarioState)]
struct MatchingWorld {
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

fn create_interaction(
    method: &str,
    path: &str,
    canonical: Value,
    hash: &str,
    response_id: &str,
) -> Interaction {
    Interaction {
        request: RecordedRequest {
            method: method.to_owned(),
            path: path.to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({})),
            canonical_request: Some(canonical),
            stable_hash: Some(hash.to_owned()),
        },
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"id": response_id})),
        },
        metadata: InteractionMetadata {
            protocol_id: "test".to_owned(),
            upstream_id: "test".to_owned(),
            recorded_at: "2025-01-01T00:00:00Z".to_owned(),
            relative_offset_ms: 0,
        },
    }
}

#[given("a cassette with three recorded interactions")]
fn a_cassette_with_three_recorded_interactions(matching_world: &MatchingWorld) {
    let mut cassette = Cassette::new();
    cassette.append(create_interaction(
        "POST",
        "/v1/chat",
        json!({"method": "POST"}),
        "hash_a",
        "resp_a",
    ));
    cassette.append(create_interaction(
        "POST",
        "/v1/chat",
        json!({"method": "POST", "messages": [1]}),
        "hash_b",
        "resp_b",
    ));
    cassette.append(create_interaction(
        "GET",
        "/v1/models",
        json!({"method": "GET"}),
        "hash_c",
        "resp_c",
    ));
    matching_world.cassette.set(cassette);
}

#[given("a cassette with three recorded interactions with distinct hashes")]
fn a_cassette_with_three_recorded_interactions_with_distinct_hashes(
    matching_world: &MatchingWorld,
) {
    // Same as above; the three interactions already have distinct hashes.
    a_cassette_with_three_recorded_interactions(matching_world);
}

#[given("a cassette with two interactions sharing the same hash")]
fn a_cassette_with_two_interactions_sharing_the_same_hash(matching_world: &MatchingWorld) {
    let mut cassette = Cassette::new();
    cassette.append(create_interaction(
        "POST",
        "/v1/chat",
        json!({"method": "POST", "content": "first"}),
        "shared_hash",
        "first_response",
    ));
    cassette.append(create_interaction(
        "POST",
        "/v1/chat",
        json!({"method": "POST", "content": "second"}),
        "shared_hash",
        "second_response",
    ));
    matching_world.cassette.set(cassette);
}

#[given("a cassette with one recorded interaction")]
fn a_cassette_with_one_recorded_interaction(matching_world: &MatchingWorld) {
    let mut cassette = Cassette::new();
    cassette.append(create_interaction(
        "POST",
        "/v1/chat",
        json!({"method": "POST"}),
        "hash_single",
        "resp_single",
    ));
    matching_world.cassette.set(cassette);
}

#[given("the replay engine is in sequential strict mode")]
fn the_replay_engine_is_in_sequential_strict_mode(matching_world: &MatchingWorld) {
    matching_world.mode.set(MatchMode::SequentialStrict);
    let cassette = matching_world
        .cassette
        .take()
        .expect("cassette must be set before creating engine");
    let engine = ReplayMatchEngine::new(&cassette, MatchMode::SequentialStrict);
    matching_world.cassette.set(cassette);
    matching_world.engine.set(engine);
}

#[given("the replay engine is in keyed mode")]
fn the_replay_engine_is_in_keyed_mode(matching_world: &MatchingWorld) {
    matching_world.mode.set(MatchMode::Keyed);
    let cassette = matching_world
        .cassette
        .take()
        .expect("cassette must be set before creating engine");
    let engine = ReplayMatchEngine::new(&cassette, MatchMode::Keyed);
    matching_world.cassette.set(cassette);
    matching_world.engine.set(engine);
}

#[when("three requests arrive with matching hashes in recorded order")]
fn three_requests_arrive_with_matching_hashes_in_recorded_order(matching_world: &MatchingWorld) {
    let cassette = matching_world
        .cassette
        .take()
        .expect("cassette must be set");
    let mut engine = matching_world
        .engine
        .take()
        .expect("engine must be set before matching");

    let mut matched_count = 0;

    let outcome_a = engine.next_match("hash_a", &json!({"method": "POST"}), &cassette);
    if matches!(outcome_a, MatchOutcome::Matched(_)) {
        matched_count += 1;
    }

    let outcome_b = engine.next_match(
        "hash_b",
        &json!({"method": "POST", "messages": [1]}),
        &cassette,
    );
    if matches!(outcome_b, MatchOutcome::Matched(_)) {
        matched_count += 1;
    }

    let outcome_c = engine.next_match("hash_c", &json!({"method": "GET"}), &cassette);
    if matches!(outcome_c, MatchOutcome::Matched(_)) {
        matched_count += 1;
    }

    matching_world.matched_count.set(matched_count);
    matching_world.cassette.set(cassette);
    matching_world.engine.set(engine);
}

#[when("three requests arrive with matching hashes in reversed order")]
fn three_requests_arrive_with_matching_hashes_in_reversed_order(matching_world: &MatchingWorld) {
    let cassette = matching_world
        .cassette
        .take()
        .expect("cassette must be set");
    let mut engine = matching_world
        .engine
        .take()
        .expect("engine must be set before matching");

    let mut matched_count = 0;

    let outcome_c = engine.next_match("hash_c", &json!({"method": "GET"}), &cassette);
    if matches!(outcome_c, MatchOutcome::Matched(_)) {
        matched_count += 1;
    }

    let outcome_b = engine.next_match(
        "hash_b",
        &json!({"method": "POST", "messages": [1]}),
        &cassette,
    );
    if matches!(outcome_b, MatchOutcome::Matched(_)) {
        matched_count += 1;
    }

    let outcome_a = engine.next_match("hash_a", &json!({"method": "POST"}), &cassette);
    if matches!(outcome_a, MatchOutcome::Matched(_)) {
        matched_count += 1;
    }

    matching_world.matched_count.set(matched_count);
    matching_world.cassette.set(cassette);
    matching_world.engine.set(engine);
}

#[when("a request arrives with a hash that does not match the next interaction")]
fn a_request_arrives_with_a_hash_that_does_not_match_the_next_interaction(
    matching_world: &MatchingWorld,
) {
    let cassette = matching_world
        .cassette
        .take()
        .expect("cassette must be set");
    let mut engine = matching_world
        .engine
        .take()
        .expect("engine must be set before matching");

    let outcome = engine.next_match("wrong_hash", &json!({"method": "DELETE"}), &cassette);

    if let MatchOutcome::Mismatch(diagnostic) = outcome {
        matching_world
            .mismatch_interaction_id
            .set(diagnostic.interaction_id);
        matching_world
            .mismatch_expected_hash
            .set(diagnostic.expected_hash.clone());
        matching_world
            .mismatch_observed_hash
            .set(diagnostic.observed_hash.clone());
        matching_world
            .mismatch_diff_summary
            .set(diagnostic.diff_summary.clone());
        matching_world.mismatch_count.set(1);
    }

    matching_world.cassette.set(cassette);
    matching_world.engine.set(engine);
}

#[when("two requests arrive with the shared hash")]
fn two_requests_arrive_with_the_shared_hash(matching_world: &MatchingWorld) {
    let cassette = matching_world
        .cassette
        .take()
        .expect("cassette must be set");
    let mut engine = matching_world
        .engine
        .take()
        .expect("engine must be set before matching");

    let outcome_1 = engine.next_match(
        "shared_hash",
        &json!({"method": "POST", "content": "first"}),
        &cassette,
    );
    if let MatchOutcome::Matched(interaction) = outcome_1 {
        if let RecordedResponse::NonStream { parsed_json, .. } = &interaction.response {
            if let Some(id) = parsed_json.as_ref().and_then(|v| v.get("id")) {
                matching_world
                    .first_response_id
                    .set(id.as_str().unwrap_or("").to_owned());
            }
        }
    }

    let outcome_2 = engine.next_match(
        "shared_hash",
        &json!({"method": "POST", "content": "second"}),
        &cassette,
    );
    if let MatchOutcome::Matched(interaction) = outcome_2 {
        if let RecordedResponse::NonStream { parsed_json, .. } = &interaction.response {
            if let Some(id) = parsed_json.as_ref().and_then(|v| v.get("id")) {
                matching_world
                    .second_response_id
                    .set(id.as_str().unwrap_or("").to_owned());
            }
        }
    }

    matching_world.cassette.set(cassette);
    matching_world.engine.set(engine);
}

#[when("the first request matches and consumes the interaction")]
fn the_first_request_matches_and_consumes_the_interaction(matching_world: &MatchingWorld) {
    let cassette = matching_world
        .cassette
        .take()
        .expect("cassette must be set");
    let mut engine = matching_world
        .engine
        .take()
        .expect("engine must be set before matching");

    let outcome = engine.next_match("hash_single", &json!({"method": "POST"}), &cassette);

    if matches!(outcome, MatchOutcome::Matched(_)) {
        matching_world.matched_count.set(1);
    }

    matching_world.cassette.set(cassette);
    matching_world.engine.set(engine);
}

#[when("a second request arrives")]
fn a_second_request_arrives(matching_world: &MatchingWorld) {
    let cassette = matching_world
        .cassette
        .take()
        .expect("cassette must be set");
    let mut engine = matching_world
        .engine
        .take()
        .expect("engine must be set before matching");

    let outcome = engine.next_match("hash_extra", &json!({"method": "GET"}), &cassette);

    if let MatchOutcome::Mismatch(diagnostic) = outcome {
        matching_world
            .mismatch_diff_summary
            .set(diagnostic.diff_summary.clone());
        matching_world.mismatch_count.set(1);
    }

    matching_world.cassette.set(cassette);
    matching_world.engine.set(engine);
}

#[then("all three requests receive the corresponding recorded interaction")]
fn all_three_requests_receive_the_corresponding_recorded_interaction(
    matching_world: &MatchingWorld,
) {
    let matched_count = matching_world
        .matched_count
        .with_ref(|c| *c)
        .expect("matched_count must be set");
    assert_eq!(
        matched_count, 3,
        "expected all three requests to match interactions"
    );
}

#[then("the engine returns a mismatch diagnostic")]
fn the_engine_returns_a_mismatch_diagnostic(matching_world: &MatchingWorld) {
    let mismatch_count = matching_world
        .mismatch_count
        .with_ref(|c| *c)
        .expect("mismatch_count must be set");
    assert_eq!(mismatch_count, 1, "expected a mismatch diagnostic");
}

#[then("the diagnostic contains the expected interaction ID")]
fn the_diagnostic_contains_the_expected_interaction_id(matching_world: &MatchingWorld) {
    let interaction_id = matching_world
        .mismatch_interaction_id
        .with_ref(|id| *id)
        .expect("mismatch interaction_id must be set");
    assert_eq!(
        interaction_id, 0,
        "expected interaction_id to be 0 for the first interaction"
    );
}

#[then("the diagnostic contains the expected and observed hashes")]
fn the_diagnostic_contains_the_expected_and_observed_hashes(matching_world: &MatchingWorld) {
    let expected_hash = matching_world
        .mismatch_expected_hash
        .with_ref(String::clone)
        .expect("expected_hash must be set");
    let observed_hash = matching_world
        .mismatch_observed_hash
        .with_ref(String::clone)
        .expect("observed_hash must be set");

    assert_eq!(expected_hash, "hash_a");
    assert_eq!(observed_hash, "wrong_hash");
}

#[then("the diagnostic contains a field-level diff summary")]
fn the_diagnostic_contains_a_field_level_diff_summary(matching_world: &MatchingWorld) {
    let diff_summary = matching_world
        .mismatch_diff_summary
        .with_ref(String::clone)
        .expect("diff_summary must be set");

    assert!(!diff_summary.is_empty(), "diff summary should not be empty");
}

#[then("the first request receives the first recorded interaction")]
fn the_first_request_receives_the_first_recorded_interaction(matching_world: &MatchingWorld) {
    let first_id = matching_world
        .first_response_id
        .with_ref(String::clone)
        .expect("first_response_id must be set");
    assert_eq!(first_id, "first_response");
}

#[then("the second request receives the second recorded interaction")]
fn the_second_request_receives_the_second_recorded_interaction(matching_world: &MatchingWorld) {
    let second_id = matching_world
        .second_response_id
        .with_ref(String::clone)
        .expect("second_response_id must be set");
    assert_eq!(second_id, "second_response");
}

#[then("the engine returns a mismatch diagnostic indicating exhaustion")]
fn the_engine_returns_a_mismatch_diagnostic_indicating_exhaustion(matching_world: &MatchingWorld) {
    let mismatch_count = matching_world
        .mismatch_count
        .with_ref(|c| *c)
        .expect("mismatch_count must be set");
    assert_eq!(mismatch_count, 1, "expected a mismatch diagnostic");

    let diff_summary = matching_world
        .mismatch_diff_summary
        .with_ref(String::clone)
        .expect("diff_summary must be set");
    assert!(
        diff_summary.contains("exhausted"),
        "expected exhaustion message in diff summary"
    );
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
