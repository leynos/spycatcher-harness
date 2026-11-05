//! Shared test infrastructure for record-mode unit tests.
use std::{
    sync::{atomic::AtomicU64, mpsc},
    thread::JoinHandle,
    time::Duration,
};

use camino::{Utf8Path, Utf8PathBuf};
use eyre::WrapErr;
use futures_util::StreamExt;
use tokio::sync::{mpsc as tokio_mpsc, oneshot};

use super::*;
use crate::{
    cassette::{
        Cassette,
        CassetteReader,
        InteractionMetadata,
        filesystem::FilesystemCassetteStore,
    },
    config::UpstreamKind,
    http_exchange::{ObservedResponse, parse_json_bytes},
    protocol::CHAT_COMPLETIONS_PATH,
    server::record_metadata::{Clock, MetadataFactory},
    upstream::StreamingObservedResponse,
};

#[derive(Debug, Clone)]
pub(crate) struct FakeEnvProvider(pub(crate) Option<String>);

impl EnvProvider for FakeEnvProvider {
    fn read(&self, _: &str) -> Option<String> { self.0.clone() }
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

/// Metadata factory that signals after every successful metadata build.
///
/// Tests use the signal to prove a request reached the persistence boundary
/// while the cassette store was locked elsewhere.
#[derive(Debug, Clone)]
pub(crate) struct SignallingMetadataFactory {
    /// Channel used to report each successful metadata build.
    signals: tokio_mpsc::UnboundedSender<()>,
}

impl SignallingMetadataFactory {
    /// Creates a factory that reports builds through `signals`.
    /// For example, a successful build sends one unit value.
    pub(crate) const fn new(signals: tokio_mpsc::UnboundedSender<()>) -> Self { Self { signals } }
}

impl MetadataFactory for SignallingMetadataFactory {
    fn create(&self) -> HarnessResult<InteractionMetadata> {
        let metadata = FakeMetadataFactory.create()?;
        let _sent = self.signals.send(());
        Ok(metadata)
    }
}

#[derive(Debug)]
pub(crate) struct FixedClock;

impl Clock for FixedClock {
    fn now_rfc3339(&self) -> HarnessResult<String> { Ok("2024-01-01T00:00:00Z".to_owned()) }
}

#[derive(Debug, Clone)]
pub(crate) struct FakeUpstream {
    pub(crate) response: Result<ObservedResponse, ()>,
}

impl ChatCompletionsUpstream for FakeUpstream {
    fn send_chat_completions(
        &self,
        _request: ChatCompletionsRequest<'_>,
    ) -> impl std::future::Future<Output = HarnessResult<ObservedResponse>> {
        std::future::ready(self.response.as_ref().map_or_else(
            |()| {
                Err(HarnessError::UpstreamRequestFailed {
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::ConnectionRefused,
                        "stub failure",
                    )),
                })
            },
            |response| Ok(response.clone()),
        ))
    }

    fn stream_chat_completions(
        &self,
        _request: ChatCompletionsRequest<'_>,
    ) -> impl std::future::Future<Output = HarnessResult<StreamingObservedResponse>> {
        std::future::ready(self.response.as_ref().map_or_else(
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
        ))
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

/// Record service whose metadata builds signal through an unbounded channel.
pub(crate) struct SignallingServiceFixture {
    /// Service under test, wired to the signalling metadata factory.
    pub(crate) service: RecordService<FakeUpstream, FakeEnvProvider, SignallingMetadataFactory>,
    /// Receiver for one signal per successful metadata build.
    pub(crate) metadata_signals: tokio_mpsc::UnboundedReceiver<()>,
}

/// Creates a record service that reports each successful metadata build.
///
/// The caller must keep the [`CassetteFixture`] holding `cassette_path` alive
/// for the duration of the test.
pub(crate) fn signalling_service_fixture(
    cassette_path: &Utf8Path,
    upstream: FakeUpstream,
    env_provider: FakeEnvProvider,
) -> eyre::Result<SignallingServiceFixture> {
    let cassette_store = FilesystemCassetteStore::open_or_create_for_record(cassette_path)
        .wrap_err("cassette should open for recording")?;
    let (signals, metadata_signals) = tokio_mpsc::unbounded_channel();

    Ok(SignallingServiceFixture {
        service: RecordService {
            cassette_store: Arc::new(Mutex::new(cassette_store)),
            upstream_client: upstream,
            env_provider,
            metadata: SignallingMetadataFactory::new(signals),
            upstream: UpstreamConfig::default(),
            redaction: RedactionConfig {
                drop_headers: vec!["authorization".to_owned()],
            },
            recorded_count: Arc::new(AtomicU64::new(0)),
            failure_count: Arc::new(AtomicU64::new(0)),
            interaction_seq: Arc::new(AtomicU64::new(0)),
        },
        metadata_signals,
    })
}

/// How a [`BlockedCassetteWrite`] holder released the shared cassette lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReleaseOutcome {
    /// The test sent an explicit release.
    Released,
    /// The holder released the lock itself after [`RELEASE_TIMEOUT`] elapsed.
    TimedOut,
}

