//! Incremental parser for Server-Sent Events byte transcripts.
//!
//! The recorder feeds upstream byte chunks into this parser while preserving
//! the original bytes separately. The parser owns only SSE framing policy:
//! comments are emitted as comment events, `data:` lines are joined according
//! to the `EventSource` rules, and valid JSON data payloads are parsed for
//! cassette consumers.

use std::str::Utf8Error;

use crate::cassette::StreamEvent;

/// Incremental parser for UTF-8 Server-Sent Events streams.
#[derive(Debug, Default)]
pub(crate) struct SseParser {
    buffer: Vec<u8>,
    data_lines: Vec<String>,
}

/// Parser failures for malformed SSE byte streams.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SseParseError {
    /// A line in the stream was not valid UTF-8.
    #[error("SSE stream contained invalid UTF-8: {0}")]
    InvalidUtf8(#[from] Utf8Error),
    /// The stream ended while a line or event was still incomplete.
    #[error("SSE stream ended before the final event was complete")]
    IncompleteEvent,
}

impl SseParser {
    /// Feeds one byte fragment and returns completed events.
    ///
    /// # Errors
    ///
    /// Returns [`SseParseError::InvalidUtf8`] when a completed line is not
    /// UTF-8.
    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Result<Vec<StreamEvent>, SseParseError> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(line) = self.pop_line()? {
            self.consume_line(&line, &mut events);
        }
        Ok(events)
    }

    /// Completes parsing after upstream EOF.
    ///
    /// Returns any events held back by a deferred trailing carriage return.
    ///
    /// # Errors
    ///
    /// Returns [`SseParseError::IncompleteEvent`] if bytes or data fields were
    /// left pending without a blank-line dispatch.
    pub(crate) fn finish(&mut self) -> Result<Vec<StreamEvent>, SseParseError> {
        let mut events = Vec::new();
        if self.buffer.as_slice() == [b'\r'] {
            self.buffer.clear();
            self.dispatch_event(&mut events);
        }
        if self.buffer.is_empty() && self.data_lines.is_empty() {
            Ok(events)
        } else {
            Err(SseParseError::IncompleteEvent)
        }
    }

    fn pop_line(&mut self) -> Result<Option<String>, SseParseError> {
        let Some((line_end, drain_to)) = line_bounds(&self.buffer) else {
            return Ok(None);
        };
        let line_bytes = self.buffer.get(..line_end).unwrap_or_default();
        let line = std::str::from_utf8(line_bytes)?.to_owned();
        self.buffer.drain(..drain_to);
        Ok(Some(line))
    }

    fn consume_line(&mut self, line: &str, events: &mut Vec<StreamEvent>) {
        if line.is_empty() {
            self.dispatch_event(events);
        } else if let Some(comment) = line.strip_prefix(':') {
            events.push(StreamEvent::Comment {
                text: strip_optional_space(comment).to_owned(),
            });
        } else if let Some(value) = field_value(line, "data") {
            self.data_lines.push(value.to_owned());
        }
    }

    fn dispatch_event(&mut self, events: &mut Vec<StreamEvent>) {
        if self.data_lines.is_empty() {
            return;
        }
        let raw = self.data_lines.join("\n");
        let parsed_json = serde_json::from_str(&raw).ok();
        events.push(StreamEvent::Data { raw, parsed_json });
        self.data_lines.clear();
    }
}

fn field_value<'a>(line: &'a str, expected_field: &str) -> Option<&'a str> {
    let (field, value) = line.split_once(':').map_or((line, ""), |(field, value)| {
        (field, strip_optional_space(value))
    });
    (field == expected_field).then_some(value)
}

fn strip_optional_space(value: &str) -> &str {
    value.strip_prefix(' ').unwrap_or(value)
}

