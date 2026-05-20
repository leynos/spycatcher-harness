//! Helpers for record-mode proxying integration scenarios.

use std::error::Error;
use std::sync::{Arc, Mutex};

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, Response, StatusCode};
use axum::{Json, Router, routing::post};
use camino::{Utf8Path, Utf8PathBuf};
use cap_std::{ambient_authority, fs_utf8::Dir};
use futures_util::stream;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use spycatcher_harness::cassette::Cassette;

/// Response data observed by the integration test client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClientResponse {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

/// Request data captured by the stub upstream server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CapturedRequest {
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

/// Stub upstream service used by record-mode BDD scenarios.
#[derive(Debug)]
pub(crate) struct StubUpstream {
    addr: std::net::SocketAddr,
    seen_requests: Arc<Mutex<Vec<CapturedRequest>>>,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl StubUpstream {
    /// Starts a stub upstream returning the provided JSON response body.
    pub(crate) fn start(
        runtime: &tokio::runtime::Runtime,
        response_body: Value,
    ) -> Result<Self, Box<dyn Error>> {
        runtime.block_on(async move {
            let seen_requests = Arc::new(Mutex::new(Vec::new()));
            let state = StubState {
                response: StubResponse::Json(response_body),
                seen_requests: Arc::clone(&seen_requests),
            };
            let router = Router::new()
                .route("/api/v1/chat/completions", post(stub_handler))
                .with_state(state);
            let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
            let addr = listener.local_addr()?;
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            let task = tokio::spawn(async move {
                axum::serve(listener, router)
                    .with_graceful_shutdown(wait_for_shutdown(shutdown_rx))
                    .await
            });

            Ok(Self {
                addr,
                seen_requests,
                shutdown: Some(shutdown_tx),
                task,
            })
        })
    }

    /// Starts a stub upstream returning an SSE byte transcript.
    pub(crate) fn start_stream(
        runtime: &tokio::runtime::Runtime,
        transcript: Vec<u8>,
    ) -> Result<Self, Box<dyn Error>> {
        runtime.block_on(async move {
            let seen_requests = Arc::new(Mutex::new(Vec::new()));
            let state = StubState {
                response: StubResponse::Stream(transcript),
                seen_requests: Arc::clone(&seen_requests),
            };
            let router = Router::new()
                .route("/api/v1/chat/completions", post(stub_handler))
                .with_state(state);
            let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
            let addr = listener.local_addr()?;
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            let task = tokio::spawn(async move {
                axum::serve(listener, router)
                    .with_graceful_shutdown(wait_for_shutdown(shutdown_rx))
                    .await
            });

            Ok(Self {
                addr,
                seen_requests,
                shutdown: Some(shutdown_tx),
                task,
            })
        })
    }

    pub(crate) fn base_url(&self) -> String {
        format!("http://{}/api/v1", self.addr)
    }

    pub(crate) fn captured_requests(&self) -> Result<Vec<CapturedRequest>, Box<dyn Error>> {
        let requests = self
            .seen_requests
            .lock()
            .map_err(|_| std::io::Error::other("captured requests mutex should not be poisoned"))?
            .clone();
        Ok(requests)
    }

    pub(crate) fn shutdown(
        mut self,
        runtime: &tokio::runtime::Runtime,
    ) -> Result<(), Box<dyn Error>> {
        runtime.block_on(async move {
            if let Some(sender) = self.shutdown.take()
                && sender.send(()).is_err()
            {}
            self.task
                .await
                .map_err(|error| std::io::Error::other(format!("stub join failed: {error}")))??;
            Ok(())
        })
    }
}

#[derive(Debug, Clone)]
struct StubState {
    response: StubResponse,
    seen_requests: Arc<Mutex<Vec<CapturedRequest>>>,
}

#[derive(Debug, Clone)]
enum StubResponse {
    Json(Value),
    Stream(Vec<u8>),
}

#[expect(
    clippy::expect_used,
    reason = "fail fast on poisoned mutex during integration tests"
)]
async fn stub_handler(
    State(state): State<StubState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response<Body> {
    let captured = CapturedRequest {
        headers: headers
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|header_value| (name.as_str().to_owned(), header_value.to_owned()))
            })
            .collect(),
        body: body.to_vec(),
    };
    state
        .seen_requests
        .lock()
        .expect("captured requests mutex should not be poisoned")
        .push(captured);

    match state.response {
        StubResponse::Json(response_body) => json_response(response_body),
        StubResponse::Stream(transcript) => stream_response(&transcript),
    }
}

