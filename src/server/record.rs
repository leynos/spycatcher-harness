//! Record-mode request orchestration and HTTP response rendering.
//!
//! The inbound handler remains thin by translating `axum` requests into the
//! adapter-neutral exchange types defined here, delegating proxying and
//! cassette assembly to small testable helpers.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::http::{HeaderValue, Response, StatusCode};
use axum::response::IntoResponse;
use log::{error, info, warn};
use serde_json::json;
use tokio::task::spawn_blocking;

use crate::cassette::{
    CassetteAppender, IgnorePathConfig, Interaction, RecordedRequest, RecordedResponse,
    filesystem::FilesystemCassetteStore,
};
use crate::config::{HarnessConfig, RedactionConfig, UpstreamConfig};
use crate::http_exchange::{ObservedRequest, ProxyResponse, redact_headers};
use crate::protocol::{
    CHAT_COMPLETIONS_PROTOCOL_ID, is_streaming_chat_completions_request, upstream_id,
};
use crate::server::record_metadata::{MetadataFactory, SessionMetadata};
use crate::upstream::{
    ChatCompletionsRequest, ChatCompletionsUpstream, EnvProvider, ProcessEnvProvider,
    ReqwestUpstreamClient,
};
use crate::{HarnessError, HarnessResult};

type SharedCassetteStore = Arc<Mutex<FilesystemCassetteStore>>;

/// Shared record-mode application state for the inbound server.
#[derive(Debug, Clone)]
pub(crate) struct RecordAppState {
    pub(crate) service: RecordService<ReqwestUpstreamClient, ProcessEnvProvider, SessionMetadata>,
}

impl RecordAppState {
    /// Builds record-mode state from validated harness configuration.
    ///
    /// # Errors
    ///
    /// Returns a harness error when the cassette or upstream client cannot be
    /// opened.
    pub(crate) fn from_config(
        cfg: &HarnessConfig,
        store: FilesystemCassetteStore,
    ) -> HarnessResult<Self> {
        let upstream = cfg
            .upstream
            .clone()
            .ok_or_else(|| HarnessError::InvalidConfig {
                message: "upstream configuration is required for record mode".to_owned(),
            })?;

        Ok(Self {
            service: RecordService {
                cassette_store: Arc::new(Mutex::new(store)),
                upstream_client: ReqwestUpstreamClient::new()?,
                env_provider: ProcessEnvProvider,
                metadata: SessionMetadata::new(upstream.kind),
                upstream,
                redaction: cfg.redaction.clone(),
                recorded_count: Arc::new(AtomicU64::new(0)),
                failure_count: Arc::new(AtomicU64::new(0)),
                interaction_seq: Arc::new(AtomicU64::new(0)),
            },
        })
    }
}

/// Small orchestration boundary for one record-mode exchange.
#[derive(Debug, Clone)]
pub(crate) struct RecordService<U, E, M> {
    cassette_store: SharedCassetteStore,
    upstream_client: U,
    env_provider: E,
    metadata: M,
    upstream: UpstreamConfig,
    redaction: RedactionConfig,
    recorded_count: Arc<AtomicU64>,
    failure_count: Arc<AtomicU64>,
    interaction_seq: Arc<AtomicU64>,
}

/// Request-level record-mode failures mapped to concrete HTTP responses.
#[derive(Debug, PartialEq)]
pub(crate) enum RecordError {
    UnsupportedStream,
    MissingApiKeyEnv,
    Internal,
}

#[derive(Debug, Clone, Copy)]
struct RecordTiming {
    upstream_latency_ms: u128,
    interaction_start: Instant,
}

impl RecordError {
    const fn status_code(&self) -> StatusCode {
        match self {
            Self::UnsupportedStream => StatusCode::NOT_IMPLEMENTED,
            Self::MissingApiKeyEnv | Self::Internal => StatusCode::BAD_GATEWAY,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::UnsupportedStream => {
                "streaming chat completions are not implemented yet".to_owned()
            }
            Self::MissingApiKeyEnv => "upstream credentials are not configured".to_owned(),
            Self::Internal => "upstream request failed".to_owned(),
        }
    }
}

