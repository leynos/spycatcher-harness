//! Native replay service for deterministic cassette playback.
//!
//! This module owns adapter-neutral replay orchestration. It canonicalizes an
//! observed request, advances the cassette matching engine, and returns an
//! owned replay response for the inbound HTTP adapter to render.

use std::sync::{Arc, Mutex};

use log::error;

use crate::cassette::{
    IgnorePathConfig, MatchOutcome, MismatchDiagnostic, RecordedRequest, RecordedResponse,
    ReplayMatchEngine,
};
use crate::http_exchange::ObservedRequest;
use crate::protocol::is_streaming_chat_completions_request;

/// Thread-safe replay orchestration boundary.
#[derive(Debug, Clone)]
pub(crate) struct ReplayService {
    engine: Arc<Mutex<ReplayMatchEngine>>,
}

impl ReplayService {
    /// Creates a replay service around a prepared match engine.
    #[must_use]
    pub(crate) fn new(engine: ReplayMatchEngine) -> Self {
        Self {
            engine: Arc::new(Mutex::new(engine)),
        }
    }

    /// Replays one non-stream chat completions request from the cassette.
    pub(crate) fn handle_chat_completions(
        &self,
        request: ObservedRequest,
    ) -> Result<ReplayResponse, ReplayError> {
        if is_streaming_chat_completions_request(request.parsed_json.as_ref()) {
            return Err(ReplayError::UnsupportedStream);
        }
        if request.parsed_json.is_none() {
            return Err(ReplayError::MalformedJson);
        }

        let mut recorded_request = observed_to_recorded_request(request);
        recorded_request
            .populate_canonical_fields(&IgnorePathConfig::default())
            .map_err(|error| {
                error!(
                    target: "spycatcher.harness.replay",
                    "failed to canonicalize replay request: {error}"
                );
                ReplayError::Internal
            })?;
        let observed_hash = recorded_request
            .stable_hash
            .as_deref()
            .ok_or(ReplayError::Internal)?;
        let canonical_request = recorded_request
            .canonical_request
            .as_ref()
            .ok_or(ReplayError::Internal)?;

        let response = {
            let mut guard = self.engine.lock().map_err(|error| {
                error!(
                    target: "spycatcher.harness.replay",
                    "failed to lock replay match engine: {error:?}"
                );
                ReplayError::Internal
            })?;
            match guard.next_match(observed_hash, canonical_request) {
                MatchOutcome::Matched(interaction) => interaction.response.clone(),
                MatchOutcome::Mismatch(diagnostic) => {
                    return Err(ReplayError::Mismatch(diagnostic));
                }
            }
        };

        match response {
            RecordedResponse::NonStream {
                status,
                headers,
                body,
                ..
            } => Ok(ReplayResponse {
                status,
                headers,
                body,
            }),
            RecordedResponse::Stream { .. } => Err(ReplayError::UnsupportedStream),
        }
    }
}

fn observed_to_recorded_request(request: ObservedRequest) -> RecordedRequest {
    RecordedRequest {
        method: request.method,
        path: request.path,
        query: request.query,
        headers: request.headers,
        body: request.body,
        parsed_json: request.parsed_json,
        canonical_request: None,
        stable_hash: None,
    }
}

/// Replay response data independent of the inbound HTTP framework.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReplayResponse {
    /// HTTP status code recorded in the cassette.
    pub(crate) status: u16,
    /// Persisted selected response headers in recorded order.
    pub(crate) headers: Vec<(String, String)>,
    /// Raw recorded body bytes.
    pub(crate) body: Vec<u8>,
}

/// Request-level replay failures reported to the HTTP adapter.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReplayError {
    /// The observed request does not match the next replayable interaction.
    Mismatch(MismatchDiagnostic),
    /// Streaming replay is outside this task's scope.
    UnsupportedStream,
    /// Chat completions replay requires a valid JSON request body.
    MalformedJson,
    /// An internal replay invariant or lock failed.
    Internal,
}

#[cfg(test)]
mod tests {
    //! Unit tests for adapter-neutral replay orchestration.

    use super::*;
    use crate::cassette::{
        Cassette, CassetteFormatVersion, Interaction, InteractionMetadata, RecordedResponse,
    };
    use crate::config::MatchMode;
    use crate::protocol::CHAT_COMPLETIONS_PATH;
    use rstest::rstest;

