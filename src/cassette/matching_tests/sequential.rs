//! Sequential strict mode tests.

use rstest::rstest;
use serde_json::json;

use super::fixtures::{assert_matched_response_eq, assert_mismatch_diagnostic, sample_cassette};
use crate::cassette::{Cassette, DIAGNOSTIC_EXHAUSTED, MatchOutcome, ReplayMatchEngine};
use crate::config::MatchMode;

#[rstest]
fn sequential_strict_three_correct_requests_match_in_order(sample_cassette: Cassette) {
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

    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict);

    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let outcome_a = engine.next_match("hash_a", &canonical_a);
    assert_matched_response_eq(outcome_a, &expected_response_0);

    let canonical_b = json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}});
    let outcome_b = engine.next_match("hash_b", &canonical_b);
    assert_matched_response_eq(outcome_b, &expected_response_1);

    let canonical_c = json!({"method": "GET", "path": "/v1/models"});
    let outcome_c = engine.next_match("hash_c", &canonical_c);
    assert_matched_response_eq(outcome_c, &expected_response_2);
}

#[rstest]
fn sequential_strict_first_request_wrong_hash_returns_mismatch(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict);

    let canonical_wrong = json!({"method": "GET", "path": "/wrong"});
    let outcome = engine.next_match("wrong_hash", &canonical_wrong);

    let d = assert_mismatch_diagnostic(outcome, 0, "hash_a", "wrong_hash");
    assert!(!d.diff_summary.is_empty());
}

#[rstest]
fn sequential_strict_second_request_wrong_hash_returns_mismatch(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict);

    // First request matches.
    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let outcome_first = engine.next_match("hash_a", &canonical_a);
    assert!(matches!(outcome_first, MatchOutcome::Matched(_)));

    // Second request has wrong hash.
    let canonical_wrong = json!({"method": "GET", "path": "/wrong"});
    let outcome_second = engine.next_match("wrong_hash", &canonical_wrong);

    assert_mismatch_diagnostic(outcome_second, 1, "hash_b", "wrong_hash");
}

#[rstest]
fn sequential_strict_mismatch_does_not_advance_cursor(sample_cassette: Cassette) {
    let expected_response_0 = sample_cassette
        .interactions
        .first()
        .expect("Interaction 0 should exist")
        .response
        .clone();

    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict);

    // First request has wrong hash (causes mismatch).
    let canonical_wrong = json!({"method": "GET", "path": "/wrong"});
    let outcome_mismatch = engine.next_match("wrong_hash", &canonical_wrong);
    assert_mismatch_diagnostic(outcome_mismatch, 0, "hash_a", "wrong_hash");

    // Second request with correct hash should still match interaction 0.
    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let outcome_match = engine.next_match("hash_a", &canonical_a);
    assert_matched_response_eq(outcome_match, &expected_response_0);
}

#[rstest]
fn sequential_strict_cassette_exhausted_returns_mismatch(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict);

    // Consume all three interactions.
    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let _ = engine.next_match("hash_a", &canonical_a);

    let canonical_b = json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}});
    let _ = engine.next_match("hash_b", &canonical_b);

    let canonical_c = json!({"method": "GET", "path": "/v1/models"});
    let _ = engine.next_match("hash_c", &canonical_c);

    // Fourth request should fail with exhaustion diagnostic.
    let canonical_extra = json!({"method": "GET", "path": "/extra"});
    let outcome = engine.next_match("hash_extra", &canonical_extra);

    let d = assert_mismatch_diagnostic(outcome, 3, "", "hash_extra");
    assert!(
        d.diff_summary.starts_with(DIAGNOSTIC_EXHAUSTED),
        "expected exhausted diagnostic, got: {}",
        d.diff_summary
    );
}
