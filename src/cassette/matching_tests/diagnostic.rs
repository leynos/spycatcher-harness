//! Diagnostic content tests.

use rstest::rstest;
use serde_json::json;

use super::fixtures::sample_cassette;
use crate::cassette::{Cassette, MatchOutcome, ReplayMatchEngine};
use crate::config::MatchMode;

#[rstest]
fn sequential_mismatch_diagnostic_contains_interaction_id(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::SequentialStrict);

    let canonical_wrong = json!({"method": "GET"});
    let outcome = engine.next_match("wrong", &canonical_wrong, &sample_cassette);

    if let MatchOutcome::Mismatch(diagnostic) = outcome {
        assert_eq!(diagnostic.interaction_id, 0);
    } else {
        panic!("Expected mismatch outcome");
    }
}

#[rstest]
fn sequential_mismatch_diagnostic_contains_both_hashes(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::SequentialStrict);

    let canonical_wrong = json!({"method": "GET"});
    let outcome = engine.next_match("observed_hash_123", &canonical_wrong, &sample_cassette);

    if let MatchOutcome::Mismatch(diagnostic) = outcome {
        assert_eq!(diagnostic.expected_hash, "hash_a");
        assert_eq!(diagnostic.observed_hash, "observed_hash_123");
    } else {
        panic!("Expected mismatch outcome");
    }
}

#[rstest]
fn sequential_mismatch_diagnostic_diff_mentions_changed_field(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::SequentialStrict);

    // Expected: {"method": "POST", "path": "/v1/chat/completions"}
    // Observed: {"method": "GET", "path": "/v1/chat/completions"}
    let canonical_wrong = json!({"method": "GET", "path": "/v1/chat/completions"});
    let outcome = engine.next_match("wrong_hash", &canonical_wrong, &sample_cassette);

    if let MatchOutcome::Mismatch(diagnostic) = outcome {
        assert!(diagnostic.diff_summary.contains("method"));
        assert!(diagnostic.diff_summary.contains("POST"));
        assert!(diagnostic.diff_summary.contains("GET"));
    } else {
        panic!("Expected mismatch outcome");
    }
}
