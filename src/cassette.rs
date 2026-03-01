//! Cassette schema, canonicalization, hashing, and store traits for
//! recorded sessions.
//!
//! A cassette is a single recorded agent session consisting of an ordered
//! list of interactions. This module will define the on-disk schema,
//! canonical request generation, stable hashing, and the store trait for
//! persistence.
//! See `docs/spycatcher-harness-design.md`, section "Cassette definition".
