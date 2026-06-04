//! Canonical stream-event comparison helpers.
//!
//! Stream transcripts can contain provider keep-alive comments whose text may
//! drift without changing the user-visible data frames. This module provides a
//! small domain policy for normalizing those event sequences before comparison.

use crate::cassette::StreamEvent;

/// Policy controlling how recorded stream events are canonicalized.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamCanonicalPolicy {
    /// Whether SSE comment frames should be ignored during comparison.
    pub ignore_comments: bool,
}

impl StreamCanonicalPolicy {
    /// Creates a policy that ignores provider comment frames.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use spycatcher_harness::cassette::StreamCanonicalPolicy;
    ///
    /// let policy = StreamCanonicalPolicy::ignore_comments();
    /// assert!(policy.ignore_comments);
    /// ```
    #[must_use]
    pub const fn ignore_comments() -> Self {
        Self {
            ignore_comments: true,
        }
    }
}

/// Returns a canonicalized copy of the stream-event sequence.
///
/// # Examples
///
/// ```rust
/// use spycatcher_harness::cassette::{
///     StreamCanonicalPolicy, StreamEvent, canonicalize_events,
/// };
///
/// let events = vec![
///     StreamEvent::Comment { text: "OPENROUTER PROCESSING".to_owned() },
///     StreamEvent::Data { raw: "[DONE]".to_owned(), parsed_json: None },
/// ];
/// let canonical = canonicalize_events(&events, StreamCanonicalPolicy::ignore_comments());
///
/// assert_eq!(canonical, vec![
///     StreamEvent::Data { raw: "[DONE]".to_owned(), parsed_json: None },
/// ]);
/// ```
#[must_use]
pub fn canonicalize_events(
    events: &[StreamEvent],
    policy: StreamCanonicalPolicy,
) -> Vec<StreamEvent> {
    if policy.ignore_comments {
        events
            .iter()
            .filter(|event| !matches!(event, StreamEvent::Comment { .. }))
            .cloned()
            .collect()
    } else {
        events.to_vec()
    }
}

#[cfg(test)]
mod tests {
    //! Unit and property tests for stream-event canonicalization.

    use proptest::prelude::*;
    use rstest::rstest;
    use serde_json::json;

    use super::*;

    #[rstest]
    #[case::empty(Vec::new(), Vec::new())]
    #[case::comment_only(
        vec![comment("OPENROUTER PROCESSING")],
        Vec::new()
    )]
    #[case::comment_followed_by_data(
        vec![comment("OPENROUTER PROCESSING"), data("{\"id\":\"a\"}")],
        vec![data("{\"id\":\"a\"}")]
    )]
    #[case::repeated_comments(
        vec![comment("one"), comment("two"), data("[DONE]")],
        vec![data("[DONE]")]
    )]
    #[case::data_only(
        vec![data("{\"id\":\"a\"}"), data("[DONE]")],
        vec![data("{\"id\":\"a\"}"), data("[DONE]")]
    )]
    fn canonicalize_drops_comments_when_policy_requests_it(
        #[case] events: Vec<StreamEvent>,
        #[case] expected: Vec<StreamEvent>,
    ) {
        assert_eq!(
            canonicalize_events(&events, StreamCanonicalPolicy::ignore_comments()),
            expected,
        );
    }

    #[rstest]
    fn canonicalize_is_identity_by_default() {
        let events = vec![comment("OPENROUTER PROCESSING"), data("[DONE]")];

        assert_eq!(
            canonicalize_events(&events, StreamCanonicalPolicy::default()),
            events,
        );
    }

    proptest! {
        #[test]
        fn canonicalization_is_idempotent(events in stream_events()) {
            let policy = StreamCanonicalPolicy::ignore_comments();
            let once = canonicalize_events(&events, policy);
            let twice = canonicalize_events(&once, policy);

            prop_assert_eq!(twice, once);
        }

        #[test]
        fn canonicalization_preserves_data_subsequence(events in stream_events()) {
            let canonical = canonicalize_events(
                &events,
                StreamCanonicalPolicy::ignore_comments(),
            );
            let expected = events
                .iter()
                .filter(|event| matches!(event, StreamEvent::Data { .. }))
                .cloned()
                .collect::<Vec<_>>();

            prop_assert_eq!(canonical, expected);
        }
    }

    fn stream_events() -> impl Strategy<Value = Vec<StreamEvent>> {
        prop::collection::vec(stream_event(), 0..32)
    }

    fn stream_event() -> impl Strategy<Value = StreamEvent> {
        prop_oneof![
            "[A-Z ]{0,24}".prop_map(|text| StreamEvent::Comment { text }),
            any::<i64>().prop_map(|number| {
                let parsed_json = json!(number);
                StreamEvent::Data {
                    raw: parsed_json.to_string(),
                    parsed_json: Some(parsed_json),
                }
            }),
            "[a-zA-Z]{0,24}".prop_map(|text| {
                let parsed_json = json!(text);
                StreamEvent::Data {
                    raw: parsed_json.to_string(),
                    parsed_json: Some(parsed_json),
                }
            }),
            "[a-zA-Z0-9 {}:\\[\\]\",._-]{0,48}".prop_map(|raw| StreamEvent::Data {
                parsed_json: None,
                raw,
            }),
        ]
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
