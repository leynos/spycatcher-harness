//! Scenario world state for record-mode proxying BDD tests.

use std::sync::Arc;

use camino::Utf8PathBuf;
use rstest_bdd::Slot;
use rstest_bdd_macros::ScenarioState;

use spycatcher_harness::{HarnessConfig, HarnessResult, RunningHarness};

use crate::record_mode_proxying::helpers::{ClientResponse, StubUpstream};
#[path = "../support/test_utils.rs"]
mod test_utils;
use test_utils::build_runtime;

/// Shared state for record-mode proxying scenarios.
#[derive(ScenarioState)]
pub(crate) struct ProxyWorld {
    pub(crate) runtime: Slot<Arc<tokio::runtime::Runtime>>,
    pub(crate) config: Slot<HarnessConfig>,
    pub(crate) cassette_path: Slot<Utf8PathBuf>,
    pub(crate) harness: Slot<RunningHarness>,
    pub(crate) response: Slot<ClientResponse>,
    pub(crate) upstream: Slot<StubUpstream>,
    pub(crate) shutdown_result: Slot<HarnessResult<()>>,
}

impl Default for ProxyWorld {
    fn default() -> Self {
        let runtime = Slot::new();
        runtime.set(Arc::new(build_runtime()));
        Self {
            runtime,
            config: Slot::default(),
            cassette_path: Slot::default(),
            harness: Slot::default(),
            response: Slot::default(),
            upstream: Slot::default(),
            shutdown_result: Slot::default(),
        }
    }
}
