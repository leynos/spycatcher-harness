//! Replay matching engine for cassette interactions.
//!
//! This module implements the core replay matching logic that decides which
//! recorded interaction to serve for an incoming request. It supports two
//! matching modes: sequential strict (default) and keyed (hash-based).

use std::collections::HashMap;

use serde_json::Value;

use crate::cassette::{Cassette, Interaction, canonical_diff_summary};
use crate::config::MatchMode;

/// Structured diagnostic for a replay mismatch.
///
/// Carries all information needed by the adapter layer to build an HTTP 409
/// response body without coupling the domain to HTTP types.
#[derive(Debug, Clone, PartialEq)]
pub struct MismatchDiagnostic {
    /// Zero-based index of the expected interaction (sequential mode) or
    /// total interaction count (keyed mode miss).
    pub interaction_id: usize,
    /// Stable hash of the expected canonical request (sequential mode) or
    /// empty string (keyed mode miss).
    pub expected_hash: String,
    /// Stable hash of the observed incoming request.
    pub observed_hash: String,
    /// Field-level diff summary of canonical request JSON.
    pub diff_summary: String,
}

/// Outcome of a replay match attempt.
#[derive(Debug)]
pub enum MatchOutcome<'a> {
    /// The incoming request matched a recorded interaction.
    Matched(&'a Interaction),
    /// No match was found; diagnostics explain why.
    Mismatch(MismatchDiagnostic),
}

/// Replay matching engine that consumes cassette interactions according to
/// the configured match mode.
pub struct ReplayMatchEngine {
    /// Reference to the cassette's interactions.
    interactions: Vec<InteractionData>,
    /// Current matching mode.
    mode: MatchMode,
    /// Sequential mode: cursor tracking the next expected interaction index.
    sequential_cursor: usize,
    /// Keyed mode: hash-to-indices map for efficient lookup.
    hash_to_indices: HashMap<String, Vec<usize>>,
    /// Keyed mode: tracks consumed interactions.
    consumed: Vec<bool>,
}

/// Interaction data extracted from the cassette for matching purposes.
#[derive(Debug, Clone)]
struct InteractionData {
    /// Stable hash of the canonical request for this interaction.
    stable_hash: String,
    /// Canonical request JSON value for diff generation.
    canonical_request: Value,
}

impl ReplayMatchEngine {
    /// Creates a new engine from a loaded cassette and match mode.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use spycatcher_harness::cassette::{Cassette, ReplayMatchEngine};
    /// use spycatcher_harness::config::MatchMode;
    ///
    /// let cassette = Cassette::new();
    /// let engine = ReplayMatchEngine::new(&cassette, MatchMode::SequentialStrict);
    /// ```
    #[must_use]
    pub fn new(cassette: &Cassette, mode: MatchMode) -> Self {
        let interactions: Vec<InteractionData> = cassette
            .interactions
            .iter()
            .map(|interaction| InteractionData {
                stable_hash: interaction.request.stable_hash.clone().unwrap_or_default(),
                canonical_request: interaction
                    .request
                    .canonical_request
                    .clone()
                    .unwrap_or(Value::Null),
            })
            .collect();

        let mut hash_to_indices = HashMap::new();
        if matches!(mode, MatchMode::Keyed) {
            for (idx, data) in interactions.iter().enumerate() {
                hash_to_indices
                    .entry(data.stable_hash.clone())
                    .or_insert_with(Vec::new)
                    .push(idx);
            }
        }

        let consumed = vec![false; interactions.len()];

        Self {
            interactions,
            mode,
            sequential_cursor: 0,
            hash_to_indices,
            consumed,
        }
    }

    /// Attempts to match an incoming request against the cassette.
    ///
    /// In sequential strict mode, the request must match the next recorded
    /// interaction in order. In keyed mode, the request matches the next
    /// unconsumed interaction with the same hash.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let outcome = engine.next_match("abc123", &canonical_request, &cassette);
    /// match outcome {
    ///     MatchOutcome::Matched(interaction) => { /* use interaction */ }
    ///     MatchOutcome::Mismatch(diagnostic) => { /* handle mismatch */ }
    /// }
    /// ```
    pub fn next_match<'a>(
        &mut self,
        observed_hash: &str,
        observed_canonical: &Value,
        cassette: &'a Cassette,
    ) -> MatchOutcome<'a> {
        match self.mode {
            MatchMode::SequentialStrict => {
                self.sequential_match(observed_hash, observed_canonical, cassette)
            }
            MatchMode::Keyed => self.keyed_match(observed_hash, observed_canonical, cassette),
        }
    }

    fn sequential_match<'a>(
        &mut self,
        observed_hash: &str,
        observed_canonical: &Value,
        cassette: &'a Cassette,
    ) -> MatchOutcome<'a> {
        let Some(expected_data) = self.interactions.get(self.sequential_cursor) else {
            return MatchOutcome::Mismatch(MismatchDiagnostic {
                interaction_id: self.sequential_cursor,
                expected_hash: String::new(),
                observed_hash: observed_hash.to_owned(),
                diff_summary: "cassette exhausted: no more interactions available".to_owned(),
            });
        };

        let expected_hash = &expected_data.stable_hash;

        if observed_hash == expected_hash {
            let Some(interaction) = cassette.interactions.get(self.sequential_cursor) else {
                // This should never happen since self.interactions mirrors cassette.interactions
                return MatchOutcome::Mismatch(MismatchDiagnostic {
                    interaction_id: self.sequential_cursor,
                    expected_hash: expected_hash.clone(),
                    observed_hash: observed_hash.to_owned(),
                    diff_summary: "internal error: cassette interaction missing".to_owned(),
                });
            };
            self.sequential_cursor += 1;
            MatchOutcome::Matched(interaction)
        } else {
            let diff_summary =
                canonical_diff_summary(&expected_data.canonical_request, observed_canonical);
            MatchOutcome::Mismatch(MismatchDiagnostic {
                interaction_id: self.sequential_cursor,
                expected_hash: expected_hash.clone(),
                observed_hash: observed_hash.to_owned(),
                diff_summary,
            })
        }
    }

    fn keyed_match<'a>(
        &mut self,
        observed_hash: &str,
        _observed_canonical: &Value,
        cassette: &'a Cassette,
    ) -> MatchOutcome<'a> {
        let Some(indices) = self.hash_to_indices.get(observed_hash) else {
            return MatchOutcome::Mismatch(MismatchDiagnostic {
                interaction_id: self.interactions.len(),
                expected_hash: String::new(),
                observed_hash: observed_hash.to_owned(),
                diff_summary: format!("no interaction with hash {observed_hash} found in cassette"),
            });
        };

        // Find the first unconsumed interaction with this hash.
        for &idx in indices {
            let Some(&is_consumed) = self.consumed.get(idx) else {
                continue;
            };
            if is_consumed {
                continue;
            }

            // Mark as consumed
            if let Some(consumed_slot) = self.consumed.get_mut(idx) {
                *consumed_slot = true;
            }

            // Return the matched interaction
            if let Some(interaction) = cassette.interactions.get(idx) {
                return MatchOutcome::Matched(interaction);
            }
        }

        // All interactions with this hash have been consumed.
        MatchOutcome::Mismatch(MismatchDiagnostic {
            interaction_id: self.interactions.len(),
            expected_hash: String::new(),
            observed_hash: observed_hash.to_owned(),
            diff_summary: format!(
                "all interactions with hash {observed_hash} have already been consumed; \
                 cassette contains {} interaction(s) with this hash",
                indices.len()
            ),
        })
    }
}
