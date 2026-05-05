//! Record-mode metadata clocks and session-relative timestamp construction.
//!
//! These types keep wall-clock access behind small injectable boundaries so
//! record-mode orchestration can be tested deterministically.

use std::fmt::Debug;
use std::sync::Arc;
use std::time::Instant;

use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::cassette::InteractionMetadata;
use crate::config::UpstreamKind;
use crate::protocol::{CHAT_COMPLETIONS_PROTOCOL_ID, upstream_id};
use crate::{HarnessError, HarnessResult};

/// Timestamp and relative-offset factory for recorded interactions.
pub(crate) trait MetadataFactory: Clone + Send + Sync + 'static {
    /// Creates one metadata payload for a newly observed interaction.
    fn create(&self) -> HarnessResult<InteractionMetadata>;
}

/// Clock abstraction used by record-mode metadata.
pub(crate) trait Clock: Debug + Send + Sync {
    /// Returns the current timestamp in RFC 3339 format.
    ///
    /// # Errors
    ///
    /// Returns a harness error when the timestamp cannot be formatted.
    fn now_rfc3339(&self) -> HarnessResult<String>;
}

/// Clock backed by the system UTC time source.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now_rfc3339(&self) -> HarnessResult<String> {
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|error| HarnessError::InvalidConfig {
                message: format!("failed to format recording timestamp: {error}"),
            })
    }
}

/// Metadata factory backed by the current UTC clock and session start time.
#[derive(Debug, Clone)]
pub(crate) struct SessionMetadata {
    session_start: Instant,
    upstream_kind: UpstreamKind,
    clock: Arc<dyn Clock>,
}

impl SessionMetadata {
    #[must_use]
    pub(crate) fn new(upstream_kind: UpstreamKind) -> Self {
        Self::with_clock(upstream_kind, Arc::new(SystemClock))
    }

    #[must_use]
    pub(crate) fn with_clock(upstream_kind: UpstreamKind, clock: Arc<dyn Clock>) -> Self {
        Self::with_clock_and_start(upstream_kind, clock, Instant::now())
    }

    #[must_use]
    pub(crate) fn with_clock_and_start(
        upstream_kind: UpstreamKind,
        clock: Arc<dyn Clock>,
        session_start: Instant,
    ) -> Self {
        Self {
            session_start,
            upstream_kind,
            clock,
        }
    }
}

impl MetadataFactory for SessionMetadata {
    fn create(&self) -> HarnessResult<InteractionMetadata> {
        let recorded_at = self.clock.now_rfc3339()?;
        let elapsed = self.session_start.elapsed().as_millis();
        let relative_offset_ms =
            u64::try_from(elapsed).map_err(|_| HarnessError::InvalidConfig {
                message: "relative offset exceeded u64 range".to_owned(),
            })?;

        Ok(InteractionMetadata {
            protocol_id: CHAT_COMPLETIONS_PROTOCOL_ID.to_owned(),
            upstream_id: upstream_id(self.upstream_kind).to_owned(),
            recorded_at,
            relative_offset_ms,
        })
    }
}
