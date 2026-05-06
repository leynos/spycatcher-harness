//! Protocol identifiers and request-shape helpers.
//!
//! The current harness slice only supports OpenAI-compatible chat completions
//! requests, so this module holds the small amount of shared protocol
//! knowledge needed by both record and replay work.

use serde_json::Value;

use crate::config::UpstreamKind;

/// OpenAI-compatible chat completions route exposed by the harness.
pub(crate) const CHAT_COMPLETIONS_PATH: &str = "/v1/chat/completions";
/// Protocol identifier written into recorded cassette metadata.
pub(crate) const CHAT_COMPLETIONS_PROTOCOL_ID: &str = "openai.chat_completions.v1";

/// Returns `true` when the parsed request body asks for streaming output.
#[must_use]
pub(crate) fn is_streaming_chat_completions_request(parsed_json: Option<&Value>) -> bool {
    parsed_json
        .and_then(|value| value.get("stream"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Maps an upstream provider kind to the cassette metadata identifier.
#[must_use]
pub(crate) const fn upstream_id(kind: UpstreamKind) -> &'static str {
    match kind {
        UpstreamKind::OpenRouter => "openrouter",
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for protocol helpers.

    use rstest::rstest;
    use serde_json::json;

    use super::*;

    #[rstest]
    fn stream_detection_defaults_to_false() {
        assert!(!is_streaming_chat_completions_request(None));
        assert!(!is_streaming_chat_completions_request(Some(
            &json!({"model": "x"})
        )));
    }

    #[rstest]
    fn stream_detection_respects_true_flag() {
        assert!(is_streaming_chat_completions_request(Some(
            &json!({"model": "x", "stream": true}),
        )));
    }

    mod prop_tests {
        //! Property tests for protocol request-shape invariants.

        use super::*;
        use crate::cassette::{IgnorePathConfig, RecordedRequest};
        use crate::config::RedactionConfig;
        use crate::http_exchange::{parse_json_bytes, redact_headers};
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn redaction_is_case_insensitive(
                name in "[a-z]{3,12}",
                mixed in "[a-zA-Z]{3,12}",
            ) {
                let mixed_name = name
                    .char_indices()
                    .map(|(i, c)| {
                        if i.is_multiple_of(2) {
                            c.to_ascii_uppercase()
                        } else {
                            c
                        }
                    })
                    .collect::<String>();
                let redaction = RedactionConfig {
                    drop_headers: vec![name.clone()],
                };
                let headers = vec![(mixed_name.clone(), "value".to_owned())];
                let result = redact_headers(&headers, &redaction);
                prop_assert!(
                    result.is_empty(),
                    "redaction must be case-insensitive: {mixed_name:?} not dropped by {name:?}"
                );
                let _ = mixed;
            }
        }

        proptest! {
            #[test]
            fn non_json_body_is_never_streaming(
                garbage in proptest::collection::vec(0u8..=127u8, 0..64),
            ) {
                if serde_json::from_slice::<serde_json::Value>(&garbage).is_ok() {
                    return Ok(());
                }

                let result =
                    is_streaming_chat_completions_request(parse_json_bytes(&garbage).as_ref());
                prop_assert!(!result, "non-JSON must never be detected as streaming");
            }
        }

        proptest! {
            #[test]
            fn redaction_is_case_insensitive_and_additive(name in "[A-Za-z]{3,16}") {
                let lower = name.to_ascii_lowercase();
                let mixed = name
                    .chars()
                    .enumerate()
                    .map(|(i, c)| {
                        if i.is_multiple_of(2) {
                            c.to_ascii_uppercase()
                        } else {
                            c
                        }
                    })
                    .collect::<String>();
                let redaction = RedactionConfig {
                    drop_headers: vec![lower],
                };
                let input = vec![(mixed, "v".to_owned())];
                let output = redact_headers(&input, &redaction);

                prop_assert!(output.is_empty());
            }
        }

        proptest! {
            #[test]
            fn canonical_and_hash_present_for_valid_json(model in "[a-z]{3,10}") {
                let body = format!(r#"{{"model":"{model}","messages":[]}}"#).into_bytes();
                let parsed_json = parse_json_bytes(&body);
                let mut request = RecordedRequest {
                    method: "POST".to_owned(),
                    path: CHAT_COMPLETIONS_PATH.to_owned(),
                    query: String::new(),
                    headers: Vec::new(),
                    body,
                    parsed_json,
                    canonical_request: None,
                    stable_hash: None,
                };

                request
                    .populate_canonical_fields(&IgnorePathConfig::default())
                    .expect("valid JSON request should canonicalize");

                prop_assert!(request.canonical_request.is_some());
                prop_assert!(request.stable_hash.is_some());
            }
        }
    }
}
