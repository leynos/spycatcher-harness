//! Regression tests for executor responsiveness during cassette writes.
//!
//! Each test holds the record service's store mutex from a standard thread and
//! runs on Tokio's single-threaded runtime. If cassette I/O happened on the
//! executor, the only worker would block and these tests would fail.

use std::sync::Arc;

use eyre::WrapErr;
use futures_util::{StreamExt, TryStreamExt, stream::BoxStream};
use rstest::rstest;

use super::{
    blocking_helpers::{
        BlockedCassetteWrite,
        block_cassette_temp_path,
        cassette_store_handle,
        poison_store_lock,
        signalling_service_fixture,
    },
    record_tests_helpers::{
        FakeEnvProvider,
        FakeUpstream,
        cassette_fixture,
        load_cassette,
        sample_request,
        sample_response,
        service_fixture,
    },
};
use crate::{
    HarnessResult,
    http_exchange::{ObservedRequest, ProxyBody, ProxyResponse, parse_json_bytes},
};

/// Number of requests spawned while the cassette store is held.
const PARALLEL_REQUESTS: usize = 4;

/// Upstream body asserted for every non-stream response.
const NON_STREAM_BODY: &[u8] = br#"{"id":"ok"}"#;

/// One SSE event that the stream recorder accepts and persists.
const STREAM_BODY: &[u8] = b"data: {\"id\":\"chunk-1\"}\n\n";

/// A store failure that must not replace a successful upstream response.
#[derive(Debug, Clone, Copy)]
enum StoreFailure {
    /// The shared store mutex is poisoned before the request.
    PoisonedLock,
    /// The store cannot create its temporary cassette file.
    AppendError,
}

#[tokio::test]
async fn non_stream_requests_progress_while_the_cassette_write_is_blocked() -> eyre::Result<()> {
    let cassette = cassette_fixture("blocked-non-stream")?;
    let (recording, metadata) = signalling_service_fixture(
        &cassette.path,
        FakeUpstream {
            response: Ok(sample_response(NON_STREAM_BODY)),
        },
        FakeEnvProvider(Some("blocked-key".to_owned())),
    )?;
    let service = Arc::new(recording);
    let mut holder = BlockedCassetteWrite::acquire(cassette_store_handle(&service))?;

    let handles: Vec<_> = (0..PARALLEL_REQUESTS)
        .map(|_| {
            let request_service = Arc::clone(&service);
            tokio::spawn(async move {
                request_service
                    .handle_chat_completions(sample_request(None))
                    .await
            })
        })
        .collect();

    // An executor-thread write would block the only worker here, so the
    // remaining signals could never arrive.
    metadata.wait_for_builds(PARALLEL_REQUESTS).await?;

    for handle in &handles {
        eyre::ensure!(
            !handle.is_finished(),
            "a request must not finish while the cassette write is blocked"
        );
    }

    holder.release();
    let is_explicit = holder.join()?;
    eyre::ensure!(is_explicit, "holder must observe an explicit release");

    for handle in handles {
        let proxied = handle
            .await
            .wrap_err("request task should not panic")?
            .map_err(|error| eyre::eyre!("request should succeed: {error:?}"))?;
        assert_preserved_response(&proxied)?;
    }

    let expected_count =
        u64::try_from(PARALLEL_REQUESTS).wrap_err("request count should fit in u64")?;
    eyre::ensure!(
        service.counters() == (expected_count, 0),
        "every request should be recorded exactly once; got {:?}",
        service.counters()
    );
    let persisted = load_cassette(&cassette.path)?;
    eyre::ensure!(
        persisted.interactions.len() == PARALLEL_REQUESTS,
        "every request should be persisted; got {} interactions",
        persisted.interactions.len()
    );
    Ok(())
}

