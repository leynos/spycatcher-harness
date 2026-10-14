//! Canonicalize observed replay requests and read their matching fields.

use tracing::error;

use super::ReplayError;
use crate::cassette::{IgnorePathConfig, RecordedRequest};
use crate::http_exchange::ObservedRequest;

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

pub(super) fn recorded_hash(recorded_request: &RecordedRequest) -> Result<&str, ReplayError> {
    recorded_request
        .stable_hash
        .as_deref()
        .ok_or(ReplayError::Internal)
}

pub(super) fn recorded_canonical_request(
    recorded_request: &RecordedRequest,
) -> Result<&serde_json::Value, ReplayError> {
    recorded_request
        .canonical_request
        .as_ref()
        .ok_or(ReplayError::Internal)
}

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
