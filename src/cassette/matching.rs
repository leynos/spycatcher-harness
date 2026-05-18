//! Replay matching engine for cassette interactions.
//!
//! This module implements the core replay matching logic that decides which
//! recorded interaction to serve for an incoming request. It supports two
//! matching modes: sequential strict (default) and keyed (hash-based).

use std::collections::HashMap;

use serde_json::Value;

use crate::cassette::diff::canonical_diff_summary;
use crate::cassette::{Cassette, Interaction};
use crate::config::MatchMode;
use crate::{HarnessError, HarnessResult};

/// Diagnostic prefix for cassette exhaustion (no more interactions available).
pub const DIAGNOSTIC_EXHAUSTED: &str = "cassette-exhausted";

/// Diagnostic prefix for no matching interaction found in keyed mode.
pub const DIAGNOSTIC_NO_MATCH: &str = "no-matching-interaction";

/// Diagnostic prefix for interaction already consumed in keyed mode.
pub const DIAGNOSTIC_CONSUMED: &str = "interaction-already-consumed";

/// Disambiguates the position information carried in a [`MismatchDiagnostic`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InteractionPosition {
    /// The zero-based index of the next expected interaction (sequential mode).
    Expected(usize),
    /// The cassette is exhausted; value is the total number of interactions.
    Exhausted(usize),
    /// Keyed mode: no interaction (or no unconsumed interaction) matched the
    /// observed hash; value is the total number of interactions.
    KeyedMiss(usize),
}

/// Structured diagnostic for a replay mismatch.
///
/// Carries all information needed by the adapter layer to build an HTTP 409
/// response body without coupling the domain to HTTP types.
#[derive(Debug, Clone, PartialEq)]
pub struct MismatchDiagnostic {
    /// Identifies which interaction (or bound) the mismatch relates to.
    pub position: InteractionPosition,
    /// Stable hash of the expected canonical request (sequential mode) or
    /// empty string (keyed mode miss).
    pub expected_hash: String,
    /// Stable hash of the observed incoming request.
    pub observed_hash: String,
    /// Field-level diff summary of canonical request JSON.
    pub diff_summary: String,
}

impl MismatchDiagnostic {
    /// Returns a bounded reason identifier suitable for logs and metrics.
    #[must_use]
    pub(crate) fn reason_code(&self) -> &'static str {
        match self.position {
            InteractionPosition::Expected(_) => "request_hash_mismatch",
            InteractionPosition::Exhausted(_) => "cassette_exhausted",
            InteractionPosition::KeyedMiss(_) => {
                if self.diff_summary.starts_with(DIAGNOSTIC_CONSUMED) {
                    "interaction_already_consumed"
                } else {
                    "no_matching_interaction"
                }
            }
        }
    }
}

/// Outcome of a replay match attempt.
#[derive(Debug)]
pub enum MatchOutcome<'a> {
    /// The incoming request matched a recorded interaction.
    Matched {
        /// Zero-based position of the matched interaction in the cassette.
        interaction_id: usize,
        /// Matched cassette interaction.
        interaction: &'a Interaction,
    },
    /// No match was found; diagnostics explain why.
    Mismatch(MismatchDiagnostic),
}

/// Replay matching engine that consumes cassette interactions according to
/// the configured match mode.
#[derive(Debug)]
pub struct ReplayMatchEngine {
    /// The cassette being replayed.
    cassette: Cassette,
    /// Extracted interaction data for matching.
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
    /// None indicates the interaction was recorded without canonical form.
    canonical_request: Option<Value>,
}

impl ReplayMatchEngine {
    /// Creates a new engine from a loaded cassette and match mode.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::InvalidCassette`] if any interaction in the
    /// cassette has a missing `stable_hash`. All interactions must have their
    /// stable hash populated before replay matching can begin.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use spycatcher_harness::cassette::{Cassette, ReplayMatchEngine};
    /// use spycatcher_harness::config::MatchMode;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let cassette = Cassette::new();
    ///     let _engine = ReplayMatchEngine::new(cassette, MatchMode::SequentialStrict)?;
    ///     Ok(())
    /// }
    /// ```
    pub fn new(cassette: Cassette, mode: MatchMode) -> HarnessResult<Self> {
        let mut interactions = Vec::with_capacity(cassette.interactions.len());
        for (idx, interaction) in cassette.interactions.iter().enumerate() {
            let stable_hash = interaction.request.stable_hash.clone().ok_or_else(|| {
                HarnessError::InvalidCassette {
                    message: format!(
                        "interaction at index {idx} has no stable_hash; \
                             all interactions must be hashed before replay"
                    ),
                }
            })?;
            interactions.push(InteractionData {
                stable_hash,
                canonical_request: interaction.request.canonical_request.clone(),
            });
        }

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

        Ok(Self {
            cassette,
            interactions,
            mode,
            sequential_cursor: 0,
            hash_to_indices,
            consumed,
        })
    }

