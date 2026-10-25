# Migration guide for 0.2.0

This guide covers the Rust toolchain and CLI configuration changes in 0.2.0.

## Rust toolchain

The minimum supported Rust version increases from 1.88 to 1.89. Update the
toolchain used to build Spycatcher Harness and any downstream crate that
depends on it.

## CLI configuration candidates

The loader checks configuration candidates in order. Missing optional
candidates are skipped. A candidate that exists but is malformed or unreadable
stops loading with an error instead of falling through to a later candidate.
Defaults remain available when no candidate exists.

If startup fails while loading configuration, check the earlier candidate's
syntax and read permissions. Correct or remove that file before relying on a
later candidate.
