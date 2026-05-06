//! Unit tests for record-mode orchestration.
use super::*;
use camino::{Utf8Path, Utf8PathBuf};
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
    let cassette = cassette_fixture(slug);
    let service = service_fixture(
        &cassette.path,
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
    assert!(load_cassette(&cassette.path).interactions.is_empty());
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
        |error| assert_eq!(error, RecordError::MissingApiKeyNotConfigured),
    )
    .await;
}
#[rstest]
fn check_not_streaming_rejects_streaming_request() {
    let request = sample_request(parse_json_bytes(
        br#"{"model":"gpt-test","messages":[],"stream":true}"#,
    ));
    let fixture = service_fixture_ephemeral(FakeEnvProvider(Some("token".to_owned())));

    let result = fixture.service.check_not_streaming(&request);

    assert_eq!(result, Err(RecordError::UnsupportedStream));
}
#[rstest]
fn check_not_streaming_allows_non_streaming_request() {
    let request = sample_request(parse_json_bytes(
        br#"{"model":"gpt-test","messages":[],"stream":false}"#,
    ));
    let fixture = service_fixture_ephemeral(FakeEnvProvider(Some("token".to_owned())));

    assert!(fixture.service.check_not_streaming(&request).is_ok());
}
#[rstest]
fn resolve_api_key_returns_key_when_present() {
    let fixture = service_fixture_ephemeral(FakeEnvProvider(Some("my-secret".to_owned())));

    assert_eq!(
        fixture
            .service
            .resolve_api_key()
            .expect("API key should resolve when env provider has a value"),
        "my-secret",
    );
}
#[rstest]
fn resolve_api_key_errors_when_absent() {
    let fixture = service_fixture_ephemeral(FakeEnvProvider(None));

    assert!(matches!(
        fixture.service.resolve_api_key(),
        Err(RecordError::MissingApiKeyNotConfigured)
    ));
}
#[rstest]
#[tokio::test]
async fn upstream_transport_failure_does_not_append() {
    let cassette = cassette_fixture("upstream-fail");
    let service = service_fixture(
        &cassette.path,
        FakeUpstream { response: Err(()) },
        FakeEnvProvider(Some("token".to_owned())),
    );

    let result = service.handle_chat_completions(sample_request(None)).await;

    assert!(
        matches!(result, Err(RecordError::Internal)),
        "upstream transport failure should return RecordError::Internal"
    );
    assert!(
        load_cassette(&cassette.path).interactions.is_empty(),
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
    let cassette = cassette_fixture("invalid-json");
    let service = service_fixture(
        &cassette.path,
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
    let persisted = load_cassette(&cassette.path);
    let interaction = persisted
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
    cassette_path: &Utf8Path,
    upstream: FakeUpstream,
    env_provider: FakeEnvProvider,
) -> RecordService<FakeUpstream, FakeEnvProvider, FakeMetadataFactory> {
    let cassette_store = FilesystemCassetteStore::open_or_create_for_record(cassette_path)
        .expect("cassette should open");

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

struct ServiceFixture {
    service: RecordService<FakeUpstream, FakeEnvProvider, FakeMetadataFactory>,
    _temp_dir: tempfile::TempDir,
}
struct CassetteFixture {
    path: Utf8PathBuf,
    temp_dir: tempfile::TempDir,
}
fn service_fixture_ephemeral(env_provider: FakeEnvProvider) -> ServiceFixture {
    let CassetteFixture { path, temp_dir } = cassette_fixture("default");
    let service = service_fixture(
        &path,
        FakeUpstream {
            response: Ok(sample_response(br#"{"id":"ok"}"#)),
        },
        env_provider,
    );
    ServiceFixture {
        service,
        _temp_dir: temp_dir,
    }
}
fn cassette_fixture(name: &str) -> CassetteFixture {
    let temp_dir = match tempfile::Builder::new()
        .prefix(&format!("record-service-{name}-"))
        .tempdir_in(".")
    {
        Ok(dir) => dir,
        Err(error) => panic!("temporary cassette directory should be created: {error}"),
    };
    let cwd = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => panic!("current directory should be available: {error}"),
    };
    let relative_dir = match temp_dir.path().strip_prefix(&cwd) {
        Ok(path) => path,
        Err(error) => panic!("temporary cassette directory should be project-relative: {error}"),
    };
    let cassette_path = match Utf8PathBuf::from_path_buf(relative_dir.join("cassette.json")) {
        Ok(path) => path,
        Err(path) => panic!("temporary cassette path should be UTF-8: {path:?}"),
    };
    CassetteFixture {
        path: cassette_path,
        temp_dir,
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
fn load_cassette(cassette_path: &Utf8Path) -> Cassette {
    let store =
        FilesystemCassetteStore::open_for_replay(cassette_path).expect("cassette should reopen");
    store.load().expect("cassette should decode")
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_requests_are_recorded_without_data_loss() {
    let cassette = cassette_fixture("concurrent");
    let body = br#"{"model":"gpt-test","messages":[]}"#;

    let service = std::sync::Arc::new(service_fixture(
        &cassette.path,
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

    let persisted = load_cassette(&cassette.path);
    assert_eq!(
        persisted.interactions.len(),
        8,
        "all eight concurrent interactions must be persisted"
    );
}
