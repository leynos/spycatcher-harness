//! Unit tests for deterministic request canonicalization and hashing.

use rstest::{fixture, rstest};
use serde_json::json;

use super::canonicalize;
use super::stable_hash;
use super::{IgnorePathConfig, RecordedRequest};

#[fixture]
fn request_with_json_body() -> RecordedRequest {
    RecordedRequest {
        method: "post".to_owned(),
        path: "/v1/chat/completions".to_owned(),
        query: "z=last&a=2&a=1".to_owned(),
        headers: Vec::new(),
        body: br#"{"metadata":{"run_id":"abc"},"model":"gpt-test","stream":false}"#.to_vec(),
        parsed_json: Some(json!({
            "metadata": {"run_id": "abc"},
            "model": "gpt-test",
            "stream": false
        })),
        canonical_request: None,
        stable_hash: None,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "test helper intentionally collapses repeated RecordedRequest boilerplate"
)]
fn make_request(
    method: &str,
    path: &str,
    query: &str,
    body: Vec<u8>,
    parsed_json: Option<serde_json::Value>,
) -> RecordedRequest {
    RecordedRequest {
        method: method.to_owned(),
        path: path.to_owned(),
        query: query.to_owned(),
        headers: Vec::new(),
        body,
        parsed_json,
        canonical_request: None,
        stable_hash: None,
    }
}

#[rstest]
fn canonicalize_sorts_query_pairs_and_json_keys(request_with_json_body: RecordedRequest) {
    let canonical = canonicalize(&request_with_json_body, &IgnorePathConfig::default());

    assert_eq!(canonical.method, "POST");
    assert_eq!(canonical.path, "/v1/chat/completions");
    assert_eq!(canonical.canonical_query, "a=1&a=2&z=last");
    assert_eq!(
        canonical.canonical_body,
        Some(json!({
            "metadata": {"run_id": "abc"},
            "model": "gpt-test",
            "stream": false
        }))
    );
}

#[rstest]
fn canonicalize_removes_ignored_json_pointer_paths(request_with_json_body: RecordedRequest) {
    let canonical = canonicalize(
        &request_with_json_body,
        &IgnorePathConfig {
            ignored_body_paths: vec!["/metadata/run_id".to_owned()],
        },
    );

    assert_eq!(
        canonical.canonical_body,
        Some(json!({
            "metadata": {},
            "model": "gpt-test",
            "stream": false
        }))
    );
}

#[rstest]
#[case(
    vec![String::new()],
    "empty ignore paths should be ignored rather than dropping the body"
)]
#[case(
    vec!["/metadata/~2bad".to_owned()],
    "invalid ignore paths should not mutate the body"
)]
fn canonicalize_ignores_misconfigured_json_pointer_paths(
    request_with_json_body: RecordedRequest,
    #[case] ignored_body_paths: Vec<String>,
    #[case] message: &str,
) {
    let canonical = canonicalize(
        &request_with_json_body,
        &IgnorePathConfig { ignored_body_paths },
    );
    assert_eq!(
        canonical.canonical_body, request_with_json_body.parsed_json,
        "{message}"
    );
}

#[rstest]
fn canonicalize_removes_multiple_array_entries_without_index_shift() {
    let request = make_request(
        "POST",
        "/v1/chat/completions",
        "",
        br#"{"items":[{"id":"zero"},{"id":"one"},{"id":"two"}],"model":"gpt-test"}"#.to_vec(),
        Some(json!({
            "items": [{"id": "zero"}, {"id": "one"}, {"id": "two"}],
            "model": "gpt-test"
        })),
    );

    let canonical = canonicalize(
        &request,
        &IgnorePathConfig {
            ignored_body_paths: vec!["/items/0".to_owned(), "/items/1".to_owned()],
        },
    );

    assert_eq!(
        canonical.canonical_body,
        Some(json!({
            "items": [{"id": "two"}],
            "model": "gpt-test"
        }))
    );
}

#[rstest]
fn canonicalize_preserves_literal_plus_signs_in_query_parameters() {
    let request = make_request("GET", "/v1/search", "q=C++&lang=en+GB", Vec::new(), None);
    let canonical = canonicalize(&request, &IgnorePathConfig::default());

    assert_eq!(canonical.canonical_query, "lang=en%2BGB&q=C%2B%2B");
}

#[rstest]
fn stable_hash_ignores_json_key_order_query_order_and_ignored_fields() {
    let left = RecordedRequest {
        method: "post".to_owned(),
        path: "/v1/chat/completions".to_owned(),
        query: "b=2&a=1".to_owned(),
        headers: Vec::new(),
        body: br#"{"metadata":{"run_id":"left"},"model":"gpt-test","stream":false}"#.to_vec(),
        parsed_json: Some(json!({
            "metadata": {"run_id": "left"},
            "model": "gpt-test",
            "stream": false
        })),
        canonical_request: None,
        stable_hash: None,
    };
    let right = RecordedRequest {
        method: "POST".to_owned(),
        path: "/v1/chat/completions".to_owned(),
        query: "a=1&b=2".to_owned(),
        headers: Vec::new(),
        body: br#"{"stream":false,"model":"gpt-test","metadata":{"run_id":"right"}}"#.to_vec(),
        parsed_json: Some(json!({
            "stream": false,
            "model": "gpt-test",
            "metadata": {"run_id": "right"}
        })),
        canonical_request: None,
        stable_hash: None,
    };
    let ignore_config = IgnorePathConfig {
        ignored_body_paths: vec!["/metadata/run_id".to_owned()],
    };

    let left_hash = stable_hash(&canonicalize(&left, &ignore_config));
    let right_hash = stable_hash(&canonicalize(&right, &ignore_config));

    assert_eq!(left_hash, right_hash);
}

#[rstest]
fn stable_hash_changes_when_non_ignored_request_content_changes(
    request_with_json_body: RecordedRequest,
) {
    let mut changed = request_with_json_body.clone();
    changed.parsed_json = Some(json!({
        "metadata": {"run_id": "abc"},
        "model": "different-model",
        "stream": false
    }));

    let original_hash = stable_hash(&canonicalize(
        &request_with_json_body,
        &IgnorePathConfig::default(),
    ));
    let changed_hash = stable_hash(&canonicalize(&changed, &IgnorePathConfig::default()));

    assert_ne!(original_hash, changed_hash);
}

#[rstest]
fn populate_canonical_fields_sets_reserved_request_fields(
    mut request_with_json_body: RecordedRequest,
) {
    request_with_json_body.populate_canonical_fields(&IgnorePathConfig {
        ignored_body_paths: vec!["/metadata/run_id".to_owned()],
    });

    assert_eq!(
        request_with_json_body.stable_hash.as_deref(),
        Some("ecf8cab2752928a41978e7dbcb5cda883e87ae69829d290226f80f93c0e64be8")
    );
    assert_eq!(
        request_with_json_body.canonical_request,
        Some(json!({
            "method": "POST",
            "path": "/v1/chat/completions",
            "canonical_query": "a=1&a=2&z=last",
            "canonical_body": {
                "metadata": {},
                "model": "gpt-test",
                "stream": false
            }
        }))
    );
}

#[rstest]
fn canonicalize_non_json_body_leaves_body_absent() {
    let request = make_request("POST", "/v1/embeddings", "", b"plain text".to_vec(), None);
    let canonical = canonicalize(&request, &IgnorePathConfig::default());

    assert_eq!(canonical.canonical_body, None);
}
