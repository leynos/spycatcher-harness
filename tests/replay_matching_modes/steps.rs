//! Step definitions for BDD test scenarios.

use rstest_bdd_macros::{given, then, when};
use serde_json::json;
use spycatcher_harness::cassette::{Cassette, DIAGNOSTIC_EXHAUSTED, MatchOutcome};
use spycatcher_harness::config::MatchMode;

use super::fixtures::{InteractionSpec, create_interaction};
use super::helpers::{extract_response_id, initialise_engine, run_requests};
use super::world::MatchingWorld;

#[given("a cassette with three recorded interactions")]
fn a_cassette_with_three_recorded_interactions(matching_world: &MatchingWorld) {
    let mut cassette = Cassette::new();
    cassette.append(create_interaction(InteractionSpec {
        method: "POST",
        path: "/v1/chat",
        canonical: json!({"method": "POST"}),
        hash: "hash_a",
        response_id: "resp_a",
    }));
    cassette.append(create_interaction(InteractionSpec {
        method: "POST",
        path: "/v1/chat",
        canonical: json!({"method": "POST", "messages": [1]}),
        hash: "hash_b",
        response_id: "resp_b",
    }));
    cassette.append(create_interaction(InteractionSpec {
        method: "GET",
        path: "/v1/models",
        canonical: json!({"method": "GET"}),
        hash: "hash_c",
        response_id: "resp_c",
    }));
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
    cassette.append(create_interaction(InteractionSpec {
        method: "POST",
        path: "/v1/chat",
        canonical: json!({"method": "POST", "content": "first"}),
        hash: "shared_hash",
        response_id: "first_response",
    }));
    cassette.append(create_interaction(InteractionSpec {
        method: "POST",
        path: "/v1/chat",
        canonical: json!({"method": "POST", "content": "second"}),
        hash: "shared_hash",
        response_id: "second_response",
    }));
    matching_world.cassette.set(cassette);
}

#[given("a cassette with one recorded interaction")]
fn a_cassette_with_one_recorded_interaction(matching_world: &MatchingWorld) {
    let mut cassette = Cassette::new();
    cassette.append(create_interaction(InteractionSpec {
        method: "POST",
        path: "/v1/chat",
        canonical: json!({"method": "POST"}),
        hash: "hash_single",
        response_id: "resp_single",
    }));
    matching_world.cassette.set(cassette);
}

#[given("the replay engine is in sequential strict mode")]
fn the_replay_engine_is_in_sequential_strict_mode(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    initialise_engine(matching_world, MatchMode::SequentialStrict)
}

#[given("the replay engine is in keyed mode")]
fn the_replay_engine_is_in_keyed_mode(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    initialise_engine(matching_world, MatchMode::Keyed)
}

#[when("three requests arrive with matching hashes in recorded order")]
fn three_requests_arrive_with_matching_hashes_in_recorded_order(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    run_requests(
        matching_world,
        &[
            ("hash_a", json!({"method": "POST"})),
            ("hash_b", json!({"method": "POST", "messages": [1]})),
            ("hash_c", json!({"method": "GET"})),
        ],
    )
}

#[when("three requests arrive with matching hashes in reversed order")]
fn three_requests_arrive_with_matching_hashes_in_reversed_order(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    run_requests(
        matching_world,
        &[
            ("hash_c", json!({"method": "GET"})),
            ("hash_b", json!({"method": "POST", "messages": [1]})),
            ("hash_a", json!({"method": "POST"})),
        ],
    )
}

#[when("a request arrives with a hash that does not match the next interaction")]
fn a_request_arrives_with_a_hash_that_does_not_match_the_next_interaction(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = matching_world
        .engine
        .take()
        .ok_or("engine must be set before matching")?;

    let outcome = engine.next_match("wrong_hash", &json!({"method": "DELETE"}));

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

    matching_world.engine.set(engine);
    Ok(())
}

#[when("two requests arrive with the shared hash")]
fn two_requests_arrive_with_the_shared_hash(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = matching_world
        .engine
        .take()
        .ok_or("engine must be set before matching")?;

    let outcome_1 = engine.next_match(
        "shared_hash",
        &json!({"method": "POST", "content": "first"}),
    );
    if let Some(id) = extract_response_id(&outcome_1) {
        matching_world.first_response_id.set(id);
    }

    let outcome_2 = engine.next_match(
        "shared_hash",
        &json!({"method": "POST", "content": "second"}),
    );
    if let Some(id) = extract_response_id(&outcome_2) {
        matching_world.second_response_id.set(id);
    }

    matching_world.engine.set(engine);
    Ok(())
}

