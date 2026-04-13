//! Diagnostic content tests.

use rstest::rstest;
use serde_json::json;

use super::fixtures::{assert_mismatch_diagnostic, sample_cassette};
use crate::cassette::{Cassette, ReplayMatchEngine};
use crate::config::MatchMode;

#[rstest]
#[case("wrong", json!({"method": "GET"}), &[])]
#[case("observed_hash_123", json!({"method": "GET"}), &[])]
#[case("wrong_hash", json!({"method": "GET", "path": "/v1/chat/completions"}), &["method", "POST", "GET"])]
fn sequential_mismatch_diagnostic_structure(
    sample_cassette: Cassette,
    #[case] observed_hash: &str,
    #[case] canonical_wrong: serde_json::Value,
    #[case] expected_tokens: &[&str],
) {
    let mut engine = ReplayMatchEngine::new(sample_cassette, MatchMode::SequentialStrict)
        .expect("fixture cassette should have valid stable hashes");

    let outcome = engine.next_match(observed_hash, &canonical_wrong);
    let diagnostic = assert_mismatch_diagnostic(outcome, 0, "hash_a", observed_hash);
    assert!(!diagnostic.diff_summary.is_empty());

    for token in expected_tokens {
        assert!(diagnostic.diff_summary.contains(token),
            "diff summary should contain '{token}', got: {}", diagnostic.diff_summary);
    }
}
