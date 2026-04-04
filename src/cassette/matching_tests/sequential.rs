//! Sequential strict mode tests.

use rstest::rstest;
use serde_json::json;

use super::fixtures::sample_cassette;
use crate::cassette::{Cassette, MatchOutcome, ReplayMatchEngine};
use crate::config::MatchMode;

#[rstest]
fn sequential_strict_three_correct_requests_match_in_order(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::SequentialStrict);

    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let outcome_a = engine.next_match("hash_a", &canonical_a, &sample_cassette);
    assert!(matches!(outcome_a, MatchOutcome::Matched(_)));
    if let MatchOutcome::Matched(interaction) = outcome_a {
        assert_eq!(
            interaction.response,
            sample_cassette
                .interactions
                .first()
                .expect("Interaction 0 should exist")
                .response
        );
    }

    let canonical_b = json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}});
    let outcome_b = engine.next_match("hash_b", &canonical_b, &sample_cassette);
    assert!(matches!(outcome_b, MatchOutcome::Matched(_)));
    if let MatchOutcome::Matched(interaction) = outcome_b {
        assert_eq!(
            interaction.response,
            sample_cassette
                .interactions
                .get(1)
                .expect("Interaction 1 should exist")
                .response
        );
    }

    let canonical_c = json!({"method": "GET", "path": "/v1/models"});
    let outcome_c = engine.next_match("hash_c", &canonical_c, &sample_cassette);
    assert!(matches!(outcome_c, MatchOutcome::Matched(_)));
    if let MatchOutcome::Matched(interaction) = outcome_c {
        assert_eq!(
            interaction.response,
            sample_cassette
                .interactions
                .get(2)
                .expect("Interaction 2 should exist")
                .response
        );
    }
}

#[rstest]
fn sequential_strict_first_request_wrong_hash_returns_mismatch(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::SequentialStrict);

    let canonical_wrong = json!({"method": "GET", "path": "/wrong"});
    let outcome = engine.next_match("wrong_hash", &canonical_wrong, &sample_cassette);

    assert!(matches!(outcome, MatchOutcome::Mismatch(_)));
    if let MatchOutcome::Mismatch(diagnostic) = outcome {
        assert_eq!(diagnostic.interaction_id, 0);
        assert_eq!(diagnostic.expected_hash, "hash_a");
        assert_eq!(diagnostic.observed_hash, "wrong_hash");
        assert!(!diagnostic.diff_summary.is_empty());
    }
}

#[rstest]
fn sequential_strict_second_request_wrong_hash_returns_mismatch(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::SequentialStrict);

    // First request matches.
    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let outcome_first = engine.next_match("hash_a", &canonical_a, &sample_cassette);
    assert!(matches!(outcome_first, MatchOutcome::Matched(_)));

    // Second request has wrong hash.
    let canonical_wrong = json!({"method": "GET", "path": "/wrong"});
    let outcome_second = engine.next_match("wrong_hash", &canonical_wrong, &sample_cassette);

    assert!(matches!(outcome_second, MatchOutcome::Mismatch(_)));
    if let MatchOutcome::Mismatch(diagnostic) = outcome_second {
        assert_eq!(diagnostic.interaction_id, 1);
        assert_eq!(diagnostic.expected_hash, "hash_b");
        assert_eq!(diagnostic.observed_hash, "wrong_hash");
    }
}

#[rstest]
fn sequential_strict_cassette_exhausted_returns_mismatch(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::SequentialStrict);

    // Consume all three interactions.
    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let _ = engine.next_match("hash_a", &canonical_a, &sample_cassette);

    let canonical_b = json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}});
    let _ = engine.next_match("hash_b", &canonical_b, &sample_cassette);

    let canonical_c = json!({"method": "GET", "path": "/v1/models"});
    let _ = engine.next_match("hash_c", &canonical_c, &sample_cassette);

    // Fourth request should fail with exhaustion diagnostic.
    let canonical_extra = json!({"method": "GET", "path": "/extra"});
    let outcome = engine.next_match("hash_extra", &canonical_extra, &sample_cassette);

    assert!(matches!(outcome, MatchOutcome::Mismatch(_)));
    if let MatchOutcome::Mismatch(diagnostic) = outcome {
        assert_eq!(diagnostic.interaction_id, 3); // Beyond the last index.
        assert_eq!(diagnostic.expected_hash, "");
        assert_eq!(diagnostic.observed_hash, "hash_extra");
        assert!(diagnostic.diff_summary.contains("exhausted"));
    }
}
