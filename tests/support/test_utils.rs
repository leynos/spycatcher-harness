//! Shared helpers for integration tests.

/// Builds a single-threaded Tokio runtime for synchronous BDD step functions.
///
/// # Errors
///
/// Returns an error if Tokio cannot construct the runtime for the current
/// process.
pub(crate) fn build_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}
