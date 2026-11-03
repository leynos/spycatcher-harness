//! Executor-responsiveness tests for record-mode cassette writes.
//!
//! Each test holds the shared cassette mutex from a standard thread and runs
//! requests on the default single-threaded Tokio test runtime. A filesystem
//! write performed directly on the executor would block the only worker, so
//! the progress assertions below would never observe their signal.
//!
//! A blocked executor also freezes Tokio timers, so the holder releases the
//! lock itself after `RELEASE_TIMEOUT`. A regression then fails on the
//! holder's `TimedOut` outcome instead of hanging the test run.

use std::{sync::Arc, time::Duration};

use axum::body::Bytes;
use camino::{Utf8Path, Utf8PathBuf};
use cap_std::{ambient_authority, fs_utf8::Dir};
use eyre::WrapErr;
use futures_util::{StreamExt, stream::BoxStream};
use rstest::rstest;
use tokio::sync::{mpsc::UnboundedReceiver, oneshot};

use super::{
    ProxyBody,
    ProxyResponse,
    RecordService,
    SharedCassetteStore,
    record_tests_helpers::{
        BlockedCassetteWrite,
        FakeEnvProvider,
        FakeMetadataFactory,
        FakeUpstream,
        ReleaseOutcome,
        SignallingServiceFixture,
        cassette_fixture,
        load_cassette,
        sample_request,
        sample_response,
        service_fixture,
        signalling_service_fixture,
    },
};
use crate::{HarnessResult, http_exchange::parse_json_bytes};

/// Number of concurrent requests used to prove the executor keeps running.
const REQUEST_COUNT: usize = 4;

/// Bounds every progress assertion so a regression fails instead of hanging.
const PROGRESS_TIMEOUT: Duration = Duration::from_secs(10);

/// Upstream body shared by the response-preservation assertions.
const UPSTREAM_BODY: &[u8] = br#"{"id":"ok"}"#;

#[rstest]
#[tokio::test]
async fn blocked_cassette_write_keeps_executor_responsive() -> eyre::Result<()> {
    let cassette = cassette_fixture("blocked-write")?;
    let SignallingServiceFixture {
        service,
        mut metadata_signals,
    } = signalling_service_fixture(
        &cassette.path,
        FakeUpstream {
            response: Ok(sample_response(UPSTREAM_BODY)),
        },
        FakeEnvProvider(Some("token".to_owned())),
    )?;
    let shared_service = Arc::new(service);

    let mut holder = BlockedCassetteWrite::lock_store(&shared_service.cassette_store);
    holder.wait_until_locked().await?;

    let handles: Vec<_> = (0..REQUEST_COUNT)
        .map(|_| {
            let request_service = Arc::clone(&shared_service);
            tokio::spawn(async move {
                request_service
                    .handle_chat_completions(sample_request(None))
                    .await
            })
        })
        .collect();

    // An executor-thread write would block the only worker at the first append
    // and prevent the remaining metadata signals from ever arriving.
    for _ in 0..REQUEST_COUNT {
        await_metadata_signal(&mut metadata_signals).await?;
    }
    eyre::ensure!(
        handles.iter().all(|handle| !handle.is_finished()),
        "no request should finish while the cassette write is blocked"
    );

    holder.release();
    eyre::ensure!(
        holder.join()? == ReleaseOutcome::Released,
        "the holder should observe an explicit release"
    );

    for handle in handles {
        let result = handle
            .await
            .wrap_err("blocked request task should not panic")?;
        let response =
            result.map_err(|error| eyre::eyre!("blocked request should succeed: {error:?}"))?;
        assert_preserved_response(&response)?;
    }

    eyre::ensure!(
        shared_service.counters() == (REQUEST_COUNT as u64, 0),
        "every request should be recorded without failures; got {:?}",
        shared_service.counters()
    );
    let persisted = load_cassette(&cassette.path)?;
    eyre::ensure!(
        persisted.interactions.len() == REQUEST_COUNT,
        "every interaction should be persisted; got {}",
        persisted.interactions.len()
    );
    Ok(())
}

