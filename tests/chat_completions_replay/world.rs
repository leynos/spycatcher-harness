//! Scenario world state for chat completions replay BDD tests.

use std::sync::Arc;

use camino::Utf8PathBuf;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;

use spycatcher_harness::cassette::StreamEvent;
use spycatcher_harness::{HarnessConfig, RunningHarness};

use crate::record_helpers::{ClientResponse, StubUpstream};
#[path = "../support/test_utils.rs"]
mod test_utils;
use test_utils::build_runtime;

/// Shared state for replay scenarios.
#[derive(Default, ScenarioState)]
pub(crate) struct ReplayWorld {
    pub(crate) runtime: Slot<Arc<tokio::runtime::Runtime>>,
    pub(crate) record_config: Slot<HarnessConfig>,
    pub(crate) replay_config: Slot<HarnessConfig>,
    pub(crate) cassette_path: Slot<Utf8PathBuf>,
    pub(crate) record_harness: Slot<RunningHarness>,
    pub(crate) replay_harness: Slot<RunningHarness>,
    pub(crate) record_response: Slot<ClientResponse>,
    pub(crate) replay_response: Slot<ClientResponse>,
    pub(crate) upstream: Slot<StubUpstream>,
    pub(crate) canonical_expected: Slot<Vec<StreamEvent>>,
    pub(crate) canonical_observed: Slot<Vec<StreamEvent>>,
}

impl ReplayWorld {
    pub(crate) fn ensure_runtime(&self) -> Result<(), std::io::Error> {
        if self.runtime.with_ref(|_| ()).is_none() {
            self.runtime.set(Arc::new(build_runtime()?));
        }
        Ok(())
    }
}
