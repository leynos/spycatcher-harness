//! Server startup and shutdown lifecycle for harness HTTP modes.
//!
//! This keeps listener binding and graceful shutdown wiring out of the crate
//! root while returning a small handle that `RunningHarness` can own.

use std::net::SocketAddr;

use axum::Router;
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

use crate::{
    HarnessError,
    HarnessResult,
    cassette::filesystem::FilesystemCassetteStore,
    config::HarnessConfig,
    protocol::CHAT_COMPLETIONS_PATH,
    server::{
        record::RecordAppState,
        record_handler::record_chat_completions_handler,
        replay::ReplayAppState,
        replay_handler::replay_chat_completions_handler,
    },
};

/// Runtime handle for a bound harness HTTP server.
#[derive(Debug)]
pub(crate) struct ServerHandle {
    /// Sender used to request graceful shutdown of the serving task.
    shutdown: Option<oneshot::Sender<()>>,
    /// Task that serves requests and reports listener or serving failures.
    task: JoinHandle<HarnessResult<()>>,
}

impl ServerHandle {
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
) -> HarnessResult<(SocketAddr, ServerHandle)> {
    let cassette_store = FilesystemCassetteStore::open_or_create_for_record(cassette_path)?;
    let state = RecordAppState::from_config(cfg, cassette_store)?;
    let router = Router::new()
        .route(
            CHAT_COMPLETIONS_PATH,
            axum::routing::post(record_chat_completions_handler),
        )
        .with_state(state);
    start_server_with_router(cfg, router).await
}

/// Binds and starts the replay-mode server.
///
/// # Errors
///
/// Returns a harness error when the listener or runtime state cannot be
/// created.
pub(crate) async fn start_replay_server(
    cfg: &HarnessConfig,
    cassette_path: &camino::Utf8Path,
) -> HarnessResult<(SocketAddr, ServerHandle)> {
    let cassette_store = FilesystemCassetteStore::open_for_replay(cassette_path)?;
    let state = ReplayAppState::from_config(cfg, &cassette_store)?;
    let router = Router::new()
        .route(
            CHAT_COMPLETIONS_PATH,
            axum::routing::post(replay_chat_completions_handler),
        )
        .with_state(state);
    start_server_with_router(cfg, router).await
}

/// Binds the configured address and starts serving the supplied router.
/// For example, the returned `SocketAddr` identifies the bound listener endpoint;
/// retain the `ServerHandle` and call `shutdown().await` to stop serving it. The
/// spawned task owns the listener; the handle controls shutdown and task completion.
///
/// # Errors
///
/// Returns a harness error if binding or local-address lookup fails.
async fn start_server_with_router(
    cfg: &HarnessConfig,
    router: Router,
) -> HarnessResult<(SocketAddr, ServerHandle)> {
    let (listener, bound_addr) = bind_listener(cfg).await?;
    Ok((bound_addr, start_server(listener, router)))
}

/// Binds the configured socket address and returns the listener's local address.
/// For example, configuring port `0` returns the port assigned by the operating system.
///
/// # Errors
///
/// Returns a harness error if binding or local-address lookup fails.
async fn bind_listener(cfg: &HarnessConfig) -> HarnessResult<(TcpListener, SocketAddr)> {
    let listener = TcpListener::bind(cfg.listen.as_socket_addr())
        .await
        .map_err(HarnessError::from)?;
    let bound_addr = listener.local_addr().map_err(HarnessError::from)?;
    Ok((listener, bound_addr))
}

/// Starts serving a router on an already-bound listener.
/// For example, retain the returned `ServerHandle` and call `shutdown().await` when finished.
fn start_server(listener: TcpListener, router: Router) -> ServerHandle {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let task = spawn_server_task(listener, router, shutdown_rx);

    ServerHandle {
        shutdown: Some(shutdown_tx),
        task,
    }
}

/// Spawns the serving task with a graceful-shutdown receiver.
/// For example, dropping its sender closes the receiver and lets serving finish.
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
