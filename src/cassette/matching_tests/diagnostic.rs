//! Diagnostic content tests.

use rstest::rstest;
use serde_json::json;

use super::fixtures::sample_cassette;
use crate::cassette::{Cassette, MatchOutcome, MismatchDiagnostic, ReplayMatchEngine};
use crate::config::MatchMode;

#[track_caller]
fn unwrap_mismatch(outcome: MatchOutcome<'_>) -> MismatchDiagnostic {
    match outcome {
        MatchOutcome::Mismatch(d) => d,
        MatchOutcome::Matched(_) => panic!("Expected mismatch outcome"),
    }
}

#[rstest]
fn sequential_mismatch_diagnostic_contains_interaction_id(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict)
        .expect("fixture cassette should have valid stable hashes");

    let canonical_wrong = json!({"method": "GET"});
    let outcome = engine.next_match("wrong", &canonical_wrong);
    let diagnostic = unwrap_mismatch(outcome);

    assert_eq!(diagnostic.interaction_id, 0);
    assert_eq!(diagnostic.expected_hash, "hash_a");
    assert_eq!(diagnostic.observed_hash, "wrong");
    assert!(!diagnostic.diff_summary.is_empty());
}

#[rstest]
fn sequential_mismatch_diagnostic_contains_both_hashes(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict)
        .expect("fixture cassette should have valid stable hashes");

    let canonical_wrong = json!({"method": "GET"});
    let outcome = engine.next_match("observed_hash_123", &canonical_wrong);
    let diagnostic = unwrap_mismatch(outcome);

    assert_eq!(diagnostic.interaction_id, 0);
    assert_eq!(diagnostic.expected_hash, "hash_a");
    assert_eq!(diagnostic.observed_hash, "observed_hash_123");
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
    let diagnostic = unwrap_mismatch(outcome);

    assert_eq!(diagnostic.interaction_id, 0);
    assert_eq!(diagnostic.expected_hash, "hash_a");
    assert_eq!(diagnostic.observed_hash, "wrong_hash");
    assert!(!diagnostic.diff_summary.is_empty());
    assert!(diagnostic.diff_summary.contains("method"));
    assert!(diagnostic.diff_summary.contains("POST"));
    assert!(diagnostic.diff_summary.contains("GET"));
}
