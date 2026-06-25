//! Replay matching engine for cassette interactions.

use std::collections::HashMap;

use serde_json::Value;

use crate::cassette::diff::canonical_diff_summary;
use crate::cassette::{Cassette, Interaction, StreamCanonicalPolicy};
use crate::config::MatchMode;
use crate::{HarnessError, HarnessResult};

/// Diagnostic prefix for cassette exhaustion.
pub const DIAGNOSTIC_EXHAUSTED: &str = "cassette-exhausted";

/// Diagnostic prefix for no keyed-mode match.
pub const DIAGNOSTIC_NO_MATCH: &str = "no-matching-interaction";

/// Diagnostic prefix for a consumed keyed-mode match.
pub const DIAGNOSTIC_CONSUMED: &str = "interaction-already-consumed";

/// Position information carried in a [`MismatchDiagnostic`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InteractionPosition {
    /// The zero-based index of the next expected interaction (sequential mode).
    Expected(usize),
    /// The cassette is exhausted; value is the total number of interactions.
    Exhausted(usize),
    /// Keyed mode miss; value is the total number of interactions.
    KeyedMiss(usize),
}

/// Structured diagnostic for a replay mismatch.
#[derive(Debug, Clone, PartialEq)]
pub struct MismatchDiagnostic {
    /// Identifies which interaction (or bound) the mismatch relates to.
    pub position: InteractionPosition,
    /// Stable hash of the expected request, or empty for keyed misses and
    /// exhaustion.
    pub expected_hash: String,
    /// Stable hash of the incoming request.
    pub observed_hash: String,
    /// Field-level canonical request JSON diff summary, or bounded diagnostic
    /// text for exhaustion, no-match, consumed, and internal-error paths.
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

/// Replay matching engine that consumes cassette interactions by match mode.
#[derive(Debug)]
pub struct ReplayMatchEngine {
    /// The cassette being replayed.
    cassette: Cassette,
    /// Extracted interaction data for matching.
    interactions: Vec<InteractionData>,
    /// Current matching mode.
    mode: MatchMode,
    /// Sequential mode cursor.
    sequential_cursor: usize,
    /// Keyed mode hash-to-indices map.
    hash_to_indices: HashMap<String, Vec<usize>>,
    /// Keyed mode consumed-interaction flags.
    consumed: Vec<bool>,
    stream_policy: StreamCanonicalPolicy,
}

/// Interaction data extracted for matching.
#[derive(Debug, Clone)]
struct InteractionData {
    stable_hash: String,
    /// Canonical request JSON value for diff generation.
    /// None indicates the interaction was recorded without canonical form.
    canonical_request: Option<Value>,
}

impl ReplayMatchEngine {
    /// Creates a new engine from a loaded cassette and match mode.
    /// # Errors
    ///
    /// Returns [`HarnessError::InvalidCassette`] if any interaction in the
    /// cassette has a missing `stable_hash`. All interactions must have their
    /// stable hash populated before replay matching can begin.
    pub fn new(cassette: Cassette, mode: MatchMode) -> HarnessResult<Self> {
        Self::with_policy(cassette, mode, StreamCanonicalPolicy::default())
    }

