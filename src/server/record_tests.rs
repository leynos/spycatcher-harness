//! Unit tests for record-mode orchestration.
use super::*;
use camino::Utf8PathBuf;
use rstest::rstest;
use serde_json::json;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

use crate::cassette::{
    Cassette, CassetteReader, InteractionMetadata, filesystem::FilesystemCassetteStore,
};
use crate::config::UpstreamKind;
use crate::http_exchange::{ObservedResponse, parse_json_bytes};
use crate::protocol::CHAT_COMPLETIONS_PATH;
use crate::server::record_metadata::{Clock, MetadataFactory, SessionMetadata, SystemClock};
#[derive(Debug, Clone)]
struct FakeEnvProvider(Option<String>);

impl EnvProvider for FakeEnvProvider {
    fn read(&self, _: &str) -> Option<String> {
        self.0.clone()
    }
}
#[derive(Debug, Clone)]
struct FakeMetadataFactory;

impl MetadataFactory for FakeMetadataFactory {
    fn create(&self) -> HarnessResult<InteractionMetadata> {
        Ok(InteractionMetadata {
            protocol_id: CHAT_COMPLETIONS_PROTOCOL_ID.to_owned(),
            upstream_id: upstream_id(UpstreamKind::OpenRouter).to_owned(),
            recorded_at: "2026-04-20T00:00:00Z".to_owned(),
            relative_offset_ms: 7,
        })
    }
}
#[derive(Debug)]
struct FixedClock;

impl Clock for FixedClock {
    fn now_rfc3339(&self) -> HarnessResult<String> {
        Ok("2024-01-01T00:00:00Z".to_owned())
    }
}

#[derive(Debug, Clone)]
struct FakeUpstream {
    response: Result<ObservedResponse, ()>,
}

impl ChatCompletionsUpstream for FakeUpstream {
    async fn send_chat_completions(
        &self,
        _request: ChatCompletionsRequest<'_>,
    ) -> HarnessResult<ObservedResponse> {
        self.response.as_ref().map_or_else(
            |()| {
                Err(HarnessError::UpstreamRequestFailed {
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::ConnectionRefused,
                        "stub failure",
                    )),
                })
            },
            |response| Ok(response.clone()),
        )
    }
}

