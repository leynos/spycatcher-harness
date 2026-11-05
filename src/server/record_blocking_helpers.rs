//! Test support for deterministically blocking cassette-store writes.
//!
//! The helpers here hold the record service's store mutex from a standard
//! thread, or break the store in a controlled way, so responsiveness and
//! boundary-failure tests need neither sleeps nor production changes.

use std::{
    sync::{
        Arc,
        Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use camino::Utf8Path;
use cap_std::{ambient_authority, fs_utf8::Dir};
use eyre::WrapErr;
use tokio::sync::Notify;

use super::{
    RecordService,
    record_tests_helpers::{
        FakeEnvProvider,
        FakeMetadataFactory,
        FakeUpstream,
        service_with_metadata,
    },
};
use crate::{
    HarnessResult,
    cassette::{
        InteractionMetadata,
        append_stall::{AppendStall, AppendStallControl},
        filesystem::FilesystemCassetteStore,
    },
    server::record_metadata::MetadataFactory,
};

/// How long the blocking holder waits before releasing the store unsolicited.
///
/// A regression that performs cassette I/O on the executor thread would
/// otherwise deadlock the current-thread test runtime, because a Tokio timeout
/// cannot fire while its only worker is blocked in `Mutex::lock`. Releasing
/// after this bound lets the test fail on its assertions instead of hanging.
const HOLD_TIMEOUT: Duration = Duration::from_secs(10);

/// How long tests wait for request-side progress while the store is held.
pub(crate) const PROGRESS_TIMEOUT: Duration = Duration::from_secs(5);

/// Metadata factory that signals every interaction it builds.
///
/// Tests use the signal to know a request has passed interaction construction
/// and is about to enter the cassette append boundary.
#[derive(Debug, Clone, Default)]
pub(crate) struct SignallingMetadataFactory {
    /// Number of metadata payloads produced so far.
    built: Arc<AtomicUsize>,
    /// Wakes waiters after each successful build.
    built_signal: Arc<Notify>,
}

impl SignallingMetadataFactory {
    /// Waits until at least `expected` interactions have been built.
    ///
    /// # Errors
    ///
    /// Returns an error when the expected builds do not occur within
    /// [`PROGRESS_TIMEOUT`].
    pub(crate) async fn wait_for_builds(&self, expected: usize) -> eyre::Result<()> {
        tokio::time::timeout(PROGRESS_TIMEOUT, self.await_build_count(expected))
            .await
            .map_err(|_| {
                eyre::eyre!(
                    "timed out waiting for {expected} interaction builds; cassette I/O may be \
                     blocking the executor"
                )
            })
    }

    /// Resolves once the built interaction count reaches `expected`.
    async fn await_build_count(&self, expected: usize) {
        while self.built.load(Ordering::SeqCst) < expected {
            self.built_signal.notified().await;
        }
    }
}

impl MetadataFactory for SignallingMetadataFactory {
    fn create(&self) -> HarnessResult<InteractionMetadata> {
        let metadata = FakeMetadataFactory.create()?;
        self.built.fetch_add(1, Ordering::SeqCst);
        self.built_signal.notify_one();
        Ok(metadata)
    }
}

/// Builds a record service that signals every interaction build.
///
/// The returned factory is a clone of the service's own, so a test can await
/// builds while requests are still running. The caller must keep the cassette
/// fixture's temporary directory alive for the service's lifetime.
///
/// # Errors
///
/// Returns an error when the cassette cannot be opened for recording.
pub(crate) fn signalling_service_fixture(
    cassette_path: &Utf8Path,
    upstream: FakeUpstream,
    env_provider: FakeEnvProvider,
) -> eyre::Result<(
    RecordService<FakeUpstream, FakeEnvProvider, SignallingMetadataFactory>,
    SignallingMetadataFactory,
)> {
    let metadata = SignallingMetadataFactory::default();
    let service = service_with_metadata(cassette_path, upstream, env_provider, metadata.clone())?;
    Ok((service, metadata))
}

/// A dedicated standard thread that holds the cassette-store mutex.
///
/// The holder simulates a slow cassette write without sleeping or patching the
/// store: while it owns the lock, every append waits inside the production
/// `spawn_blocking` boundary. A Tokio timeout cannot rescue a worker blocked in
/// `Mutex::lock`, so the holder releases itself after [`HOLD_TIMEOUT`] and
/// reports that the release was not explicit.
pub(crate) struct BlockedCassetteWrite {
    /// Sender whose message releases the holding thread.
    release: Option<mpsc::Sender<()>>,
    /// Thread that owns the guard and reports whether release was explicit.
    thread: Option<thread::JoinHandle<bool>>,
}

impl BlockedCassetteWrite {
    /// Acquires `store`'s mutex on a dedicated thread.
    ///
    /// # Errors
    ///
    /// Returns an error when the thread cannot start or does not confirm the
    /// lock within [`HOLD_TIMEOUT`].
    pub(crate) fn acquire(store: Arc<Mutex<FilesystemCassetteStore>>) -> eyre::Result<Self> {
        let (locked_tx, locked_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("blocked-cassette-write".to_owned())
            .spawn(move || hold_store_lock_until_released(&store, &locked_tx, &release_rx))
            .wrap_err("blocked cassette holder thread should start")?;
        locked_rx
            .recv_timeout(HOLD_TIMEOUT)
            .wrap_err("blocked cassette holder should acquire the store lock")?;
        Ok(Self {
            release: Some(release_tx),
            thread: Some(thread),
        })
    }

    /// Releases the store lock so waiting appends can proceed.
    pub(crate) fn release(&mut self) {
        if let Some(sender) = self.release.take() {
            // A send failure means the holder already timed out and stopped waiting.
            _ = sender.send(());
        }
    }

    /// Releases the lock and joins the holder thread.
    ///
    /// Returns `true` when the holder observed an explicit release and `false`
    /// when it timed out and released the lock on its own.
    ///
    /// # Errors
    ///
    /// Returns an error when the holder thread panicked.
    pub(crate) fn join(&mut self) -> eyre::Result<bool> {
        self.release();
        let handle = self
            .thread
            .take()
            .ok_or_else(|| eyre::eyre!("blocked cassette holder was already joined"))?;
        handle
            .join()
            .map_err(|_| eyre::eyre!("blocked cassette holder thread panicked"))
    }
}

impl Drop for BlockedCassetteWrite {
    fn drop(&mut self) {
        self.release();
        if let Some(handle) = self.thread.take() {
            // Release even when an assertion aborts the test; otherwise the lock
            // and its thread would outlive the test's useful work.
            drop(handle.join());
        }
    }
}

/// Owns the store guard until an explicit release or [`HOLD_TIMEOUT`].
fn hold_store_lock_until_released(
    store: &Mutex<FilesystemCassetteStore>,
    locked: &mpsc::Sender<()>,
    release: &mpsc::Receiver<()>,
) -> bool {
    let Ok(guard) = store.lock() else {
        return false;
    };
    if locked.send(()).is_err() {
        return false;
    }
    let is_explicit = release.recv_timeout(HOLD_TIMEOUT).is_ok();
    drop(guard);
    is_explicit
}

/// Clones the shared cassette-store handle from a record service.
pub(crate) fn cassette_store_handle<U, E, M>(
    service: &RecordService<U, E, M>,
) -> Arc<Mutex<FilesystemCassetteStore>> {
    Arc::clone(&service.cassette_store)
}

/// Installs a stall point inside the store's append path.
///
/// Unlike [`BlockedCassetteWrite`], which stalls `Mutex::lock`, this stall is
/// reached only once the append owns the lock and has opened its temporary
/// file. A test can therefore prove that the write itself, not just the lock
/// wait, runs on the blocking pool.
///
/// # Errors
///
/// Returns an error when the store mutex is poisoned.
pub(crate) fn install_append_stall(
    store: &Arc<Mutex<FilesystemCassetteStore>>,
) -> eyre::Result<AppendStallControl> {
    let (stall, control) = AppendStall::pair();
    let mut guard = store.lock().map_err(|error| {
        eyre::eyre!("store lock should be available before stalling: {error:?}")
    })?;
    guard.install_append_stall(stall);
    Ok(control)
}

/// Poisons the service's cassette-store mutex.
///
/// A later `Mutex::lock` then fails, exercising the append boundary's poisoned
/// lock branch without changing production code.
///
/// # Errors
///
/// Returns an error when the poisoning thread completes without panicking.
pub(crate) fn poison_store_lock<U, E, M>(service: &RecordService<U, E, M>) -> eyre::Result<()> {
    let store = Arc::clone(&service.cassette_store);
    let handle = thread::spawn(move || hold_lock_and_panic(&store));
    let panicked = handle.join().is_err();
    eyre::ensure!(
        panicked,
        "poisoning thread should panic while holding the store lock"
    );
    Ok(())
}

/// Acquires the store lock and panics, poisoning it for later lockers.
fn hold_lock_and_panic(store: &Mutex<FilesystemCassetteStore>) {
    match store.lock() {
        Ok(_guard) => panic!("intentional cassette-store lock poisoning"),
        Err(error) => panic!("store lock should be available before poisoning: {error:?}"),
    }
}

/// Creates a directory where the store writes its temporary cassette file.
///
/// The next append fails while flushing, exercising the write-failure branch
/// without changing production code.
///
/// # Errors
///
/// Returns an error when the parent directory cannot be opened or the
/// directory cannot be created.
pub(crate) fn block_cassette_temp_path(cassette_path: &Utf8Path) -> eyre::Result<()> {
    let parent = cassette_path
        .parent()
        .ok_or_else(|| eyre::eyre!("cassette path should have a parent: {cassette_path}"))?;
    let file_name = cassette_path
        .file_name()
        .ok_or_else(|| eyre::eyre!("cassette path should have a file name: {cassette_path}"))?;
    let dir = Dir::open_ambient_dir(parent, ambient_authority())
        .wrap_err("cassette parent directory should open")?;
    dir.create_dir(format!("{file_name}.tmp"))
        .wrap_err("temporary cassette path should become a directory")?;
    Ok(())
}
