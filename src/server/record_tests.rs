//! Unit tests for record-mode orchestration.

use super::*;
use camino::Utf8PathBuf;
use rstest::rstest;
use serde_json::json;
use std::sync::atomic::AtomicU64;

use crate::cassette::{Cassette, CassetteReader, filesystem::FilesystemCassetteStore};
use crate::config::UpstreamKind;
use crate::http_exchange::ObservedResponse;

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

#[expect(
    clippy::too_many_arguments,
    reason = "test helper signature keeps each failing request case explicit"
)]
async fn assert_error_does_not_append(
    slug: &str,
    env_provider: FakeEnvProvider,
    request: ObservedRequest,
    fail_msg: &str,
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
        .expect_err(fail_msg);

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
        "stream requests should be rejected",
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
        "missing key should fail",
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
        body: br#"{"model":"gpt-test"}"#.to_vec(),
        parsed_json,
    }
}

fn sample_response(body: &[u8]) -> ObservedResponse {
    ObservedResponse {
        status: 200,
        headers: vec![("content-type".to_owned(), "application/json".to_owned())],
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
