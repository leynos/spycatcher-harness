//! Keyed mode tests.

use rstest::rstest;
use serde_json::json;

use super::fixtures::{
    assert_matched_response_eq, consume_all, duplicate_hash_cassette, expect_matched,
    expect_mismatch_diagnostic, extract_response_id, sample_cassette,
};
use crate::cassette::{
    Cassette, DIAGNOSTIC_CONSUMED, DIAGNOSTIC_NO_MATCH, InteractionPosition, MatchOutcome,
    ReplayMatchEngine,
};
use crate::config::MatchMode;

#[rstest]
fn keyed_mode_three_correct_requests_in_order_match(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::Keyed)
        .expect("fixture cassette should have valid stable hashes");

    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let outcome_a = engine.next_match("hash_a", &canonical_a);
    assert!(matches!(outcome_a, MatchOutcome::Matched(_)));

    let canonical_b = json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}});
    let outcome_b = engine.next_match("hash_b", &canonical_b);
    assert!(matches!(outcome_b, MatchOutcome::Matched(_)));

    let canonical_c = json!({"method": "GET", "path": "/v1/models"});
    let outcome_c = engine.next_match("hash_c", &canonical_c);
    assert!(matches!(outcome_c, MatchOutcome::Matched(_)));
}

#[rstest]
fn keyed_mode_three_correct_requests_in_reverse_order_match(sample_cassette: Cassette) {
    let expected_response_0 = sample_cassette
        .interactions
        .first()
        .expect("Interaction 0 should exist")
        .response
        .clone();
    let expected_response_1 = sample_cassette
        .interactions
        .get(1)
        .expect("Interaction 1 should exist")
        .response
        .clone();
    let expected_response_2 = sample_cassette
        .interactions
        .get(2)
        .expect("Interaction 2 should exist")
        .response
        .clone();

    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::Keyed)
        .expect("fixture cassette should have valid stable hashes");

    // Request in reverse order: hash_c, hash_b, hash_a.
    let canonical_c = json!({"method": "GET", "path": "/v1/models"});
    let outcome_c = engine.next_match("hash_c", &canonical_c);
    assert_matched_response_eq(outcome_c, &expected_response_2);

    let canonical_b = json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}});
    let outcome_b = engine.next_match("hash_b", &canonical_b);
    assert_matched_response_eq(outcome_b, &expected_response_1);

    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let outcome_a = engine.next_match("hash_a", &canonical_a);
    assert_matched_response_eq(outcome_a, &expected_response_0);
}

#[rstest]
fn keyed_mode_duplicate_hashes_consumed_in_order(duplicate_hash_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(duplicate_hash_cassette, MatchMode::Keyed)
        .expect("fixture cassette should have valid stable hashes");

    // First request with hash_a should match the first interaction.
    let canonical_a = json!({"method": "POST", "messages": [{"content": "first"}]});
    let outcome = engine.next_match("hash_a", &canonical_a);
    let interaction = expect_matched(outcome);
    assert_eq!(extract_response_id(&interaction), "first_response");

    // Second request with hash_a should match the second interaction.
    let canonical_a2 = json!({"method": "POST", "messages": [{"content": "second"}]});
    let outcome_2 = engine.next_match("hash_a", &canonical_a2);
    let interaction_2 = expect_matched(outcome_2);
    assert_eq!(extract_response_id(&interaction_2), "second_response");
}

#[rstest]
fn keyed_mode_matches_on_hash_regardless_of_canonical_json(sample_cassette: Cassette) {
    let expected_response_0 = sample_cassette
        .interactions
        .first()
        .expect("Interaction 0 should exist")
        .response
        .clone();

    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::Keyed)
        .expect("fixture cassette should have valid stable hashes");

    // Request with hash_a but completely different canonical JSON should still match.
    let canonical_different = json!({"totally": "different", "structure": 123});
    let outcome = engine.next_match("hash_a", &canonical_different);

    // Should match interaction 0 because hash matches, even though canonical JSON differs.
    assert_matched_response_eq(outcome, &expected_response_0);
}

#[rstest]
fn keyed_mode_request_with_unknown_hash_returns_mismatch(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::Keyed)
        .expect("fixture cassette should have valid stable hashes");

    let canonical_unknown = json!({"method": "DELETE", "path": "/unknown"});
    let outcome = engine.next_match("unknown_hash", &canonical_unknown);

    let d = expect_mismatch_diagnostic(
        outcome,
        InteractionPosition::KeyedMiss(3),
        "",
        "unknown_hash",
    );
    assert!(
        d.diff_summary.starts_with(DIAGNOSTIC_NO_MATCH),
        "expected no-match diagnostic, got: {}",
        d.diff_summary
    );
}

#[rstest]
fn keyed_mode_all_consumed_then_request_returns_mismatch(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::Keyed)
        .expect("fixture cassette should have valid stable hashes");

    // Consume all three interactions.
    consume_all(&mut engine);

    // Try to request hash_a again (already consumed).
    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let outcome = engine.next_match("hash_a", &canonical_a);

    let d = expect_mismatch_diagnostic(outcome, InteractionPosition::KeyedMiss(3), "", "hash_a");
    assert!(
        d.diff_summary.starts_with(DIAGNOSTIC_CONSUMED),
        "expected consumed diagnostic, got: {}",
        d.diff_summary
    );
}
