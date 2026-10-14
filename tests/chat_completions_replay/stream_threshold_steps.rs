//! HTTP replay coverage for buffered, crossing and trailing recorded SSE frames.
//!
//! Dynamic payloads reach the 64 KiB transition without expanding golden fixtures.

use std::error::Error;

use rstest_bdd_macros::{given, then};
use spycatcher_harness::cassette::{RecordedResponse, StreamEvent};

use super::{
    comment, data, make_replay_config, replay_response, stream_shaped_non_stream_cassette,
    unique_cassette_path, write_cassette,
};
use crate::chat_completions_replay::world::ReplayWorld;
use crate::record_helpers::ClientResponse;

/// Constructs four recorded events and an independent expected wire transcript.
/// For example, two frames fit below 64 KiB, the third crosses it, and the final
/// `[DONE]` frame must still arrive after the buffered and crossing frames.
fn threshold_stream() -> (Vec<StreamEvent>, Vec<u8>) {
    let first = "a".repeat(32 * 1024);
    let processing = "b".repeat(16 * 1024);
    let crossing = "c".repeat(24 * 1024);
    let expected =
        format!("data: {first}\n\n: {processing}\n\ndata: {crossing}\n\ndata: [DONE]\n\n");
    (
        vec![
            data(&first),
            comment(&processing),
            data(&crossing),
            data("[DONE]"),
        ],
        expected.into_bytes(),
    )
}

/// Supplies recorded events directly so replay must use its HTTP stream path.
#[given("a replay-mode harness is configured from a threshold-crossing stream cassette")]
fn a_replay_mode_harness_is_configured_from_a_threshold_crossing_stream_cassette(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    replay_world.ensure_runtime()?;
    let cassette_path = unique_cassette_path("stream-threshold");
    let mut cassette = stream_shaped_non_stream_cassette()?;
    let interaction = cassette
        .interactions
        .first_mut()
        .ok_or_else(|| std::io::Error::other("stream fixture must contain an interaction"))?;
    let (events, raw_transcript) = threshold_stream();
    interaction.response = RecordedResponse::Stream {
        status: 200,
        headers: Vec::new(),
        events,
        raw_transcript,
        timing: None,
    };
    write_cassette(&cassette_path, &cassette)?;
    replay_world.cassette_path.set(cassette_path.clone());
    replay_world
        .replay_config
        .set(make_replay_config(&cassette_path)?);
    Ok(())
}

/// Checks the externally observed status, default SSE header, and complete bytes.
#[then("the replay client receives the threshold-crossing recorded stream unchanged")]
fn the_replay_client_receives_the_threshold_crossing_recorded_stream_unchanged(
    replay_world: &ReplayWorld,
) -> Result<(), Box<dyn Error>> {
    let response = replay_response(replay_world)?;
    let (_, expected) = threshold_stream();
    assert_threshold_stream_response(&response, &expected);
    Ok(())
}

/// Asserts the complete HTTP contract of the recorded threshold-crossing stream.
/// Keep these assertions separate from step orchestration so failures report the
/// caller while response retrieval remains fallible.
#[track_caller]
fn assert_threshold_stream_response(response: &ClientResponse, expected: &[u8]) {
    assert_eq!(
        response.status, 200,
        "threshold replay must keep the recorded status"
    );
    assert!(
        response.headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("content-type") && value == "text/event-stream"
        }),
        "threshold replay must expose the stable event-stream content type",
    );
    assert_eq!(
        response.body, expected,
        "HTTP replay must keep every SSE frame in order"
    );
}
