//! Cassette schema, canonicalization, hashing, and store traits for
//! recorded sessions.
//!
//! A cassette is a single recorded agent session consisting of an ordered
//! list of interactions. This module will define the on-disk schema,
//! canonical request generation, stable hashing, and the store trait for
//! persistence.
//! See `docs/spycatcher-harness-design.md`, section "Cassette definition".

pub(crate) mod filesystem;

use std::collections::BTreeMap;
use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{HarnessError, HarnessResult};

/// The schema version supported by this build.
pub const SUPPORTED_FORMAT_VERSION: u32 = 1;

/// A single recorded agent session persisted on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cassette {
    /// Schema version used to decode the cassette.
    pub format_version: u32,
    /// Ordered request/response interactions in this session.
    pub interactions: Vec<Interaction>,
}

impl Cassette {
    /// Creates an empty cassette using the current schema version.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            format_version: SUPPORTED_FORMAT_VERSION,
            interactions: Vec::new(),
        }
    }

    /// Appends an interaction without mutating earlier entries.
    pub fn append(&mut self, interaction: Interaction) {
        self.interactions.push(interaction);
    }

    /// Validates that the cassette can be consumed by this build.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::UnsupportedCassetteFormatVersion`] when the
    /// on-disk version is not supported.
    pub const fn validate(&self) -> HarnessResult<()> {
        if self.format_version == SUPPORTED_FORMAT_VERSION {
            Ok(())
        } else {
            Err(HarnessError::UnsupportedCassetteFormatVersion {
                found: self.format_version,
                supported: SUPPORTED_FORMAT_VERSION,
            })
        }
    }

    /// Deserializes and validates a cassette from a reader.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::InvalidCassette`] when JSON decoding fails or
    /// required fields are missing, and
    /// [`HarnessError::UnsupportedCassetteFormatVersion`] when the version is
    /// unknown.
    pub fn from_reader(reader: impl Read) -> HarnessResult<Self> {
        let cassette: Self =
            serde_json::from_reader(reader).map_err(|error| HarnessError::InvalidCassette {
                message: error.to_string(),
            })?;
        cassette.validate()?;
        Ok(cassette)
    }

    /// Serializes the cassette as JSON.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::InvalidCassette`] when serialization fails.
    pub fn write_to(&self, writer: impl Write) -> HarnessResult<()> {
        serde_json::to_writer_pretty(writer, self).map_err(|error| HarnessError::InvalidCassette {
            message: error.to_string(),
        })
    }
}

impl Default for Cassette {
    fn default() -> Self {
        Self::new()
    }
}

/// Domain-owned reader port for loading an existing cassette.
pub trait CassetteReader {
    /// Loads and validates a cassette.
    ///
    /// # Errors
    ///
    /// Returns a harness error when the underlying cassette cannot be loaded
    /// or validated.
    fn load(&self) -> HarnessResult<Cassette>;
}

/// Domain-owned writer port for append-only cassette persistence.
pub trait CassetteAppender {
    /// Appends one interaction to the end of the cassette.
    ///
    /// # Errors
    ///
    /// Returns a harness error when the append cannot be persisted.
    fn append(&mut self, interaction: Interaction) -> HarnessResult<()>;
}

/// A recorded request/response exchange with metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Interaction {
    /// Recorded request information.
    pub request: RecordedRequest,
    /// Recorded response information.
    pub response: RecordedResponse,
    /// Metadata used for protocol-aware replay and diagnostics.
    pub metadata: InteractionMetadata,
}

/// Recorded request details stored in a cassette.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedRequest {
    /// HTTP method used by the request.
    pub method: String,
    /// Request path without query parameters.
    pub path: String,
    /// Raw query string in its observed order.
    pub query: String,
    /// Selected, redacted-safe request headers.
    pub headers: BTreeMap<String, String>,
    /// Raw request body bytes.
    pub body: Vec<u8>,
    /// Parsed JSON representation when the request body is JSON.
    pub parsed_json: Option<Value>,
    /// Reserved field for task `1.2.2`.
    pub canonical_request: Option<Value>,
    /// Reserved field for task `1.2.2`.
    pub stable_hash: Option<String>,
}

/// Recorded response details stored in a cassette.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordedResponse {
    /// An ordinary non-stream HTTP response body.
    NonStream {
        /// HTTP status code returned by the upstream or replay server.
        status: u16,
        /// Selected response headers.
        headers: BTreeMap<String, String>,
        /// Raw response body bytes.
        body: Vec<u8>,
        /// Parsed JSON representation when the response body is JSON.
        parsed_json: Option<Value>,
    },
    /// A stream transcript plus parsed stream events.
    Stream {
        /// HTTP status code returned by the upstream or replay server.
        status: u16,
        /// Selected response headers.
        headers: BTreeMap<String, String>,
        /// Parsed stream events preserved in order.
        events: Vec<StreamEvent>,
        /// Raw stream transcript bytes for faithful replay later.
        raw_transcript: Vec<u8>,
        /// Optional stream timing metadata captured during recording.
        timing: Option<StreamTiming>,
    },
}

