//! Sequential strict mode tests.

use rstest::rstest;
use serde_json::json;

use super::fixtures::{
    assert_matched_response_eq, assert_mismatch_diagnostic, consume_all,
    expect_mismatch_diagnostic, nth_response, sample_cassette, sequential_engine,
};
use crate::cassette::{
    Cassette, DIAGNOSTIC_EXHAUSTED, InteractionPosition, MatchOutcome, ReplayMatchEngine,
};
use crate::config::MatchMode;

fn canonical_a() -> serde_json::Value {
    json!({"method": "POST", "path": "/v1/chat/completions"})
}

fn canonical_b() -> serde_json::Value {
    json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}})
}

fn canonical_c() -> serde_json::Value {
    json!({"method": "GET", "path": "/v1/models"})
}

#[rstest]
fn sequential_strict_three_correct_requests_match_in_order(sample_cassette: Cassette) {
    let expected_response_0 = nth_response(&sample_cassette, 0);
    let expected_response_1 = nth_response(&sample_cassette, 1);
    let expected_response_2 = nth_response(&sample_cassette, 2);

    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict)
        .expect("fixture cassette should have valid stable hashes");

    let outcome_a = engine.next_match("hash_a", &canonical_a());
    assert_matched_response_eq(outcome_a, &expected_response_0);

    let outcome_b = engine.next_match("hash_b", &canonical_b());
    assert_matched_response_eq(outcome_b, &expected_response_1);

    let outcome_c = engine.next_match("hash_c", &canonical_c());
    assert_matched_response_eq(outcome_c, &expected_response_2);
}

#[rstest]
fn sequential_strict_first_request_wrong_hash_returns_mismatch(
    mut sequential_engine: ReplayMatchEngine,
) {
    let engine = &mut sequential_engine;

    let canonical_wrong = json!({"method": "GET", "path": "/wrong"});
    let outcome = engine.next_match("wrong_hash", &canonical_wrong);

    let d = expect_mismatch_diagnostic(
        outcome,
        InteractionPosition::Expected(0),
        "hash_a",
        "wrong_hash",
    );
    assert!(!d.diff_summary.is_empty());
}

#[rstest]
fn sequential_strict_second_request_wrong_hash_returns_mismatch(
    mut sequential_engine: ReplayMatchEngine,
) {
    let engine = &mut sequential_engine;

    // First request matches.
    let outcome_first = engine.next_match("hash_a", &canonical_a());
    assert!(matches!(outcome_first, MatchOutcome::Matched { .. }));

    // Second request has wrong hash.
    let canonical_wrong = json!({"method": "GET", "path": "/wrong"});
    let outcome_second = engine.next_match("wrong_hash", &canonical_wrong);

    assert_mismatch_diagnostic(
        outcome_second,
        InteractionPosition::Expected(1),
        "hash_b",
        "wrong_hash",
    );
}

#[rstest]
fn sequential_strict_mismatch_does_not_advance_cursor(
    sample_cassette: Cassette,
    mut sequential_engine: ReplayMatchEngine,
) {
    let expected_response_0 = nth_response(&sample_cassette, 0);
    let engine = &mut sequential_engine;

    // First request has wrong hash (causes mismatch).
    let canonical_wrong = json!({"method": "GET", "path": "/wrong"});
    let outcome_mismatch = engine.next_match("wrong_hash", &canonical_wrong);
    assert_mismatch_diagnostic(
        outcome_mismatch,
        InteractionPosition::Expected(0),
        "hash_a",
        "wrong_hash",
    );

    // Second request with correct hash should still match interaction 0.
    let outcome_match = engine.next_match("hash_a", &canonical_a());
    assert_matched_response_eq(outcome_match, &expected_response_0);
}

#[rstest]
fn sequential_strict_cassette_exhausted_returns_mismatch(mut sequential_engine: ReplayMatchEngine) {
    // Consume all three interactions.
    consume_all(&mut sequential_engine);

    // Fourth request should fail with exhaustion diagnostic.
    let canonical_extra = json!({"method": "GET", "path": "/extra"});
    let outcome = sequential_engine.next_match("hash_extra", &canonical_extra);

    let d =
        expect_mismatch_diagnostic(outcome, InteractionPosition::Exhausted(3), "", "hash_extra");
    assert!(
        d.diff_summary.starts_with(DIAGNOSTIC_EXHAUSTED),
        "expected exhausted diagnostic, got: {}",
        d.diff_summary
    );
}