async fn assert_error_does_not_append(
    slug: &str,
    env_provider: FakeEnvProvider,
    request: ObservedRequest,
    check_error: impl FnOnce(RecordError),
) {
    let cassette_path = unique_cassette_path(slug);
    let service = service_fixture(
        &cassette_path,
        FakeUpstream {
            response: Ok(sample_response(br#"{"id":"unused"}"#)),
        },
        env_provider,
    );

    let error = service
        .handle_chat_completions(request)
        .await
        .expect_err("expected handle_chat_completions to return Err");

    check_error(error);
    assert!(load_cassette(&cassette_path).interactions.is_empty());
}

#[rstest]
#[tokio::test]
async fn unsupported_stream_requests_do_not_append() {
    assert_error_does_not_append(
        "stream",
        FakeEnvProvider(Some("token".to_owned())),
        sample_request(Some(json!({"stream": true}))),
        |error| assert_eq!(error, RecordError::UnsupportedStream),
    )
    .await;
}

#[rstest]
#[tokio::test]
async fn missing_api_key_does_not_append() {
    assert_error_does_not_append(
        "missing-key",
        FakeEnvProvider(None),
        sample_request(None),
        |error| assert_eq!(error, RecordError::MissingApiKeyEnv),
    )
    .await;
}

#[rstest]
fn check_not_streaming_rejects_streaming_request() {
    let request = sample_request(parse_json_bytes(
        br#"{"model":"gpt-test","messages":[],"stream":true}"#,
    ));
    let service = service_fixture_default(FakeEnvProvider(Some("token".to_owned())));

    let result = service.check_not_streaming(&request);

    assert_eq!(result, Err(RecordError::UnsupportedStream));
}

#[rstest]
fn check_not_streaming_allows_non_streaming_request() {
    let request = sample_request(parse_json_bytes(
        br#"{"model":"gpt-test","messages":[],"stream":false}"#,
    ));
    let service = service_fixture_default(FakeEnvProvider(Some("token".to_owned())));

    assert!(service.check_not_streaming(&request).is_ok());
}

#[rstest]
fn resolve_api_key_returns_key_when_present() {
    let service = service_fixture_default(FakeEnvProvider(Some("my-secret".to_owned())));

    assert_eq!(
        service
            .resolve_api_key()
            .expect("API key should resolve when env provider has a value"),
        "my-secret",
    );
}

#[rstest]
fn resolve_api_key_errors_when_absent() {
    let service = service_fixture_default(FakeEnvProvider(None));

    assert!(matches!(
        service.resolve_api_key(),
        Err(RecordError::MissingApiKeyEnv)
    ));
}

#[rstest]
#[tokio::test]
async fn missing_api_key_response_hides_env_var_name() {
    let response = RecordError::MissingApiKeyEnv.into_response();
    let body_bytes = axum::body::to_bytes(response.into_body(), 1024)
        .await
        .expect("error response body should be readable");
    let body_text = String::from_utf8(body_bytes.to_vec()).expect("error response should be UTF-8");

    assert!(body_text.contains("upstream credentials are not configured"));
    assert!(!body_text.contains("SPYCATCHER"));
    assert!(!body_text.contains("API_KEY"));
}

#[rstest]
#[tokio::test]
async fn upstream_transport_failure_does_not_append() {
    let cassette_path = unique_cassette_path("upstream-fail");
    let service = service_fixture(
        &cassette_path,
        FakeUpstream { response: Err(()) },
        FakeEnvProvider(Some("token".to_owned())),
    );

    let result = service.handle_chat_completions(sample_request(None)).await;

    assert!(
        matches!(result, Err(RecordError::Internal)),
        "upstream transport failure should return RecordError::Internal"
    );
    assert!(
        load_cassette(&cassette_path).interactions.is_empty(),
        "no interaction should be recorded on upstream failure"
    );
}

#[rstest]
fn system_clock_produces_rfc3339_string() {
    let ts = SystemClock
        .now_rfc3339()
        .expect("system clock should produce an RFC 3339 timestamp");

    assert!(!ts.is_empty());
    assert!(ts.contains('T'));
}

#[rstest]
fn session_metadata_uses_injected_clock() {
    let metadata = SessionMetadata::with_clock(UpstreamKind::OpenRouter, Arc::new(FixedClock))
        .create()
        .expect("session metadata should be created with fixed clock");

    assert_eq!(metadata.recorded_at, "2024-01-01T00:00:00Z");
}

#[rstest]
fn session_metadata_uses_injected_session_start() {
    let injected_offset = Duration::from_millis(100);
    let session_start = Instant::now()
        .checked_sub(injected_offset)
        .expect("fixed offset should be within Instant range");
    let metadata = SessionMetadata::with_clock_and_start(
        UpstreamKind::OpenRouter,
        Arc::new(FixedClock),
        session_start,
    )
    .create()
    .expect("session metadata should be created with fixed start");

    assert!(
        u128::from(metadata.relative_offset_ms) >= injected_offset.as_millis(),
        "expected relative offset to be at least {} ms, got {} ms",
        injected_offset.as_millis(),
        u128::from(metadata.relative_offset_ms)
    );
}

#[rstest]
#[tokio::test]
async fn invalid_json_response_keeps_exact_bytes() {
    let cassette_path = unique_cassette_path("invalid-json");
    let service = service_fixture(
        &cassette_path,
        FakeUpstream {
            response: Ok(sample_response(br#"{"broken": true"#)),
        },
        FakeEnvProvider(Some("token".to_owned())),
    );

    let proxied = service
        .handle_chat_completions(sample_request(None))
        .await
        .expect("request should succeed");

    assert_eq!(proxied.body, br#"{"broken": true"#.to_vec());
    let cassette = load_cassette(&cassette_path);
    let interaction = cassette
        .interactions
        .first()
        .expect("expected one recorded interaction");
    let RecordedResponse::NonStream {
        body, parsed_json, ..
    } = &interaction.response
    else {
        panic!("expected non-stream response");
    };
    assert_eq!(body, &br#"{"broken": true"#.to_vec());
    assert_eq!(parsed_json, &None);
}

fn service_fixture(
    cassette_path: &camino::Utf8Path,
    upstream: FakeUpstream,
    env_provider: FakeEnvProvider,
) -> RecordService<FakeUpstream, FakeEnvProvider, FakeMetadataFactory> {
    let cassette_store = match FilesystemCassetteStore::open_or_create_for_record(cassette_path) {
        Ok(store) => store,
        Err(error) => panic!("cassette should open: {error}"),
    };

    RecordService {
        cassette_store: Arc::new(Mutex::new(cassette_store)),
        upstream_client: upstream,
        env_provider,
        metadata: FakeMetadataFactory,
        upstream: UpstreamConfig::default(),
        redaction: RedactionConfig {
            drop_headers: vec!["authorization".to_owned()],
        },
        recorded_count: Arc::new(AtomicU64::new(0)),
        failure_count: Arc::new(AtomicU64::new(0)),
        interaction_seq: Arc::new(AtomicU64::new(0)),
    }
}

fn service_fixture_default(
    env_provider: FakeEnvProvider,
) -> RecordService<FakeUpstream, FakeEnvProvider, FakeMetadataFactory> {
    let cassette_path = unique_cassette_path("default");
    service_fixture(
        &cassette_path,
        FakeUpstream {
            response: Ok(sample_response(br#"{"id":"ok"}"#)),
        },
        env_provider,
    )
}

fn sample_request(parsed_json: Option<serde_json::Value>) -> ObservedRequest {
    ObservedRequest {
        method: "POST".to_owned(),
        path: CHAT_COMPLETIONS_PATH.to_owned(),
        query: String::new(),
        headers: vec![
            ("content-type".to_owned(), "application/json".to_owned()),
            ("authorization".to_owned(), "Bearer secret".to_owned()),
        ],
        forward_headers: vec![
            ("content-type".to_owned(), b"application/json".to_vec()),
            ("authorization".to_owned(), b"Bearer secret".to_vec()),
        ],
        body: br#"{"model":"gpt-test"}"#.to_vec(),
        parsed_json,
    }
}

fn sample_response(body: &[u8]) -> ObservedResponse {
    ObservedResponse {
        status: 200,
        headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        proxy_headers: vec![("content-type".to_owned(), b"application/json".to_vec())],
        body: body.to_vec(),
        parsed_json: parse_json_bytes(body),
    }
}

fn load_cassette(cassette_path: &camino::Utf8Path) -> Cassette {
    let store = match FilesystemCassetteStore::open_for_replay(cassette_path) {
        Ok(store) => store,
        Err(error) => panic!("cassette should reopen: {error}"),
    };

    match store.load() {
        Ok(cassette) => cassette,
        Err(error) => panic!("cassette should decode: {error}"),
    }
}

fn unique_cassette_path(name: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(format!(
        "target/test-record-service/{name}-{}.json",
        uuid::Uuid::new_v4()
    ))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_requests_are_recorded_without_data_loss() {
    let cassette_path = unique_cassette_path("concurrent");
    let body = br#"{"model":"gpt-test","messages":[]}"#;

    let service = std::sync::Arc::new(service_fixture(
        &cassette_path,
        FakeUpstream {
            response: Ok(sample_response(br#"{"id":"ok"}"#)),
        },
        FakeEnvProvider(Some("concurrent-key".to_owned())),
    ));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let svc = std::sync::Arc::clone(&service);
            let req = ObservedRequest {
                method: "POST".to_owned(),
                path: crate::protocol::CHAT_COMPLETIONS_PATH.to_owned(),
                query: String::new(),
                headers: vec![("content-type".to_owned(), "application/json".to_owned())],
                forward_headers: vec![("content-type".to_owned(), b"application/json".to_vec())],
                body: body.to_vec(),
                parsed_json: crate::http_exchange::parse_json_bytes(body),
            };
            tokio::spawn(async move {
                svc.handle_chat_completions(req)
                    .await
                    .expect("concurrent request should succeed")
            })
        })
        .collect();

    for handle in handles {
        handle.await.expect("task should not panic");
    }

    let cassette = load_cassette(&cassette_path);
    assert_eq!(
        cassette.interactions.len(),
        8,
        "all eight concurrent interactions must be persisted"
    );
}
