//! Record-mode request orchestration and HTTP response rendering.
//!
//! The inbound handler remains thin by translating `axum` requests into the
//! adapter-neutral exchange types defined here, delegating proxying and
//! cassette assembly to small testable helpers.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use axum::body::Bytes;
use axum::extract::{OriginalUri, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Response, StatusCode};
use axum::response::IntoResponse;
use serde_json::json;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use tokio::task::spawn_blocking;

use crate::cassette::{
    CassetteAppender, IgnorePathConfig, Interaction, InteractionMetadata, RecordedRequest,
    RecordedResponse, filesystem::FilesystemCassetteStore,
};
use crate::config::{HarnessConfig, RedactionConfig, UpstreamConfig};
use crate::http_exchange::{
    ObservedRequest, ProxyResponse, parse_json_bytes, redact_headers, selected_request_headers,
};
use crate::protocol::{
    CHAT_COMPLETIONS_PATH, CHAT_COMPLETIONS_PROTOCOL_ID, is_streaming_chat_completions_request,
    upstream_id,
};
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
}

/// Timestamp and relative-offset factory for recorded interactions.
pub(crate) trait MetadataFactory: Clone + Send + Sync + 'static {
    /// Creates one metadata payload for a newly observed interaction.
    fn create(&self) -> HarnessResult<InteractionMetadata>;
}

/// Metadata factory backed by the current UTC clock and session start time.
#[derive(Debug, Clone)]
pub(crate) struct SessionMetadata {
    session_start: Instant,
    upstream_kind: crate::config::UpstreamKind,
}

impl SessionMetadata {
    #[must_use]
    pub(crate) fn new(upstream_kind: crate::config::UpstreamKind) -> Self {
        Self {
            session_start: Instant::now(),
            upstream_kind,
        }
    }
}

impl MetadataFactory for SessionMetadata {
    fn create(&self) -> HarnessResult<InteractionMetadata> {
        let recorded_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| HarnessError::InvalidConfig {
                message: format!("failed to format recording timestamp: {error}"),
            })?;
        let elapsed = self.session_start.elapsed().as_millis();
        let relative_offset_ms =
            u64::try_from(elapsed).map_err(|_| HarnessError::InvalidConfig {
                message: "relative offset exceeded u64 range".to_owned(),
            })?;

        Ok(InteractionMetadata {
            protocol_id: CHAT_COMPLETIONS_PROTOCOL_ID.to_owned(),
            upstream_id: upstream_id(self.upstream_kind).to_owned(),
            recorded_at,
            relative_offset_ms,
        })
    }
}

/// Request-level record-mode failures mapped to concrete HTTP responses.
#[derive(Debug, PartialEq)]
pub(crate) enum RecordError {
    UnsupportedStream,
    MissingApiKeyEnv { env_var: String },
    Internal,
}

impl RecordError {
    const fn status_code(&self) -> StatusCode {
        match self {
            Self::UnsupportedStream => StatusCode::NOT_IMPLEMENTED,
            Self::MissingApiKeyEnv { .. } | Self::Internal => StatusCode::BAD_GATEWAY,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::UnsupportedStream => {
                "streaming chat completions are not implemented yet".to_owned()
            }
            Self::MissingApiKeyEnv { env_var } => {
                format!("upstream API key environment variable {env_var:?} is not set")
            }
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
    /// Proxies one non-stream chat completions request and records it.
    pub(crate) async fn handle_chat_completions(
        &self,
        request: ObservedRequest,
    ) -> Result<ProxyResponse, RecordError> {
        if is_streaming_chat_completions_request(request.parsed_json.as_ref()) {
            return Err(RecordError::UnsupportedStream);
        }

        let api_key = self
            .env_provider
            .read(&self.upstream.api_key_env)
            .ok_or_else(|| RecordError::MissingApiKeyEnv {
                env_var: self.upstream.api_key_env.clone(),
            })?;

        let upstream_response = self
            .upstream_client
            .send_chat_completions(ChatCompletionsRequest {
                config: &self.upstream,
                api_key: &api_key,
                headers: &request.headers,
                body: &request.body,
            })
            .await
            .map_err(|_| RecordError::Internal)?;

        let interaction = self.build_interaction(request, &upstream_response)?;
        self.append_interaction(interaction).await?;

        Ok(ProxyResponse {
            status: upstream_response.status,
            headers: upstream_response.headers,
            body: upstream_response.body,
        })
    }

    fn build_interaction(
        &self,
        request: ObservedRequest,
        response: &crate::http_exchange::ObservedResponse,
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
            .map_err(|_| RecordError::Internal)?;

        Ok(Interaction {
            request: recorded_request,
            response: RecordedResponse::NonStream {
                status: response.status,
                headers: redact_headers(&response.headers, &self.redaction),
                body: response.body.clone(),
                parsed_json: response.parsed_json.clone(),
            },
            metadata: self.metadata.create().map_err(|_| RecordError::Internal)?,
        })
    }

    async fn append_interaction(&self, interaction: Interaction) -> Result<(), RecordError> {
        let store = Arc::clone(&self.cassette_store);
        spawn_blocking(move || {
            let mut guard = store.lock().map_err(|_| RecordError::Internal)?;
            CassetteAppender::append(&mut *guard, interaction).map_err(|_| RecordError::Internal)
        })
        .await
        .map_err(|_| RecordError::Internal)?
    }
}

/// Axum route handler for record-mode chat completions proxying.
pub(crate) async fn record_chat_completions_handler(
    State(state): State<RecordAppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response<axum::body::Body>, RecordError> {
    let body_bytes = body.to_vec();
    let request = ObservedRequest {
        method: "POST".to_owned(),
        path: CHAT_COMPLETIONS_PATH.to_owned(),
        query: uri.query().unwrap_or_default().to_owned(),
        headers: selected_request_headers(&headers),
        parsed_json: parse_json_bytes(&body_bytes),
        body: body_bytes,
    };
    let proxied = state.service.handle_chat_completions(request).await?;
    Ok(build_proxy_response(proxied))
}

fn build_proxy_response(response: ProxyResponse) -> Response<axum::body::Body> {
    let mut built = Response::new(axum::body::Body::from(response.body));
    *built.status_mut() = StatusCode::from_u16(response.status).unwrap_or(StatusCode::BAD_GATEWAY);
    for (name, value) in response.headers {
        if let (Ok(header_name), Ok(header_value)) = (
            HeaderName::try_from(name.as_str()),
            HeaderValue::from_str(&value),
        ) {
            built.headers_mut().append(header_name, header_value);
        }
    }
    built
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
