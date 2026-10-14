//! Failure-policy tests for record-mode orchestration.

use super::record_tests_helpers::{cassette_fixture, load_cassette};
use super::*;
use eyre::WrapErr;
use futures_util::StreamExt;
use rstest::rstest;
use std::sync::atomic::AtomicU64;

use crate::cassette::{InteractionMetadata, filesystem::FilesystemCassetteStore};
use crate::http_exchange::{ObservedResponse, parse_json_bytes};
use crate::protocol::CHAT_COMPLETIONS_PATH;
use crate::server::record_metadata::MetadataFactory;
use crate::upstream::StreamingObservedResponse;

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

    async fn stream_chat_completions(
        &self,
        _request: ChatCompletionsRequest<'_>,
    ) -> HarnessResult<StreamingObservedResponse> {
        Ok(StreamingObservedResponse {
            status: self.response.status,
            headers: self.response.headers.clone(),
            proxy_headers: self.response.proxy_headers.clone(),
            body: futures_util::stream::iter(vec![Ok(axum::body::Bytes::from(
                self.response.body.clone(),
            ))])
            .boxed(),
        })
    }
}

#[derive(Debug, Clone)]
struct MalformedStreamUpstream {
    chunks: Vec<Vec<u8>>,
}

impl ChatCompletionsUpstream for MalformedStreamUpstream {
    async fn send_chat_completions(
        &self,
        _request: ChatCompletionsRequest<'_>,
    ) -> HarnessResult<ObservedResponse> {
        Ok(sample_response(b"{}"))
    }

    async fn stream_chat_completions(
        &self,
        _request: ChatCompletionsRequest<'_>,
    ) -> HarnessResult<StreamingObservedResponse> {
        Ok(StreamingObservedResponse {
            status: 200,
            headers: vec![("content-type".to_owned(), "text/event-stream".to_owned())],
            proxy_headers: vec![("content-type".to_owned(), b"text/event-stream".to_vec())],
            body: futures_util::stream::iter(
                self.chunks
                    .clone()
                    .into_iter()
                    .map(|chunk| Ok(axum::body::Bytes::from(chunk))),
            )
            .boxed(),
        })
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
async fn recording_failure_still_returns_upstream_response() -> eyre::Result<()> {
    let cassette = cassette_fixture("record-fail")?;
    let cassette_store = FilesystemCassetteStore::open_or_create_for_record(&cassette.path)
        .wrap_err("cassette should open for recording")?;
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
        .map_err(|error| {
            eyre::eyre!("upstream response should be returned despite recording failure: {error:?}")
        })?;

    eyre::ensure!(
        proxied.status == 200,
        "upstream response status should be preserved; got {}",
        proxied.status
    );
    let ProxyBody::Buffered(body) = proxied.body else {
        eyre::bail!("expected buffered response");
    };
    let expected_body = br#"{"id":"ok"}"#.to_vec();
    eyre::ensure!(
        body == expected_body,
        "buffered response should preserve the upstream bytes; got {:?}, expected {:?}",
        body,
        expected_body
    );
    let actual_failure_count = failure_count.load(Ordering::Relaxed);
    eyre::ensure!(
        actual_failure_count == 1,
        "recording failure should increment the failure count once; got {actual_failure_count}"
    );
    Ok(())
}

#[rstest]
#[case::invalid_utf8(vec![b"data: \xff\n\n".to_vec()], b"data: \xff\n\n".to_vec())]
#[case::incomplete_final_event(vec![b"data: unfinished".to_vec()], b"data: unfinished".to_vec())]
#[tokio::test]
async fn stream_parse_failure_still_returns_upstream_chunks_without_recording(
    #[case] chunks: Vec<Vec<u8>>,
    #[case] expected_streamed: Vec<u8>,
) -> eyre::Result<()> {
    let cassette = cassette_fixture("stream-parse-fail")?;
    let cassette_store = FilesystemCassetteStore::open_or_create_for_record(&cassette.path)
        .wrap_err("cassette should open for recording")?;
    let failure_count = Arc::new(AtomicU64::new(0));
    let service = RecordService {
        cassette_store: Arc::new(Mutex::new(cassette_store)),
        upstream_client: MalformedStreamUpstream { chunks },
        env_provider: FakeEnvProvider(Some("token".to_owned())),
        metadata: FixedMetadataFactory,
        upstream: UpstreamConfig::default(),
        redaction: RedactionConfig::default(),
        recorded_count: Arc::new(AtomicU64::new(0)),
        failure_count: Arc::clone(&failure_count),
        interaction_seq: Arc::new(AtomicU64::new(0)),
    };
    let mut request = sample_request();
    request.body = br#"{"model":"gpt-test","stream":true}"#.to_vec();
    request.parsed_json = parse_json_bytes(&request.body);

    let proxied = service
        .handle_chat_completions(request)
        .await
        .map_err(|error| eyre::eyre!("malformed stream should still be proxied: {error:?}"))?;
    let ProxyBody::Stream(mut body) = proxied.body else {
        eyre::bail!("expected streaming response");
    };
    let mut streamed = Vec::new();
    while let Some(chunk_res) = body.next().await {
        let forwarded_chunk = chunk_res.wrap_err("upstream chunk should be forwarded")?;
        streamed.extend_from_slice(&forwarded_chunk);
    }

    eyre::ensure!(
        streamed == expected_streamed,
        "streamed bytes should match the upstream chunks; got {:?}, expected {:?}",
        streamed,
        expected_streamed
    );
    let actual_failure_count = failure_count.load(Ordering::Relaxed);
    eyre::ensure!(
        actual_failure_count == 1,
        "stream parsing failure should increment the failure count once; got {actual_failure_count}"
    );
    let persisted = load_cassette(&cassette.path)?;
    eyre::ensure!(
        persisted.interactions.is_empty(),
        "stream parse failure should not record an interaction; got {}",
        persisted.interactions.len()
    );
    Ok(())
}

#[derive(Debug, Clone)]
struct FixedMetadataFactory;

impl MetadataFactory for FixedMetadataFactory {
    fn create(&self) -> HarnessResult<InteractionMetadata> {
        Ok(InteractionMetadata {
            protocol_id: CHAT_COMPLETIONS_PROTOCOL_ID.to_owned(),
            upstream_id: "openrouter".to_owned(),
            recorded_at: "2026-05-20T12:00:00Z".to_owned(),
            relative_offset_ms: 0,
        })
    }
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
