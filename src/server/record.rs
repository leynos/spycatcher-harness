//! Record-mode request orchestration and HTTP response rendering.
//!
//! The inbound handler remains thin by translating `axum` requests into the
//! adapter-neutral exchange types defined here, delegating proxying and
//! cassette assembly to small testable helpers.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use log::{error, info, warn};
use tokio::task::spawn_blocking;

use crate::cassette::{
    CassetteAppender, IgnorePathConfig, Interaction, RecordedRequest, RecordedResponse,
    filesystem::FilesystemCassetteStore,
};
use crate::config::{RedactionConfig, UpstreamConfig};
use crate::http_exchange::{ObservedRequest, ProxyBody, ProxyResponse, redact_headers};
use crate::protocol::{
    CHAT_COMPLETIONS_PROTOCOL_ID, is_streaming_chat_completions_request, upstream_id,
};
use crate::server::record_metadata::{MetadataFactory, SessionMetadata};
use crate::upstream::{
    ChatCompletionsRequest, ChatCompletionsUpstream, EnvProvider, ProcessEnvProvider,
    ReqwestUpstreamClient,
};
#[cfg(test)]
use crate::{HarnessError, HarnessResult};

/// Shared, lock-protected cassette store used by cloned record services.
type SharedCassetteStore = Arc<Mutex<FilesystemCassetteStore>>;

#[path = "record_stream/mod.rs"]
mod record_stream;

#[path = "record_state.rs"]
mod record_state;

/// Shared record-mode application state for the inbound server.
#[derive(Debug, Clone)]
pub(crate) struct RecordAppState {
    /// Service that forwards requests and persists completed interactions.
    pub(crate) service: RecordService<ReqwestUpstreamClient, ProcessEnvProvider, SessionMetadata>,
}

/// Small orchestration boundary for one record-mode exchange.
#[derive(Debug, Clone)]
pub(crate) struct RecordService<U, E, M> {
    /// Shared store used to append interactions from concurrent requests.
    cassette_store: SharedCassetteStore,
    /// Upstream client used to forward chat-completions requests.
    upstream_client: U,
    /// Injectable source for the upstream API key.
    env_provider: E,
    /// Factory for timestamps and session-relative offsets.
    metadata: M,
    /// Upstream endpoint and credential-variable configuration.
    upstream: UpstreamConfig,
    /// Header redaction policy applied before cassette persistence.
    redaction: RedactionConfig,
    /// Number of interactions successfully persisted in this session.
    recorded_count: Arc<AtomicU64>,
    /// Number of upstream, build, or persistence failures observed.
    failure_count: Arc<AtomicU64>,
    /// Next sequence number used to correlate request diagnostics.
    interaction_seq: Arc<AtomicU64>,
}

/// Request-level record-mode failures reported to the HTTP adapter.
#[derive(Debug, PartialEq)]
pub(crate) enum RecordError {
    /// The configured credential variable was absent from the environment provider.
    MissingApiKeyNotConfigured,
    /// An upstream, metadata, or cassette operation failed.
    Internal,
}

/// Timing context retained between an upstream response and cassette persistence.
#[derive(Debug, Clone, Copy)]
struct RecordTiming {
    /// Milliseconds spent waiting for the upstream non-stream response.
    upstream_latency_ms: u128,
    /// Instant used to derive the interaction's session-relative offset.
    interaction_start: Instant,
}

