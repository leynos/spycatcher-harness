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
}
