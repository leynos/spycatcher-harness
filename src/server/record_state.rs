//! Construction of record-mode services from validated harness configuration.
//!
//! For example, startup opens the upstream client and initializes empty session
//! counters here before request orchestration begins in the parent module.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use super::{RecordAppState, RecordService};
use crate::cassette::filesystem::FilesystemCassetteStore;
use crate::config::HarnessConfig;
use crate::server::record_metadata::SessionMetadata;
use crate::upstream::{ProcessEnvProvider, ReqwestUpstreamClient};
use crate::{HarnessError, HarnessResult};

impl RecordAppState {
    /// Builds record-mode state from validated harness configuration.
    ///
    /// # Errors
    ///
    /// Returns a harness error when the cassette or upstream client cannot be
    /// opened.
    pub(crate) fn from_config(
        cfg: &HarnessConfig,
        store: FilesystemCassetteStore,
    ) -> HarnessResult<Self> {
        let upstream = cfg
            .upstream
            .clone()
            .ok_or_else(|| HarnessError::InvalidConfig {
                message: "upstream configuration is required for record mode".to_owned(),
            })?;

        Ok(Self {
            service: RecordService {
                cassette_store: Arc::new(Mutex::new(store)),
                upstream_client: ReqwestUpstreamClient::new()?,
                env_provider: ProcessEnvProvider,
                metadata: SessionMetadata::new(upstream.kind),
                upstream,
                redaction: cfg.redaction.clone(),
                recorded_count: Arc::new(AtomicU64::new(0)),
                failure_count: Arc::new(AtomicU64::new(0)),
                interaction_seq: Arc::new(AtomicU64::new(0)),
            },
        })
    }
}
