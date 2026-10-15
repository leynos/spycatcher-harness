//! Adapter-neutral replay response, failure, and observability contracts.
//!
//! For example, a recorded 200 response keeps its status and body in
//! `ReplayResponse`, while an unmatched request carries a `ReplayError`.

use crate::cassette::{MismatchDiagnostic, StreamEvent};
use crate::protocol::CHAT_COMPLETIONS_PROTOCOL_ID;

/// Contextual fields shared by replay-mode observability events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReplayContext {
    /// Configured cassette name.
    pub(crate) cassette: String,
    /// Protocol identifier served by this replay service.
    pub(crate) protocol: &'static str,
}

impl ReplayContext {
    /// Creates replay context from a configured cassette name.
    #[must_use]
    pub(crate) const fn new(cassette: String) -> Self {
        Self {
            cassette,
            protocol: CHAT_COMPLETIONS_PROTOCOL_ID,
        }
    }
}

impl Default for ReplayContext {
    fn default() -> Self {
        Self::new("unknown".to_owned())
    }
}

/// Replay response data independent of the inbound HTTP framework.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReplayResponse {
    /// HTTP status code recorded in the cassette.
    pub(crate) status: u16,
    /// Persisted selected response headers in recorded order.
    pub(crate) headers: Vec<(String, String)>,
    /// Recorded body data.
    pub(crate) body: ReplayBody,
}

/// Replay body data independent of the inbound HTTP framework.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReplayBody {
    /// Raw recorded body bytes for non-stream responses.
    OneShot(Vec<u8>),
    /// Parsed stream events preserved in observed order.
    Events(Vec<StreamEvent>),
}

/// Request-level replay failures reported to the HTTP adapter.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ReplayError {
    /// The observed request does not match the next replayable interaction.
    Mismatch(MismatchDiagnostic),
    /// Raw-transcript streaming replay is outside this task's scope.
    // FIXME(task 2.1.3): return this when exact raw-transcript streaming replay
    // errors are introduced.
    #[expect(
        dead_code,
        reason = "reserved for task 2.1.3 raw-transcript streaming replay errors"
    )]
    UnsupportedStream,
    /// A streaming request matched a non-stream cassette interaction.
    StreamCassetteRequiredForStreamRequest,
    /// Chat completions replay requires a valid JSON request body.
    MalformedJson,
    /// An internal replay invariant or lock failed.
    Internal,
}
