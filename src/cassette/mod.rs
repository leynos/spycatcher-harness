//! Cassette schema, canonicalization, hashing, and store traits for
//! recorded sessions.
//!
//! A cassette is a single recorded agent session consisting of an ordered
//! list of interactions. This module will define the on-disk schema,
//! canonical request generation, stable hashing, and the store trait for
//! persistence.
//! See `docs/spycatcher-harness-design.md`, section "Cassette definition".

pub(crate) mod filesystem;

use std::io::{Read, Write};
use std::num::ParseIntError;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{HarnessError, HarnessResult};

/// Schema version used to encode and validate cassette documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CassetteFormatVersion(u32);

impl CassetteFormatVersion {
    /// Schema version supported by this build.
    pub const SUPPORTED: Self = Self(1);

    /// Returns the numeric schema version stored on disk.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// Returns whether this version can be consumed by this build.
    #[must_use]
    pub const fn is_supported(self) -> bool {
        self.0 == Self::SUPPORTED.0
    }
}

impl std::fmt::Display for CassetteFormatVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl From<CassetteFormatVersion> for u32 {
    fn from(value: CassetteFormatVersion) -> Self {
        value.0
    }
}

impl From<u32> for CassetteFormatVersion {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl FromStr for CassetteFormatVersion {
    type Err = ParseIntError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        source.parse::<u32>().map(Self)
    }
}

/// A single recorded agent session persisted on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cassette {
    /// Schema version used to decode the cassette.
    pub format_version: CassetteFormatVersion,
    /// Ordered request/response interactions in this session.
    pub interactions: Vec<Interaction>,
}

impl Cassette {
    /// Creates an empty cassette using the current schema version.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            format_version: CassetteFormatVersion::SUPPORTED,
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
        if self.format_version.is_supported() {
            Ok(())
        } else {
            Err(HarnessError::UnsupportedCassetteFormatVersion {
                found: self.format_version.as_u32(),
                supported: CassetteFormatVersion::SUPPORTED.as_u32(),
            })
        }
    }
}

/// Maps a [`serde_json::Error`] to the appropriate [`HarnessError`] variant.
///
/// I/O errors are wrapped in [`HarnessError::Io`]; all other errors
/// (schema mismatches, missing fields, type errors) become
/// [`HarnessError::InvalidCassette`].
fn map_serde_error(error: serde_json::Error) -> HarnessError {
    if error.is_io() {
        HarnessError::Io {
            source: error.into(),
        }
    } else {
        HarnessError::InvalidCassette {
            message: error.to_string(),
        }
    }
}

impl Cassette {
    /// Deserializes and validates a cassette from a reader.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::Io`] when reading fails,
    /// [`HarnessError::InvalidCassette`] when JSON decoding fails or
    /// required fields are missing, and
    /// [`HarnessError::UnsupportedCassetteFormatVersion`] when the version is
    /// unknown.
    pub fn from_reader(reader: impl Read) -> HarnessResult<Self> {
        let cassette: Self = serde_json::from_reader(reader).map_err(map_serde_error)?;
        cassette.validate()?;
        Ok(cassette)
    }

    /// Serializes the cassette as JSON.
    ///
    /// # Errors
    ///
    /// Returns [`HarnessError::Io`] when writing fails, and
    /// [`HarnessError::InvalidCassette`] when serialization fails.
    pub fn write_to(&self, writer: impl Write) -> HarnessResult<()> {
        serde_json::to_writer_pretty(writer, self).map_err(map_serde_error)
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
    /// Selected, redacted-safe request headers in observed order.
    /// Preserves duplicate header names (e.g., multiple Set-Cookie).
    pub headers: Vec<(String, String)>,
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
        /// Selected response headers in observed order.
        /// Preserves duplicate header names (e.g., multiple Set-Cookie).
        headers: Vec<(String, String)>,
        /// Raw response body bytes.
        body: Vec<u8>,
        /// Parsed JSON representation when the response body is JSON.
        parsed_json: Option<Value>,
    },
    /// A stream transcript plus parsed stream events.
    Stream {
        /// HTTP status code returned by the upstream or replay server.
        status: u16,
        /// Selected response headers in observed order.
        /// Preserves duplicate header names (e.g., multiple Set-Cookie).
        headers: Vec<(String, String)>,
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
mod tests;
