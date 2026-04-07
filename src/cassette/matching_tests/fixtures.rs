//! Shared test fixtures for replay matching engine tests.

use rstest::fixture;
use serde_json::json;

use crate::cassette::{
    Cassette, Interaction, InteractionMetadata, MatchOutcome, MismatchDiagnostic, RecordedRequest,
    RecordedResponse,
};

fn openai_metadata(recorded_at: &str, relative_offset_ms: u64) -> InteractionMetadata {
    InteractionMetadata {
        protocol_id: "openai_chat".to_owned(),
        upstream_id: "openai".to_owned(),
        recorded_at: recorded_at.to_owned(),
        relative_offset_ms,
    }
}

fn ok_response(parsed_json: Option<serde_json::Value>) -> RecordedResponse {
    RecordedResponse::NonStream {
        status: 200,
        headers: Vec::new(),
        body: Vec::new(),
        parsed_json,
    }
}

fn make_interaction(
    method: &str,
    path: &str,
    parsed_json: Option<serde_json::Value>,
    canonical_request: Option<serde_json::Value>,
    stable_hash: &str,
    response_json: Option<serde_json::Value>,
    recorded_at: &str,
    relative_offset_ms: u64,
) -> Interaction {
    Interaction {
        request: RecordedRequest {
            method: method.to_owned(),
            path: path.to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json,
            canonical_request,
            stable_hash: Some(stable_hash.to_owned()),
        },
        response: ok_response(response_json),
        metadata: openai_metadata(recorded_at, relative_offset_ms),
    }
}

#[fixture]
pub(super) fn sample_cassette() -> Cassette {
    let mut cassette = Cassette::new();
    cassette.append(make_interaction(
        "POST",
        "/v1/chat/completions",
        Some(json!({"model": "gpt-4", "messages": []})),
        Some(json!({"method": "POST", "path": "/v1/chat/completions"})),
        "hash_a",
        Some(json!({"id": "resp_a"})),
        "2025-01-01T00:00:00Z",
        0,
    ));
    cassette.append(make_interaction(
        "POST",
        "/v1/chat/completions",
        Some(json!({"model": "gpt-4", "messages": [{"role": "user"}]})),
        Some(json!({
            "method": "POST",
            "path": "/v1/chat/completions",
            "body": {"messages": [{"role": "user"}]}
        })),
        "hash_b",
        Some(json!({"id": "resp_b"})),
        "2025-01-01T00:01:00Z",
        60_000,
    ));
    cassette.append(make_interaction(
        "GET",
        "/v1/models",
        None,
        Some(json!({"method": "GET", "path": "/v1/models"})),
        "hash_c",
        Some(json!({"data": []})),
        "2025-01-01T00:02:00Z",
        120_000,
    ));
    cassette
}

#[fixture]
pub(super) fn duplicate_hash_cassette() -> Cassette {
    let mut cassette = Cassette::new();
    cassette.append(make_interaction(
        "POST",
        "/v1/chat/completions",
        Some(json!({"model": "gpt-4", "messages": [{"content": "first"}]})),
        Some(json!({"method": "POST", "messages": [{"content": "first"}]})),
        "hash_a",
        Some(json!({"id": "first_response"})),
        "2025-01-01T00:00:00Z",
        0,
    ));
    cassette.append(make_interaction(
        "POST",
        "/v1/chat/completions",
        Some(json!({"model": "gpt-4", "messages": [{"content": "second"}]})),
        Some(json!({"method": "POST", "messages": [{"content": "second"}]})),
        "hash_a",
        Some(json!({"id": "second_response"})),
        "2025-01-01T00:01:00Z",
        60_000,
    ));
    cassette.append(make_interaction(
        "GET",
        "/v1/models",
        None,
        Some(json!({"method": "GET"})),
        "hash_b",
        Some(json!({"data": []})),
        "2025-01-01T00:02:00Z",
        120_000,
    ));
    cassette
}

// ── test helpers ─────────────────────────────────────────────────────────────

#[track_caller]
pub(super) fn assert_matched(outcome: MatchOutcome<'_>) -> Interaction {
    match outcome {
        MatchOutcome::Matched(i) => i.clone(),
        other @ MatchOutcome::Mismatch(_) => {
            panic!("expected MatchOutcome::Matched, got {other:?}")
        }
    }
}

#[track_caller]
pub(super) fn assert_mismatch(outcome: MatchOutcome<'_>) -> MismatchDiagnostic {
    match outcome {
        MatchOutcome::Mismatch(d) => d,
        other @ MatchOutcome::Matched(_) => {
            panic!("expected MatchOutcome::Mismatch, got {other:?}")
        }
    }
}

#[track_caller]
pub(super) fn assert_mismatch_diagnostic(
    outcome: MatchOutcome<'_>,
    expected_interaction_id: usize,
    expected_hash: &str,
    observed_hash: &str,
) -> MismatchDiagnostic {
    let d = assert_mismatch(outcome);
    assert_eq!(d.interaction_id, expected_interaction_id);
    assert_eq!(d.expected_hash, expected_hash);
    assert_eq!(d.observed_hash, observed_hash);
    d
}

#[track_caller]
pub(super) fn assert_matched_response_eq(outcome: MatchOutcome<'_>, expected: &RecordedResponse) {
    match outcome {
        MatchOutcome::Matched(interaction) => {
            assert_eq!(&interaction.response, expected);
        }
        other @ MatchOutcome::Mismatch(_) => {
            panic!("expected MatchOutcome::Matched, got {other:?}")
        }
    }
}

#[track_caller]
pub(super) fn extract_response_id(interaction: &Interaction) -> String {
    match &interaction.response {
        RecordedResponse::NonStream { parsed_json, .. } => parsed_json
            .as_ref()
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .map_or_else(
                || panic!("response has no string \"id\" field"),
                std::borrow::ToOwned::to_owned,
            ),
        RecordedResponse::Stream { .. } => panic!("expected NonStream response"),
    }
}
