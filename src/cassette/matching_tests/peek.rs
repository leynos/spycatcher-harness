//! Non-committing replay match inspection tests.

use rstest::rstest;
use serde_json::json;

use super::fixtures::{
    assert_matched_response_eq, assert_mismatch_diagnostic, duplicate_hash_cassette,
    expect_matched, extract_response_id, nth_response, sample_cassette,
};
use crate::cassette::{
    Cassette, InteractionPosition, MatchOutcome, RecordedResponse, ReplayMatchEngine,
};
use crate::config::MatchMode;

fn canonical_a() -> serde_json::Value {
    json!({"method": "POST", "path": "/v1/chat/completions"})
}

#[rstest]
fn sequential_peek_match_does_not_advance_cursor(sample_cassette: Cassette) {
    let expected_response_0 = nth_response(&sample_cassette, 0).expect("first interaction exists");
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
    let expected_response_0 = nth_response(&sample_cassette, 0).expect("first interaction exists");
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
    assert_eq!(extract_response_id(&peeked_first), Some("first_response"));
    let peeked_again = expect_matched(engine.peek_match("hash_a", &canonical));
    assert_eq!(extract_response_id(&peeked_again), Some("first_response"));

    let consumed_first = expect_matched(engine.next_match("hash_a", &canonical));
    assert_eq!(extract_response_id(&consumed_first), Some("first_response"));
    let consumed_second = expect_matched(engine.next_match("hash_a", &canonical));
    assert_eq!(
        extract_response_id(&consumed_second),
        Some("second_response")
    );
}

#[rstest]
fn keyed_peek_mismatch_does_not_consume_later_match(sample_cassette: Cassette) {
    let expected_response_0 = nth_response(&sample_cassette, 0).expect("first interaction exists");
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

/// Looking beyond the cassette returns absence rather than panicking.
#[rstest]
fn nth_response_returns_none_past_the_last_interaction(sample_cassette: Cassette) {
    assert_eq!(
        nth_response(&sample_cassette, sample_cassette.interactions.len()),
        None,
        "an index equal to the interaction count should have no response"
    );
}

/// Response IDs must be present as strings rather than merely as JSON values.
#[rstest]
#[case::missing(json!({}))]
#[case::non_string(json!({"id": 7}))]
fn extract_response_id_returns_none_without_a_string_id(
    mut sample_cassette: Cassette,
    #[case] response_json: serde_json::Value,
) {
    let interaction = sample_cassette
        .interactions
        .first_mut()
        .expect("the sample cassette should contain an interaction");
    let RecordedResponse::NonStream { parsed_json, .. } = &mut interaction.response else {
        panic!("the sample interaction should have a non-stream response");
    };
    *parsed_json = Some(response_json);
    assert_eq!(
        extract_response_id(interaction),
        None,
        "a missing or non-string response ID should be absent"
    );
}

/// A stream response has no non-stream response ID.
#[rstest]
fn extract_response_id_returns_none_for_a_stream(mut sample_cassette: Cassette) {
    let interaction = sample_cassette
        .interactions
        .first_mut()
        .expect("the sample cassette should contain an interaction");
    interaction.response = RecordedResponse::Stream {
        status: 200,
        headers: Vec::new(),
        events: Vec::new(),
        raw_transcript: Vec::new(),
        timing: None,
    };
    assert_eq!(
        extract_response_id(interaction),
        None,
        "a stream response should not expose a non-stream ID"
    );
}
