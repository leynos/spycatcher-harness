//! Test fixture types and constructors.

use serde_json::{Value, json};
use spycatcher_harness::cassette::{
    Interaction, InteractionMetadata, RecordedRequest, RecordedResponse,
};

pub(super) struct InteractionSpec<'a> {
    pub(super) method: &'a str,
    pub(super) path: &'a str,
    pub(super) canonical: Value,
    pub(super) hash: &'a str,
    pub(super) response_id: &'a str,
}

pub(super) fn create_interaction(spec: InteractionSpec<'_>) -> Interaction {
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
