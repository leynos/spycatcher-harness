//! Shared test infrastructure for record-mode unit tests.
use super::*;
use camino::{Utf8Path, Utf8PathBuf};
use eyre::WrapErr;
use futures_util::StreamExt;
use std::sync::atomic::AtomicU64;

use crate::cassette::{
    Cassette, CassetteReader, InteractionMetadata, filesystem::FilesystemCassetteStore,
};
use crate::config::UpstreamKind;
use crate::http_exchange::{ObservedResponse, parse_json_bytes};
use crate::protocol::CHAT_COMPLETIONS_PATH;
use crate::server::record_metadata::{Clock, MetadataFactory};
use crate::upstream::StreamingObservedResponse;

#[derive(Debug, Clone)]
pub(crate) struct FakeEnvProvider(pub(crate) Option<String>);

impl EnvProvider for FakeEnvProvider {
    fn read(&self, _: &str) -> Option<String> {
        self.0.clone()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FakeMetadataFactory;

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
pub(crate) struct FixedClock;

impl Clock for FixedClock {
    fn now_rfc3339(&self) -> HarnessResult<String> {
        Ok("2024-01-01T00:00:00Z".to_owned())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FakeUpstream {
    pub(crate) response: Result<ObservedResponse, ()>,
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

    async fn stream_chat_completions(
        &self,
        _request: ChatCompletionsRequest<'_>,
    ) -> HarnessResult<StreamingObservedResponse> {
        self.response.as_ref().map_or_else(
            |()| {
                Err(HarnessError::UpstreamRequestFailed {
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::ConnectionRefused,
                        "stub failure",
                    )),
                })
            },
            |response| {
                Ok(StreamingObservedResponse {
                    status: response.status,
                    headers: response.headers.clone(),
                    proxy_headers: response.proxy_headers.clone(),
                    body: futures_util::stream::iter(vec![Ok(axum::body::Bytes::from(
                        response.body.clone(),
                    ))])
                    .boxed(),
                })
            },
        )
    }
}

pub(crate) async fn assert_error_does_not_append(
    slug: &str,
    env_provider: FakeEnvProvider,
    request: ObservedRequest,
    check_error: impl FnOnce(RecordError),
) -> eyre::Result<()> {
    let cassette = cassette_fixture(slug)?;
    let service = service_fixture(
        &cassette.path,
        FakeUpstream {
            response: Ok(sample_response(br#"{"id":"unused"}"#)),
        },
        env_provider,
    )?;

    let Err(error) = service.handle_chat_completions(request).await else {
        eyre::bail!("expected handle_chat_completions to return Err");
    };

    check_error(error);
    let persisted = load_cassette(&cassette.path)?;
    eyre::ensure!(
        persisted.interactions.is_empty(),
        "no interaction should be recorded after the error"
    );
    Ok(())
}

pub(crate) fn service_fixture(
    cassette_path: &Utf8Path,
    upstream: FakeUpstream,
    env_provider: FakeEnvProvider,
) -> eyre::Result<RecordService<FakeUpstream, FakeEnvProvider, FakeMetadataFactory>> {
    let cassette_store = FilesystemCassetteStore::open_or_create_for_record(cassette_path)
        .wrap_err("cassette should open for recording")?;

    Ok(RecordService {
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
    })
}

pub(crate) struct ServiceFixture {
    pub(crate) service: RecordService<FakeUpstream, FakeEnvProvider, FakeMetadataFactory>,
    pub(crate) _temp_dir: tempfile::TempDir,
}

pub(crate) struct CassetteFixture {
    pub(crate) path: Utf8PathBuf,
    pub(crate) temp_dir: tempfile::TempDir,
}

pub(crate) fn service_fixture_ephemeral(
    env_provider: FakeEnvProvider,
) -> eyre::Result<ServiceFixture> {
    let CassetteFixture { path, temp_dir } = cassette_fixture("default")?;
    let service = service_fixture(
        &path,
        FakeUpstream {
            response: Ok(sample_response(br#"{"id":"ok"}"#)),
        },
        env_provider,
    )?;
    Ok(ServiceFixture {
        service,
        _temp_dir: temp_dir,
    })
}

pub(crate) fn cassette_fixture(name: &str) -> eyre::Result<CassetteFixture> {
    let temp_dir = tempfile::Builder::new()
        .prefix(&format!("record-service-{name}-"))
        .tempdir_in(".")
        .wrap_err("temporary cassette directory should be created")?;
    let cwd = std::env::current_dir().wrap_err("current directory should be available")?;
    let relative_dir = temp_dir
        .path()
        .strip_prefix(&cwd)
        .wrap_err("temporary cassette directory should be project-relative")?;
    let cassette_path = Utf8PathBuf::from_path_buf(relative_dir.join("cassette.json"))
        .map_err(|path| eyre::eyre!("temporary cassette path should be UTF-8: {path:?}"))?;
    Ok(CassetteFixture {
        path: cassette_path,
        temp_dir,
    })
}

pub(crate) fn sample_request(parsed_json: Option<serde_json::Value>) -> ObservedRequest {
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

pub(crate) fn sample_response(body: &[u8]) -> ObservedResponse {
    ObservedResponse {
        status: 200,
        headers: vec![("content-type".to_owned(), "application/json".to_owned())],
        proxy_headers: vec![("content-type".to_owned(), b"application/json".to_vec())],
        body: body.to_vec(),
        parsed_json: parse_json_bytes(body),
    }
}

pub(crate) fn load_cassette(cassette_path: &Utf8Path) -> eyre::Result<Cassette> {
    let store = FilesystemCassetteStore::open_for_replay(cassette_path)
        .wrap_err("cassette should reopen for replay")?;
    store
        .load()
        .wrap_err("cassette should decode after reopening")
}
