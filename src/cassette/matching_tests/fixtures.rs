//! Shared test fixtures for replay matching engine tests.

use rstest::fixture;
use serde_json::json;

use crate::cassette::{
    Cassette, Interaction, InteractionMetadata, InteractionPosition, MatchOutcome,
    MismatchDiagnostic, RecordedRequest, RecordedResponse, ReplayMatchEngine,
};
use crate::config::MatchMode;

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

struct InteractionSpec<'a> {
    method: &'a str,
    path: &'a str,
    parsed_json: Option<serde_json::Value>,
    canonical_request: Option<serde_json::Value>,
    stable_hash: &'a str,
    response_json: Option<serde_json::Value>,
    recorded_at: &'a str,
    relative_offset_ms: u64,
}

fn make_interaction(spec: InteractionSpec<'_>) -> Interaction {
    Interaction {
        request: RecordedRequest {
            method: spec.method.to_owned(),
            path: spec.path.to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: spec.parsed_json,
            canonical_request: spec.canonical_request,
            stable_hash: Some(spec.stable_hash.to_owned()),
        },
        response: ok_response(spec.response_json),
        metadata: openai_metadata(spec.recorded_at, spec.relative_offset_ms),
    }
}

fn cassette_from_interactions(interactions: impl IntoIterator<Item = Interaction>) -> Cassette {
    let mut cassette = Cassette::new();
    for interaction in interactions {
        cassette.append(interaction);
    }
    cassette
}

#[fixture]
pub(super) fn sample_cassette() -> Cassette {
    cassette_from_interactions([
        make_interaction(InteractionSpec {
            method: "POST",
            path: "/v1/chat/completions",
            parsed_json: Some(json!({"model": "gpt-4", "messages": []})),
            canonical_request: Some(json!({"method": "POST", "path": "/v1/chat/completions"})),
            stable_hash: "hash_a",
            response_json: Some(json!({"id": "resp_a"})),
            recorded_at: "2025-01-01T00:00:00Z",
            relative_offset_ms: 0,
        }),
        make_interaction(InteractionSpec {
            method: "POST",
            path: "/v1/chat/completions",
            parsed_json: Some(json!({"model": "gpt-4", "messages": [{"role": "user"}]})),
            canonical_request: Some(json!({
                "method": "POST",
                "path": "/v1/chat/completions",
                "body": {"messages": [{"role": "user"}]}
            })),
            stable_hash: "hash_b",
            response_json: Some(json!({"id": "resp_b"})),
            recorded_at: "2025-01-01T00:01:00Z",
            relative_offset_ms: 60_000,
        }),
        make_interaction(InteractionSpec {
            method: "GET",
            path: "/v1/models",
            parsed_json: None,
            canonical_request: Some(json!({"method": "GET", "path": "/v1/models"})),
            stable_hash: "hash_c",
            response_json: Some(json!({"data": []})),
            recorded_at: "2025-01-01T00:02:00Z",
            relative_offset_ms: 120_000,
        }),
    ])
}

#[fixture]
pub(super) fn duplicate_hash_cassette() -> Cassette {
    cassette_from_interactions([
        make_interaction(InteractionSpec {
            method: "POST",
            path: "/v1/chat/completions",
            parsed_json: Some(json!({"model": "gpt-4", "messages": [{"content": "first"}]})),
            canonical_request: Some(json!({"method": "POST", "messages": [{"content": "first"}]})),
            stable_hash: "hash_a",
            response_json: Some(json!({"id": "first_response"})),
            recorded_at: "2025-01-01T00:00:00Z",
            relative_offset_ms: 0,
        }),
        make_interaction(InteractionSpec {
            method: "POST",
            path: "/v1/chat/completions",
            parsed_json: Some(json!({"model": "gpt-4", "messages": [{"content": "second"}]})),
            canonical_request: Some(json!({"method": "POST", "messages": [{"content": "second"}]})),
            stable_hash: "hash_a",
            response_json: Some(json!({"id": "second_response"})),
            recorded_at: "2025-01-01T00:01:00Z",
            relative_offset_ms: 60_000,
        }),
        make_interaction(InteractionSpec {
            method: "GET",
            path: "/v1/models",
            parsed_json: None,
            canonical_request: Some(json!({"method": "GET"})),
            stable_hash: "hash_b",
            response_json: Some(json!({"data": []})),
            recorded_at: "2025-01-01T00:02:00Z",
            relative_offset_ms: 120_000,
        }),
    ])
}

/// Creates a `ReplayMatchEngine` in sequential strict mode from the sample cassette.
#[expect(
    unknown_lints,
    reason = "no_expect_outside_tests is a Dylint custom lint"
)]
#[expect(
    no_expect_outside_tests,
    reason = "`#[fixture]` is a test-only context but is not recognised as such \
              by the whitaker_suite linter"
)]
#[fixture]
pub(super) fn sequential_engine(sample_cassette: Cassette) -> ReplayMatchEngine {
    ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict)
        .expect("fixture cassette should have valid stable hashes")
}

// ── test helpers ─────────────────────────────────────────────────────────────

/// Consumes all three interactions from the sample cassette in order.
pub(super) fn consume_all(engine: &mut ReplayMatchEngine) {
    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    assert_matched(engine.next_match("hash_a", &canonical_a));

    let canonical_b = json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}});
    assert_matched(engine.next_match("hash_b", &canonical_b));

    let canonical_c = json!({"method": "GET", "path": "/v1/models"});
    assert_matched(engine.next_match("hash_c", &canonical_c));
}

/// Retrieves the nth response from the cassette.
#[track_caller]
pub(super) fn nth_response(cassette: &Cassette, n: usize) -> RecordedResponse {
    cassette
        .interactions
        .get(n)
        .expect("interaction index within cassette bounds")
        .response
        .clone()
}

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
    expected_position: InteractionPosition,
    expected_hash: &str,
    observed_hash: &str,
) -> MismatchDiagnostic {
    let d = assert_mismatch(outcome);
    assert_eq!(d.position, expected_position);
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
