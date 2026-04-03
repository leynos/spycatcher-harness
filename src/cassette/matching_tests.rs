//! Unit tests for the replay matching engine.

use rstest::{fixture, rstest};
use serde_json::{Value, json};

use super::matching::{MatchOutcome, ReplayMatchEngine};
use crate::cassette::{
    Cassette, Interaction, InteractionMetadata, RecordedRequest, RecordedResponse,
};
use crate::config::MatchMode;

#[fixture]
fn sample_cassette() -> Cassette {
    let mut cassette = Cassette::new();

    // Interaction 0: hash_a
    cassette.append(Interaction {
        request: RecordedRequest {
            method: "POST".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"model": "gpt-4", "messages": []})),
            canonical_request: Some(json!({"method": "POST", "path": "/v1/chat/completions"})),
            stable_hash: Some("hash_a".to_owned()),
        },
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"id": "resp_a"})),
        },
        metadata: InteractionMetadata {
            protocol_id: "openai_chat".to_owned(),
            upstream_id: "openai".to_owned(),
            recorded_at: "2025-01-01T00:00:00Z".to_owned(),
            relative_offset_ms: 0,
        },
    });

    // Interaction 1: hash_b
    cassette.append(Interaction {
        request: RecordedRequest {
            method: "POST".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"model": "gpt-4", "messages": [{"role": "user"}]})),
            canonical_request: Some(json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}})),
            stable_hash: Some("hash_b".to_owned()),
        },
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"id": "resp_b"})),
        },
        metadata: InteractionMetadata {
            protocol_id: "openai_chat".to_owned(),
            upstream_id: "openai".to_owned(),
            recorded_at: "2025-01-01T00:01:00Z".to_owned(),
            relative_offset_ms: 60_000,
        },
    });

    // Interaction 2: hash_c
    cassette.append(Interaction {
        request: RecordedRequest {
            method: "GET".to_owned(),
            path: "/v1/models".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: None,
            canonical_request: Some(json!({"method": "GET", "path": "/v1/models"})),
            stable_hash: Some("hash_c".to_owned()),
        },
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"data": []})),
        },
        metadata: InteractionMetadata {
            protocol_id: "openai_chat".to_owned(),
            upstream_id: "openai".to_owned(),
            recorded_at: "2025-01-01T00:02:00Z".to_owned(),
            relative_offset_ms: 120_000,
        },
    });

    cassette
}

#[fixture]
fn duplicate_hash_cassette() -> Cassette {
    let mut cassette = Cassette::new();

    // Interaction 0: hash_a (first occurrence)
    cassette.append(Interaction {
        request: RecordedRequest {
            method: "POST".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"model": "gpt-4", "messages": [{"content": "first"}]})),
            canonical_request: Some(json!({"method": "POST", "messages": [{"content": "first"}]})),
            stable_hash: Some("hash_a".to_owned()),
        },
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"id": "first_response"})),
        },
        metadata: InteractionMetadata {
            protocol_id: "openai_chat".to_owned(),
            upstream_id: "openai".to_owned(),
            recorded_at: "2025-01-01T00:00:00Z".to_owned(),
            relative_offset_ms: 0,
        },
    });

    // Interaction 1: hash_a (second occurrence)
    cassette.append(Interaction {
        request: RecordedRequest {
            method: "POST".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"model": "gpt-4", "messages": [{"content": "second"}]})),
            canonical_request: Some(json!({"method": "POST", "messages": [{"content": "second"}]})),
            stable_hash: Some("hash_a".to_owned()),
        },
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"id": "second_response"})),
        },
        metadata: InteractionMetadata {
            protocol_id: "openai_chat".to_owned(),
            upstream_id: "openai".to_owned(),
            recorded_at: "2025-01-01T00:01:00Z".to_owned(),
            relative_offset_ms: 60_000,
        },
    });

    // Interaction 2: hash_b
    cassette.append(Interaction {
        request: RecordedRequest {
            method: "GET".to_owned(),
            path: "/v1/models".to_owned(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: None,
            canonical_request: Some(json!({"method": "GET"})),
            stable_hash: Some("hash_b".to_owned()),
        },
        response: RecordedResponse::NonStream {
            status: 200,
            headers: Vec::new(),
            body: Vec::new(),
            parsed_json: Some(json!({"data": []})),
        },
        metadata: InteractionMetadata {
            protocol_id: "openai_chat".to_owned(),
            upstream_id: "openai".to_owned(),
            recorded_at: "2025-01-01T00:02:00Z".to_owned(),
            relative_offset_ms: 120_000,
        },
    });

    cassette
}

// Sequential strict mode tests.

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

// Keyed mode tests.

