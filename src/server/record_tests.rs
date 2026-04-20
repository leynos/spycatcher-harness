//! Unit tests for record-mode orchestration.

use super::*;
use camino::Utf8PathBuf;
use rstest::rstest;
use serde_json::json;

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
            |()| Err(HarnessError::UpstreamRequestFailed),
            |response| Ok(response.clone()),
        )
    }
}

#[rstest]
#[tokio::test]
async fn unsupported_stream_requests_do_not_append() {
    let cassette_path = unique_cassette_path("stream");
    let service = service_fixture(
        &cassette_path,
        FakeUpstream {
            response: Ok(sample_response(br#"{"id":"unused"}"#)),
        },
        FakeEnvProvider(Some("token".to_owned())),
    );

    let error = service
        .handle_chat_completions(sample_request(Some(json!({"stream": true}))))
        .await
        .expect_err("stream requests should be rejected");

    assert_eq!(error, RecordError::UnsupportedStream);
    assert!(load_cassette(&cassette_path).interactions.is_empty());
}

#[rstest]
#[tokio::test]
async fn missing_api_key_does_not_append() {
    let cassette_path = unique_cassette_path("missing-key");
    let service = service_fixture(
        &cassette_path,
        FakeUpstream {
            response: Ok(sample_response(br#"{"id":"unused"}"#)),
        },
        FakeEnvProvider(None),
    );

    let error = service
        .handle_chat_completions(sample_request(None))
        .await
        .expect_err("missing key should fail");

    assert!(matches!(error, RecordError::MissingApiKeyEnv { .. }));
    assert!(load_cassette(&cassette_path).interactions.is_empty());
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
    RecordService {
        cassette_store: Arc::new(Mutex::new(
            FilesystemCassetteStore::open_or_create_for_record(cassette_path)
                .expect("cassette should open"),
        )),
        upstream_client: upstream,
        env_provider,
        metadata: FakeMetadataFactory,
        upstream: UpstreamConfig::default(),
        redaction: RedactionConfig {
            drop_headers: vec!["authorization".to_owned()],
        },
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
    FilesystemCassetteStore::open_for_replay(cassette_path)
        .expect("cassette should reopen")
        .load()
        .expect("cassette should decode")
}

fn unique_cassette_path(name: &str) -> Utf8PathBuf {
    Utf8PathBuf::from(format!(
        "target/test-record-service/{name}-{}.json",
        uuid::Uuid::new_v4()
    ))
}
