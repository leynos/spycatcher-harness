//! Unit tests for adapter-neutral replay orchestration.

use super::*;
use crate::cassette::{
    Cassette, CassetteFormatVersion, Interaction, InteractionMetadata, RecordedResponse,
    StreamEvent,
};
use crate::config::MatchMode;
use crate::protocol::CHAT_COMPLETIONS_PATH;
use rstest::rstest;

#[rstest]
fn matching_non_stream_request_returns_recorded_response() {
    let request = sample_observed_request(br#"{"model":"test","messages":[]}"#);
    let service = service_with_single_interaction(
        request.clone(),
        RecordedResponse::NonStream {
            status: 201,
            headers: vec![("x-replay".to_owned(), "yes".to_owned())],
            body: b"recorded".to_vec(),
            parsed_json: None,
        },
    )
    .unwrap_or_else(|message| panic!("{message}"));

    let response = service
        .handle_chat_completions(request)
        .expect("request should replay");

    assert_eq!(response.status, 201);
    assert_eq!(
        response.headers,
        vec![("x-replay".to_owned(), "yes".to_owned())]
    );
    assert_eq!(response.body, ReplayBody::OneShot(b"recorded".to_vec()));
    assert_eq!(service.counters(), (1, 0));
}

#[rstest]
fn mismatched_request_returns_diagnostic() {
    let recorded = sample_observed_request(br#"{"model":"test","messages":[]}"#);
    let observed = sample_observed_request(br#"{"model":"other","messages":[]}"#);
    let service = service_with_single_interaction(
        recorded,
        RecordedResponse::NonStream {
            status: 200,
            headers: vec![],
            body: vec![],
            parsed_json: None,
        },
    )
    .unwrap_or_else(|message| panic!("{message}"));

    let error = service
        .handle_chat_completions(observed)
        .expect_err("request should not match");

    assert!(matches!(error, ReplayError::Mismatch(_)));
    assert_eq!(service.counters(), (0, 1));
}

#[rstest]
fn matched_stream_response_replays_as_events() {
    let request = sample_observed_request(br#"{"model":"test","messages":[]}"#);
    let service = service_with_single_interaction(
        request.clone(),
        RecordedResponse::Stream {
            status: 200,
            headers: vec![],
            events: vec![],
            raw_transcript: b"data: {}\n\n".to_vec(),
            timing: None,
        },
    )
    .unwrap_or_else(|message| panic!("{message}"));

    let response = service
        .handle_chat_completions(request)
        .expect("stream response should replay as events");

    assert_eq!(response.body, ReplayBody::Events(Vec::new()));
    assert_eq!(service.counters(), (1, 0));
}

#[rstest]
fn matched_stream_response_preserves_recorded_events() {
    let request = sample_observed_request(br#"{"model":"test","stream":true,"messages":[]}"#);
    let events = vec![
        StreamEvent::Comment {
            text: "OPENROUTER PROCESSING".to_owned(),
        },
        StreamEvent::Data {
            raw: "{\"id\":\"chunk\"}".to_owned(),
            parsed_json: Some(serde_json::json!({"id": "chunk"})),
        },
        StreamEvent::Data {
            raw: "[DONE]".to_owned(),
            parsed_json: None,
        },
    ];
    let service = service_with_single_interaction(
        request.clone(),
        RecordedResponse::Stream {
            status: 202,
            headers: vec![("x-stream".to_owned(), "yes".to_owned())],
            events: events.clone(),
            raw_transcript: Vec::new(),
            timing: None,
        },
    )
    .unwrap_or_else(|message| panic!("{message}"));

    let response = service
        .handle_chat_completions(request)
        .expect("stream request should replay from stream cassette");

    assert_eq!(response.status, 202);
    assert_eq!(
        response.headers,
        vec![("x-stream".to_owned(), "yes".to_owned())]
    );
    assert_eq!(response.body, ReplayBody::Events(events));
    assert_eq!(service.counters(), (1, 0));
}

#[rstest]
#[case(
    (
        br#"{"model":"test","stream":true,"messages":[]}"# as &[u8],
        b"" as &[u8],
        br#"{"model":"test","stream":true,"messages":[]}"# as &[u8],
        ReplayError::StreamCassetteRequiredForStreamRequest,
        "streaming replay request should fail",
    )
)]
#[case(
    (
        br#"{"model":"test","messages":[]}"# as &[u8],
        b"would hide the malformed body" as &[u8],
        br#"{"model":"test""# as &[u8],
        ReplayError::MalformedJson,
        "malformed JSON replay request should fail before matching",
    )
)]
#[case(
    (
        br#"{"model":"recorded""# as &[u8],
        b"must not replay for a different malformed body" as &[u8],
        br#"{"model":"observed""# as &[u8],
        ReplayError::MalformedJson,
        "malformed JSON must be rejected before cassette matching",
    )
)]
fn request_is_rejected_before_matching(
    #[case] test_case: (&[u8], &[u8], &[u8], ReplayError, &'static str),
) {
    let (recorded_body, cassette_body, trigger_body, expected_error, expect_err_msg) = test_case;
    let recorded = sample_observed_request(recorded_body);
    let service = service_with_single_interaction(
        recorded,
        RecordedResponse::NonStream {
            status: 200,
            headers: vec![],
            body: cassette_body.to_vec(),
            parsed_json: None,
        },
    )
    .unwrap_or_else(|message| panic!("{message}"));
    let trigger = sample_observed_request(trigger_body);

    let error = service
        .handle_chat_completions(trigger)
        .expect_err(expect_err_msg);

    assert_eq!(error, expected_error);
    let expected_counters = if matches!(
        expected_error,
        ReplayError::StreamCassetteRequiredForStreamRequest
    ) {
        (1, 0)
    } else {
        (0, 0)
    };
    assert_eq!(service.counters(), expected_counters);
}