#[when("the first request matches and consumes the interaction")]
fn the_first_request_matches_and_consumes_the_interaction(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = matching_world
        .engine
        .take()
        .ok_or("engine must be set before matching")?;

    let outcome = engine.next_match("hash_single", &json!({"method": "POST"}));

    if matches!(outcome, MatchOutcome::Matched(_)) {
        matching_world.matched_count.set(1);
    }

    matching_world.engine.set(engine);
    Ok(())
}

#[when("a second request arrives")]
fn a_second_request_arrives(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = matching_world
        .engine
        .take()
        .ok_or("engine must be set before matching")?;

    let outcome = engine.next_match("hash_extra", &json!({"method": "GET"}));

    if let MatchOutcome::Mismatch(diagnostic) = outcome {
        matching_world
            .mismatch_diff_summary
            .set(diagnostic.diff_summary.clone());
        matching_world.mismatch_count.set(1);
    }

    matching_world.engine.set(engine);
    Ok(())
}

#[then("all three requests receive the corresponding recorded interaction")]
#[expect(
    clippy::expect_used,
    reason = "then step uses expect for post-condition assertion"
)]
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

    // Check that response IDs match expectations based on request order.
    let response_ids = matching_world
        .matched_response_ids
        .with_ref(Vec::clone)
        .expect("matched_response_ids must be set");

    // For sequential mode (in-order): ["resp_a", "resp_b", "resp_c"]
    // For keyed mode (reversed): ["resp_c", "resp_b", "resp_a"]
    // We check that all IDs are from the expected set and count is correct.
    assert_eq!(response_ids.len(), 3);
    for id in &response_ids {
        assert!(
            id == "resp_a" || id == "resp_b" || id == "resp_c",
            "unexpected response ID: {id}"
        );
    }

    // Additional check: verify the mode-specific order.
    let mode = matching_world
        .mode
        .with_ref(|m| *m)
        .expect("mode must be set");
    match mode {
        MatchMode::SequentialStrict => {
            assert_eq!(
                response_ids,
                vec!["resp_a", "resp_b", "resp_c"],
                "sequential mode should return IDs in recorded order"
            );
        }
        MatchMode::Keyed => {
            assert_eq!(
                response_ids,
                vec!["resp_c", "resp_b", "resp_a"],
                "keyed mode should return IDs matching request order"
            );
        }
    }
}

#[then("the engine returns a mismatch diagnostic")]
#[expect(
    clippy::expect_used,
    reason = "then step uses expect for post-condition assertion"
)]
fn the_engine_returns_a_mismatch_diagnostic(matching_world: &MatchingWorld) {
    let mismatch_count = matching_world
        .mismatch_count
        .with_ref(|c| *c)
        .expect("mismatch_count must be set");
    assert_eq!(mismatch_count, 1, "expected a mismatch diagnostic");
}

#[then("the diagnostic contains the expected interaction ID")]
#[expect(
    clippy::expect_used,
    reason = "then step uses expect for post-condition assertion"
)]
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
#[expect(
    clippy::expect_used,
    reason = "then step uses expect for post-condition assertion"
)]
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
#[expect(
    clippy::expect_used,
    reason = "then step uses expect for post-condition assertion"
)]
fn the_diagnostic_contains_a_field_level_diff_summary(matching_world: &MatchingWorld) {
    let diff_summary = matching_world
        .mismatch_diff_summary
        .with_ref(String::clone)
        .expect("diff_summary must be set");

    assert!(!diff_summary.is_empty(), "diff summary should not be empty");
}

#[then("the first request receives the first recorded interaction")]
#[expect(
    clippy::expect_used,
    reason = "then step uses expect for post-condition assertion"
)]
fn the_first_request_receives_the_first_recorded_interaction(matching_world: &MatchingWorld) {
    let first_id = matching_world
        .first_response_id
        .with_ref(String::clone)
        .expect("first_response_id must be set");
    assert_eq!(first_id, "first_response");
}

#[then("the second request receives the second recorded interaction")]
#[expect(
    clippy::expect_used,
    reason = "then step uses expect for post-condition assertion"
)]
fn the_second_request_receives_the_second_recorded_interaction(matching_world: &MatchingWorld) {
    let second_id = matching_world
        .second_response_id
        .with_ref(String::clone)
        .expect("second_response_id must be set");
    assert_eq!(second_id, "second_response");
}

#[then("the engine returns a mismatch diagnostic indicating exhaustion")]
#[expect(
    clippy::expect_used,
    reason = "then step uses expect for post-condition assertion"
)]
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
        diff_summary.starts_with(DIAGNOSTIC_EXHAUSTED),
        "expected exhaustion diagnostic prefix, got: {diff_summary}"
    );
}
