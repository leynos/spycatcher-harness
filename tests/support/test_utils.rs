//! Shared helpers for integration tests.

/// Builds a single-threaded Tokio runtime for synchronous BDD step functions.
///
/// # Panics
///
/// Panics if Tokio cannot construct the runtime for the current process.
#[must_use]
pub(crate) fn build_runtime() -> tokio::runtime::Runtime {
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => panic!("failed to build tokio runtime: {error}"),
    }
}
