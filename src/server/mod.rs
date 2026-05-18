//! Inbound HTTP server adapters and runtime lifecycle management.
//!
//! This module owns listener binding, route registration, and graceful
//! shutdown for HTTP modes without leaking `axum` types into cassette logic.

mod record;
mod record_handler;
pub(crate) mod record_metadata;
mod replay;
mod replay_handler;
mod runtime;

pub(crate) use runtime::{ServerHandle, start_record_server, start_replay_server};

#[cfg(test)]
#[path = "../server_tests.rs"]
mod tests;
