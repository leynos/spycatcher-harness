//! Helper functions for BDD test steps.

use serde_json::Value;
use spycatcher_harness::cassette::{MatchOutcome, RecordedResponse, ReplayMatchEngine};
use spycatcher_harness::config::MatchMode;

use super::world::MatchingWorld;

pub(super) fn initialise_engine(
    matching_world: &MatchingWorld,
    mode: MatchMode,
) -> Result<(), Box<dyn std::error::Error>> {
    matching_world.mode.set(mode);
    let cassette = matching_world
        .cassette
        .take()
        .ok_or("cassette must be set before creating engine")?;
    let engine = ReplayMatchEngine::new(cassette, mode)?;
    matching_world.engine.set(engine);
    Ok(())
}

pub(super) fn run_requests(
    matching_world: &MatchingWorld,
    requests: &[(&str, Value)],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = matching_world
        .engine
        .take()
        .ok_or("engine must be set before matching")?;

    let mut response_ids = Vec::new();
    for (hash, canonical) in requests {
        let outcome = engine.next_match(hash, canonical);
        if let Some(id) = extract_response_id(&outcome) {
            response_ids.push(id);
        }
    }

    matching_world.matched_count.set(response_ids.len());
    matching_world.matched_response_ids.set(response_ids);
    matching_world.engine.set(engine);
    Ok(())
}

/// Extracts response ID from a match outcome if it's a `NonStream` response.
pub(super) fn extract_response_id(outcome: &MatchOutcome<'_>) -> Option<String> {
    if let MatchOutcome::Matched(interaction) = outcome
        && let RecordedResponse::NonStream { parsed_json, .. } = &interaction.response
    {
        return parsed_json
            .as_ref()
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .map(String::from);
    }
    None
}