#[tokio::test]
async fn stream_chunks_are_forwarded_while_the_cassette_write_is_blocked() -> eyre::Result<()> {
    let cassette = cassette_fixture("blocked-stream")?;
    let (recording, metadata) = signalling_service_fixture(
        &cassette.path,
        FakeUpstream {
            response: Ok(sample_response(STREAM_BODY)),
        },
        FakeEnvProvider(Some("blocked-key".to_owned())),
    )?;
    let service = Arc::new(recording);
    let mut holder = BlockedCassetteWrite::acquire(cassette_store_handle(&service))?;

    let proxied = service
        .handle_chat_completions(streaming_request())
        .await
        .map_err(|error| eyre::eyre!("streaming request should start: {error:?}"))?;
    let ProxyBody::Stream(mut body) = proxied.body else {
        eyre::bail!("expected a streaming response body");
    };

    let first_chunk = body
        .next()
        .await
        .ok_or_else(|| eyre::eyre!("stream should yield the upstream chunk"))?
        .wrap_err("stream chunk should be forwarded")?;
    eyre::ensure!(
        first_chunk.as_ref() == STREAM_BODY,
        "the first chunk should be forwarded unchanged; got {first_chunk:?}"
    );

    let (completed_tx, mut completed_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        _ = completed_tx.send(drain_stream(body).await);
    });

    // The build signal means the drain task has reached the append boundary.
    metadata.wait_for_builds(1).await?;

    let progress_rx = spawn_progress_signal();

    tokio::select! {
        biased;
        completion = &mut completed_rx => {
            eyre::bail!(
                "stream must not complete while the cassette write is blocked; got {completion:?}"
            );
        }
        result = progress_rx => {
            result.wrap_err(
                "an unrelated task should make progress while the cassette write is blocked",
            )?;
        }
    }

    holder.release();
    let is_explicit = holder.join()?;
    eyre::ensure!(is_explicit, "holder must observe an explicit release");

    completed_rx
        .await
        .wrap_err("stream drain task should report its outcome")?
        .map_err(|message| eyre::eyre!("stream should finish cleanly: {message}"))?;

    eyre::ensure!(
        service.counters() == (1, 0),
        "the stream should be recorded once; got {:?}",
        service.counters()
    );
    let persisted = load_cassette(&cassette.path)?;
    eyre::ensure!(
        persisted.interactions.len() == 1,
        "the stream should persist one interaction; got {}",
        persisted.interactions.len()
    );
    Ok(())
}

#[rstest]
#[case::poisoned_lock(StoreFailure::PoisonedLock)]
#[case::append_error(StoreFailure::AppendError)]
#[tokio::test]
async fn store_boundary_failures_preserve_the_upstream_response(
    #[case] failure: StoreFailure,
) -> eyre::Result<()> {
    let cassette = cassette_fixture("store-failure")?;
    let service = service_fixture(
        &cassette.path,
        FakeUpstream {
            response: Ok(sample_response(NON_STREAM_BODY)),
        },
        FakeEnvProvider(Some("token".to_owned())),
    )?;
    match failure {
        StoreFailure::PoisonedLock => poison_store_lock(&service)?,
        StoreFailure::AppendError => block_cassette_temp_path(&cassette.path)?,
    }

    let proxied = service
        .handle_chat_completions(sample_request(None))
        .await
        .map_err(|error| eyre::eyre!("upstream response should survive {failure:?}: {error:?}"))?;
    assert_preserved_response(&proxied)?;
    eyre::ensure!(
        service.counters() == (0, 1),
        "a store boundary failure should count once; got {:?}",
        service.counters()
    );
    let persisted = load_cassette(&cassette.path)?;
    eyre::ensure!(
        persisted.interactions.is_empty(),
        "a failed append must not persist an interaction; got {}",
        persisted.interactions.len()
    );
    Ok(())
}

/// Spawns a task that signals once the executor schedules it.
fn spawn_progress_signal() -> tokio::sync::oneshot::Receiver<()> {
    let (progress_tx, progress_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        tokio::task::yield_now().await;
        _ = progress_tx.send(());
    });
    progress_rx
}

/// Drains the rest of the stream, formatting the first error it yields.
async fn drain_stream(
    body: BoxStream<'static, HarnessResult<axum::body::Bytes>>,
) -> Result<(), String> {
    body.map_err(|error| error.to_string())
        .try_fold((), |(), _chunk| std::future::ready(Ok(())))
        .await
}

/// Builds a chat-completions request that selects the streaming path.
fn streaming_request() -> ObservedRequest {
    let body = br#"{"model":"gpt-test","stream":true}"#.to_vec();
    ObservedRequest {
        parsed_json: parse_json_bytes(&body),
        body,
        ..sample_request(None)
    }
}

/// Asserts that a proxied response keeps the upstream status and bytes.
fn assert_preserved_response(proxied: &ProxyResponse) -> eyre::Result<()> {
    eyre::ensure!(
        proxied.status == 200,
        "upstream status should be preserved; got {}",
        proxied.status
    );
    let ProxyBody::Buffered(body) = &proxied.body else {
        eyre::bail!("expected a buffered response body");
    };
    eyre::ensure!(
        body.as_slice() == NON_STREAM_BODY,
        "upstream bytes should be preserved; got {body:?}"
    );
    Ok(())
}
