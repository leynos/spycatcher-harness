//! Generated checks for replay body order across the eager/streamed boundary.
//!
//! Every generated prefix/suffix pair exercises below-limit, exact-limit and
//! crossing bodies rather than relying on a rare oversized random payload.

use axum::body::to_bytes;
use proptest::prelude::*;

use super::{
    EAGER_STREAM_LIMIT_BYTES, StreamEvent, StreamMetricRecorder, build_stream_body, data, labels,
    serialize_event, stream_event_with_newlines,
};

/// Generates varied comment/data sequences on both sides of a padding event.
/// For example, a prefix comment and trailing multiline data exercise both SSE
/// frame kinds while shrinking preserves at least one event on each side.
fn event_sequences() -> impl Strategy<Value = (Vec<StreamEvent>, Vec<StreamEvent>)> {
    (
        proptest::collection::vec(stream_event_with_newlines(), 1..7),
        proptest::collection::vec(stream_event_with_newlines(), 1..7),
    )
}

/// Concatenates the original sequence's SSE frames as the complete-body oracle.
/// For example, data `"one"` followed by comment `"two"` yields
/// `"data: one\n\n: two\n\n"` without inspecting body assembly helpers.
fn serialized_bytes(events: &[StreamEvent]) -> Vec<u8> {
    events.iter().flat_map(serialize_event).collect()
}

/// Inserts one data event to reach an exact serialized byte target.
/// For example, a target of 64 KiB produces an exact-limit body; the fixed
/// single-line frame overhead is eight bytes and every other byte is padding.
fn events_at_size(
    prefix: &[StreamEvent],
    suffix: &[StreamEvent],
    target_len: usize,
) -> Vec<StreamEvent> {
    let fixed_len = serialized_bytes(prefix).len() + serialized_bytes(suffix).len() + 8;
    let padding_len = target_len - fixed_len;
    let mut events = prefix.to_vec();
    events.push(data(&"x".repeat(padding_len)));
    events.extend_from_slice(suffix);
    events
}

proptest! {
    // Three boundary bodies per case keep the normal gate bounded while ensuring
    // every case enters both delivery modes with generated prefixes and suffixes.
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn replay_body_preserves_generated_sequences_across_boundary(
        (prefix, suffix) in event_sequences(),
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        let suffix_len = serialized_bytes(&suffix).len();
        // The over-limit padding event crosses before any suffix is consumed.
        let targets = [
            EAGER_STREAM_LIMIT_BYTES - 1,
            EAGER_STREAM_LIMIT_BYTES,
            EAGER_STREAM_LIMIT_BYTES + suffix_len + 1,
        ];
        for target_len in targets {
            let recorder = StreamMetricRecorder::default();
            let _recorder_guard = metrics::set_default_local_recorder(&recorder);
            let events = events_at_size(&prefix, &suffix, target_len);
            let expected = serialized_bytes(&events);
            prop_assert_eq!(expected.len(), target_len);
            let body = build_stream_body(events, &labels());
            let bytes = runtime.block_on(to_bytes(body, target_len + 1))
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            prop_assert_eq!(bytes.as_ref(), expected.as_slice());
            let delivery = if target_len <= EAGER_STREAM_LIMIT_BYTES {
                "eager"
            } else {
                "streamed"
            };
            prop_assert_eq!(
                recorder.increment_summary(
                    "spycatcher.replay.stream.delivery.total",
                    &[("delivery", delivery), ("outcome", "replayed")],
                ),
                (1, 1),
            );
        }
    }
}