/// Upper bound on how long a stalled test may keep the cassette lock held.
/// The holder releases the lock itself once this elapses. A Tokio timeout
/// cannot rescue a worker blocked in `Mutex::lock`, because the timeout future
/// also needs the executor thread that the blocked worker is occupying.
const RELEASE_TIMEOUT: Duration = Duration::from_secs(10);

/// Holds the shared cassette lock on a standard thread to simulate slow I/O.
///
/// Acquisition is acknowledged through a Tokio `oneshot` channel, so an async
/// test can wait without blocking its executor. The lock is released on an
/// explicit [`release`](Self::release) or after [`RELEASE_TIMEOUT`], and
/// [`Drop`] releases and joins the thread on every exit path, including panics.
pub(crate) struct BlockedCassetteWrite {
    /// Receives the acknowledgement sent once the holder owns the lock.
    acknowledgement: Option<oneshot::Receiver<()>>,
    /// Tells the holder to drop the lock.
    release_tx: Option<mpsc::Sender<()>>,
    /// Thread that holds the lock until released or timed out.
    handle: Option<JoinHandle<ReleaseOutcome>>,
}

impl BlockedCassetteWrite {
    /// Starts a standard thread that acquires `store`'s mutex.
    /// For example, the holder reports [`ReleaseOutcome::Released`] after
    /// [`release`](Self::release) is called.
    pub(crate) fn lock_store(store: &SharedCassetteStore) -> Self {
        let (ack_tx, ack_rx) = oneshot::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let shared_store = Arc::clone(store);
        let handle = std::thread::spawn(move || {
            let _guard = shared_store
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let _sent = ack_tx.send(());
            match release_rx.recv_timeout(RELEASE_TIMEOUT) {
                Ok(()) => ReleaseOutcome::Released,
                Err(_) => ReleaseOutcome::TimedOut,
            }
        });
        Self {
            acknowledgement: Some(ack_rx),
            release_tx: Some(release_tx),
            handle: Some(handle),
        }
    }

    /// Waits until the holder thread owns the lock, failing when it never did.
    pub(crate) async fn wait_until_locked(&mut self) -> eyre::Result<()> {
        let acknowledgement = self
            .acknowledgement
            .take()
            .ok_or_else(|| eyre::eyre!("cassette lock holder was already awaited"))?;
        acknowledgement
            .await
            .wrap_err("cassette lock holder should acknowledge acquisition")
    }

    /// Signals the holder to release the lock.
    pub(crate) fn release(&mut self) {
        if let Some(release_tx) = self.release_tx.take() {
            let _sent = release_tx.send(());
        }
    }

    /// Joins the holder thread and reports how the lock was released.
    pub(crate) fn join(&mut self) -> eyre::Result<ReleaseOutcome> {
        let handle = self
            .handle
            .take()
            .ok_or_else(|| eyre::eyre!("cassette lock holder was already joined"))?;
        handle
            .join()
            .map_err(|_| eyre::eyre!("cassette lock holder thread panicked"))
    }
}

impl Drop for BlockedCassetteWrite {
    fn drop(&mut self) {
        self.release();
        if let Some(handle) = self.handle.take() {
            drop(handle.join());
        }
    }
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
