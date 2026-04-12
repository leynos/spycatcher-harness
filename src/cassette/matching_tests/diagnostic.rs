//! Diagnostic content tests.

use rstest::rstest;
use serde_json::json;

use super::fixtures::{assert_mismatch_diagnostic, sample_cassette};
use crate::cassette::{Cassette, ReplayMatchEngine};
use crate::config::MatchMode;

#[rstest]
fn sequential_mismatch_diagnostic_contains_interaction_id(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict)
        .expect("fixture cassette should have valid stable hashes");

    let canonical_wrong = json!({"method": "GET"});
    let outcome = engine.next_match("wrong", &canonical_wrong);
    let diagnostic = assert_mismatch_diagnostic(outcome, 0, "hash_a", "wrong");
    assert!(!diagnostic.diff_summary.is_empty());
}

#[rstest]
fn sequential_mismatch_diagnostic_contains_both_hashes(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict)
        .expect("fixture cassette should have valid stable hashes");

    let canonical_wrong = json!({"method": "GET"});
    let outcome = engine.next_match("observed_hash_123", &canonical_wrong);
    let diagnostic = assert_mismatch_diagnostic(outcome, 0, "hash_a", "observed_hash_123");
    assert!(!diagnostic.diff_summary.is_empty());
}

#[rstest]
fn sequential_mismatch_diagnostic_diff_mentions_changed_field(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict)
        .expect("fixture cassette should have valid stable hashes");

    // Expected: {"method": "POST", "path": "/v1/chat/completions"}
    // Observed: {"method": "GET", "path": "/v1/chat/completions"}
    let canonical_wrong = json!({"method": "GET", "path": "/v1/chat/completions"});
    let outcome = engine.next_match("wrong_hash", &canonical_wrong);
    let diagnostic = assert_mismatch_diagnostic(outcome, 0, "hash_a", "wrong_hash");
    assert!(!diagnostic.diff_summary.is_empty());
    assert!(diagnostic.diff_summary.contains("method"));
    assert!(diagnostic.diff_summary.contains("POST"));
    assert!(diagnostic.diff_summary.contains("GET"));
}
