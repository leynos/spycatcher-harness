# Architectural decision record (ADR) 0002: Adopt the Rust build standard

## Status

Accepted on 2026-10-03: apply the estate Rust build standard while retaining
LLVM as a documented exception. See [Amendments](#amendments) for the later
toolchain-pin and filename updates, neither of which changed the decision.

## Date

2026-11-03

## Context and problem statement

The estate Rust build standard (concordat rule `rust-build-defaults`, BD-001 to
BD-006) makes the parallel `rustc` frontend, the `mold` linker on Linux, and
the Cranelift backend where the whole suite passes the defaults for development
builds, committed to `.cargo/config.toml` so that Cargo auto-discovers them.

Cargo applies one `rustflags` source rather than merging them, and an assigned
`RUSTFLAGS` replaces every source. CI's setup-rust exports `RUSTFLAGS`, so the
Makefile and the workflow cannot rely on the configuration alone.

Cranelift does not currently pass this suite on the pinned nightly. A
2026-09-28 observation found every target that links `aws-lc-rs` failing at the
link step with `mold` reporting undefined `aws_lc_0_40_0_*` symbols, while LLVM
with the same flags links and passes; a 2026-09-30 probe found panic
propagation failing on the Cranelift route (`catch_unwind_reports_err` fails and
`thread_join_reports_err` aborts). Neither observation establishes a compiler
root cause.

## Decision

- Every `rustflags` source in `.cargo/config.toml` carries `-Zthreads=8`, and
  the `cfg(target_os = "linux")` source adds `mold`.
- The Makefile restates both flags wherever it assigns `RUSTFLAGS`, composing
  them with an inherited value, and adds `mold` only when the host and the
  compilation target are both Linux.
- `make release` assigns the inherited `RUSTFLAGS`, so a release adds neither
  standard flag, and coverage takes neither only when its caller exports
  `RUSTFLAGS`.
- Development builds stay on LLVM. This is a recorded exception to the
  standard; revisit it on or after 2027-04-03, as issue #146 records, or when
  the toolchain pin changes.

## Consequences

- A Linux host needs `mold` installed before any build, and CI installs it
  before the first gate target.
- `tests/build_standard_contract.rs` holds the configuration and the Makefile
  recipes to this decision, and `tests/build_standard_ci.rs` holds the CI
  install order.
- Bare Cargo on a non-Linux host cross-building for a Linux target is outside
  the standard: the configuration selects `mold` by the compilation target
  alone, so that build would be handed a linker the host lacks. Make avoids it
  by checking the host as well; use Make or a Linux host.
- The Cranelift exception is documented in the developers' guide, which this
  record complements.

## Amendments

### 2026-10-31: toolchain pin nightly-2026-10-30

The toolchain pin moved from `nightly-2026-02-26` to `nightly-2026-10-30`. No
Cranelift measurement was taken on the new pin, and none is required now: the
exception stands on the 2026-09-28 and 2026-09-30 observations. This entry
defers the next measurement to the 2027-04-03 revisit date recorded in the
tracking issue (number 146), and supersedes the "or when the toolchain pin
changes" trigger in the decision above for this pin move. The developers' guide
states the same deferral.

### 2026-11-03: canonical filename and status summary

This decision was published as ADR 002 at
`docs/adr/2026-10-03-adopt-the-rust-build-standard.md`. Its canonical path now
uses ADR 0002 to follow the four-digit filename convention; the ordinal remains
2. The expanded Status summary records the original acceptance date and
decision. This is a documentation-format update, not a change to the accepted
decision.