fn line_bounds(buffer: &[u8]) -> Option<(usize, usize)> {
    let mut index = 0;
    while let Some(byte) = buffer.get(index) {
        match *byte {
            b'\n' => return Some((index, index + 1)),
            b'\r' => {
                let next = index + 1;
                return match buffer.get(next) {
                    Some(&b'\n') => Some((index, index + 2)),
                    Some(_) => Some((index, index + 1)),
                    // A trailing CR may be the first half of CRLF split across chunks.
                    None => None,
                };
            }
            _ => index += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    //! Unit tests for incremental SSE parsing.

    use proptest::prelude::*;
    use rstest::rstest;
    use serde_json::json;

    use super::*;

    #[rstest]
    #[case::single_json(
        b"data: {\"id\":\"chunk\"}\n\n".as_slice(),
        vec![StreamEvent::Data {
            raw: "{\"id\":\"chunk\"}".to_owned(),
            parsed_json: Some(json!({"id": "chunk"})),
        }],
    )]
    #[case::multiple_data_lines(
        b"data: hello\ndata: world\n\n".as_slice(),
        vec![StreamEvent::Data {
            raw: "hello\nworld".to_owned(),
            parsed_json: None,
        }],
    )]
    #[case::comment_only(
        b": OPENROUTER PROCESSING\n\n".as_slice(),
        vec![StreamEvent::Comment {
            text: "OPENROUTER PROCESSING".to_owned(),
        }],
    )]
    #[case::done_marker(
        b"data: [DONE]\n\n".as_slice(),
        vec![StreamEvent::Data {
            raw: "[DONE]".to_owned(),
            parsed_json: None,
        }],
    )]
    #[case::usage_chunk(
        concat!(
            "data: {\"choices\":[],",
            "\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2}}\n\n",
        )
        .as_bytes(),
        vec![StreamEvent::Data {
            raw: concat!(
                "{\"choices\":[],",
                "\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2}}",
            )
            .to_owned(),
            parsed_json: Some(json!({
                "choices": [],
                "usage": {
                    "prompt_tokens": 1,
                    "completion_tokens": 1,
                    "total_tokens": 2,
                },
            })),
        }],
    )]
    fn parser_handles_common_events(#[case] transcript: &[u8], #[case] expected: Vec<StreamEvent>) {
        let mut parser = SseParser::default();

        let mut events = parser.feed(transcript).expect("transcript should parse");
        events.extend(parser.finish().expect("transcript should be complete"));
        assert_eq!(events, expected);
    }

    #[rstest]
    #[case(b"data: one\n\n")]
    #[case(b"data: one\r\n\r\n")]
    #[case(b"data: one\r\r")]
    fn parser_handles_line_endings(#[case] transcript: &[u8]) {
        let mut parser = SseParser::default();

        let mut events = parser.feed(transcript).expect("line ending should parse");
        events.extend(parser.finish().expect("transcript should be complete"));
        assert_eq!(
            events,
            vec![StreamEvent::Data {
                raw: "one".to_owned(),
                parsed_json: None,
            }],
        );
    }

    #[rstest]
    fn parser_defers_trailing_cr_until_next_chunk() {
        let mut parser = SseParser::default();

        let first = parser
            .feed(b"data: {\"id\":\"chunk\"}\r")
            .expect("first fragment should wait for CRLF completion");
        assert!(first.is_empty());

        let second = parser
            .feed(b"\n\n")
            .expect("second fragment should complete the event");
        let mut events = first;
        events.extend(second);
        events.extend(parser.finish().expect("transcript should be complete"));

        assert_eq!(
            events,
            vec![StreamEvent::Data {
                raw: "{\"id\":\"chunk\"}".to_owned(),
                parsed_json: Some(json!({"id": "chunk"})),
            }],
        );
    }

    #[rstest]
    fn parser_handles_frames_split_across_every_boundary() {
        let transcript = b": OPENROUTER PROCESSING\n\ndata: {\"id\":\"chunk\"}\n\ndata: [DONE]\n\n";
        let expected = parse_all(transcript).expect("baseline parse should succeed");
        for split in 0..=transcript.len() {
            let mut parser = SseParser::default();
            let first = transcript.get(..split).expect("split is in bounds");
            let second = transcript.get(split..).expect("split is in bounds");
            let mut events = parser.feed(first).expect("first fragment should parse");
            events.extend(parser.feed(second).expect("second fragment should parse"));
            events.extend(parser.finish().expect("split transcript should complete"));
            assert_eq!(events, expected, "split at byte {split}");
        }
    }

    #[rstest]
    fn parser_handles_crlf_frames_split_across_every_boundary() {
        let transcript =
            b": OPENROUTER PROCESSING\r\n\r\ndata: {\"id\":\"chunk\"}\r\n\r\ndata: [DONE]\r\n\r\n";
        let expected = parse_all(transcript).expect("baseline parse should succeed");
        for split in 0..=transcript.len() {
            let mut parser = SseParser::default();
            let first = transcript.get(..split).expect("split is in bounds");
            let second = transcript.get(split..).expect("split is in bounds");
            let mut events = parser.feed(first).expect("first fragment should parse");
            events.extend(parser.feed(second).expect("second fragment should parse"));
            events.extend(parser.finish().expect("split transcript should complete"));
            assert_eq!(events, expected, "split at byte {split}");
        }
    }

    #[rstest]
    fn parser_reports_malformed_utf8_without_panicking() {
        let mut parser = SseParser::default();

        let error = parser
            .feed(b"data: \xff\n\n")
            .expect_err("invalid UTF-8 should be rejected");

        assert!(matches!(error, SseParseError::InvalidUtf8(_)));
    }

    #[rstest]
    fn parser_reports_incomplete_final_event() {
        let mut parser = SseParser::default();

        let events = parser
            .feed(b"data: unfinished")
            .expect("pending line has not completed yet");

        assert!(events.is_empty());
        assert!(matches!(
            parser.finish(),
            Err(SseParseError::IncompleteEvent)
        ));
    }

    #[rstest]
    fn parser_ignores_unknown_fields() {
        let mut parser = SseParser::default();

        let events = parser
            .feed(b"event: message\nid: 1\ndata: kept\n\n")
            .expect("unknown fields should be ignored");

        assert_eq!(
            events,
            vec![StreamEvent::Data {
                raw: "kept".to_owned(),
                parsed_json: None,
            }],
        );
    }

    proptest! {
        #[test]
        fn fragmented_valid_transcripts_match_unfragmented(
            chunks in proptest::collection::vec("[a-z]{0,16}", 0..10),
            split_points in proptest::collection::vec(0usize..200, 0..20),
        ) {
            let transcript = build_transcript(chunks);
            let expected = parse_all(&transcript)?;

            let mut parser = SseParser::default();
            let mut events = Vec::new();
            let mut cursor = 0;
            for requested_split in split_points {
                let split = requested_split.min(transcript.len());
                if split < cursor {
                    continue;
                }
                let fragment = transcript.get(cursor..split).unwrap_or_default();
                events.extend(parser.feed(fragment)?);
                cursor = split;
            }
            let remainder = transcript.get(cursor..).unwrap_or_default();
            events.extend(parser.feed(remainder)?);
            events.extend(parser.finish()?);

            prop_assert_eq!(events, expected);
        }
    }

    fn parse_all(transcript: &[u8]) -> Result<Vec<StreamEvent>, SseParseError> {
        let mut parser = SseParser::default();
        let mut events = parser.feed(transcript)?;
        events.extend(parser.finish()?);
        Ok(events)
    }

    fn build_transcript(chunks: Vec<String>) -> Vec<u8> {
        let mut transcript = String::new();
        for chunk in chunks {
            transcript.push_str("data: ");
            transcript.push_str(&chunk);
            transcript.push_str("\n\n");
        }
        transcript.into_bytes()
    }
}
