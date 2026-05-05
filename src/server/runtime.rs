//! Record-mode server startup and shutdown lifecycle.
//!
//! This keeps listener binding and graceful shutdown wiring out of the crate
//! root while returning a small handle that `RunningHarness` can own.

use std::net::SocketAddr;

use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::cassette::filesystem::FilesystemCassetteStore;
use crate::config::HarnessConfig;
use crate::protocol::CHAT_COMPLETIONS_PATH;
use crate::server::record::RecordAppState;
use crate::server::record_handler::record_chat_completions_handler;
use crate::{HarnessError, HarnessResult};

/// Runtime handle for a bound record-mode HTTP server.
#[derive(Debug)]
pub(crate) struct RecordServerHandle {
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<HarnessResult<()>>,
}

impl RecordServerHandle {
    /// Gracefully stops the server and waits for the task to finish.
    ///
    /// # Errors
    ///
    /// Returns a harness error when the task exits unsuccessfully.
    pub(crate) async fn shutdown(mut self) -> HarnessResult<()> {
        if let Some(sender) = self.shutdown.take()
            && sender.send(()).is_err()
        {}

        self.task.await.map_err(|error| HarnessError::Io {
            source: std::io::Error::other(format!("server task join failed: {error}")),
        })?
    }
}

/// Binds and starts the record-mode server.
///
/// # Errors
///
/// Returns a harness error when the listener or runtime state cannot be
/// created.
pub(crate) async fn start_record_server(
    cfg: &HarnessConfig,
    cassette_path: &camino::Utf8Path,
) -> HarnessResult<(SocketAddr, RecordServerHandle)> {
    let listener = TcpListener::bind(cfg.listen.as_socket_addr())
        .await
        .map_err(HarnessError::from)?;
    let bound_addr = listener.local_addr().map_err(HarnessError::from)?;
    let cassette_store = FilesystemCassetteStore::open_or_create_for_record(cassette_path)?;
    let state = RecordAppState::from_config(cfg, cassette_store)?;
    let router = Router::new()
        .route(
            CHAT_COMPLETIONS_PATH,
            axum::routing::post(record_chat_completions_handler),
        )
        .with_state(state);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let task = spawn_server_task(listener, router, shutdown_rx);

    Ok((
        bound_addr,
        RecordServerHandle {
            shutdown: Some(shutdown_tx),
            task,
        },
    ))
}

fn spawn_server_task(
    listener: TcpListener,
    router: Router,
    shutdown_rx: oneshot::Receiver<()>,
) -> JoinHandle<HarnessResult<()>> {
    tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async { if shutdown_rx.await.is_err() {} })
            .await
            .map_err(HarnessError::from)
    })
}
