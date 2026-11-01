//! Native replay service for deterministic cassette playback.
//!
//! This module owns adapter-neutral replay orchestration. It canonicalizes an
//! observed request, advances the cassette matching engine, and returns an
//! owned replay response for the inbound HTTP adapter to render.

use std::sync::{
    Arc,
    Mutex,
    atomic::{AtomicU64, Ordering},
};

use tracing::{debug, error, info, warn};

use crate::{
    cassette::{MatchOutcome, MismatchDiagnostic, RecordedResponse, ReplayMatchEngine},
    http_exchange::ObservedRequest,
    protocol::{CHAT_COMPLETIONS_PATH, is_streaming_chat_completions_request},
    replay_observability::{
        ReplayMetricLabels,
        record_replay_commit_failure,
        record_replay_mismatch,
        record_replay_rejection,
        record_replay_request,
    },
};

#[path = "replay_request.rs"]
mod replay_request;

#[path = "replay_types.rs"]
mod replay_types;

use replay_request::{canonicalize_observed_request, recorded_canonical_request, recorded_hash};
pub(crate) use replay_types::{ReplayBody, ReplayContext, ReplayError, ReplayResponse};

/// Thread-safe replay orchestration boundary.
#[derive(Debug, Clone)]
pub(crate) struct ReplayService {
    /// Shared cassette matcher guarded while a request is peeked and committed.
    engine: Arc<Mutex<ReplayMatchEngine>>,
    /// Cassette and protocol labels attached to this service's telemetry.
    context: ReplayContext,
    /// Number of successfully replayed interactions reported by this service.
    matched_count: Arc<AtomicU64>,
    /// Number of cassette mismatches reported by this service.
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

