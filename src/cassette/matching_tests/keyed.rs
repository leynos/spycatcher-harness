//! Keyed mode tests.

use rstest::rstest;
use serde_json::json;

use super::fixtures::{
    assert_matched, assert_matched_response_eq, assert_mismatch_diagnostic,
    duplicate_hash_cassette, extract_response_id, sample_cassette,
};
use crate::cassette::{Cassette, MatchOutcome, ReplayMatchEngine};
use crate::config::MatchMode;

#[rstest]
fn keyed_mode_three_correct_requests_in_order_match(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::Keyed);

    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let outcome_a = engine.next_match("hash_a", &canonical_a, &sample_cassette);
    assert!(matches!(outcome_a, MatchOutcome::Matched(_)));

    let canonical_b = json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}});
    let outcome_b = engine.next_match("hash_b", &canonical_b, &sample_cassette);
    assert!(matches!(outcome_b, MatchOutcome::Matched(_)));

    let canonical_c = json!({"method": "GET", "path": "/v1/models"});
    let outcome_c = engine.next_match("hash_c", &canonical_c, &sample_cassette);
    assert!(matches!(outcome_c, MatchOutcome::Matched(_)));
}

#[rstest]
fn keyed_mode_three_correct_requests_in_reverse_order_match(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::Keyed);

    // Request in reverse order: hash_c, hash_b, hash_a.
    let canonical_c = json!({"method": "GET", "path": "/v1/models"});
    let outcome_c = engine.next_match("hash_c", &canonical_c, &sample_cassette);
    assert_matched_response_eq(
        outcome_c,
        &sample_cassette
            .interactions
            .get(2)
            .expect("Interaction 2 should exist")
            .response,
    );

    let canonical_b = json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}});
    let outcome_b = engine.next_match("hash_b", &canonical_b, &sample_cassette);
    assert_matched_response_eq(
        outcome_b,
        &sample_cassette
            .interactions
            .get(1)
            .expect("Interaction 1 should exist")
            .response,
    );

    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let outcome_a = engine.next_match("hash_a", &canonical_a, &sample_cassette);
    assert_matched_response_eq(
        outcome_a,
        &sample_cassette
            .interactions
            .first()
            .expect("Interaction 0 should exist")
            .response,
    );
}

#[rstest]
fn keyed_mode_duplicate_hashes_consumed_in_order(duplicate_hash_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&duplicate_hash_cassette, MatchMode::Keyed);

    // First request with hash_a should match the first interaction.
    let canonical_a = json!({"method": "POST", "messages": [{"content": "first"}]});
    let outcome = engine.next_match("hash_a", &canonical_a, &duplicate_hash_cassette);
    let interaction = assert_matched(outcome);
    assert_eq!(extract_response_id(&interaction), "first_response");

    // Second request with hash_a should match the second interaction.
    let canonical_a2 = json!({"method": "POST", "messages": [{"content": "second"}]});
    let outcome_2 = engine.next_match("hash_a", &canonical_a2, &duplicate_hash_cassette);
    let interaction_2 = assert_matched(outcome_2);
    assert_eq!(extract_response_id(&interaction_2), "second_response");
}

#[rstest]
fn keyed_mode_request_with_unknown_hash_returns_mismatch(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::Keyed);

    let canonical_unknown = json!({"method": "DELETE", "path": "/unknown"});
    let outcome = engine.next_match("unknown_hash", &canonical_unknown, &sample_cassette);

    let d = assert_mismatch_diagnostic(outcome, 3, "", "unknown_hash");
    assert!(d.diff_summary.contains("no interaction"));
}

#[rstest]
fn keyed_mode_all_consumed_then_request_returns_mismatch(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::Keyed);

    // Consume all three interactions.
    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let _ = engine.next_match("hash_a", &canonical_a, &sample_cassette);

    let canonical_b = json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}});
    let _ = engine.next_match("hash_b", &canonical_b, &sample_cassette);

    let canonical_c = json!({"method": "GET", "path": "/v1/models"});
    let _ = engine.next_match("hash_c", &canonical_c, &sample_cassette);

    // Try to request hash_a again (already consumed).
    let outcome = engine.next_match("hash_a", &canonical_a, &sample_cassette);

    let d = assert_mismatch_diagnostic(outcome, 3, "", "hash_a");
    assert!(d.diff_summary.contains("already been consumed"));
}