    #[rstest]
    fn matching_non_stream_request_returns_recorded_response() {
        let request = sample_observed_request(br#"{"model":"test","messages":[]}"#);
        let cassette = cassette_for_request(
            request.clone(),
            RecordedResponse::NonStream {
                status: 201,
                headers: vec![("x-replay".to_owned(), "yes".to_owned())],
                body: b"recorded".to_vec(),
                parsed_json: None,
            },
        )
        .expect("request should canonicalize");
        let service = ReplayService::new(
            ReplayMatchEngine::new(cassette, MatchMode::SequentialStrict)
                .expect("cassette should build replay engine"),
        );

        let response = service
            .handle_chat_completions(request)
            .expect("request should replay");

        assert_eq!(response.status, 201);
        assert_eq!(
            response.headers,
            vec![("x-replay".to_owned(), "yes".to_owned())]
        );
        assert_eq!(response.body, b"recorded");
    }

    #[rstest]
    fn mismatched_request_returns_diagnostic() {
        let recorded = sample_observed_request(br#"{"model":"test","messages":[]}"#);
        let observed = sample_observed_request(br#"{"model":"other","messages":[]}"#);
        let cassette = cassette_for_request(
            recorded,
            RecordedResponse::NonStream {
                status: 200,
                headers: vec![],
                body: vec![],
                parsed_json: None,
            },
        )
        .expect("request should canonicalize");
        let service = ReplayService::new(
            ReplayMatchEngine::new(cassette, MatchMode::SequentialStrict)
                .expect("cassette should build replay engine"),
        );

        let error = service
            .handle_chat_completions(observed)
            .expect_err("request should not match");

        assert!(matches!(error, ReplayError::Mismatch(_)));
    }

    #[rstest]
    fn stream_request_is_rejected_before_matching() {
        let recorded = sample_observed_request(br#"{"model":"test","messages":[]}"#);
        let cassette = cassette_for_request(
            recorded,
            RecordedResponse::NonStream {
                status: 200,
                headers: vec![],
                body: vec![],
                parsed_json: None,
            },
        )
        .expect("request should canonicalize");
        let service = ReplayService::new(
            ReplayMatchEngine::new(cassette, MatchMode::SequentialStrict)
                .expect("cassette should build replay engine"),
        );
        let streaming = sample_observed_request(br#"{"model":"test","stream":true,"messages":[]}"#);

        let error = service
            .handle_chat_completions(streaming)
            .expect_err("streaming replay request should fail");

        assert_eq!(error, ReplayError::UnsupportedStream);
    }

    #[rstest]
    fn matched_stream_response_is_rejected() {
        let request = sample_observed_request(br#"{"model":"test","messages":[]}"#);
        let cassette = cassette_for_request(
            request.clone(),
            RecordedResponse::Stream {
                status: 200,
                headers: vec![],
                events: vec![],
                raw_transcript: b"data: {}\n\n".to_vec(),
                timing: None,
            },
        )
        .expect("request should canonicalize");
        let service = ReplayService::new(
            ReplayMatchEngine::new(cassette, MatchMode::SequentialStrict)
                .expect("cassette should build replay engine"),
        );

        let error = service
            .handle_chat_completions(request)
            .expect_err("stream response should fail");

        assert_eq!(error, ReplayError::UnsupportedStream);
    }

    #[rstest]
    fn malformed_json_request_is_rejected_before_matching() {
        let recorded = sample_observed_request(br#"{"model":"test","messages":[]}"#);
        let cassette = cassette_for_request(
            recorded,
            RecordedResponse::NonStream {
                status: 200,
                headers: vec![],
                body: b"would hide the malformed body".to_vec(),
                parsed_json: None,
            },
        )
        .expect("request should canonicalize");
        let service = ReplayService::new(
            ReplayMatchEngine::new(cassette, MatchMode::SequentialStrict)
                .expect("cassette should build replay engine"),
        );
        let malformed = sample_observed_request(br#"{"model":"test""#);

        let error = service
            .handle_chat_completions(malformed)
            .expect_err("malformed JSON replay request should fail before matching");

        assert_eq!(error, ReplayError::MalformedJson);
    }

    #[rstest]
    fn concurrent_sequential_replay_consumes_duplicate_hashes_once_each() {
        let request = sample_observed_request(br#"{"model":"test","messages":[]}"#);
        let cassette = cassette_with_duplicate_requests(request.clone(), 8)
            .expect("requests should canonicalize");
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

        let mut bodies = handles
            .into_iter()
            .map(|handle| handle.join().expect("thread should not panic"))
            .collect::<Vec<_>>();
        bodies.sort();

        assert_eq!(
            bodies,
            (0..8)
                .map(|index| format!("response-{index}").into_bytes())
                .collect::<Vec<_>>()
        );
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
}
