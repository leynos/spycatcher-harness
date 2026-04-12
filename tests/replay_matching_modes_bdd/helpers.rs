//! Helper types and functions for BDD test scenarios.

use serde_json::Value;
use spycatcher_harness::cassette::{
    Interaction, InteractionMetadata, MatchOutcome, RecordedRequest, RecordedResponse,
};

pub(crate) struct InteractionSpec<'a> {
    pub(crate) method: &'a str,
    pub(crate) path: &'a str,
    pub(crate) canonical: Value,
    pub(crate) hash: &'a str,
    pub(crate) response_id: &'a str,
}

/// Extracts response ID from a match outcome if it's a `NonStream` response.
pub(crate) fn extract_response_id(outcome: &MatchOutcome<'_>) -> Option<String> {
    if let MatchOutcome::Matched(interaction) = outcome
        && let RecordedResponse::NonStream { parsed_json, .. } = &interaction.response
    {
        return parsed_json
            .as_ref()
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .map(String::from);
    }
    None
}

pub(crate) fn create_interaction(spec: InteractionSpec<'_>) -> Interaction {
    use serde_json::json;
    Interaction {
        request: RecordedRequest {
            method: spec.method.to_owned(),
            path: spec.path.to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({})),
            canonical_request: Some(spec.canonical),
            stable_hash: Some(spec.hash.to_owned()),
        },
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"id": spec.response_id})),
        },
        metadata: InteractionMetadata {
            protocol_id: "test".to_owned(),
            upstream_id: "test".to_owned(),
            recorded_at: "2025-01-01T00:00:00Z".to_owned(),
            relative_offset_ms: 0,
        },
    }
}
