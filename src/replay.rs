//! Native replay engine and optional `VidaiMock` backend driver.
//!
//! This module will implement deterministic replay of recorded
//! interactions, including timing controls and optional `VidaiMock`
//! subprocess management for streaming physics and chaos injection.
//! See `docs/spycatcher-harness-design.md`, section "Replay backends".
