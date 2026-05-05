//! Inbound HTTP server adapters and runtime lifecycle management.
//!
//! This module owns listener binding, route registration, and graceful
//! shutdown for record mode without leaking `axum` types into cassette logic.

mod record;
mod record_handler;
pub(crate) mod record_metadata;
mod runtime;

pub(crate) use runtime::{RecordServerHandle, start_record_server};
