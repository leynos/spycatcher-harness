//! Non-committing replay match inspection tests.

use rstest::rstest;
use serde_json::json;

use super::fixtures::{
    assert_matched_response_eq, assert_mismatch_diagnostic, duplicate_hash_cassette,
    expect_matched, extract_response_id, nth_response, sample_cassette,
};
use crate::cassette::{Cassette, InteractionPosition, MatchOutcome, ReplayMatchEngine};
use crate::config::MatchMode;

fn canonical_a() -> serde_json::Value {
    json!({"method": "POST", "path": "/v1/chat/completions"})
}

#[rstest]
fn sequential_peek_match_does_not_advance_cursor(sample_cassette: Cassette) {
    let expected_response_0 = nth_response(&sample_cassette, 0);
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict)
        .expect("fixture cassette should have valid stable hashes");

    assert!(matches!(
        engine.peek_match("hash_a", &canonical_a()),
        MatchOutcome::Matched {
            interaction_id: 0,
            ..
        }
    ));
    assert!(matches!(
        engine.peek_match("hash_a", &canonical_a()),
        MatchOutcome::Matched {
            interaction_id: 0,
            ..
        }
    ));
    assert_matched_response_eq(
        engine.next_match("hash_a", &canonical_a()),
        &expected_response_0,
    );
}

#[rstest]
fn sequential_peek_mismatch_does_not_advance_cursor(sample_cassette: Cassette) {
    let expected_response_0 = nth_response(&sample_cassette, 0);
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict)
        .expect("fixture cassette should have valid stable hashes");
    let wrong_canonical = json!({"method": "GET", "path": "/wrong"});

    assert_mismatch_diagnostic(
        engine.peek_match("wrong_hash", &wrong_canonical),
        InteractionPosition::Expected(0),
        "hash_a",
        "wrong_hash",
    );
    assert_mismatch_diagnostic(
        engine.peek_match("wrong_hash", &wrong_canonical),
        InteractionPosition::Expected(0),
        "hash_a",
        "wrong_hash",
    );
    assert_matched_response_eq(
        engine.next_match("hash_a", &canonical_a()),
        &expected_response_0,
    );
}

#[rstest]
fn keyed_peek_match_does_not_mark_interaction_consumed(duplicate_hash_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(duplicate_hash_cassette, MatchMode::Keyed)
        .expect("fixture cassette should have valid stable hashes");
    let canonical = json!({"method": "POST", "messages": [{"content": "first"}]});

    let peeked_first = expect_matched(engine.peek_match("hash_a", &canonical));
    assert_eq!(extract_response_id(&peeked_first), "first_response");
    let peeked_again = expect_matched(engine.peek_match("hash_a", &canonical));
    assert_eq!(extract_response_id(&peeked_again), "first_response");

    let consumed_first = expect_matched(engine.next_match("hash_a", &canonical));
    assert_eq!(extract_response_id(&consumed_first), "first_response");
    let consumed_second = expect_matched(engine.next_match("hash_a", &canonical));
    assert_eq!(extract_response_id(&consumed_second), "second_response");
}

#[rstest]
fn keyed_peek_mismatch_does_not_consume_later_match(sample_cassette: Cassette) {
    let expected_response_0 = nth_response(&sample_cassette, 0);
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::Keyed)
        .expect("fixture cassette should have valid stable hashes");
    let unknown_canonical = json!({"method": "DELETE", "path": "/unknown"});

    assert_mismatch_diagnostic(
        engine.peek_match("unknown_hash", &unknown_canonical),
        InteractionPosition::KeyedMiss(3),
        "",
        "unknown_hash",
    );
    assert_mismatch_diagnostic(
        engine.peek_match("unknown_hash", &unknown_canonical),
        InteractionPosition::KeyedMiss(3),
        "",
        "unknown_hash",
    );
    assert_matched_response_eq(
        engine.next_match("hash_a", &canonical_a()),
        &expected_response_0,
    );
}
