//! Test-only stall point for the filesystem cassette append path.
//!
//! [`FilesystemCassetteStore::flush`](super::filesystem::FilesystemCassetteStore)
//! waits on an installed stall after opening its temporary file. Tests use it
//! to prove a slow append runs on the blocking pool. Production builds do not
//! compile this module, so `flush` has no stall path outside tests.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use tokio::sync::Notify;

/// How long a stalled append waits for the test's explicit release.
///
/// A regression that performs the write on the executor would otherwise
/// deadlock the current-thread test runtime, because a Tokio timeout cannot
/// fire while its only worker is blocked. Releasing after this bound lets the
/// test fail on its assertions instead of hanging.
const STALL_TIMEOUT: Duration = Duration::from_secs(10);

/// Test-only stall point inside the cassette append path.
///
/// Installing one makes the next `flush` signal that it reached the write and
/// then wait for an explicit release. Tests use it to prove a slow append runs
/// off the async executor. Without an installed stall the write path is
/// unchanged, so production behaviour is unaffected.
#[derive(Debug)]
pub(crate) struct AppendStall {
    /// Signalled once `flush` reaches the stall point.
    entered: Arc<Notify>,
    /// Receives the test's explicit release.
    release: mpsc::Receiver<()>,
    /// Set when the release arrived within [`STALL_TIMEOUT`].
    is_explicit: Arc<AtomicBool>,
}

impl AppendStall {
    /// Creates a stall and the test-side control that drives it.
    pub(crate) fn pair() -> (Self, AppendStallControl) {
        let (release_control, release) = mpsc::channel();
        let entered = Arc::new(Notify::new());
        let is_explicit = Arc::new(AtomicBool::new(false));
        let stall = Self {
            entered: Arc::clone(&entered),
            release,
            is_explicit: Arc::clone(&is_explicit),
        };
        let control = AppendStallControl {
            entered,
            release: release_control,
            is_explicit,
        };
        (stall, control)
    }

    /// Reports the stall point and waits for the test to release it.
    pub(crate) fn wait(&self) {
        self.entered.notify_one();
        let is_explicit = self.release.recv_timeout(STALL_TIMEOUT).is_ok();
        self.is_explicit.store(is_explicit, Ordering::SeqCst);
    }
}

/// Test-side handle for a stalled append.
#[derive(Debug)]
pub(crate) struct AppendStallControl {
    /// Signalled once `flush` reaches the stall point.
    entered: Arc<Notify>,
    /// Releases the stalled append.
    release: mpsc::Sender<()>,
    /// Set when the append observed an explicit release.
    is_explicit: Arc<AtomicBool>,
}

impl AppendStallControl {
    /// Resolves once the append path reaches the stall point.
    ///
    /// Callers bound this wait themselves so a regression fails instead of
    /// hanging the test run.
    pub(crate) async fn wait_until_entered(&self) { self.entered.notified().await }

    /// Releases the stalled append.
    pub(crate) fn release(&self) {
        // A send failure means the append already timed out and stopped waiting.
        _ = self.release.send(());
    }

    /// Reports whether the append observed an explicit release.
    pub(crate) fn observed_explicit_release(&self) -> bool {
        self.is_explicit.load(Ordering::SeqCst)
    }
}