    /// Creates a new engine with explicit stream canonicalization policy.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use spycatcher_harness::cassette::{Cassette, ReplayMatchEngine, StreamCanonicalPolicy};
    /// use spycatcher_harness::config::MatchMode;
    ///
    /// let policy = StreamCanonicalPolicy::ignore_comments();
    /// let engine = ReplayMatchEngine::with_policy(Cassette::new(), MatchMode::SequentialStrict, policy).unwrap();
    /// assert_eq!(engine.stream_policy(), policy);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::InvalidCassette`] if any interaction in the
    /// cassette has a missing `stable_hash`.
    pub fn with_policy(
        cassette: Cassette,
        mode: MatchMode,
        stream_policy: StreamCanonicalPolicy,
    ) -> HarnessResult<Self> {
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
            stream_policy,
        })
    }

    /// Returns the configured stream canonicalization policy.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use spycatcher_harness::cassette::{Cassette, ReplayMatchEngine, StreamCanonicalPolicy};
    /// # use spycatcher_harness::config::MatchMode;
    /// let policy = StreamCanonicalPolicy::ignore_comments();
    /// let engine = ReplayMatchEngine::with_policy(Cassette::new(), MatchMode::SequentialStrict, policy).unwrap();
    /// assert_eq!(engine.stream_policy(), policy);
    /// ```
    #[must_use]
    pub const fn stream_policy(&self) -> StreamCanonicalPolicy {
        self.stream_policy
    }

    /// Attempts to match an incoming request against the cassette.
    ///
    /// In sequential strict mode, the request must match the next recorded
    /// interaction. In keyed mode, the request matches the next unconsumed
    /// interaction with the same hash.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use spycatcher_harness::cassette::{Cassette, MatchOutcome, ReplayMatchEngine};
    /// # use spycatcher_harness::config::MatchMode;
    /// let mut engine = ReplayMatchEngine::new(Cassette::new(), MatchMode::SequentialStrict).unwrap();
    /// assert!(matches!(engine.next_match("abc123", &serde_json::json!({})), MatchOutcome::Mismatch(_)));
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

    /// Inspects the next replay match before committing replay state.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use spycatcher_harness::cassette::{Cassette, Interaction, InteractionMetadata, MatchOutcome, RecordedRequest, RecordedResponse, ReplayMatchEngine};
    /// # use spycatcher_harness::config::MatchMode;
    /// let mut cassette = Cassette::new();
    /// cassette.append(Interaction {
    ///     request: RecordedRequest { method: "POST".to_owned(), path: "/v1/chat/completions".to_owned(),
    ///         query: String::new(), headers: Vec::new(), body: Vec::new(), parsed_json: None,
    ///         canonical_request: Some(serde_json::json!({"method": "POST"})), stable_hash: Some("hash_a".to_owned()) },
    ///     response: RecordedResponse::NonStream { status: 200, headers: Vec::new(), body: Vec::new(),
    ///         parsed_json: None },
    ///     metadata: InteractionMetadata { protocol_id: "openai_chat".to_owned(), upstream_id: "openai".to_owned(),
    ///         recorded_at: "2026-06-23T00:00:00Z".to_owned(), relative_offset_ms: 0 },
    /// });
    /// let mut engine = ReplayMatchEngine::new(cassette, MatchMode::SequentialStrict).unwrap();
    /// let canonical = serde_json::json!({"method": "POST"});
    ///
    /// assert!(matches!(engine.peek_match("hash_a", &canonical), MatchOutcome::Matched { interaction_id: 0, .. }));
    /// assert!(matches!(engine.next_match("hash_a", &canonical), MatchOutcome::Matched { interaction_id: 0, .. }));
    /// ```
    #[must_use]
    pub fn peek_match<'a>(
        &'a self,
        observed_hash: &str,
        observed_canonical: &Value,
    ) -> MatchOutcome<'a> {
        let candidate = match self.mode {
            MatchMode::SequentialStrict => {
                self.sequential_candidate(observed_hash, observed_canonical)
            }
            MatchMode::Keyed => self.keyed_candidate(observed_hash),
        };
        match candidate {
            Ok(idx) => self.matched_at(idx, observed_hash),
            Err(diagnostic) => MatchOutcome::Mismatch(diagnostic),
        }
    }

    pub(crate) fn commit_match(&mut self, interaction_id: usize) -> bool {
        match self.mode {
            MatchMode::SequentialStrict if self.sequential_cursor == interaction_id => {
                self.sequential_cursor += 1;
                true
            }
            MatchMode::Keyed => {
                let Some(consumed_slot) = self.consumed.get_mut(interaction_id) else {
                    return false;
                };
                if *consumed_slot {
                    false
                } else {
                    *consumed_slot = true;
                    true
                }
            }
            MatchMode::SequentialStrict => false,
        }
    }

    fn sequential_candidate(
        &self,
        observed_hash: &str,
        observed_canonical: &Value,
    ) -> Result<usize, MismatchDiagnostic> {
        let Some(expected_data) = self.interactions.get(self.sequential_cursor) else {
            return Err(MismatchDiagnostic {
                position: InteractionPosition::Exhausted(self.sequential_cursor),
                expected_hash: String::new(),
                observed_hash: observed_hash.to_owned(),
                diff_summary: format!("{DIAGNOSTIC_EXHAUSTED}: no more interactions available"),
            });
        };

        let expected_hash = &expected_data.stable_hash;

        if observed_hash == expected_hash {
            Ok(self.sequential_cursor)
        } else {
            let diff_summary = expected_data.canonical_request.as_ref().map_or_else(
                || "diff unavailable: expected interaction has no canonical_request".to_owned(),
                |expected_canonical| canonical_diff_summary(expected_canonical, observed_canonical),
            );
            Err(MismatchDiagnostic {
                position: InteractionPosition::Expected(self.sequential_cursor),
                expected_hash: expected_hash.clone(),
                observed_hash: observed_hash.to_owned(),
                diff_summary,
            })
        }
    }

    fn keyed_candidate(&self, observed_hash: &str) -> Result<usize, MismatchDiagnostic> {
        let Some(indices) = self.hash_to_indices.get(observed_hash) else {
            return Err(MismatchDiagnostic {
                position: InteractionPosition::KeyedMiss(self.interactions.len()),
                expected_hash: String::new(),
                observed_hash: observed_hash.to_owned(),
                diff_summary: format!(
                    "{DIAGNOSTIC_NO_MATCH}: no interaction with hash {observed_hash} found in cassette"
                ),
            });
        };

        for &idx in indices {
            let Some(&is_consumed) = self.consumed.get(idx) else {
                continue;
            };
            if is_consumed {
                continue;
            }

            return Ok(idx);
        }

        Err(MismatchDiagnostic {
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

    fn matched_at<'a>(&'a self, idx: usize, observed_hash: &str) -> MatchOutcome<'a> {
        let Some(interaction) = self.cassette.interactions.get(idx) else {
            let expected_hash = self
                .interactions
                .get(idx)
                .map_or(String::new(), |data| data.stable_hash.clone());
            return MatchOutcome::Mismatch(MismatchDiagnostic {
                position: InteractionPosition::Expected(idx),
                expected_hash,
                observed_hash: observed_hash.to_owned(),
                diff_summary: "internal error: cassette interaction missing".to_owned(),
            });
        };
        MatchOutcome::Matched {
            interaction_id: idx,
            interaction,
        }
    }

    fn sequential_match<'a>(
        &'a mut self,
        observed_hash: &str,
        observed_canonical: &Value,
    ) -> MatchOutcome<'a> {
        let idx = match self.sequential_candidate(observed_hash, observed_canonical) {
            Ok(idx) => idx,
            Err(diagnostic) => return MatchOutcome::Mismatch(diagnostic),
        };
        let Some(interaction) = self.cassette.interactions.get(idx) else {
            return self.matched_at(idx, observed_hash);
        };
        self.sequential_cursor += 1;
        MatchOutcome::Matched {
            interaction_id: idx,
            interaction,
        }
    }

    fn keyed_match<'a>(
        &'a mut self,
        observed_hash: &str,
        _observed_canonical: &Value,
    ) -> MatchOutcome<'a> {
        let idx = match self.keyed_candidate(observed_hash) {
            Ok(idx) => idx,
            Err(diagnostic) => return MatchOutcome::Mismatch(diagnostic),
        };
        if let Some(consumed_slot) = self.consumed.get_mut(idx) {
            *consumed_slot = true;
        }
        self.matched_at(idx, observed_hash)
    }
}
