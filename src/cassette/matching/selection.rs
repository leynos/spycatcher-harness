//! Candidate selection and state updates for replay matching.

use serde_json::Value;

use crate::cassette::diff::canonical_diff_summary;

use super::{
    DIAGNOSTIC_CONSUMED, DIAGNOSTIC_EXHAUSTED, DIAGNOSTIC_NO_MATCH, InteractionPosition,
    MatchOutcome, MismatchDiagnostic, ReplayMatchEngine,
};

impl ReplayMatchEngine {
    /// Selects the next sequential interaction or diagnoses a miss or exhaustion.
    ///
    /// For example, with cursor zero and the first interaction's hash `"h1"`,
    /// `engine.sequential_candidate("h1", &observed)` returns `Ok(0)` without
    /// advancing the cursor. Calling it with `"other"` instead returns a
    /// diagnostic at `InteractionPosition::Expected(0)`.
    pub(super) fn sequential_candidate(
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

    /// Selects the first unused interaction with the observed request hash.
    ///
    /// For example, if hash `"h"` indexes interactions zero and two, with zero
    /// consumed and two unused, `engine.keyed_candidate("h")` returns `Ok(2)`.
    /// The selection leaves interaction two unused until a match is committed.
    pub(super) fn keyed_candidate(&self, observed_hash: &str) -> Result<usize, MismatchDiagnostic> {
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
                "{DIAGNOSTIC_CONSUMED}: all interactions with hash {observed_hash} have already been consumed; cassette contains {} interaction(s) with this hash",
                indices.len()
            ),
        })
    }

    /// Retrieves a selected interaction, diagnosing an inconsistent index.
    ///
    /// For example, when the cassette contains a second interaction,
    /// `engine.matched_at(1, "h")` returns `MatchOutcome::Matched` with
    /// `interaction_id: 1` and a reference to that interaction. An index absent
    /// from the cassette instead yields a mismatch at `Expected(idx)`.
    pub(super) fn matched_at<'a>(&'a self, idx: usize, observed_hash: &str) -> MatchOutcome<'a> {
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

    /// Matches and advances the next sequential interaction on success.
    ///
    /// For example, with cursor zero and the first interaction's hash `"h1"`,
    /// `engine.sequential_match("h1", &observed)` returns a matched interaction
    /// with ID zero and advances the cursor to one. From the same initial state,
    /// hash `"other"` returns a mismatch and leaves the cursor at zero.
    pub(super) fn sequential_match<'a>(
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

    /// Matches by hash and marks the selected keyed interaction consumed.
    ///
    /// For example, if unused interactions zero and two both have hash `"h"`,
    /// `engine.keyed_match("h", &observed)` returns a match with ID zero and
    /// consumes that interaction. Calling it again matches and consumes ID two.
    /// The canonical value does not influence this hash-based selection.
    pub(super) fn keyed_match<'a>(
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
