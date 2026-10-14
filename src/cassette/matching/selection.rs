//! Candidate selection and state updates for replay matching.

use serde_json::Value;

use crate::cassette::diff::canonical_diff_summary;

use super::{
    DIAGNOSTIC_CONSUMED, DIAGNOSTIC_EXHAUSTED, DIAGNOSTIC_NO_MATCH, InteractionPosition,
    MatchOutcome, MismatchDiagnostic, ReplayMatchEngine,
};

impl ReplayMatchEngine {
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
