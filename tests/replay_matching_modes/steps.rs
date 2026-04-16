//! Step definitions for BDD test scenarios.

use rstest_bdd_macros::{given, then, when};
use serde_json::json;
use spycatcher_harness::cassette::{
    Cassette, DIAGNOSTIC_EXHAUSTED, InteractionPosition, MatchOutcome,
};
use spycatcher_harness::config::MatchMode;

use super::fixtures::{InteractionSpec, create_interaction};
use super::helpers::{
    check_matched_count, check_mode_order, check_response_set, extract_response_id,
    initialise_engine, run_requests,
};
use super::world::MatchingWorld;

#[given("a cassette with three recorded interactions")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "BDD step functions use uniform Result signature for consistency"
)]
fn a_cassette_with_three_recorded_interactions(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
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
    Ok(())
}

#[given("a cassette with three recorded interactions with distinct hashes")]
fn a_cassette_with_three_recorded_interactions_with_distinct_hashes(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    // Same as above; the three interactions already have distinct hashes.
    a_cassette_with_three_recorded_interactions(matching_world)
}

#[given("a cassette with two interactions sharing the same hash")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "BDD step functions use uniform Result signature for consistency"
)]
fn a_cassette_with_two_interactions_sharing_the_same_hash(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
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
    Ok(())
}

#[given("a cassette with one recorded interaction")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "BDD step functions use uniform Result signature for consistency"
)]
fn a_cassette_with_one_recorded_interaction(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cassette = Cassette::new();
    cassette.append(create_interaction(InteractionSpec {
        method: "POST",
        path: "/v1/chat",
        canonical: json!({"method": "POST"}),
        hash: "hash_single",
        response_id: "resp_single",
    }));
    matching_world.cassette.set(cassette);
    Ok(())
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

    let result = match outcome {
        MatchOutcome::Mismatch(diagnostic) => {
            matching_world.mismatch_position.set(diagnostic.position);
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
            Ok(())
        }
        other @ MatchOutcome::Matched(_) => {
            Err(format!("expected MatchOutcome::Mismatch for wrong hash, got: {other:?}").into())
        }
    };

    matching_world.engine.set(engine);
    result
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
    let id_1_result = extract_response_id(&outcome_1)
        .ok_or_else(|| format!("first request expected to match but got: {outcome_1:?}"));

    // Propagate first error before mutating engine state further
    let id_1 = match id_1_result {
        Ok(id) => id,
        Err(e) => {
            matching_world.engine.set(engine);
            return Err(e.into());
        }
    };
    matching_world.first_response_id.set(id_1);

    let outcome_2 = engine.next_match(
        "shared_hash",
        &json!({"method": "POST", "content": "second"}),
    );
    let id_2_result = extract_response_id(&outcome_2)
        .ok_or_else(|| format!("second request expected to match but got: {outcome_2:?}"));

    matching_world.engine.set(engine);

    let id_2 = id_2_result?;
    matching_world.second_response_id.set(id_2);

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
    let matched = matches!(outcome, MatchOutcome::Matched(_));

    matching_world.engine.set(engine);

    if matched {
        matching_world.matched_count.set(1);
        Ok(())
    } else {
        Err("Expected MatchOutcome::Matched but got non-matching outcome".into())
    }
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

    let result = match outcome {
        MatchOutcome::Mismatch(diagnostic) => {
            matching_world
                .mismatch_diff_summary
                .set(diagnostic.diff_summary.clone());
            matching_world.mismatch_count.set(1);
            Ok(())
        }
        other @ MatchOutcome::Matched(_) => Err(format!(
            "expected MatchOutcome::Mismatch for exhausted cassette, got: {other:?}"
        )
        .into()),
    };

    matching_world.engine.set(engine);
    result
}

fn assert_slot_string_eq(
    slot: &rstest_bdd::Slot<String>,
    slot_name: &str,
    expected: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let value = slot
        .with_ref(String::clone)
        .ok_or_else(|| format!("{slot_name} must be set"))?;
    if value != expected {
        return Err(format!("{slot_name} should equal {expected:?}, got {value:?}").into());
    }
    Ok(())
}

#[then("all three requests receive the corresponding recorded interaction")]
fn all_three_requests_receive_the_corresponding_recorded_interaction(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    const VALID_IDS: &[&str] = &["resp_a", "resp_b", "resp_c"];
    check_matched_count(matching_world, 3)?;
    let response_ids = check_response_set(matching_world, VALID_IDS, 3)?;
    check_mode_order(&response_ids, matching_world)?;
    Ok(())
}

#[then("the engine returns a mismatch diagnostic")]
fn the_engine_returns_a_mismatch_diagnostic(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    let mismatch_count = matching_world
        .mismatch_count
        .with_ref(|c| *c)
        .ok_or("mismatch_count must be set")?;
    assert_eq!(mismatch_count, 1, "expected a mismatch diagnostic");
    Ok(())
}

#[then("the diagnostic contains the expected interaction ID")]
fn the_diagnostic_contains_the_expected_interaction_id(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    let position = matching_world
        .mismatch_position
        .with_ref(|p| *p)
        .ok_or("mismatch position must be set")?;
    assert_eq!(
        position,
        InteractionPosition::Expected(0),
        "expected position to be Expected(0) for the first interaction"
    );
    Ok(())
}

#[then("the diagnostic contains the expected and observed hashes")]
fn the_diagnostic_contains_the_expected_and_observed_hashes(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    let expected_hash = matching_world
        .mismatch_expected_hash
        .with_ref(String::clone)
        .ok_or("expected_hash must be set")?;
    let observed_hash = matching_world
        .mismatch_observed_hash
        .with_ref(String::clone)
        .ok_or("observed_hash must be set")?;

    assert_eq!(expected_hash, "hash_a");
    assert_eq!(observed_hash, "wrong_hash");
    Ok(())
}

#[then("the diagnostic contains a field-level diff summary")]
fn the_diagnostic_contains_a_field_level_diff_summary(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    let diff_summary = matching_world
        .mismatch_diff_summary
        .with_ref(String::clone)
        .ok_or("diff_summary must be set")?;

    assert!(!diff_summary.is_empty(), "diff summary should not be empty");
    Ok(())
}

#[then("the first request receives the first recorded interaction")]
fn the_first_request_receives_the_first_recorded_interaction(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    assert_slot_string_eq(
        &matching_world.first_response_id,
        "first_response_id",
        "first_response",
    )
}

#[then("the second request receives the second recorded interaction")]
fn the_second_request_receives_the_second_recorded_interaction(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    assert_slot_string_eq(
        &matching_world.second_response_id,
        "second_response_id",
        "second_response",
    )
}

#[then("the engine returns a mismatch diagnostic indicating exhaustion")]
fn the_engine_returns_a_mismatch_diagnostic_indicating_exhaustion(
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    let mismatch_count = matching_world
        .mismatch_count
        .with_ref(|c| *c)
        .ok_or("mismatch_count must be set")?;
    assert_eq!(mismatch_count, 1, "expected a mismatch diagnostic");

    let diff_summary = matching_world
        .mismatch_diff_summary
        .with_ref(String::clone)
        .ok_or("diff_summary must be set")?;
    assert!(
        diff_summary.starts_with(DIAGNOSTIC_EXHAUSTED),
        "expected exhaustion diagnostic prefix, got: {diff_summary}"
    );
    Ok(())
}
