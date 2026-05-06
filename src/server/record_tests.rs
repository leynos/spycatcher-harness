//! Unit tests for record-mode orchestration.

use super::*;
use camino::Utf8PathBuf;
use rstest::rstest;
use serde_json::json;
use std::sync::atomic::AtomicU64;
use std::time::Instant;

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
        |error| assert!(matches!(error, RecordError::MissingApiKeyEnv { .. })),
    )
    .await;
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
    let session_start = Instant::now();
    let metadata = SessionMetadata::with_clock_and_start(
        UpstreamKind::OpenRouter,
        Arc::new(FixedClock),
        session_start,
    )
    .create()
    .expect("session metadata should be created with fixed start");

    assert!(metadata.relative_offset_ms < 100);
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