#[rstest]
fn keyed_mode_three_correct_requests_in_order_match(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::Keyed);

    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let outcome_a = engine.next_match("hash_a", &canonical_a, &sample_cassette);
    assert!(matches!(outcome_a, MatchOutcome::Matched(_)));

    let canonical_b = json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}});
    let outcome_b = engine.next_match("hash_b", &canonical_b, &sample_cassette);
    assert!(matches!(outcome_b, MatchOutcome::Matched(_)));

    let canonical_c = json!({"method": "GET", "path": "/v1/models"});
    let outcome_c = engine.next_match("hash_c", &canonical_c, &sample_cassette);
    assert!(matches!(outcome_c, MatchOutcome::Matched(_)));
}

#[rstest]
fn keyed_mode_three_correct_requests_in_reverse_order_match(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::Keyed);

    // Request in reverse order: hash_c, hash_b, hash_a.
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
}

#[rstest]
fn keyed_mode_duplicate_hashes_consumed_in_order(duplicate_hash_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&duplicate_hash_cassette, MatchMode::Keyed);

    // First request with hash_a should match the first interaction.
    let canonical_a = json!({"method": "POST", "messages": [{"content": "first"}]});
    let outcome = engine.next_match("hash_a", &canonical_a, &duplicate_hash_cassette);
    assert!(matches!(outcome, MatchOutcome::Matched(_)));
    if let MatchOutcome::Matched(interaction) = outcome {
        let resp_id = interaction
            .response
            .clone()
            .into_non_stream()
            .expect("Expected NonStream response")
            .parsed_json
            .and_then(|v| v.get("id").cloned())
            .and_then(|v| v.as_str().map(String::from))
            .expect("Expected response id");
        assert_eq!(resp_id, "first_response");
    }

    // Second request with hash_a should match the second interaction.
    let canonical_a2 = json!({"method": "POST", "messages": [{"content": "second"}]});
    let outcome_2 = engine.next_match("hash_a", &canonical_a2, &duplicate_hash_cassette);
    assert!(matches!(outcome_2, MatchOutcome::Matched(_)));
    if let MatchOutcome::Matched(interaction) = outcome_2 {
        let resp_id = interaction
            .response
            .clone()
            .into_non_stream()
            .expect("Expected NonStream response")
            .parsed_json
            .and_then(|v| v.get("id").cloned())
            .and_then(|v| v.as_str().map(String::from))
            .expect("Expected response id");
        assert_eq!(resp_id, "second_response");
    }
}

#[rstest]
fn keyed_mode_request_with_unknown_hash_returns_mismatch(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::Keyed);

    let canonical_unknown = json!({"method": "DELETE", "path": "/unknown"});
    let outcome = engine.next_match("unknown_hash", &canonical_unknown, &sample_cassette);

    assert!(matches!(outcome, MatchOutcome::Mismatch(_)));
    if let MatchOutcome::Mismatch(diagnostic) = outcome {
        assert_eq!(diagnostic.interaction_id, 3); // Total interaction count.
        assert_eq!(diagnostic.expected_hash, "");
        assert_eq!(diagnostic.observed_hash, "unknown_hash");
        assert!(diagnostic.diff_summary.contains("no interaction"));
    }
}

#[rstest]
fn keyed_mode_all_consumed_then_request_returns_mismatch(sample_cassette: Cassette) {
    let mut engine = ReplayMatchEngine::new(&sample_cassette, MatchMode::Keyed);

    // Consume all three interactions.
    let canonical_a = json!({"method": "POST", "path": "/v1/chat/completions"});
    let _ = engine.next_match("hash_a", &canonical_a, &sample_cassette);

    let canonical_b = json!({"method": "POST", "path": "/v1/chat/completions", "body": {"messages": [{"role": "user"}]}});
    let _ = engine.next_match("hash_b", &canonical_b, &sample_cassette);

    let canonical_c = json!({"method": "GET", "path": "/v1/models"});
    let _ = engine.next_match("hash_c", &canonical_c, &sample_cassette);

    // Try to request hash_a again (already consumed).
    let outcome = engine.next_match("hash_a", &canonical_a, &sample_cassette);

    assert!(matches!(outcome, MatchOutcome::Mismatch(_)));
    if let MatchOutcome::Mismatch(diagnostic) = outcome {
        assert_eq!(diagnostic.interaction_id, 3);
        assert_eq!(diagnostic.expected_hash, "");
        assert_eq!(diagnostic.observed_hash, "hash_a");
        assert!(diagnostic.diff_summary.contains("already been consumed"));
    }
}

// Diagnostic content tests.

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

// Helper to convert RecordedResponse to NonStream variant for testing.
impl RecordedResponse {
    fn into_non_stream(self) -> Option<NonStreamResponse> {
        match self {
            Self::NonStream {
                status,
                headers,
                body,
                parsed_json,
            } => Some(NonStreamResponse {
                status,
                headers,
                body,
                parsed_json,
            }),
            Self::Stream { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct NonStreamResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    parsed_json: Option<Value>,
}
