//! Support helpers for chat completions replay BDD assertions.

use std::error::Error;

use crate::chat_completions_replay::world::ReplayWorld;
use crate::record_helpers::ClientResponse;

pub(crate) fn response_error(
    body: &serde_json::Value,
) -> Result<&serde_json::Value, Box<dyn Error>> {
    body.get("error")
        .ok_or_else(|| std::io::Error::other("error field should be present").into())
}

pub(crate) fn replay_response(
    replay_world: &ReplayWorld,
) -> Result<ClientResponse, Box<dyn Error>> {
    replay_world
        .replay_response
        .with_ref(Clone::clone)
        .ok_or_else(|| std::io::Error::other("replay response must be stored").into())
}

pub(crate) fn response_error_kind(error: &serde_json::Value) -> Option<&str> {
    error.get("kind").and_then(serde_json::Value::as_str)
}

pub(crate) fn assert_response_error_kind(
    body: &serde_json::Value,
    expected: &str,
) -> Result<(), Box<dyn Error>> {
    let actual = response_error_kind(response_error(body)?);
    if actual == Some(expected) {
        Ok(())
    } else {
        Err(std::io::Error::other(format!("expected error kind {expected}, got {actual:?}")).into())
    }
}