impl<U, E, M> RecordService<U, E, M>
where
    U: ChatCompletionsUpstream + Clone + Send + Sync + 'static,
    E: EnvProvider + Clone + Send + Sync + 'static,
    M: MetadataFactory,
{
    /// Reads the configured credential through the injected environment provider.
    /// For example, a missing variable yields `MissingApiKeyNotConfigured`.
    fn resolve_api_key(&self) -> Result<String, RecordError> {
        self.env_provider
            .read(&self.upstream.api_key_env)
            .ok_or_else(|| {
                warn!(
                    target: "spycatcher.harness.record",
                    "upstream API key not found env_var={env_var} mode=record protocol={protocol}",
                    env_var = self.upstream.api_key_env,
                    protocol = CHAT_COMPLETIONS_PROTOCOL_ID,
                );
                RecordError::MissingApiKeyNotConfigured
            })
    }

    /// Returns persisted-interaction and failure counts in that order.
    /// For example, one successful append and no failures yields `(1, 0)`.
    pub(crate) fn counters(&self) -> (u64, u64) {
        (
            self.recorded_count.load(Ordering::Relaxed),
            self.failure_count.load(Ordering::Relaxed),
        )
    }

    /// Proxies one non-stream chat completions request and records it.
    pub(crate) async fn handle_chat_completions(
        &self,
        request: ObservedRequest,
    ) -> Result<ProxyResponse, RecordError> {
        let interaction_start = Instant::now();

        if is_streaming_chat_completions_request(request.parsed_json.as_ref()) {
            return self
                .handle_streaming_chat_completions(request, interaction_start)
                .await;
        }
        let api_key = self.resolve_api_key()?;

        let interaction_id = format!(
            "{proto}-{seq}",
            proto = CHAT_COMPLETIONS_PROTOCOL_ID,
            seq = self.interaction_seq.fetch_add(1, Ordering::Relaxed),
        );
        let upstream_start = Instant::now();

        let upstream_result = self
            .upstream_client
            .send_chat_completions(ChatCompletionsRequest {
                config: &self.upstream,
                api_key: &api_key,
                headers: &request.forward_headers,
                body: &request.body,
                query: &request.query,
            })
            .await;

        let upstream_latency = upstream_start.elapsed().as_millis();

        let upstream_response = upstream_result.map_err(|err| {
            self.failure_count.fetch_add(1, Ordering::Relaxed);
            error!(
                target: "spycatcher.harness.record",
                concat!(
                    "upstream request failed interaction_id={interaction_id} ",
                    "mode=record protocol={protocol} upstream_latency_ms={upstream_latency} ",
                    "outcome=failed cassette={cassette} error={err}",
                ),
                protocol = CHAT_COMPLETIONS_PROTOCOL_ID,
                cassette = upstream_id(self.upstream.kind),
                interaction_id = interaction_id,
                upstream_latency = upstream_latency,
                err = err,
            );
            RecordError::Internal
        })?;

        let record_result = self
            .record_response(
                (request, &upstream_response),
                &interaction_id,
                RecordTiming {
                    upstream_latency_ms: upstream_latency,
                    interaction_start,
                },
            )
            .await;

        if let Err(error) = record_result {
            self.log_recording_failure(&interaction_id, &error);
        }

        Ok(ProxyResponse {
            status: upstream_response.status,
            headers: upstream_response.proxy_headers,
            body: ProxyBody::Buffered(upstream_response.body),
        })
    }

    /// Keeps recording failures from replacing a successful upstream response.
    /// For example, a metadata error is counted and logged while the upstream 200 is returned.
    fn log_recording_failure(&self, interaction_id: &str, error: &RecordError) {
        self.failure_count.fetch_add(1, Ordering::Relaxed);
        error!(
            target: "spycatcher.harness.record",
            concat!(
                "recording failed interaction_id={interaction_id} mode=record ",
                "protocol={protocol} outcome=record_failed cassette={cassette} error={error:?}",
            ),
            protocol = CHAT_COMPLETIONS_PROTOCOL_ID,
            cassette = upstream_id(self.upstream.kind),
            interaction_id = interaction_id,
            error = error,
        );
    }

    /// Builds and persists a non-stream response, logging the outcome.
    /// For example, a successful append increments the recorded count once.
    async fn record_response(
        &self,
        (request, upstream_response): (ObservedRequest, &crate::http_exchange::ObservedResponse),
        interaction_id: &str,
        timing: RecordTiming,
    ) -> Result<(), RecordError> {
        match self.build_interaction(request, upstream_response, timing.interaction_start) {
            Ok(interaction) => {
                self.append_interaction(interaction).await.map_err(|e| {
                    error!(
                        target: "spycatcher.harness.record",
                        concat!(
                            "cassette write failed interaction_id={interaction_id} ",
                            "mode=record protocol={protocol} upstream_latency_ms={upstream_latency} ",
                            "outcome=write_failed cassette={cassette} error={e:?}",
                        ),
                        protocol = CHAT_COMPLETIONS_PROTOCOL_ID,
                        upstream_latency = timing.upstream_latency_ms,
                        cassette = upstream_id(self.upstream.kind),
                        interaction_id = interaction_id,
                        e = e,
                    );
                    e
                })?;
                self.recorded_count.fetch_add(1, Ordering::Relaxed);
                let (recorded_count, failure_count) = self.counters();
                info!(
                    target: "spycatcher.harness.record",
                    concat!(
                        "interaction recorded interaction_id={interaction_id} ",
                        "mode=record protocol={protocol} upstream_latency_ms={upstream_latency} ",
                        "outcome=recorded cassette={cassette} recorded_count={recorded_count} ",
                        "failure_count={failure_count}",
                    ),
                    protocol = CHAT_COMPLETIONS_PROTOCOL_ID,
                    upstream_latency = timing.upstream_latency_ms,
                    cassette = upstream_id(self.upstream.kind),
                    interaction_id = interaction_id,
                    recorded_count = recorded_count,
                    failure_count = failure_count,
                );
                Ok(())
            }
            Err(e) => {
                error!(
                    target: "spycatcher.harness.record",
                    concat!(
                        "cassette build failed interaction_id={interaction_id} ",
                        "mode=record protocol={protocol} upstream_latency_ms={upstream_latency} ",
                        "outcome=build_failed cassette={cassette} error={e:?}",
                    ),
                    protocol = CHAT_COMPLETIONS_PROTOCOL_ID,
                    upstream_latency = timing.upstream_latency_ms,
                    cassette = upstream_id(self.upstream.kind),
                    interaction_id = interaction_id,
                    e = e,
                );
                Err(e)
            }
        }
    }

    /// Builds one cassette interaction with redacted headers and canonical fields.
    /// For example, response bytes are retained while configured headers are redacted.
    fn build_interaction(
        &self,
        request: ObservedRequest,
        response: &crate::http_exchange::ObservedResponse,
        interaction_start: Instant,
    ) -> Result<Interaction, RecordError> {
        let mut recorded_request = RecordedRequest {
            method: request.method,
            path: request.path,
            query: request.query,
            headers: redact_headers(&request.headers, &self.redaction),
            body: request.body,
            parsed_json: request.parsed_json,
            canonical_request: None,
            stable_hash: None,
        };
        recorded_request
            .populate_canonical_fields(&IgnorePathConfig::default())
            .map_err(|e| {
                error!(
                    target: "spycatcher.harness.record",
                    "failed to populate canonical fields: {e}"
                );
                RecordError::Internal
            })?;

        let metadata = self.metadata.create_at(interaction_start).map_err(|e| {
            error!(
                target: "spycatcher.harness.record",
                "failed to create interaction metadata: {e}"
            );
            RecordError::Internal
        })?;

        Ok(Interaction {
            request: recorded_request,
            response: RecordedResponse::NonStream {
                status: response.status,
                headers: redact_headers(&response.headers, &self.redaction),
                body: response.body.clone(),
                parsed_json: response.parsed_json.clone(),
            },
            metadata,
        })
    }

    /// Appends through the shared store on a blocking task so async workers can proceed.
    /// For example, a poisoned store lock yields `RecordError::Internal`.
    async fn append_interaction(&self, interaction: Interaction) -> Result<(), RecordError> {
        let store = Arc::clone(&self.cassette_store);
        spawn_blocking(move || {
            let mut guard = store.lock().map_err(|e| {
                error!(
                    target: "spycatcher.harness.record",
                    "failed to lock cassette_store: {e:?}"
                );
                RecordError::Internal
            })?;
            CassetteAppender::append(&mut *guard, interaction).map_err(|e| {
                error!(
                    target: "spycatcher.harness.record",
                    "failed to append interaction: {e:?}"
                );
                RecordError::Internal
            })
        })
        .await
        .map_err(|e| {
            error!(
                target: "spycatcher.harness.record",
                "failed to spawn blocking: {e:?}"
            );
            RecordError::Internal
        })?
    }
}

#[cfg(test)]
#[path = "record_failure_tests.rs"]
mod failure_tests;

#[cfg(test)]
#[path = "record_tests_helpers.rs"]
pub(crate) mod record_tests_helpers;

#[cfg(test)]
#[path = "record_concurrent_tests.rs"]
mod concurrent_tests;

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
