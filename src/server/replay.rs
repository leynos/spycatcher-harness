//! Replay-mode application state for chat completions.
//!
//! This module loads a read-only cassette into the adapter-neutral replay
//! service. It deliberately owns no upstream configuration or HTTP client so
//! replay mode cannot perform outbound provider calls.

use crate::HarnessResult;
use crate::cassette::{CassetteReader, ReplayMatchEngine, filesystem::FilesystemCassetteStore};
use crate::config::HarnessConfig;
use crate::replay::ReplayService;

/// Shared replay-mode application state for the inbound server.
#[derive(Debug, Clone)]
pub(crate) struct ReplayAppState {
    pub(crate) service: ReplayService,
}

impl ReplayAppState {
    /// Builds replay-mode state from validated harness configuration.
    ///
    /// # Errors
    ///
    /// Returns a harness error when the cassette cannot be loaded or matched.
    pub(crate) fn from_config(
        cfg: &HarnessConfig,
        store: &FilesystemCassetteStore,
    ) -> HarnessResult<Self> {
        let cassette = store.load()?;
        let engine = ReplayMatchEngine::new(cassette, cfg.match_mode)?;
        Ok(Self {
            service: ReplayService::new(engine),
        })
    }
}
