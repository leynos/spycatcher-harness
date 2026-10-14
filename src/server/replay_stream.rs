//! Stream response rendering for replay-mode handlers.
//!
//! The replay domain returns typed stream events. This adapter serializes those
//! events into canonical Server-Sent Events bytes for Axum response bodies.

use axum::body::{Body, Bytes};
use futures_util::{StreamExt, stream};
use tracing::{debug, debug_span};

use crate::cassette::StreamEvent;
use crate::replay_observability::{
    MODE_REPLAY, ReplayMetricLabels, StreamDeliveryMode, record_stream_delivery,
};

/// Soft size limit for eager stream replay buffers.
const EAGER_STREAM_LIMIT_BYTES: usize = 64 * 1024;

/// Builds an Axum [`Body`] from recorded [`StreamEvent`] values.
///
/// The `events` vector is consumed in recorded order. Each event is serialized
/// into canonical SSE bytes; the helper eagerly concatenates the response when
/// the total byte length is no larger than [`EAGER_STREAM_LIMIT_BYTES`] and
/// otherwise returns a streaming [`Body`] backed by `futures_util`.
///
/// # Examples
///
/// ```rust
/// use axum::body::Body;
/// use spycatcher_harness::cassette::StreamEvent;
///
/// # fn build_stream_body(_: Vec<StreamEvent>) -> Body { Body::empty() }
/// let body = build_stream_body(vec![StreamEvent::Data {
///     raw: "[DONE]".to_owned(),
///     parsed_json: None,
/// }]);
/// # let _: Body = body;
/// ```
pub(crate) fn build_stream_body(events: Vec<StreamEvent>, labels: &ReplayMetricLabels) -> Body {
    let event_count = events.len();
    let comment_count = events
        .iter()
        .filter(|event| matches!(event, StreamEvent::Comment { .. }))
        .count();
    let data_count = event_count - comment_count;
    let span = debug_span!(
        "replay_stream_render",
        mode = MODE_REPLAY,
        protocol = labels.protocol,
        route = labels.route,
        event_count,
        comment_count,
        data_count,
    );
    let _span_guard = span.enter();
    let mut chunks = Vec::new();
    let mut total_len = 0;
    let mut remaining_events = events.into_iter();
    while let Some(event) = remaining_events.next() {
        let chunk = Bytes::from(serialize_event(&event));
        let next_len = total_len + chunk.len();
        if next_len > EAGER_STREAM_LIMIT_BYTES {
            let delivery = StreamDeliveryMode::Streamed;
            record_stream_delivery(labels, delivery, event_count);
            debug!(
                target: "spycatcher.harness.replay_stream",
                delivery = delivery.as_str(),
                event_count,
                comment_count,
                data_count,
                buffered_len = total_len,
                eager_limit = EAGER_STREAM_LIMIT_BYTES,
                "building streamed replay body"
            );
            let prefix_stream = stream::iter(
                chunks
                    .into_iter()
                    .chain(std::iter::once(chunk))
                    .map(Ok::<_, std::io::Error>),
            );
            let remaining_stream = stream::iter(remaining_events.map(|stream_event| {
                Ok::<_, std::io::Error>(Bytes::from(serialize_event(&stream_event)))
            }));
            return Body::from_stream(prefix_stream.chain(remaining_stream));
        }
        total_len = next_len;
        chunks.push(chunk);
    }

    let delivery = StreamDeliveryMode::Eager;
    record_stream_delivery(labels, delivery, event_count);
    debug!(
        target: "spycatcher.harness.replay_stream",
        delivery = delivery.as_str(),
        event_count,
        comment_count,
        data_count,
        total_len,
        eager_limit = EAGER_STREAM_LIMIT_BYTES,
        "building eager replay stream body"
    );
    Body::from(concat_chunks(chunks, total_len))
}

fn concat_chunks(chunks: Vec<Bytes>, total_len: usize) -> Bytes {
    let mut body = Vec::with_capacity(total_len);
    for chunk in chunks {
        body.extend_from_slice(&chunk);
    }
    Bytes::from(body)
}

fn serialize_event(event: &StreamEvent) -> Vec<u8> {
    match event {
        StreamEvent::Comment { text } => serialize_lines(b": ", text),
        StreamEvent::Data { raw, .. } => serialize_lines(b"data: ", raw),
    }
}

fn serialize_lines(prefix: &[u8], text: &str) -> Vec<u8> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut bytes = Vec::with_capacity(text.len() + (prefix.len() + 1) * lines.len() + 1);
    for line in &lines {
        bytes.extend_from_slice(prefix);
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
    }
    bytes.push(b'\n');
    bytes
}

#[cfg(test)]
#[path = "replay_stream_tests.rs"]
mod tests;