/// Protocol-aware stream event captured during recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEvent {
    /// An SSE comment frame.
    Comment {
        /// The comment payload without the leading `:`.
        text: String,
    },
    /// An SSE `data:` frame.
    Data {
        /// The raw event payload text.
        raw: String,
        /// Parsed JSON when the payload is JSON.
        parsed_json: Option<Value>,
    },
}

/// Timing metadata captured for streamed responses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamTiming {
    /// Delay before the first streamed chunk, in milliseconds.
    pub ttft_ms: u64,
    /// Relative offsets for each recorded event, in milliseconds.
    pub chunk_offsets_ms: Vec<u64>,
}

/// Metadata attached to each recorded interaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InteractionMetadata {
    /// Protocol identifier for this interaction.
    pub protocol_id: String,
    /// Upstream service identifier for this interaction.
    pub upstream_id: String,
    /// RFC 3339 timestamp describing when recording occurred.
    pub recorded_at: String,
    /// Milliseconds elapsed since session start when the interaction began.
    pub relative_offset_ms: u64,
}

#[cfg(test)]
mod tests {
    //! Unit tests for cassette schema round-trips and validation.

    use super::*;
    use rstest::rstest;
    use serde_json::json;

    #[rstest]
    fn non_stream_interaction_round_trips_without_loss() {
        let cassette = Cassette {
            interactions: vec![sample_non_stream_interaction()],
            ..Cassette::new()
        };
        let mut bytes = Vec::new();
        cassette
            .write_to(&mut bytes)
            .expect("cassette serialization should succeed");

        let decoded = Cassette::from_reader(bytes.as_slice())
            .expect("cassette deserialization should succeed");

        assert_eq!(decoded, cassette);
    }

    #[rstest]
    fn stream_interaction_round_trips_without_loss() {
        let cassette = Cassette {
            interactions: vec![sample_stream_interaction()],
            ..Cassette::new()
        };
        let mut bytes = Vec::new();
        cassette
            .write_to(&mut bytes)
            .expect("cassette serialization should succeed");

        let decoded = Cassette::from_reader(bytes.as_slice())
            .expect("cassette deserialization should succeed");

        assert_eq!(decoded, cassette);
    }

    #[rstest]
    fn unsupported_format_version_is_rejected() {
        let json = r#"{"format_version":2,"interactions":[]}"#;

        let error =
            Cassette::from_reader(json.as_bytes()).expect_err("unsupported version must fail");

        assert!(matches!(
            error,
            HarnessError::UnsupportedCassetteFormatVersion {
                found: 2,
                supported: SUPPORTED_FORMAT_VERSION,
            }
        ));
    }

    #[rstest]
    fn malformed_cassette_is_rejected() {
        let json = r#"{"interactions":[]}"#;

        let error =
            Cassette::from_reader(json.as_bytes()).expect_err("missing format_version must fail");

        assert!(matches!(error, HarnessError::InvalidCassette { .. }));
    }

    fn sample_non_stream_interaction() -> Interaction {
        let request = RecordedRequest {
            method: "POST".to_owned(),
            path: "/v1/chat/completions".to_owned(),
            query: "stream=false".to_owned(),
            headers: BTreeMap::from([("content-type".to_owned(), "application/json".to_owned())]),
            body: br#"{"model":"gpt-test","stream":false}"#.to_vec(),
            parsed_json: Some(json!({"model": "gpt-test", "stream": false})),
            canonical_request: None,
            stable_hash: None,
        };
        let response = RecordedResponse::NonStream {
            status: 200,
            headers: BTreeMap::from([("content-type".to_owned(), "application/json".to_owned())]),
            body: br#"{"id":"chatcmpl-1","choices":[]}"#.to_vec(),
            parsed_json: Some(json!({"id": "chatcmpl-1", "choices": []})),
        };
        let metadata = InteractionMetadata {
            protocol_id: "openai.chat_completions.v1".to_owned(),
            upstream_id: "openrouter".to_owned(),
            recorded_at: "2026-03-10T00:00:00Z".to_owned(),
            relative_offset_ms: 0,
        };
        Interaction {
            request,
            response,
            metadata,
        }
    }

    fn sample_stream_interaction() -> Interaction {
        let mut interaction = sample_non_stream_interaction();
        interaction.response = RecordedResponse::Stream {
            status: 200,
            headers: BTreeMap::from([("content-type".to_owned(), "text/event-stream".to_owned())]),
            events: vec![
                StreamEvent::Comment {
                    text: "OPENROUTER PROCESSING".to_owned(),
                },
                StreamEvent::Data {
                    raw: "{\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}".to_owned(),
                    parsed_json: Some(json!({"choices": [{"delta": {"content": "hi"}}]})),
                },
            ],
            raw_transcript: b": OPENROUTER PROCESSING\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n"
                .to_vec(),
            timing: Some(StreamTiming {
                ttft_ms: 12,
                chunk_offsets_ms: vec![12, 17],
            }),
        };
        interaction.metadata.relative_offset_ms = 17;
        interaction
    }
}