fn json_response(response_body: Value) -> Response<Body> {
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    let body = match serde_json::to_vec(&Json(response_body).0) {
        Ok(bytes) => Body::from(bytes),
        Err(error) => Body::from(format!(r#"{{"error":"{error}"}}"#)),
    };
    build_response(response_headers, body)
}

fn stream_response(transcript: &[u8]) -> Response<Body> {
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        axum::http::header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    let chunks = transcript
        .chunks(7)
        .map(|chunk| Ok::<_, std::io::Error>(Bytes::copy_from_slice(chunk)))
        .collect::<Vec<_>>();
    build_response(response_headers, Body::from_stream(stream::iter(chunks)))
}

fn build_response(headers: HeaderMap, body: Body) -> Response<Body> {
    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    *response.headers_mut() = headers;
    response
}

async fn wait_for_shutdown(shutdown_rx: oneshot::Receiver<()>) {
    if shutdown_rx.await.is_err() {}
}

pub(crate) fn send_request(
    runtime: &tokio::runtime::Runtime,
    addr: std::net::SocketAddr,
    body: &[u8],
    extra_headers: &[(&str, &str)],
) -> Result<ClientResponse, Box<dyn Error>> {
    runtime.block_on(async move {
        let client = reqwest::Client::new();
        let mut request = client
            .post(format!("http://{addr}/v1/chat/completions"))
            .header(reqwest::header::CONTENT_TYPE, "application/json");
        for (name, value) in extra_headers {
            request = request.header(*name, *value);
        }
        let response = request.body(body.to_vec()).send().await?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|header_value| (name.as_str().to_owned(), header_value.to_owned()))
            })
            .collect();
        let response_body = response.bytes().await?.to_vec();

        Ok(ClientResponse {
            status,
            headers,
            body: response_body,
        })
    })
}

pub(crate) fn load_cassette(path: &Utf8Path) -> Result<Cassette, Box<dyn Error>> {
    let root = Dir::open_ambient_dir(".", ambient_authority())?;
    let file = root.open(path)?;
    Ok(Cassette::from_reader(file)?)
}

pub(crate) fn assert_cassette_matches_success_snapshot(
    cassette: &Cassette,
) -> Result<(), Box<dyn Error>> {
    let mut value =
        serde_json::to_value(cassette).map_err(|error| std::io::Error::other(error.to_string()))?;
    redact_snapshot_metadata(&mut value);
    redact_snapshot_response_date(&mut value);
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("../snapshots");
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        insta::assert_json_snapshot!("cassette_successful_proxying", value);
    });
    Ok(())
}

fn redact_snapshot_metadata(value: &mut serde_json::Value) {
    let Some(interactions) = value
        .get_mut("interactions")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    for interaction in interactions {
        if let Some(meta) = interaction.get_mut("metadata") {
            meta["recorded_at"] = serde_json::Value::String("<redacted>".to_owned());
            meta["relative_offset_ms"] = serde_json::Value::Number(0.into());
        }
    }
}

fn redact_snapshot_response_date(value: &mut serde_json::Value) {
    let Some(interactions) = value
        .get_mut("interactions")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    for interaction in interactions {
        let Some(headers) = interaction
            .get_mut("response")
            .and_then(|response| response.get_mut("headers"))
            .and_then(serde_json::Value::as_array_mut)
        else {
            continue;
        };
        for header in headers {
            let Some(pair) = header.as_array_mut() else {
                continue;
            };
            if pair
                .first()
                .and_then(serde_json::Value::as_str)
                .is_some_and(|name| name.eq_ignore_ascii_case("date"))
                && let Some(header_value) = pair.get_mut(1)
            {
                *header_value = serde_json::Value::String("<redacted>".to_owned());
            }
        }
    }
}

pub(crate) fn unique_cassette_path(prefix: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(format!(
        "target/test-record-proxying/{prefix}-{}.json",
        uuid::Uuid::new_v4()
    ))
}

pub(crate) fn sample_success_body() -> Value {
    json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": "hello"}}]
    })
}

pub(crate) fn present_env_name() -> Result<&'static str, Box<dyn Error>> {
    for candidate in ["HOME", "PWD", "USER", "PATH"] {
        if std::env::var(candidate).is_ok() {
            return Ok(candidate);
        }
    }

    Err(Box::new(std::io::Error::other(
        "expected at least one stable environment variable for integration tests",
    )))
}

/// Returns the value of the present-in-process env var used in record-mode tests.
pub(crate) fn present_env_value() -> Result<String, Box<dyn std::error::Error>> {
    let name = present_env_name()?;
    std::env::var(name).map_err(|error| {
        std::io::Error::other(format!("env var {name:?} must be set for tests: {error}")).into()
    })
}

pub(crate) fn assert_upstream_bearer_token(
    request: &CapturedRequest,
) -> Result<(), Box<dyn Error>> {
    let api_key = present_env_value()?;
    let expected = format!("Bearer {api_key}");
    let has_authorization = request
        .headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("authorization"));
    let matches_expected = request
        .headers
        .iter()
        .any(|(name, value)| name.eq_ignore_ascii_case("authorization") && value == &expected);
    if matches_expected {
        return Ok(());
    }
    let header_names = request
        .headers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    Err(std::io::Error::other(format!(
        "expected upstream Authorization Bearer token to match configured API key; \
         authorization_present={has_authorization} header_count={} header_names={header_names:?}",
        request.headers.len()
    ))
    .into())
}