    /// Attempts to match an incoming request against the cassette.
    ///
    /// In sequential strict mode, the request must match the next recorded
    /// interaction in order. In keyed mode, the request matches the next
    /// unconsumed interaction with the same hash.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use spycatcher_harness::cassette::{Cassette, MatchOutcome, ReplayMatchEngine};
    /// use spycatcher_harness::config::MatchMode;
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut engine = ReplayMatchEngine::new(Cassette::new(), MatchMode::SequentialStrict)?;
    ///     let canonical_request = serde_json::json!({});
    ///     match engine.next_match("abc123", &canonical_request) {
    ///         MatchOutcome::Matched { interaction: _interaction, .. } => { /* use interaction */ }
    ///         MatchOutcome::Mismatch(_diagnostic) => { /* handle mismatch */ }
    ///     }
    ///     Ok(())
    /// }
    /// ```
    pub fn next_match<'a>(
        &'a mut self,
        observed_hash: &str,
        observed_canonical: &Value,
    ) -> MatchOutcome<'a> {
        match self.mode {
            MatchMode::SequentialStrict => self.sequential_match(observed_hash, observed_canonical),
            MatchMode::Keyed => self.keyed_match(observed_hash, observed_canonical),
        }
    }

    fn sequential_match<'a>(
        &'a mut self,
        observed_hash: &str,
        observed_canonical: &Value,
    ) -> MatchOutcome<'a> {
        let Some(expected_data) = self.interactions.get(self.sequential_cursor) else {
            return MatchOutcome::Mismatch(MismatchDiagnostic {
                position: InteractionPosition::Exhausted(self.sequential_cursor),
                expected_hash: String::new(),
                observed_hash: observed_hash.to_owned(),
                diff_summary: format!("{DIAGNOSTIC_EXHAUSTED}: no more interactions available"),
            });
        };

        let expected_hash = &expected_data.stable_hash;

        if observed_hash == expected_hash {
            let Some(interaction) = self.cassette.interactions.get(self.sequential_cursor) else {
                // This should never happen since self.interactions mirrors cassette.interactions
                return MatchOutcome::Mismatch(MismatchDiagnostic {
                    position: InteractionPosition::Expected(self.sequential_cursor),
                    expected_hash: expected_hash.clone(),
                    observed_hash: observed_hash.to_owned(),
                    diff_summary: "internal error: cassette interaction missing".to_owned(),
                });
            };
            self.sequential_cursor += 1;
            MatchOutcome::Matched {
                interaction_id: self.sequential_cursor - 1,
                interaction,
            }
        } else {
            let diff_summary = expected_data.canonical_request.as_ref().map_or_else(
                || "diff unavailable: expected interaction has no canonical_request".to_owned(),
                |expected_canonical| canonical_diff_summary(expected_canonical, observed_canonical),
            );
            MatchOutcome::Mismatch(MismatchDiagnostic {
                position: InteractionPosition::Expected(self.sequential_cursor),
                expected_hash: expected_hash.clone(),
                observed_hash: observed_hash.to_owned(),
                diff_summary,
            })
        }
    }

    fn keyed_match<'a>(
        &'a mut self,
        observed_hash: &str,
        _observed_canonical: &Value,
    ) -> MatchOutcome<'a> {
        let Some(indices) = self.hash_to_indices.get(observed_hash) else {
            return MatchOutcome::Mismatch(MismatchDiagnostic {
                position: InteractionPosition::KeyedMiss(self.interactions.len()),
                expected_hash: String::new(),
                observed_hash: observed_hash.to_owned(),
                diff_summary: format!(
                    "{DIAGNOSTIC_NO_MATCH}: no interaction with hash {observed_hash} found in cassette"
                ),
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
            if let Some(interaction) = self.cassette.interactions.get(idx) {
                return MatchOutcome::Matched {
                    interaction_id: idx,
                    interaction,
                };
            }
        }

        // All interactions with this hash have been consumed.
        MatchOutcome::Mismatch(MismatchDiagnostic {
            position: InteractionPosition::KeyedMiss(self.interactions.len()),
            expected_hash: String::new(),
            observed_hash: observed_hash.to_owned(),
            diff_summary: format!(
                "{DIAGNOSTIC_CONSUMED}: all interactions with hash {observed_hash} have already been consumed; \
                 cassette contains {} interaction(s) with this hash",
                indices.len()
            ),
        })
    }
}