    /// Returns bounded replay metric labels for this service.
    #[must_use]
    pub(crate) const fn metric_labels(&self) -> ReplayMetricLabels {
        ReplayMetricLabels::new(self.context.protocol, CHAT_COMPLETIONS_PATH)
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

    /// Rejects requests without a parsed JSON body before cassette matching.
    /// For example, a request without parsed JSON returns `MalformedJson`.
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

    /// Records rejection metrics and logs the outcome with replay context.
    /// For example, `malformed_json` labels the rejection metric and warning.
    fn log_rejection(&self, outcome: &'static str, path: &str) {
        let labels = self.metric_labels();
        record_replay_request(&labels, outcome);
        record_replay_rejection(&labels, outcome);
        warn!(
            target: "spycatcher.harness.replay",
            concat!(
                "replay request rejected mode=replay protocol={protocol} outcome={outcome} ",
                "cassette={cassette} path={path}",
            ),
            protocol = self.context.protocol,
            cassette = self.context.cassette,
            outcome = outcome,
            path = path,
        );
    }

    /// Peeks at a cassette match, builds its response, and commits the interaction.
    /// For example, a valid matched response consumes one interaction; a
    /// rejected response leaves that interaction available.
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
            let labels = self.metric_labels();
            record_replay_request(&labels, "commit_failure");
            record_replay_commit_failure(&labels);
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

    /// Records a successful match in metrics, counters, and the replay log.
    /// For example, logging interaction 2 increments the matched count and
    /// records the `matched` request outcome.
    fn log_match(&self, interaction_id: usize, observed_hash: &str) {
        record_replay_request(&self.metric_labels(), "matched");
        let matched_count = self.matched_count.fetch_add(1, Ordering::Relaxed) + 1;
        let mismatch_count = self.mismatch_count.load(Ordering::Relaxed);
        info!(
            target: "spycatcher.harness.replay",
            concat!(
                "interaction replayed interaction_id={interaction_id} mode=replay ",
                "protocol={protocol} outcome=matched cassette={cassette} ",
                "observed_hash={observed_hash} matched_count={matched_count} ",
                "mismatch_count={mismatch_count}",
            ),
            protocol = self.context.protocol,
            cassette = self.context.cassette,
            interaction_id = interaction_id,
            observed_hash = observed_hash,
            matched_count = matched_count,
            mismatch_count = mismatch_count,
        );
    }

    /// Records mismatch metrics and logs the expected and observed request details.
    /// For example, a diagnostic at `Expected(3)` reports position 3 and
    /// increments the mismatch count.
    fn log_mismatch(&self, diagnostic: &MismatchDiagnostic) {
        let labels = self.metric_labels();
        record_replay_request(&labels, "mismatch");
        record_replay_mismatch(&labels, diagnostic.reason_code());
        let mismatch_count = self.mismatch_count.fetch_add(1, Ordering::Relaxed) + 1;
        let matched_count = self.matched_count.load(Ordering::Relaxed);
        warn!(
            target: "spycatcher.harness.replay",
            concat!(
                "replay request mismatched interaction_id={interaction_id} mode=replay ",
                "protocol={protocol} outcome=mismatch cassette={cassette} ",
                "expected_hash={expected_hash} observed_hash={observed_hash} ",
                "matched_count={matched_count} mismatch_count={mismatch_count} reason={reason}",
            ),
            interaction_id = diagnostic.position.metric_id(),
            protocol = self.context.protocol,
            cassette = self.context.cassette,
            expected_hash = diagnostic.expected_hash,
            observed_hash = diagnostic.observed_hash,
            reason = diagnostic.reason_code(),
            matched_count = matched_count,
            mismatch_count = mismatch_count,
        );
    }

    /// Builds a replay response while preserving the recorded body shape.
    /// For example, a non-stream recording retains its status, headers and bytes.
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
            } => self.response_from_non_stream(
                NonStreamReplayParts {
                    status: *status,
                    headers,
                    body,
                },
                is_stream_request,
            ),
            RecordedResponse::Stream {
                status,
                headers,
                events,
                ..
            } => {
                debug!(
                    target: "spycatcher.harness.replay",
                    concat!(
                        "building replay response from recorded interaction ",
                        "is_stream_request={is_stream_request} response_kind=stream ",
                        "event_count={}",
                    ),
                    events.len(),
                    is_stream_request = is_stream_request,
                );
                Ok(ReplayResponse {
                    status: *status,
                    headers: headers.clone(),
                    body: ReplayBody::Events(events.clone()),
                })
            }
        }
    }

    /// Rejects streaming requests that only have a non-stream recording.
    ///
    /// Build the owned response only after validation so rejected requests do
    /// not clone cassette data.
    /// For example, a streaming request returns
    /// `StreamCassetteRequiredForStreamRequest` before copying the body.
    fn response_from_non_stream(
        &self,
        parts: NonStreamReplayParts<'_>,
        is_stream_request: bool,
    ) -> Result<ReplayResponse, ReplayError> {
        if is_stream_request {
            self.log_rejection("stream_cassette_required", CHAT_COMPLETIONS_PATH);
            return Err(ReplayError::StreamCassetteRequiredForStreamRequest);
        }

        debug!(
            target: "spycatcher.harness.replay",
            concat!(
                "building replay response from recorded interaction ",
                "is_stream_request={is_stream_request} response_kind=non_stream",
            ),
            is_stream_request = is_stream_request,
        );
        Ok(ReplayResponse {
            status: parts.status,
            headers: parts.headers.to_vec(),
            body: ReplayBody::OneShot(parts.body.to_vec()),
        })
    }
}

/// Provides an interaction index for cassette-position diagnostics.
trait InteractionPositionMetrics {
    /// Returns the numeric interaction index associated with this position.
    /// For example, `Expected(3)` yields 3.
    fn metric_id(&self) -> usize;
}

impl InteractionPositionMetrics for crate::cassette::InteractionPosition {
    fn metric_id(&self) -> usize {
        match *self {
            Self::Expected(index) | Self::Exhausted(index) | Self::KeyedMiss(index) => index,
        }
    }
}

/// Borrowed cassette fields used to build a non-stream replay response.
#[derive(Clone, Copy)]
struct NonStreamReplayParts<'a> {
    /// Status code recorded in the cassette.
    status: u16,
    /// Headers recorded in their original order.
    headers: &'a [(String, String)],
    /// Response bytes recorded in the cassette.
    body: &'a [u8],
}

#[cfg(test)]
#[path = "replay_tests/mod.rs"]
mod tests;
