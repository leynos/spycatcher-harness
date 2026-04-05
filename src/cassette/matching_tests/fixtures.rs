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

fn sample_interaction_hash_a() -> Interaction {
    Interaction {
        request: RecordedRequest {
            method: "POST".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"model": "gpt-4", "messages": []})),
            canonical_request: Some(json!({"method": "POST", "path": "/v1/chat/completions"})),
            stable_hash: Some("hash_a".to_owned()),
        },
        response: ok_response(Some(json!({"id": "resp_a"}))),
        metadata: openai_metadata("2025-01-01T00:00:00Z", 0),
    }
}

fn sample_interaction_hash_b() -> Interaction {
    Interaction {
        request: RecordedRequest {
            method: "POST".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"model": "gpt-4", "messages": [{"role": "user"}]})),
            canonical_request: Some(json!({
                "method": "POST",
                "path": "/v1/chat/completions",
                "body": {"messages": [{"role": "user"}]}
            })),
            stable_hash: Some("hash_b".to_owned()),
        },
        response: ok_response(Some(json!({"id": "resp_b"}))),
        metadata: openai_metadata("2025-01-01T00:01:00Z", 60_000),
    }
}

fn sample_interaction_hash_c() -> Interaction {
    Interaction {
        request: RecordedRequest {
            method: "GET".to_owned(),
            path: "/v1/models".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: None,
            canonical_request: Some(json!({"method": "GET", "path": "/v1/models"})),
            stable_hash: Some("hash_c".to_owned()),
        },
        response: ok_response(Some(json!({"data": []}))),
        metadata: openai_metadata("2025-01-01T00:02:00Z", 120_000),
    }
}

fn dup_interaction_hash_a_first() -> Interaction {
    Interaction {
        request: RecordedRequest {
            method: "POST".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"model": "gpt-4", "messages": [{"content": "first"}]})),
            canonical_request: Some(json!({"method": "POST", "messages": [{"content": "first"}]})),
            stable_hash: Some("hash_a".to_owned()),
        },
        response: ok_response(Some(json!({"id": "first_response"}))),
        metadata: openai_metadata("2025-01-01T00:00:00Z", 0),
    }
}

fn dup_interaction_hash_a_second() -> Interaction {
    Interaction {
        request: RecordedRequest {
            method: "POST".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"model": "gpt-4", "messages": [{"content": "second"}]})),
            canonical_request: Some(json!({"method": "POST", "messages": [{"content": "second"}]})),
            stable_hash: Some("hash_a".to_owned()),
        },
        response: ok_response(Some(json!({"id": "second_response"}))),
        metadata: openai_metadata("2025-01-01T00:01:00Z", 60_000),
    }
}

fn dup_interaction_hash_b() -> Interaction {
    Interaction {
        request: RecordedRequest {
            method: "GET".to_owned(),
            path: "/v1/models".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: None,
            canonical_request: Some(json!({"method": "GET"})),
            stable_hash: Some("hash_b".to_owned()),
        },
        response: ok_response(Some(json!({"data": []}))),
        metadata: openai_metadata("2025-01-01T00:02:00Z", 120_000),
    }
}

#[fixture]
pub(super) fn sample_cassette() -> Cassette {
    let mut cassette = Cassette::new();
    cassette.append(sample_interaction_hash_a());
    cassette.append(sample_interaction_hash_b());
    cassette.append(sample_interaction_hash_c());
    cassette
}

#[fixture]
pub(super) fn duplicate_hash_cassette() -> Cassette {
    let mut cassette = Cassette::new();
    cassette.append(dup_interaction_hash_a_first());
    cassette.append(dup_interaction_hash_a_second());
    cassette.append(dup_interaction_hash_b());
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
            .unwrap_or_else(|| panic!("response has no string \"id\" field"))
            .to_owned(),
        RecordedResponse::Stream { .. } => panic!("expected NonStream response"),
    }
}
