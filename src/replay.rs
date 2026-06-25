//! Native replay service for deterministic cassette playback.
//!
//! This module owns adapter-neutral replay orchestration. It canonicalizes an
//! observed request, advances the cassette matching engine, and returns an
//! owned replay response for the inbound HTTP adapter to render.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use tracing::{debug, error, info, warn};

use crate::cassette::{
    IgnorePathConfig, MatchOutcome, MismatchDiagnostic, RecordedRequest, RecordedResponse,
    ReplayMatchEngine, StreamEvent,
};
use crate::http_exchange::ObservedRequest;
use crate::protocol::{
    CHAT_COMPLETIONS_PATH, CHAT_COMPLETIONS_PROTOCOL_ID, is_streaming_chat_completions_request,
};

/// Thread-safe replay orchestration boundary.
#[derive(Debug, Clone)]
pub(crate) struct ReplayService {
    engine: Arc<Mutex<ReplayMatchEngine>>,
    context: ReplayContext,
    matched_count: Arc<AtomicU64>,
    mismatch_count: Arc<AtomicU64>,
}

impl ReplayService {
    /// Creates a replay service around a prepared match engine.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn new(engine: ReplayMatchEngine) -> Self {
        Self::with_context(engine, ReplayContext::default())
    }

    /// Creates a replay service with contextual observability fields.
    #[must_use]
    pub(crate) fn with_context(engine: ReplayMatchEngine, context: ReplayContext) -> Self {
        Self {
            engine: Arc::new(Mutex::new(engine)),
            context,
            matched_count: Arc::new(AtomicU64::new(0)),
            mismatch_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Returns replay match and mismatch counters for operational checks.
    #[cfg(test)]
    pub(crate) fn counters(&self) -> (u64, u64) {
        (
            self.matched_count.load(Ordering::Relaxed),
            self.mismatch_count.load(Ordering::Relaxed),
        )
    }

    /// Replays one chat completions request from the cassette.
    pub(crate) fn handle_chat_completions(
        &self,
        request: ObservedRequest,
    ) -> Result<ReplayResponse, ReplayError> {
        let is_stream_request = is_streaming_chat_completions_request(request.parsed_json.as_ref());
        self.reject_unreplayable_request_shape(&request)?;
        let recorded_request = canonicalize_observed_request(request)?;
        let observed_hash = recorded_hash(&recorded_request)?;
        let canonical_request = recorded_canonical_request(&recorded_request)?;
        self.match_recorded_response(observed_hash, canonical_request, is_stream_request)
    }

    fn reject_unreplayable_request_shape(
        &self,
        request: &ObservedRequest,
    ) -> Result<(), ReplayError> {
        if request.parsed_json.is_none() {
            self.log_rejection("malformed_json", &request.path);
            Err(ReplayError::MalformedJson)
        } else {
            Ok(())
        }
    }

    fn log_rejection(&self, outcome: &str, path: &str) {
        warn!(
            target: "spycatcher.harness.replay",
            "replay request rejected mode=replay protocol={protocol} outcome={outcome} \
             cassette={cassette} path={path}",
            protocol = self.context.protocol,
            cassette = self.context.cassette,
        );
    }

    fn match_recorded_response(
        &self,
        observed_hash: &str,
        canonical_request: &serde_json::Value,
        is_stream_request: bool,
    ) -> Result<ReplayResponse, ReplayError> {
        let mut guard = self.engine.lock().map_err(|error| {
            error!(
                target: "spycatcher.harness.replay",
                "failed to lock replay match engine: {error:?}"
            );
            ReplayError::Internal
        })?;

        let (interaction_id, response) = {
            let peek = guard.peek_match(observed_hash, canonical_request);
            match peek {
                MatchOutcome::Matched {
                    interaction_id,
                    interaction,
                } => (
                    interaction_id,
                    self.response_from_recorded(&interaction.response, is_stream_request)?,
                ),
                MatchOutcome::Mismatch(diagnostic) => {
                    drop(guard);
                    self.log_mismatch(&diagnostic);
                    return Err(ReplayError::Mismatch(diagnostic));
                }
            }
        };
        if !guard.commit_match(interaction_id) {
            error!(
                target: "spycatcher.harness.replay",
                "failed to commit previously peeked replay match interaction_id={interaction_id}"
            );
            return Err(ReplayError::Internal);
        }
        drop(guard);

        self.log_match(interaction_id, observed_hash);
        Ok(response)
    }

    fn log_match(&self, interaction_id: usize, observed_hash: &str) {
        let matched_count = self.matched_count.fetch_add(1, Ordering::Relaxed) + 1;
        let mismatch_count = self.mismatch_count.load(Ordering::Relaxed);
        info!(
            target: "spycatcher.harness.replay",
            "interaction replayed interaction_id={interaction_id} mode=replay \
             protocol={protocol} outcome=matched cassette={cassette} \
             observed_hash={observed_hash} matched_count={matched_count} \
             mismatch_count={mismatch_count}",
            protocol = self.context.protocol,
            cassette = self.context.cassette,
        );
    }

    fn log_mismatch(&self, diagnostic: &MismatchDiagnostic) {
        let mismatch_count = self.mismatch_count.fetch_add(1, Ordering::Relaxed) + 1;
        let matched_count = self.matched_count.load(Ordering::Relaxed);
        warn!(
            target: "spycatcher.harness.replay",
            "replay request mismatched interaction_id={interaction_id} mode=replay \
             protocol={protocol} outcome=mismatch cassette={cassette} \
             expected_hash={expected_hash} observed_hash={observed_hash} \
             matched_count={matched_count} mismatch_count={mismatch_count} reason={reason}",
            interaction_id = diagnostic.position.metric_id(),
            protocol = self.context.protocol,
            cassette = self.context.cassette,
            expected_hash = diagnostic.expected_hash,
            observed_hash = diagnostic.observed_hash,
            reason = diagnostic.reason_code(),
        );
    }

    fn response_from_recorded(
        &self,
        response: &RecordedResponse,
        is_stream_request: bool,
    ) -> Result<ReplayResponse, ReplayError> {
        match response {
            RecordedResponse::NonStream {
                status,
                headers,
                body,
                ..
            } => {
                if is_stream_request {
                    self.log_rejection("stream_cassette_required", CHAT_COMPLETIONS_PATH);
                    Err(ReplayError::StreamCassetteRequiredForStreamRequest)
                } else {
                    debug!(
                        target: "spycatcher.harness.replay",
                        "building replay response from recorded interaction \
                         is_stream_request={is_stream_request} response_kind=non_stream",
                    );
                    Ok(ReplayResponse {
                        status: *status,
                        headers: headers.clone(),
                        body: ReplayBody::OneShot(body.clone()),
                    })
                }
            }
            RecordedResponse::Stream {
                status,
                headers,
                events,
                ..
            } => {
                debug!(
                    target: "spycatcher.harness.replay",
                    "building replay response from recorded interaction \
                     is_stream_request={is_stream_request} response_kind=stream \
                     event_count={}",
                    events.len(),
                );
                Ok(ReplayResponse {
                    status: *status,
                    headers: headers.clone(),
                    body: ReplayBody::Events(events.clone()),
                })
            }
        }
    }
}

fn canonicalize_observed_request(request: ObservedRequest) -> Result<RecordedRequest, ReplayError> {
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
    Ok(recorded_request)
}

fn recorded_hash(recorded_request: &RecordedRequest) -> Result<&str, ReplayError> {
    recorded_request
        .stable_hash
        .as_deref()
        .ok_or(ReplayError::Internal)
}

fn recorded_canonical_request(
    recorded_request: &RecordedRequest,
) -> Result<&serde_json::Value, ReplayError> {
    recorded_request
        .canonical_request
        .as_ref()
        .ok_or(ReplayError::Internal)
}

/// Contextual fields shared by replay-mode observability events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReplayContext {
    /// Configured cassette name.
    pub(crate) cassette: String,
    /// Protocol identifier served by this replay service.
    pub(crate) protocol: &'static str,
}

