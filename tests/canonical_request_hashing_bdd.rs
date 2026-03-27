//! BDD scenarios for canonical request generation and stable hashing.
#![expect(
    clippy::expect_used,
    reason = "BDD step functions use expect for step precondition enforcement"
)]

use rstest::fixture;
use rstest_bdd::Slot;
use rstest_bdd_macros::{ScenarioState, given, scenario, then, when};
use serde_json::json;

use spycatcher_harness::cassette::{IgnorePathConfig, RecordedRequest, canonicalize, stable_hash};

#[derive(Default, ScenarioState)]
struct CanonicalHashWorld {
    left_request: Slot<RecordedRequest>,
    right_request: Slot<RecordedRequest>,
    ignore_config: Slot<IgnorePathConfig>,
    left_hash: Slot<String>,
    right_hash: Slot<String>,
}

#[fixture]
fn canonical_hash_world() -> CanonicalHashWorld {
    CanonicalHashWorld::default()
}

#[given("two equivalent recorded requests with different query ordering")]
fn two_equivalent_recorded_requests_with_different_query_ordering(
    canonical_hash_world: &CanonicalHashWorld,
) {
    canonical_hash_world.left_request.set(request(
        "post",
        "b=2&a=1",
        json!({"model": "gpt-test", "stream": false}),
    ));
    canonical_hash_world.right_request.set(request(
        "POST",
        "a=1&b=2",
        json!({"stream": false, "model": "gpt-test"}),
    ));
}

#[given("two materially different recorded requests")]
fn two_materially_different_recorded_requests(canonical_hash_world: &CanonicalHashWorld) {
    canonical_hash_world.left_request.set(request(
        "POST",
        "a=1&b=2",
        json!({"model": "gpt-test", "stream": false}),
    ));
    canonical_hash_world.right_request.set(request(
        "POST",
        "a=1&b=2",
        json!({"model": "different-model", "stream": false}),
    ));
}

#[given("two requests that differ only in metadata run ids")]
fn two_requests_that_differ_only_in_metadata_run_ids(canonical_hash_world: &CanonicalHashWorld) {
    canonical_hash_world.left_request.set(request(
        "POST",
        "a=1&b=2",
        json!({"metadata": {"run_id": "left"}, "model": "gpt-test", "stream": false}),
    ));
    canonical_hash_world.right_request.set(request(
        "POST",
        "b=2&a=1",
        json!({"stream": false, "metadata": {"run_id": "right"}, "model": "gpt-test"}),
    ));
}

#[given("ignore paths configured as {ignore_path}")]
fn ignore_paths_configured_as(canonical_hash_world: &CanonicalHashWorld, ignore_path: String) {
    let trimmed = ignore_path.trim_matches('"').to_owned();
    canonical_hash_world.ignore_config.set(IgnorePathConfig {
        ignored_body_paths: vec![trimmed],
    });
}

#[when("both requests are canonicalized")]
fn both_requests_are_canonicalized(canonical_hash_world: &CanonicalHashWorld) {
    let left_request = canonical_hash_world
        .left_request
        .take()
        .expect("left request must be configured");
    let right_request = canonical_hash_world
        .right_request
        .take()
        .expect("right request must be configured");
    let ignore_config = canonical_hash_world
        .ignore_config
        .take()
        .unwrap_or_default();

    let left_hash = stable_hash(&canonicalize(&left_request, &ignore_config));
    let right_hash = stable_hash(&canonicalize(&right_request, &ignore_config));

    canonical_hash_world.left_hash.set(left_hash);
    canonical_hash_world.right_hash.set(right_hash);
}

#[then("both stable hashes are identical")]
fn both_stable_hashes_are_identical(canonical_hash_world: &CanonicalHashWorld) {
    let left_hash = canonical_hash_world
        .left_hash
        .with_ref(Clone::clone)
        .expect("left hash must be set");
    let right_hash = canonical_hash_world
        .right_hash
        .with_ref(Clone::clone)
        .expect("right hash must be set");

    assert_eq!(left_hash, right_hash);
}

#[then("the stable hashes differ")]
fn the_stable_hashes_differ(canonical_hash_world: &CanonicalHashWorld) {
    let left_hash = canonical_hash_world
        .left_hash
        .with_ref(Clone::clone)
        .expect("left hash must be set");
    let right_hash = canonical_hash_world
        .right_hash
        .with_ref(Clone::clone)
        .expect("right hash must be set");

    assert_ne!(left_hash, right_hash);
}

#[scenario(
    path = "tests/features/canonical_request_hashing.feature",
    name = "Equivalent requests produce identical hashes"
)]
fn equivalent_requests_produce_identical_hashes(canonical_hash_world: CanonicalHashWorld) {
    let _ = canonical_hash_world;
}

#[scenario(
    path = "tests/features/canonical_request_hashing.feature",
    name = "Materially different requests produce different hashes"
)]
fn materially_different_requests_produce_different_hashes(
    canonical_hash_world: CanonicalHashWorld,
) {
    let _ = canonical_hash_world;
}

#[scenario(
    path = "tests/features/canonical_request_hashing.feature",
    name = "Ignore paths remove metadata drift from hashing"
)]
fn ignore_paths_remove_metadata_drift_from_hashing(canonical_hash_world: CanonicalHashWorld) {
    let _ = canonical_hash_world;
}

fn request(method: &str, query: &str, parsed_json: serde_json::Value) -> RecordedRequest {
    RecordedRequest {
        method: method.to_owned(),
        path: "/v1/chat/completions".to_owned(),
        query: query.to_owned(),
        headers: Vec::new(),
        body: serde_json::to_vec(&parsed_json).expect("request JSON should serialize"),
        parsed_json: Some(parsed_json),
        canonical_request: None,
        stable_hash: None,
    }
}
