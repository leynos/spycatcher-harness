//! Helper functions for BDD test steps.

use serde_json::Value;
use spycatcher_harness::cassette::{MatchOutcome, RecordedResponse, ReplayMatchEngine};
use spycatcher_harness::config::MatchMode;

use super::world::MatchingWorld;

pub(super) fn initialise_engine(
    matching_world: &MatchingWorld,
    mode: MatchMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let cassette = matching_world
        .cassette
        .take()
        .ok_or("cassette must be set before creating engine")?;
    let engine = ReplayMatchEngine::new(cassette, mode)?;
    matching_world.mode.set(mode);
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
    let mut matched_count = 0;
    for (hash, canonical) in requests {
        let outcome = engine.next_match(hash, canonical);
        if matches!(outcome, MatchOutcome::Matched { .. }) {
            matched_count += 1;
        }
        if let Some(id) = extract_response_id(&outcome) {
            response_ids.push(id);
        }
    }

    matching_world.matched_count.set(matched_count);
    matching_world.matched_response_ids.set(response_ids);
    matching_world.engine.set(engine);
    Ok(())
}

/// Extracts response ID from a match outcome if it's a `NonStream` response.
pub(super) fn extract_response_id(outcome: &MatchOutcome<'_>) -> Option<String> {
    if let MatchOutcome::Matched { interaction, .. } = outcome
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

pub(super) fn check_matched_count(
    matching_world: &MatchingWorld,
    expected: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let matched_count = matching_world
        .matched_count
        .with_ref(|c| *c)
        .ok_or("matched_count must be set")?;
    if matched_count != expected {
        return Err(format!(
            "expected {expected} requests to match interactions, got {matched_count}"
        )
        .into());
    }
    Ok(())
}

fn assert_ids_order(
    response_ids: &[String],
    expected: &[&str],
    mode_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if response_ids != expected {
        return Err(format!(
            "{mode_name} mode should return IDs in recorded order, \
             expected {expected:?}, got {response_ids:?}"
        )
        .into());
    }
    Ok(())
}

pub(super) fn check_response_set(
    matching_world: &MatchingWorld,
    valid_ids: &[&str],
    expected_count: usize,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let response_ids = matching_world
        .matched_response_ids
        .with_ref(Vec::clone)
        .ok_or("matched_response_ids must be set")?;

    if response_ids.len() != expected_count {
        return Err(format!(
            "expected {expected_count} response IDs, got {}",
            response_ids.len()
        )
        .into());
    }
    for id in &response_ids {
        if !valid_ids.contains(&id.as_str()) {
            return Err(format!("unexpected response ID: {id}").into());
        }
    }
    Ok(response_ids)
}

pub(super) fn check_mode_order(
    response_ids: &[String],
    matching_world: &MatchingWorld,
) -> Result<(), Box<dyn std::error::Error>> {
    let mode = matching_world
        .mode
        .with_ref(|m| *m)
        .ok_or("mode must be set")?;
    match mode {
        MatchMode::SequentialStrict => {
            assert_ids_order(response_ids, &["resp_a", "resp_b", "resp_c"], "sequential")
        }
        MatchMode::Keyed => {
            assert_ids_order(response_ids, &["resp_c", "resp_b", "resp_a"], "keyed")
        }
    }
}