#[rstest]
#[tokio::test]
async fn blocked_stream_persistence_keeps_executor_progress() -> eyre::Result<()> {
    let cassette = cassette_fixture("blocked-stream")?;
    let stream_body = b"data: {\"id\":\"chunk-1\"}\n\n";
    let SignallingServiceFixture {
        service,
        metadata_signals: _metadata_signals,
    } = signalling_service_fixture(
        &cassette.path,
        FakeUpstream {
            response: Ok(sample_response(stream_body)),
        },
        FakeEnvProvider(Some("token".to_owned())),
    )?;

    let mut holder = BlockedCassetteWrite::lock_store(&service.cassette_store);
    holder.wait_until_locked().await?;

    let mut request = sample_request(None);
    request.body = br#"{"model":"gpt-test","stream":true}"#.to_vec();
    request.parsed_json = parse_json_bytes(&request.body);

    let proxied = service
        .handle_chat_completions(request)
        .await
        .map_err(|error| eyre::eyre!("stream request should be proxied: {error:?}"))?;
    eyre::ensure!(
        proxied.status == 200,
        "upstream status should be preserved; got {}",
        proxied.status
    );
    let ProxyBody::Stream(mut body) = proxied.body else {
        eyre::bail!("expected streaming response");
    };

    assert_first_chunk(&mut body, stream_body).await?;
    assert_stream_end_stays_pending(&mut body).await?;

    holder.release();
    eyre::ensure!(
        holder.join()? == ReleaseOutcome::Released,
        "the holder should observe an explicit release"
    );

    assert_stream_completed(&mut body).await?;
    eyre::ensure!(
        service.counters() == (1, 0),
        "the stream should be recorded once without failures; got {:?}",
        service.counters()
    );
    let persisted = load_cassette(&cassette.path)?;
    eyre::ensure!(
        persisted.interactions.len() == 1,
        "the completed stream should be persisted once; got {}",
        persisted.interactions.len()
    );
    Ok(())
}

/// Store failures that must preserve the upstream response.
#[derive(Debug, Clone, Copy)]
enum StoreFailure {
    /// Another thread panicked while holding the store mutex.
    PoisonedLock,
    /// A directory occupies the temporary file path used for atomic writes.
    UnwritableTempFile,
}

#[rstest]
#[case::poisoned_lock(StoreFailure::PoisonedLock)]
#[case::unwritable_temp_file(StoreFailure::UnwritableTempFile)]
#[tokio::test]
async fn cassette_write_failure_preserves_upstream_response(
    #[case] failure: StoreFailure,
) -> eyre::Result<()> {
    let cassette = cassette_fixture("write-failure")?;
    let service = service_fixture(
        &cassette.path,
        FakeUpstream {
            response: Ok(sample_response(UPSTREAM_BODY)),
        },
        FakeEnvProvider(Some("token".to_owned())),
    )?;
    induce_store_failure(failure, &service, &cassette.path)?;

    let response = service
        .handle_chat_completions(sample_request(None))
        .await
        .map_err(|error| {
            eyre::eyre!("upstream response should survive a cassette write failure: {error:?}")
        })?;
    assert_preserved_response(&response)?;

    eyre::ensure!(
        service.counters() == (0, 1),
        "a failed write should count one failure; got {:?}",
        service.counters()
    );
    let persisted = load_cassette(&cassette.path)?;
    eyre::ensure!(
        persisted.interactions.is_empty(),
        "a failed write should not persist an interaction; got {}",
        persisted.interactions.len()
    );
    Ok(())
}

/// Stream body produced by record-mode proxying.
type RecordedBody = BoxStream<'static, HarnessResult<Bytes>>;

/// Asserts that the upstream's single chunk is forwarded unchanged.
async fn assert_first_chunk(body: &mut RecordedBody, expected: &[u8]) -> eyre::Result<()> {
    let forwarded = body
        .next()
        .await
        .ok_or_else(|| eyre::eyre!("stream should yield the upstream chunk"))?
        .wrap_err("upstream chunk should be forwarded")?;
    eyre::ensure!(
        forwarded.as_ref() == expected,
        "forwarded chunk should match the upstream bytes; got {forwarded:?}"
    );
    Ok(())
}

