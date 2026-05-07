//! Failure-policy tests for record-mode orchestration.

use super::*;
use camino::Utf8PathBuf;
use rstest::rstest;
use std::sync::atomic::AtomicU64;

use crate::cassette::{InteractionMetadata, filesystem::FilesystemCassetteStore};
use crate::http_exchange::{ObservedResponse, parse_json_bytes};
use crate::protocol::CHAT_COMPLETIONS_PATH;
use crate::server::record_metadata::MetadataFactory;

#[derive(Debug, Clone)]
struct FakeEnvProvider(Option<String>);

impl EnvProvider for FakeEnvProvider {
    fn read(&self, _: &str) -> Option<String> {
        self.0.clone()
    }
}

#[derive(Debug, Clone)]
struct FakeUpstream {
    response: ObservedResponse,
}

impl ChatCompletionsUpstream for FakeUpstream {
    async fn send_chat_completions(
        &self,
        _request: ChatCompletionsRequest<'_>,
    ) -> HarnessResult<ObservedResponse> {
        Ok(self.response.clone())
    }
}

#[derive(Debug, Clone)]
struct FailingMetadataFactory;

impl MetadataFactory for FailingMetadataFactory {
    fn create(&self) -> HarnessResult<InteractionMetadata> {
        Err(HarnessError::InvalidConfig {
            message: "metadata failure".to_owned(),
        })
    }
}

#[rstest]
#[tokio::test]
async fn recording_failure_still_returns_upstream_response() {
    let cassette = cassette_fixture("record-fail");
    let cassette_store = FilesystemCassetteStore::open_or_create_for_record(&cassette.path)
        .expect("cassette should open");
    let failure_count = Arc::new(AtomicU64::new(0));
    let service = RecordService {
        cassette_store: Arc::new(Mutex::new(cassette_store)),
        upstream_client: FakeUpstream {
            response: sample_response(br#"{"id":"ok"}"#),
        },
        env_provider: FakeEnvProvider(Some("token".to_owned())),
        metadata: FailingMetadataFactory,
        upstream: UpstreamConfig::default(),
        redaction: RedactionConfig {
            drop_headers: vec!["authorization".to_owned()],
        },
        recorded_count: Arc::new(AtomicU64::new(0)),
        failure_count: Arc::clone(&failure_count),
        interaction_seq: Arc::new(AtomicU64::new(0)),
    };

    let proxied = service
        .handle_chat_completions(sample_request())
        .await
        .expect("upstream response should be returned despite recording failure");

    assert_eq!(proxied.status, 200);
    assert_eq!(proxied.body, br#"{"id":"ok"}"#.to_vec());
    assert_eq!(failure_count.load(Ordering::Relaxed), 1);
}

fn sample_request() -> ObservedRequest {
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
        parsed_json: None,
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

struct CassetteFixture {
    path: Utf8PathBuf,
    _temp_dir: tempfile::TempDir,
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
    let path = match Utf8PathBuf::from_path_buf(relative_dir.join("cassette.json")) {
        Ok(path) => path,
        Err(path) => panic!("temporary cassette path should be UTF-8: {path:?}"),
    };
    CassetteFixture {
        path,
        _temp_dir: temp_dir,
    }
}