impl IntoResponse for RecordError {
    fn into_response(self) -> Response<axum::body::Body> {
        let message = self.message();
        let body_bytes = format!(r#"{{"error":{{"message":{}}}}}"#, json!(message));
        let body = axum::body::Body::from(body_bytes.into_bytes());
        let mut response = Response::new(body);
        *response.status_mut() = self.status_code();
        response.headers_mut().insert(
            axum::http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        response
    }
}

impl<U, E, M> RecordService<U, E, M>
where
    U: ChatCompletionsUpstream + Clone + Send + Sync + 'static,
    E: EnvProvider + Clone + Send + Sync + 'static,
    M: MetadataFactory,
{
    fn check_not_streaming(&self, request: &ObservedRequest) -> Result<(), RecordError> {
        if is_streaming_chat_completions_request(request.parsed_json.as_ref()) {
            warn!(
                target: "spycatcher.harness.record",
                "streaming request rejected path={path} mode=record \
                 protocol={protocol} cassette={cassette}",
                path = request.path,
                protocol = CHAT_COMPLETIONS_PROTOCOL_ID,
                cassette = upstream_id(self.upstream.kind),
            );
            return Err(RecordError::UnsupportedStream);
        }
        Ok(())
    }

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
                RecordError::MissingApiKeyEnv
            })
    }

    /// Proxies one non-stream chat completions request and records it.
    pub(crate) async fn handle_chat_completions(
        &self,
        request: ObservedRequest,
    ) -> Result<ProxyResponse, RecordError> {
        let interaction_start = Instant::now();

        self.check_not_streaming(&request)?;
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
                "upstream request failed interaction_id={interaction_id} \
                 mode=record protocol={protocol} upstream_latency_ms={upstream_latency} \
                 outcome=failed cassette={cassette} error={err}",
                protocol = CHAT_COMPLETIONS_PROTOCOL_ID,
                cassette = upstream_id(self.upstream.kind),
            );
            RecordError::Internal
        })?;

        self.record_response(
            (request, &upstream_response),
            &interaction_id,
            RecordTiming {
                upstream_latency_ms: upstream_latency,
                interaction_start,
            },
        )
        .await?;

        Ok(ProxyResponse {
            status: upstream_response.status,
            headers: upstream_response.proxy_headers,
            body: upstream_response.body,
        })
    }

    async fn record_response(
        &self,
        (request, upstream_response): (ObservedRequest, &crate::http_exchange::ObservedResponse),
        interaction_id: &str,
        timing: RecordTiming,
    ) -> Result<(), RecordError> {
        match self.build_interaction(request, upstream_response, timing.interaction_start) {
            Ok(interaction) => {
                self.append_interaction(interaction).await.map_err(|e| {
                    self.failure_count.fetch_add(1, Ordering::Relaxed);
                    error!(
                        target: "spycatcher.harness.record",
                        "cassette write failed interaction_id={interaction_id} \
                         mode=record protocol={protocol} upstream_latency_ms={upstream_latency} \
                         outcome=write_failed cassette={cassette} error={e:?}",
                        protocol = CHAT_COMPLETIONS_PROTOCOL_ID,
                        upstream_latency = timing.upstream_latency_ms,
                        cassette = upstream_id(self.upstream.kind),
                    );
                    e
                })?;
                self.recorded_count.fetch_add(1, Ordering::Relaxed);
                info!(
                    target: "spycatcher.harness.record",
                    "interaction recorded interaction_id={interaction_id} \
                     mode=record protocol={protocol} upstream_latency_ms={upstream_latency} \
                     outcome=recorded cassette={cassette}",
                    protocol = CHAT_COMPLETIONS_PROTOCOL_ID,
                    upstream_latency = timing.upstream_latency_ms,
                    cassette = upstream_id(self.upstream.kind),
                );
                Ok(())
            }
            Err(e) => {
                self.failure_count.fetch_add(1, Ordering::Relaxed);
                error!(
                    target: "spycatcher.harness.record",
                    "cassette build failed interaction_id={interaction_id} \
                     mode=record protocol={protocol} upstream_latency_ms={upstream_latency} \
                     outcome=build_failed cassette={cassette} error={e:?}",
                    protocol = CHAT_COMPLETIONS_PROTOCOL_ID,
                    upstream_latency = timing.upstream_latency_ms,
                    cassette = upstream_id(self.upstream.kind),
                );
                Err(e)
            }
        }
    }

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
#[path = "record_tests.rs"]
mod tests;
