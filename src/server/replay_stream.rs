//! Stream response rendering for replay-mode handlers.
//!
//! The replay domain returns typed stream events. This adapter serializes those
//! events into canonical Server-Sent Events bytes for Axum response bodies.

use axum::body::{Body, Bytes};
use tracing::debug;

use crate::cassette::StreamEvent;

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
pub(crate) fn build_stream_body(events: Vec<StreamEvent>) -> Body {
    let event_count = events.len();
    let comment_count = events
        .iter()
        .filter(|event| matches!(event, StreamEvent::Comment { .. }))
        .count();
    let data_count = event_count - comment_count;
    let total_len = events.iter().map(serialized_event_len).sum::<usize>();
    if total_len <= EAGER_STREAM_LIMIT_BYTES {
        debug!(
            target: "spycatcher.harness.replay_stream",
            delivery = "eager",
            event_count,
            comment_count,
            data_count,
            total_len,
            eager_limit = EAGER_STREAM_LIMIT_BYTES,
            "building eager replay stream body"
        );
        Body::from(serialize_events(events, total_len))
    } else {
        debug!(
            target: "spycatcher.harness.replay_stream",
            delivery = "streamed",
            event_count,
            comment_count,
            data_count,
            total_len,
            eager_limit = EAGER_STREAM_LIMIT_BYTES,
            "building streamed replay body"
        );
        // The error type exists only to satisfy `Body::from_stream`; event
        // serialization is infallible and happens lazily per yielded chunk.
        let stream = futures_util::stream::iter(
            events
                .into_iter()
                .map(|event| Ok::<_, std::io::Error>(Bytes::from(serialize_event(&event)))),
        );
        Body::from_stream(stream)
    }
}

fn serialize_events(events: Vec<StreamEvent>, total_len: usize) -> Bytes {
    let mut body = Vec::with_capacity(total_len);
    for event in events {
        body.extend_from_slice(&serialize_event(&event));
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

fn serialized_event_len(event: &StreamEvent) -> usize {
    match event {
        StreamEvent::Comment { text } => serialized_lines_len(b": ", text),
        StreamEvent::Data { raw, .. } => serialized_lines_len(b"data: ", raw),
    }
}

fn serialized_lines_len(prefix: &[u8], text: &str) -> usize {
    text.len() + (prefix.len() + 1) * text.split('\n').count() + 1
}

#[cfg(test)]
mod tests {
    //! Unit tests for replay stream serialization.

    use axum::body::to_bytes;
    use proptest::prelude::*;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[tokio::test]
    async fn parsed_event_replay_emits_data_frames() {
        let body = build_stream_body(vec![data("{\"id\":\"chunk\"}"), data("[DONE]")]);

        let bytes = to_bytes(body, 1024).await.expect("body should be readable");

        assert_eq!(
            bytes.as_ref(),
            b"data: {\"id\":\"chunk\"}\n\ndata: [DONE]\n\n"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn parsed_event_replay_emits_comment_frames_in_order() {
        let body = build_stream_body(vec![
            data("{\"id\":\"first\"}"),
            comment("OPENROUTER PROCESSING"),
            data("[DONE]"),
        ]);

        let bytes = to_bytes(body, 1024).await.expect("body should be readable");

        assert_eq!(
            bytes.as_ref(),
            b"data: {\"id\":\"first\"}\n\n: OPENROUTER PROCESSING\n\ndata: [DONE]\n\n",
        );
    }

    #[rstest]
    #[tokio::test]
    async fn empty_event_list_produces_empty_body() {
        let body = build_stream_body(Vec::new());

        let bytes = to_bytes(body, 1024).await.expect("body should be readable");

        assert!(bytes.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn parsed_event_replay_splits_multiline_payloads() {
        let body = build_stream_body(vec![data("alpha\nbeta"), comment("one\ntwo")]);

        let bytes = to_bytes(body, 1024).await.expect("body should be readable");

        assert_eq!(
            bytes.as_ref(),
            b"data: alpha\ndata: beta\n\n: one\n: two\n\n"
        );
    }

    #[rstest]
    #[tokio::test]
    async fn eager_stream_limit_includes_boundary_size() {
        let body = build_stream_body(vec![data(&"x".repeat(EAGER_STREAM_LIMIT_BYTES - 8))]);

        let bytes = to_bytes(body, EAGER_STREAM_LIMIT_BYTES + 1)
            .await
            .expect("body should be readable");

        assert_eq!(bytes.len(), EAGER_STREAM_LIMIT_BYTES);
    }

    #[rstest]
    #[tokio::test]
    async fn oversized_stream_body_remains_readable() {
        let body = build_stream_body(vec![data(&"x".repeat(EAGER_STREAM_LIMIT_BYTES - 7))]);

        let bytes = to_bytes(body, EAGER_STREAM_LIMIT_BYTES + 2)
            .await
            .expect("body should be readable");

        assert_eq!(bytes.len(), EAGER_STREAM_LIMIT_BYTES + 1);
    }

    proptest! {
        #[test]
        fn serialized_stream_event_lines_keep_sse_prefix(
            event in stream_event_with_newlines(),
        ) {
            let expected_prefix = match &event {
                StreamEvent::Comment { .. } => ":",
                StreamEvent::Data { .. } => "data:",
            };
            let serialized = String::from_utf8(serialize_event(&event))
                .expect("serialized SSE should be UTF-8");

            for line in serialized.lines().filter(|line| !line.is_empty()) {
                prop_assert!(
                    line.starts_with(expected_prefix),
                    "line {line:?} should start with {expected_prefix:?}",
                );
            }
        }
    }

    fn comment(text: &str) -> StreamEvent {
        StreamEvent::Comment {
            text: text.to_owned(),
        }
    }

    fn data(raw: &str) -> StreamEvent {
        StreamEvent::Data {
            raw: raw.to_owned(),
            parsed_json: None,
        }
    }

    fn stream_event_with_newlines() -> impl Strategy<Value = StreamEvent> {
        prop_oneof![
            multiline_text().prop_map(|text| StreamEvent::Comment { text }),
            multiline_text().prop_map(|raw| StreamEvent::Data {
                raw,
                parsed_json: None,
            }),
        ]
    }

    fn multiline_text() -> impl Strategy<Value = String> {
        proptest::collection::vec("[A-Za-z0-9 ]{0,16}", 0..8).prop_map(|lines| lines.join("\n"))
    }
}
