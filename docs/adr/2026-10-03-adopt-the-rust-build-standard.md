# Architectural decision record (ADR) 002: Adopt the Rust build standard

## Status

Accepted

## Date

2026-10-03

## Context and problem statement

The estate Rust build standard (concordat rule `rust-build-defaults`, BD-001 to
BD-006) makes the parallel `rustc` frontend, the `mold` linker on Linux, and
the Cranelift backend where the whole suite passes the defaults for development
builds, committed to `.cargo/config.toml` so that Cargo auto-discovers them.

Cargo applies one `rustflags` source rather than merging them, and an assigned
`RUSTFLAGS` replaces every source. CI's setup-rust exports `RUSTFLAGS`, so the
Makefile and the workflow cannot rely on the configuration alone.

Cranelift does not build this suite: every target that links `aws-lc-rs` fails
at the link step with `mold` reporting undefined `aws_lc_0_40_0_*` symbols,
while LLVM with the same flags links and passes.

## Decision

- Every `rustflags` source in `.cargo/config.toml` carries `-Zthreads=8`, and
  the `cfg(target_os = "linux")` source adds `mold`.
- The Makefile restates both flags wherever it assigns `RUSTFLAGS`, composing
  them with an inherited value, and adds `mold` only when the host and the
  compilation target are both Linux.
- `make release` assigns the inherited `RUSTFLAGS`, so a release takes neither
  flag, and coverage takes neither only when its caller exports `RUSTFLAGS`.
- Development builds stay on LLVM. This is a recorded exception to the
  standard; revisit it when the toolchain pin changes or `aws-lc-rs` links
  under Cranelift.

## Consequences

- A Linux host needs `mold` installed before any build, and CI installs it
  before the first gate target.
- `tests/build_standard_contract.rs` holds the configuration, the Makefile
  recipes and the CI install order to this decision.
- The Cranelift exception is documented in the developers' guide, which this
  record complements.
