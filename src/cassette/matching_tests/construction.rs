//! Constructor validation tests for `ReplayMatchEngine`.

use serde_json::json;

use crate::HarnessError;
use crate::cassette::{
    Cassette, Interaction, InteractionMetadata, RecordedRequest, RecordedResponse,
    ReplayMatchEngine,
};
use crate::config::MatchMode;

/// Builds a minimal interaction with an optional `stable_hash`.
fn interaction_with_hash(stable_hash: Option<&str>) -> Interaction {
    Interaction {
        request: RecordedRequest {
            method: "POST".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"model": "gpt-4"})),
            canonical_request: Some(json!({"method": "POST"})),
            stable_hash: stable_hash.map(str::to_owned),
        },
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"id": "resp"})),
        },
        metadata: InteractionMetadata {
            protocol_id: "openai_chat".to_owned(),
            upstream_id: "openai".to_owned(),
            recorded_at: "2025-01-01T00:00:00Z".to_owned(),
            relative_offset_ms: 0,
        },
    }
}

/// Extracts the error from a `Result`, panicking if it is `Ok`.
#[track_caller]
fn assert_invalid_cassette_error(result: Result<ReplayMatchEngine, HarnessError>) -> String {
    let Err(err) = result else {
        panic!("expected error for missing stable_hash, got Ok");
    };
    assert!(
        matches!(err, HarnessError::InvalidCassette { .. }),
        "expected InvalidCassette variant, got: {err:?}"
    );
    err.to_string()
}

#[test]
fn new_returns_error_when_first_interaction_has_no_stable_hash() {
    let mut cassette = Cassette::new();
    cassette.append(interaction_with_hash(None));

    let result = ReplayMatchEngine::new(cassette, MatchMode::SequentialStrict);

    let msg = assert_invalid_cassette_error(result);
    assert!(
        msg.contains("index 0"),
        "error message should mention the interaction index, got: {msg}"
    );
}

#[test]
fn new_returns_error_when_later_interaction_has_no_stable_hash() {
    let mut cassette = Cassette::new();
    cassette.append(interaction_with_hash(Some("hash_ok")));
    cassette.append(interaction_with_hash(None));

    let result = ReplayMatchEngine::new(cassette, MatchMode::SequentialStrict);

    let msg = assert_invalid_cassette_error(result);
    assert!(
        msg.contains("index 1"),
        "error message should mention interaction index 1, got: {msg}"
    );
}

#[test]
fn new_returns_error_for_missing_hash_in_keyed_mode() {
    let mut cassette = Cassette::new();
    cassette.append(interaction_with_hash(None));

    let result = ReplayMatchEngine::new(cassette, MatchMode::Keyed);

    assert_invalid_cassette_error(result);
}

#[test]
fn new_succeeds_when_all_interactions_have_stable_hashes() {
    let mut cassette = Cassette::new();
    cassette.append(interaction_with_hash(Some("hash_a")));
    cassette.append(interaction_with_hash(Some("hash_b")));

    let result = ReplayMatchEngine::new(cassette, MatchMode::SequentialStrict);

    assert!(result.is_ok(), "expected success when all hashes present");
}

#[test]
fn new_succeeds_with_empty_cassette() {
    let cassette = Cassette::new();

    let result = ReplayMatchEngine::new(cassette, MatchMode::SequentialStrict);

    assert!(result.is_ok(), "empty cassette should succeed");
}
