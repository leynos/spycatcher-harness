//! Stream response rendering for replay-mode handlers.
//!
//! The replay domain returns typed stream events. This adapter serializes those
//! events into canonical Server-Sent Events bytes for Axum response bodies.

use axum::body::{Body, Bytes};
use futures_util::{StreamExt, stream};
use tracing::{debug, debug_span};

use crate::{
    cassette::StreamEvent,
    replay_observability::{
        MODE_REPLAY,
        ReplayMetricLabels,
        StreamDeliveryMode,
        record_stream_delivery,
    },
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
    let context = StreamBodyContext::new(&events, labels);
    let span = context.span();
    let _span_guard = span.enter();

    let mut buffered_events = BufferedStream::default();
    let mut remaining_events = events.into_iter();
    while let Some(event) = remaining_events.next() {
        let Some(overflow_chunk) = buffered_events.try_buffer(&event) else {
            continue;
        };
        return build_streamed_body(buffered_events, overflow_chunk, remaining_events, &context);
    }

    build_eager_body(buffered_events, &context)
}

/// Context shared by the eager and streamed rendering paths.
struct StreamBodyContext<'a> {
    /// Bounded labels attached to replay metrics.
    labels: &'a ReplayMetricLabels,
    /// Number of recorded events, used by logs and metrics.
    event_count: usize,
    /// Number of comment events, used by tracing fields.
    comment_count: usize,
    /// Number of data events, used by tracing fields.
    data_count: usize,
}

impl<'a> StreamBodyContext<'a> {
    /// Counts event kinds once so both delivery paths report the same values.
    /// For example, one comment and two data events yield counts of 1 and 2.
    fn new(events: &[StreamEvent], labels: &'a ReplayMetricLabels) -> Self {
        let event_count = events.len();
        let comment_count = events
            .iter()
            .filter(|event| matches!(event, StreamEvent::Comment { .. }))
            .count();

        Self {
            labels,
            event_count,
            comment_count,
            data_count: event_count - comment_count,
        }
    }

    /// Creates the render span with the same protocol and event fields.
    /// For example, a three-event context attaches `event_count = 3` to the span.
    fn span(&self) -> tracing::Span {
        debug_span!(
            "replay_stream_render",
            mode = MODE_REPLAY,
            protocol = self.labels.protocol,
            route = self.labels.route,
            event_count = self.event_count,
            comment_count = self.comment_count,
            data_count = self.data_count,
        )
    }

    /// Records delivery metrics for the selected body construction path.
    /// For example, `Streamed` records streamed delivery with the total event count.
    fn record_delivery(&self, delivery: StreamDeliveryMode) {
        record_stream_delivery(self.labels, delivery, self.event_count);
    }
}

/// Chunks serialized while the response remains within the eager size limit.
#[derive(Default)]
struct BufferedStream {
    /// Serialized events in recorded order.
    chunks: Vec<Bytes>,
    /// Combined length of `chunks`.
    total_len: usize,
}

impl BufferedStream {
    /// Buffers an event when it fits, returning its bytes when it crosses the limit.
    /// For example, `[DONE]` fits an empty buffer and returns `None`.
    fn try_buffer(&mut self, event: &StreamEvent) -> Option<Bytes> {
        let chunk = Bytes::from(serialize_event(event));
        let next_len = self.total_len + chunk.len();
        if next_len > EAGER_STREAM_LIMIT_BYTES {
            return Some(chunk);
        }

        self.total_len = next_len;
        self.chunks.push(chunk);
        None
    }
}

/// Builds a streamed body after the first event exceeds the eager limit.
/// For example, buffered `first`, overflowing `second`, and trailing `third`
/// frames appear in that order in the body.
fn build_streamed_body(
    buffered_events: BufferedStream,
    overflow_chunk: Bytes,
    remaining_events: std::vec::IntoIter<StreamEvent>,
    context: &StreamBodyContext<'_>,
) -> Body {
    let delivery = StreamDeliveryMode::Streamed;
    context.record_delivery(delivery);
    debug!(
        target: "spycatcher.harness.replay_stream",
        delivery = delivery.as_str(),
        event_count = context.event_count,
        comment_count = context.comment_count,
        data_count = context.data_count,
        buffered_len = buffered_events.total_len,
        eager_limit = EAGER_STREAM_LIMIT_BYTES,
        "building streamed replay body"
    );
    let prefix_stream = stream::iter(
        buffered_events
            .chunks
            .into_iter()
            .chain(std::iter::once(overflow_chunk))
            .map(Ok::<_, std::io::Error>),
    );
    let remaining_stream =
        stream::iter(remaining_events.map(|stream_event| {
            Ok::<_, std::io::Error>(Bytes::from(serialize_event(&stream_event)))
        }));

    Body::from_stream(prefix_stream.chain(remaining_stream))
}

/// Builds a single buffered body when every event fits inside the eager limit.
/// For example, two short data frames become one body with both frames in order.
fn build_eager_body(buffered_events: BufferedStream, context: &StreamBodyContext<'_>) -> Body {
    let delivery = StreamDeliveryMode::Eager;
    context.record_delivery(delivery);
    debug!(
        target: "spycatcher.harness.replay_stream",
        delivery = delivery.as_str(),
        event_count = context.event_count,
        comment_count = context.comment_count,
        data_count = context.data_count,
        total_len = buffered_events.total_len,
        eager_limit = EAGER_STREAM_LIMIT_BYTES,
        "building eager replay stream body"
    );

    Body::from(concat_chunks(
        buffered_events.chunks,
        buffered_events.total_len,
    ))
}

/// Joins framed stream chunks into one buffer sized from their known total length.
/// For example, chunks containing `ab` and `cd` become `abcd`.
fn concat_chunks(chunks: Vec<Bytes>, total_len: usize) -> Bytes {
    let mut body = Vec::with_capacity(total_len);
    for chunk in chunks {
        body.extend_from_slice(&chunk);
    }
    Bytes::from(body)
}

/// Serializes a stream event with the Server-Sent Events comment or data prefix.
/// For example, comment text uses `: ` while event data uses `data: `.
fn serialize_event(event: &StreamEvent) -> Vec<u8> {
    match event {
        StreamEvent::Comment { text } => serialize_lines(b": ", text),
        StreamEvent::Data { raw, .. } => serialize_lines(b"data: ", raw),
    }
}

/// Prefixes each newline-delimited line and terminates the event with a blank line.
/// For example, data lines `a` and `b` become `data: a\ndata: b\n\n`.
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
