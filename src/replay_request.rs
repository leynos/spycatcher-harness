//! Canonicalize observed replay requests and read their matching fields.

use tracing::error;

use super::ReplayError;
use crate::cassette::{IgnorePathConfig, RecordedRequest};
use crate::http_exchange::ObservedRequest;

/// Builds the recorded request and populates its canonical matching fields.
/// For example, an observed `get /health?b=2&a=1` with no parsed JSON produces
/// a recorded request whose canonical value uses method `GET`, query `a=1&b=2`
/// and a null body, with a populated stable hash. The observed method and query
/// remain unchanged in the recorded fields.
///
/// # Errors
///
/// Returns `ReplayError::Internal` if canonicalization fails.
pub(super) fn canonicalize_observed_request(
    request: ObservedRequest,
) -> Result<RecordedRequest, ReplayError> {
    let mut recorded_request = observed_to_recorded_request(request);
    recorded_request
        .populate_canonical_fields(&IgnorePathConfig::default())
        .map_err(|error| {
            error!(
                target: "spycatcher.harness.replay",
                "failed to canonicalize replay request: {error}"
            );
            ReplayError::Internal
        })?;
    Ok(recorded_request)
}

/// Borrows the stable hash already populated on a recorded request.
/// For example, a request with `stable_hash: Some("abc".to_owned())` yields
/// `Ok("abc")` without copying the hash.
///
/// # Errors
///
/// Returns `ReplayError::Internal` when the stored hash is absent.
pub(super) fn recorded_hash(recorded_request: &RecordedRequest) -> Result<&str, ReplayError> {
    recorded_request
        .stable_hash
        .as_deref()
        .ok_or(ReplayError::Internal)
}

/// Borrows the canonical JSON already populated on a recorded request.
/// For example, storing `Some(json!({"method": "GET"}))` yields `Ok(&value)`
/// referring to that stored JSON, rather than rebuilding or copying it.
///
/// # Errors
///
/// Returns `ReplayError::Internal` when the stored canonical value is absent.
pub(super) fn recorded_canonical_request(
    recorded_request: &RecordedRequest,
) -> Result<&serde_json::Value, ReplayError> {
    recorded_request
        .canonical_request
        .as_ref()
        .ok_or(ReplayError::Internal)
}

/// Moves captured request fields into the cassette persistence shape.
/// For example, an observed `GET /health?b=2&a=1` keeps the method, path and
/// query strings; `canonical_request` and `stable_hash` start as `None` for
/// later population. Raw forwarding headers are not persisted here.
fn observed_to_recorded_request(request: ObservedRequest) -> RecordedRequest {
    RecordedRequest {
        method: request.method,
        path: request.path,
        query: request.query,
        headers: request.headers,
        body: request.body,
        parsed_json: request.parsed_json,
        canonical_request: None,
        stable_hash: None,
    }
}