/// Proves that end-of-stream stays pending while another task makes progress.
async fn assert_stream_end_stays_pending(body: &mut RecordedBody) -> eyre::Result<()> {
    // Polling the stream again finalizes the recording, which blocks on the
    // held cassette lock. An unrelated task must still be able to complete.
    let (progress_tx, progress_rx) = oneshot::channel();
    tokio::spawn(async move {
        let _sent = progress_tx.send(());
    });
    tokio::select! {
        biased;
        progress = progress_rx => {
            progress.wrap_err("unrelated task should complete while the write is blocked")?;
        }
        () = tokio::time::sleep(PROGRESS_TIMEOUT) => {
            eyre::bail!("unrelated task should make progress while the write is blocked");
        }
        next = body.next() => {
            eyre::bail!(
                "end of stream should remain pending while the cassette write is blocked; got \
                 {next:?}"
            );
        }
    }
    Ok(())
}

/// Asserts that the stream ends after the cassette lock is released.
async fn assert_stream_completed(body: &mut RecordedBody) -> eyre::Result<()> {
    let end = tokio::time::timeout(PROGRESS_TIMEOUT, body.next())
        .await
        .wrap_err("stream should end after the cassette lock is released")?;
    eyre::ensure!(end.is_none(), "stream should end exactly once; got {end:?}");
    Ok(())
}

/// Waits for one metadata build signal under the progress timeout.
async fn await_metadata_signal(signals: &mut UnboundedReceiver<()>) -> eyre::Result<()> {
    let signal = tokio::time::timeout(PROGRESS_TIMEOUT, signals.recv())
        .await
        .wrap_err("request should reach cassette persistence while the store is locked")?;
    eyre::ensure!(signal.is_some(), "metadata signal channel should stay open");
    Ok(())
}

/// Asserts that a proxied response preserves the upstream status and body.
fn assert_preserved_response(response: &ProxyResponse) -> eyre::Result<()> {
    eyre::ensure!(
        response.status == 200,
        "upstream status should be preserved; got {}",
        response.status
    );
    let ProxyBody::Buffered(body) = &response.body else {
        eyre::bail!("expected buffered response");
    };
    eyre::ensure!(
        body.as_slice() == UPSTREAM_BODY,
        "upstream body should be preserved; got {body:?}"
    );
    Ok(())
}

/// Breaks the store in the way selected by `failure`.
fn induce_store_failure(
    failure: StoreFailure,
    service: &RecordService<FakeUpstream, FakeEnvProvider, FakeMetadataFactory>,
    cassette_path: &Utf8Path,
) -> eyre::Result<()> {
    match failure {
        StoreFailure::PoisonedLock => poison_store(&service.cassette_store),
        StoreFailure::UnwritableTempFile => create_temp_directory(cassette_path),
    }
}

/// Panics on a standard thread while holding a clone of the store mutex.
fn poison_store(store: &SharedCassetteStore) -> eyre::Result<()> {
    eyre::ensure!(
        thread_panics_while_holding(store),
        "the poisoning thread should panic"
    );
    Ok(())
}

/// Reports whether a thread can panic while holding a clone of `store`.
fn thread_panics_while_holding(store: &SharedCassetteStore) -> bool {
    let shared_store = Arc::clone(store);
    std::thread::spawn(move || {
        let _guard = shared_store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        panic!("poison the cassette store mutex");
    })
    .join()
    .is_err()
}

/// Creates a directory where the store writes its temporary sibling file.
fn create_temp_directory(cassette_path: &Utf8Path) -> eyre::Result<()> {
    let temp_path = Utf8PathBuf::from(format!("{cassette_path}.tmp"));
    Dir::open_ambient_dir(".", ambient_authority())?
        .create_dir(&temp_path)
        .wrap_err("temporary sibling directory should be created")?;
    Ok(())
}