impl ReplayContext {
    /// Creates replay context from a configured cassette name.
    #[must_use]
    pub(crate) const fn new(cassette: String) -> Self {
        Self {
            cassette,
            protocol: CHAT_COMPLETIONS_PROTOCOL_ID,
        }
    }
}

impl Default for ReplayContext {
    fn default() -> Self {
        Self::new("unknown".to_owned())
    }
}

trait InteractionPositionMetrics {
    fn metric_id(&self) -> usize;
}

impl InteractionPositionMetrics for crate::cassette::InteractionPosition {
    fn metric_id(&self) -> usize {
        match *self {
            Self::Expected(index) | Self::Exhausted(index) | Self::KeyedMiss(index) => index,
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
    /// Recorded body data.
    pub(crate) body: ReplayBody,
}

/// Replay body data independent of the inbound HTTP framework.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReplayBody {
    /// Raw recorded body bytes for non-stream responses.
    OneShot(Vec<u8>),
    /// Parsed stream events preserved in observed order.
    Events(Vec<StreamEvent>),
}

/// Request-level replay failures reported to the HTTP adapter.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReplayError {
    /// The observed request does not match the next replayable interaction.
    Mismatch(MismatchDiagnostic),
    /// Raw-transcript streaming replay is outside this task's scope.
    // FIXME(task 2.1.3): return this when exact raw-transcript streaming replay
    // errors are introduced.
    #[expect(
        dead_code,
        reason = "reserved for task 2.1.3 raw-transcript streaming replay errors"
    )]
    UnsupportedStream,
    /// A streaming request matched a non-stream cassette interaction.
    StreamCassetteRequiredForStreamRequest,
    /// Chat completions replay requires a valid JSON request body.
    MalformedJson,
    /// An internal replay invariant or lock failed.
    Internal,
}

#[cfg(test)]
#[path = "replay_tests.rs"]
mod tests;