#[rstest]
fn concurrent_sequential_replay_consumes_duplicate_hashes_once_each() {
    let request = sample_observed_request(br#"{"model":"test","messages":[]}"#);
    let cassette =
        cassette_with_duplicate_requests(request.clone(), 8).expect("requests should canonicalize");
    let service = ReplayService::new(
        ReplayMatchEngine::new(cassette, MatchMode::SequentialStrict)
            .expect("cassette should build replay engine"),
    );
    let handles = (0..8)
        .map(|_| {
            let replay_service = service.clone();
            let replay_request = request.clone();
            std::thread::spawn(move || {
                replay_service
                    .handle_chat_completions(replay_request)
                    .expect("duplicate request should replay")
                    .body
            })
        })
        .collect::<Vec<_>>();

    let bodies = handles
        .into_iter()
        .map(|handle| handle.join().expect("thread should not panic"))
        .collect::<Vec<_>>();
    let mut body_bytes = bodies
        .into_iter()
        .map(|body| match body {
            ReplayBody::OneShot(bytes) => bytes,
            ReplayBody::Events(_) => panic!("duplicate non-stream responses should be bytes"),
        })
        .collect::<Vec<_>>();
    body_bytes.sort();

    assert_eq!(
        body_bytes,
        (0..8)
            .map(|index| format!("response-{index}").into_bytes())
            .collect::<Vec<_>>()
    );
    assert_eq!(service.counters(), (8, 0));
}

fn sample_observed_request(body: &[u8]) -> ObservedRequest {
    ObservedRequest {
        method: "POST".to_owned(),
        path: CHAT_COMPLETIONS_PATH.to_owned(),
        query: String::new(),
        headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        forward_headers: vec![],
        body: body.to_vec(),
        parsed_json: serde_json::from_slice(body).ok(),
    }
}

fn cassette_for_request(
    request: ObservedRequest,
    response: RecordedResponse,
) -> Result<Cassette, crate::cassette::CanonicalError> {
    let mut recorded = RecordedRequest {
        method: request.method,
        path: request.path,
        query: request.query,
        headers: request.headers,
        body: request.body,
        parsed_json: request.parsed_json,
        canonical_request: None,
        stable_hash: None,
    };
    recorded.populate_canonical_fields(&IgnorePathConfig::default())?;

    Ok(Cassette {
        format_version: CassetteFormatVersion::SUPPORTED,
        interactions: vec![Interaction {
            request: recorded,
            response,
            metadata: InteractionMetadata {
                protocol_id: "openai.chat_completions.v1".to_owned(),
                upstream_id: "test".to_owned(),
                recorded_at: "2026-05-08T00:00:00Z".to_owned(),
                relative_offset_ms: 0,
            },
        }],
    })
}

fn service_with_single_interaction(
    request: ObservedRequest,
    response: RecordedResponse,
) -> Result<ReplayService, &'static str> {
    let cassette =
        cassette_for_request(request, response).map_err(|_| "request should canonicalize")?;
    let engine = ReplayMatchEngine::new(cassette, MatchMode::SequentialStrict)
        .map_err(|_| "cassette should build replay engine")?;
    Ok(ReplayService::new(engine))
}

fn cassette_with_duplicate_requests(
    request: ObservedRequest,
    count: usize,
) -> Result<Cassette, crate::cassette::CanonicalError> {
    let mut recorded = RecordedRequest {
        method: request.method,
        path: request.path,
        query: request.query,
        headers: request.headers,
        body: request.body,
        parsed_json: request.parsed_json,
        canonical_request: None,
        stable_hash: None,
    };
    recorded.populate_canonical_fields(&IgnorePathConfig::default())?;
    let interactions = (0..count)
        .map(|index| Interaction {
            request: recorded.clone(),
            response: RecordedResponse::NonStream {
                status: 200,
                headers: Vec::new(),
                body: format!("response-{index}").into_bytes(),
                parsed_json: None,
            },
            metadata: InteractionMetadata {
                protocol_id: "openai.chat_completions.v1".to_owned(),
                upstream_id: "test".to_owned(),
                recorded_at: "2026-05-08T00:00:00Z".to_owned(),
                relative_offset_ms: index as u64,
            },
        })
        .collect();

    Ok(Cassette {
        format_version: CassetteFormatVersion::SUPPORTED,
        interactions,
    })
}
