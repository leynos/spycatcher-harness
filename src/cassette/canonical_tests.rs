//! Unit tests for deterministic request canonicalization and hashing.

use rstest::{fixture, rstest};
use serde_json::json;

use super::canonicalize;
use super::stable_hash;
use super::{CanonicalError, CanonicalRequest, IgnorePathConfig, RecordedRequest};

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

struct RequestSpec {
    method: String,
    path: String,
    query: String,
    body: Vec<u8>,
    parsed_json: Option<serde_json::Value>,
}

fn make_request(spec: RequestSpec) -> RecordedRequest {
    RecordedRequest {
        method: spec.method,
        path: spec.path,
        query: spec.query,
        headers: Vec::new(),
        body: spec.body,
        parsed_json: spec.parsed_json,
        canonical_request: None,
        stable_hash: None,
    }
}

#[rstest]
fn canonicalize_sorts_query_pairs_and_json_keys(request_with_json_body: RecordedRequest) {
    let canonical =
        canonicalize(&request_with_json_body, &IgnorePathConfig::default()).expect("valid config");

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
    )
    .expect("valid config");

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
#[case(String::new())]
#[case("metadata/run_id".to_owned())]
#[case("/items/01".to_owned())]
fn canonicalize_rejects_invalid_json_pointer_paths(#[case] ignored_body_path: String) {
    let request = make_request(RequestSpec {
        method: "POST".to_owned(),
        path: "/v1/chat/completions".to_owned(),
        query: String::new(),
        body: br#"{"metadata":{"run_id":"abc"},"model":"gpt-test"}"#.to_vec(),
        parsed_json: Some(json!({
            "metadata": {"run_id": "abc"},
            "model": "gpt-test"
        })),
    });

    let canonical = canonicalize(
        &request,
        &IgnorePathConfig {
            ignored_body_paths: vec![ignored_body_path.clone()],
        },
    );

    assert_eq!(
        canonical,
        Err(CanonicalError::InvalidPointerPath(ignored_body_path))
    );
}

#[rstest]
#[case(
    json!({
        "items": [{"id": "zero"}, {"id": "one"}, {"id": "two"}],
        "model": "gpt-test"
    }),
    vec!["/items/0".to_owned(), "/items/1".to_owned()],
    json!({
        "items": [{"id": "two"}],
        "model": "gpt-test"
    }),
)]
#[case(
    json!({
        "items": [{"id": "zero"}, {"id": "one"}, {"id": "two"}],
        "model": "gpt-test"
    }),
    vec!["/items/1".to_owned(), "/items/1".to_owned()],
    json!({
        "items": [{"id": "zero"}, {"id": "two"}],
        "model": "gpt-test"
    }),
)]
#[case(
    json!({
        "items": [
            {"id": "zero"},
            {"id": "one", "name": "keep"},
            {"id": "two"}
        ],
        "model": "gpt-test"
    }),
    vec!["/items/0".to_owned(), "/items/1/id".to_owned()],
    json!({
        "items": [{"name": "keep"}, {"id": "two"}],
        "model": "gpt-test"
    }),
)]
#[case(
    json!([{"id": "zero"}, {"id": "one"}, {"id": "two"}]),
    vec!["/0".to_owned(), "/1".to_owned()],
    json!([{"id": "two"}]),
)]
fn canonicalize_removes_array_entries_without_index_shift(
    #[case] parsed_json: serde_json::Value,
    #[case] ignored_body_paths: Vec<String>,
    #[case] expected_body: serde_json::Value,
) {
    let request = make_request(RequestSpec {
        method: "POST".to_owned(),
        path: "/v1/chat/completions".to_owned(),
        query: String::new(),
        body: Vec::new(),
        parsed_json: Some(parsed_json),
    });

    let canonical = match canonicalize(&request, &IgnorePathConfig { ignored_body_paths }) {
        Ok(canonical) => canonical,
        Err(error) => panic!("array entry removal should use valid ignored body paths: {error}"),
    };

    assert_eq!(canonical.canonical_body, Some(expected_body));
}

#[rstest]
fn canonicalize_preserves_literal_plus_signs_in_query_parameters() {
    let request = make_request(RequestSpec {
        method: "GET".to_owned(),
        path: "/v1/search".to_owned(),
        query: "q=C++&lang=en+GB".to_owned(),
        body: Vec::new(),
        parsed_json: None,
    });
    let canonical = canonicalize(&request, &IgnorePathConfig::default()).expect("valid config");

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

    let left_hash = stable_hash(&canonicalize(&left, &ignore_config).expect("valid config"));
    let right_hash = stable_hash(&canonicalize(&right, &ignore_config).expect("valid config"));

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

    let original_hash = stable_hash(
        &canonicalize(&request_with_json_body, &IgnorePathConfig::default()).expect("valid config"),
    );
    let changed_hash =
        stable_hash(&canonicalize(&changed, &IgnorePathConfig::default()).expect("valid config"));

    assert_ne!(original_hash, changed_hash);
}

#[rstest]
fn stable_hash_distinguishes_newline_collision_candidates() {
    let left = CanonicalRequest {
        method: "POST\nPATH\n/left".to_owned(),
        path: "body".to_owned(),
        canonical_query: "query".to_owned(),
        canonical_body: Some(json!({"value": "same"})),
    };
    let right = CanonicalRequest {
        method: "POST".to_owned(),
        path: "/left\nPATH\nbody".to_owned(),
        canonical_query: "query".to_owned(),
        canonical_body: Some(json!({"value": "same"})),
    };

    assert_ne!(stable_hash(&left), stable_hash(&right));
}

#[rstest]
fn populate_canonical_fields_sets_reserved_request_fields(
    mut request_with_json_body: RecordedRequest,
) {
    request_with_json_body
        .populate_canonical_fields(&IgnorePathConfig {
            ignored_body_paths: vec!["/metadata/run_id".to_owned()],
        })
        .expect("valid config");

    assert_eq!(
        request_with_json_body.stable_hash.as_deref(),
        Some("f61ef018c056ab58448c0c0152d17225fbc8e9aecc83dc6f4713f9594e86990f")
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
    let request = make_request(RequestSpec {
        method: "POST".to_owned(),
        path: "/v1/embeddings".to_owned(),
        query: String::new(),
        body: b"plain text".to_vec(),
        parsed_json: None,
    });
    let canonical = canonicalize(&request, &IgnorePathConfig::default()).expect("valid config");

    assert_eq!(canonical.canonical_body, None);
}
