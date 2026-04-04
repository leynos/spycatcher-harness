//! Shared test fixtures for replay matching engine tests.

use rstest::fixture;
use serde_json::{Value, json};

use crate::cassette::{Cassette, Interaction, InteractionMetadata, RecordedRequest, RecordedResponse};

#[fixture]
pub(super) fn sample_cassette() -> Cassette {
    let mut cassette = Cassette::new();

    // Interaction 0: hash_a
    cassette.append(Interaction {
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
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"id": "resp_a"})),
        },
        metadata: InteractionMetadata {
            protocol_id: "openai_chat".to_owned(),
            upstream_id: "openai".to_owned(),
            recorded_at: "2025-01-01T00:00:00Z".to_owned(),
            relative_offset_ms: 0,
        },
    });

    // Interaction 1: hash_b
    cassette.append(Interaction {
        request: RecordedRequest {
            method: "POST".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"model": "gpt-4", "messages": [{"role": "user"}]})),
            canonical_request: Some(json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}})),
            stable_hash: Some("hash_b".to_owned()),
        },
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"id": "resp_b"})),
        },
        metadata: InteractionMetadata {
            protocol_id: "openai_chat".to_owned(),
            upstream_id: "openai".to_owned(),
            recorded_at: "2025-01-01T00:01:00Z".to_owned(),
            relative_offset_ms: 60_000,
        },
    });

    // Interaction 2: hash_c
    cassette.append(Interaction {
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
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"data": []})),
        },
        metadata: InteractionMetadata {
            protocol_id: "openai_chat".to_owned(),
            upstream_id: "openai".to_owned(),
            recorded_at: "2025-01-01T00:02:00Z".to_owned(),
            relative_offset_ms: 120_000,
        },
    });

    cassette
}

#[fixture]
pub(super) fn duplicate_hash_cassette() -> Cassette {
    let mut cassette = Cassette::new();

    // Interaction 0: hash_a (first occurrence)
    cassette.append(Interaction {
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
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"id": "first_response"})),
        },
        metadata: InteractionMetadata {
            protocol_id: "openai_chat".to_owned(),
            upstream_id: "openai".to_owned(),
            recorded_at: "2025-01-01T00:00:00Z".to_owned(),
            relative_offset_ms: 0,
        },
    });

    // Interaction 1: hash_a (second occurrence)
    cassette.append(Interaction {
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
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"id": "second_response"})),
        },
        metadata: InteractionMetadata {
            protocol_id: "openai_chat".to_owned(),
            upstream_id: "openai".to_owned(),
            recorded_at: "2025-01-01T00:01:00Z".to_owned(),
            relative_offset_ms: 60_000,
        },
    });

    // Interaction 2: hash_b
    cassette.append(Interaction {
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
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"data": []})),
        },
        metadata: InteractionMetadata {
            protocol_id: "openai_chat".to_owned(),
            upstream_id: "openai".to_owned(),
            recorded_at: "2025-01-01T00:02:00Z".to_owned(),
            relative_offset_ms: 120_000,
        },
    });

    cassette
}

/// Helper to convert `RecordedResponse` to `NonStream` variant for testing.
impl RecordedResponse {
    pub(super) fn into_non_stream(self) -> Option<NonStreamResponse> {
        match self {
            Self::NonStream {
                status,
                headers,
                body,
                parsed_json,
            } => Some(NonStreamResponse {
                status,
                headers,
                body,
                parsed_json,
            }),
            Self::Stream { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct NonStreamResponse {
    pub(super) status: u16,
    pub(super) headers: Vec<(String, String)>,
    pub(super) body: Vec<u8>,
    pub(super) parsed_json: Option<Value>,
}
