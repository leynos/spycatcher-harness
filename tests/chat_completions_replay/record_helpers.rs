//! Replay-focused integration helpers for chat completions BDD scenarios.

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
use url::Url;

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

/// Stub upstream service used by replay BDD setup scenarios.
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
        Self::start_with_response(runtime, StubResponse::Json(response_body))
    }

    /// Starts a stub upstream returning an SSE byte transcript.
    pub(crate) fn start_stream(
        runtime: &tokio::runtime::Runtime,
        transcript: Vec<u8>,
    ) -> Result<Self, Box<dyn Error>> {
        Self::start_with_response(runtime, StubResponse::Stream(transcript))
    }

    fn start_with_response(
        runtime: &tokio::runtime::Runtime,
        response: StubResponse,
    ) -> Result<Self, Box<dyn Error>> {
        runtime.block_on(async move {
            let seen_requests = Arc::new(Mutex::new(Vec::new()));
            let state = StubState {
                response,
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

    pub(crate) fn base_url(&self) -> Url {
        match Url::parse(&format!("http://{}/api/v1", self.addr)) {
            Ok(url) => url,
            Err(error) => panic!("test fixture URL is invalid: {error}"),
        }
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
            if let Some(sender) = self.shutdown.take() {
                sender.send(()).unwrap_or(());
            }
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
    let Ok(mut seen_requests) = state.seen_requests.lock() else {
        return internal_error_response("captured requests mutex is poisoned");
    };
    seen_requests.push(captured);

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

fn internal_error_response(message: &str) -> Response<Body> {
    let mut response = Response::new(Body::from(message.to_owned()));
    *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    response
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
