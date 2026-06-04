//! Stream response rendering for replay-mode handlers.
//!
//! The replay domain returns typed stream events. This adapter serializes those
//! events into canonical Server-Sent Events bytes for Axum response bodies.

use axum::body::{Body, Bytes};

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
    let chunks = events
        .into_iter()
        .map(|event| Bytes::from(serialize_event(&event)))
        .collect::<Vec<_>>();
    let total_len = chunks.iter().map(Bytes::len).sum::<usize>();
    if total_len <= EAGER_STREAM_LIMIT_BYTES {
        Body::from(concat_chunks(chunks, total_len))
    } else {
        // The error type exists only to satisfy `Body::from_stream`; the
        // stream iterates pre-serialized chunks and cannot fail.
        let stream = futures_util::stream::iter(chunks.into_iter().map(Ok::<_, std::io::Error>));
        Body::from_stream(stream)
    }
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
        StreamEvent::Comment { text } => format!(": {text}\n\n").into_bytes(),
        StreamEvent::Data { raw, .. } => format!("data: {raw}\n\n").into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for replay stream serialization.

    use axum::body::to_bytes;
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
}
