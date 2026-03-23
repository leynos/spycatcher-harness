//! Unit tests for cassette schema round-trips and validation.

use super::*;
use rstest::{fixture, rstest};
use serde_json::json;

#[rstest]
fn non_stream_interaction_round_trips_without_loss(sample_non_stream_interaction: Interaction) {
    let cassette = Cassette {
        interactions: vec![sample_non_stream_interaction],
        ..Cassette::new()
    };
    let mut bytes = Vec::new();
    cassette
        .write_to(&mut bytes)
        .expect("cassette serialization should succeed");

    let decoded =
        Cassette::from_reader(bytes.as_slice()).expect("cassette deserialization should succeed");

    assert_eq!(decoded, cassette);
}

#[rstest]
fn stream_interaction_round_trips_without_loss(sample_stream_interaction: Interaction) {
    let cassette = Cassette {
        interactions: vec![sample_stream_interaction],
        ..Cassette::new()
    };
    let mut bytes = Vec::new();
    cassette
        .write_to(&mut bytes)
        .expect("cassette serialization should succeed");

    let decoded =
        Cassette::from_reader(bytes.as_slice()).expect("cassette deserialization should succeed");

    assert_eq!(decoded, cassette);
}

#[rstest]
fn unsupported_format_version_is_rejected() {
    let supported = CassetteFormatVersion::SUPPORTED.as_u32();
    let json = r#"{"format_version":2,"interactions":[]}"#;

    let error = Cassette::from_reader(json.as_bytes()).expect_err("unsupported version must fail");

    assert!(matches!(
        error,
        HarnessError::UnsupportedCassetteFormatVersion {
            found: 2,
            supported: found_supported,
        }
        if found_supported == supported
    ));
}

#[rstest]
fn malformed_cassette_is_rejected() {
    let json = r#"{"interactions":[]}"#;

    let error =
        Cassette::from_reader(json.as_bytes()).expect_err("missing format_version must fail");

    assert!(matches!(error, HarnessError::InvalidCassette { .. }));
}

#[fixture]
fn sample_non_stream_interaction() -> Interaction {
    let request = RecordedRequest {
        method: "POST".to_owned(),
        path: "/v1/chat/completions".to_owned(),
        query: "stream=false".to_owned(),
        headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        body: br#"{"model":"gpt-test","stream":false}"#.to_vec(),
        parsed_json: Some(json!({"model": "gpt-test", "stream": false})),
        canonical_request: None,
        stable_hash: None,
    };
    let response = RecordedResponse::NonStream {
        status: 200,
        headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        body: br#"{"id":"chatcmpl-1","choices":[]}"#.to_vec(),
        parsed_json: Some(json!({"id": "chatcmpl-1", "choices": []})),
    };
    let metadata = InteractionMetadata {
        protocol_id: "openai.chat_completions.v1".to_owned(),
        upstream_id: "openrouter".to_owned(),
        recorded_at: "2026-03-10T00:00:00Z".to_owned(),
        relative_offset_ms: 0,
    };
    Interaction {
        request,
        response,
        metadata,
    }
}

#[fixture]
fn sample_stream_interaction(sample_non_stream_interaction: Interaction) -> Interaction {
    let mut interaction = sample_non_stream_interaction;
    interaction.response = RecordedResponse::Stream {
        status: 200,
        headers: vec![("content-type".to_owned(), "text/event-stream".to_owned())],
        events: vec![
            StreamEvent::Comment {
                text: "OPENROUTER PROCESSING".to_owned(),
            },
            StreamEvent::Data {
                raw: "{\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}".to_owned(),
                parsed_json: Some(json!({"choices": [{"delta": {"content": "hi"}}]})),
            },
        ],
        raw_transcript:
            b": OPENROUTER PROCESSING\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n"
                .to_vec(),
        timing: Some(StreamTiming {
            ttft_ms: 12,
            chunk_offsets_ms: vec![12, 17],
        }),
    };
    interaction.metadata.relative_offset_ms = 17;
    interaction
}
