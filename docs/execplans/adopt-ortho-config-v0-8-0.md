# Adopt `ortho_config` v0.8.0

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

## Purpose / big picture

Upgrade this crate's `ortho_config` usage from `0.7.0` to `0.8.0` without
changing the user-visible precedence model for layered configuration loading.
After the change, the repository should build and test against Rust `1.88` or
newer, the existing layered CLI behaviour in `src/cli.rs` should still work,
and the repository documentation should no longer describe an older minimum
Rust version or older `ortho_config` integration guidance.

Observable success:

- `Cargo.toml` declares `ortho_config = "0.8.0"` and a minimum supported Rust
  version of `1.88`.
- The layered configuration tests in
  `tests/cli_layering_unit.rs` and `tests/harness_cli_layering_bdd.rs` still
  pass and continue to prove `CLI > env > config files > defaults`.
- Any source imports or derive attributes required by `ortho_config` `0.8.0`
  are updated, with no unresolved generated paths or clap default conflicts.
- Documentation that describes toolchain requirements or `ortho_config`
  behaviour is synchronized with the implemented state.

## Constraints

- Preserve the current user-visible configuration precedence:
  `CLI > env > config files > defaults`.
- Keep the adapter/domain separation intact. `src/cli.rs` may change to stay
  compatible with `ortho_config` `0.8.0`, but domain modules must not gain
  direct CLI or process-environment coupling.
- Do not introduce crate aliases for `ortho_config` unless there is a concrete
  reason. If an alias is introduced, every relevant derive site must add
  `#[ortho_config(crate = "...")]`.
- Do not add `ortho_config_macros` unless the implementation actually needs
  derive macros that are not already re-exported or otherwise available.
- Treat the migration notes as requirements, not suggestions. Each note must be
  either implemented or explicitly documented as not applicable to this crate.
- Keep documentation in `docs/` accurate, especially
  `docs/ortho-config-users-guide.md`, `docs/spycatcher-harness-design.md`, and
  `docs/rstest-bdd-users-guide.md` if they are affected.
- Before finishing, run all relevant quality gates with `tee` and
  `set -o pipefail`: `make fmt`, `make check-fmt`, `make lint`, `make test`,
  `make markdownlint`, and `make nixie`.

## Tolerances (exception triggers)

- Scope: if the upgrade requires changes in more than 10 files or more than 500
  net lines, stop and escalate with a summary of the unexpected blast radius.
- API surface: if meeting `ortho_config` `0.8.0` compatibility requires a
  change to the public signatures exposed by this crate, stop and escalate.
- Toolchain: if the pinned toolchain in `rust-toolchain.toml` cannot satisfy
  Rust `1.88` or newer without a wider compiler-policy decision, stop and
  escalate.
- Behaviour: if `load_and_merge_subcommand` semantics in `0.8.0` differ from
  the current tests in a way that changes user-visible precedence or config
  path discovery, stop and escalate before changing behaviour.
- Documentation artifacts: if wiring `[package.metadata.ortho_config]` would
  require introducing a new derive root type or a new documentation pipeline
  that does not already exist in this crate, record that as a decision and do
  not improvize beyond the agreed scope.

## Risks

- Risk: ancillary documentation may still describe the old Rust `1.85`
  baseline or imply a stable pinned toolchain, which would become inaccurate
  after the migration. Severity: low. Likelihood: medium. Mitigation: audit
  `docs/` for stale toolchain statements and update them in the same change set.

- Risk: `ortho_config` `0.8.0` may alter helper signatures or behaviour for
  `load_and_merge_subcommand`, `Prefix`, or figment-backed test utilities.
  Severity: medium. Likelihood: medium. Mitigation: keep the current targeted
  unit and behaviour-driven development (BDD) tests as the behavioural contract
  and add focused regression tests if compilation failures reveal a changed
  call pattern.

- Risk: documentation already references `ortho_config` `0.8.0`, while the
  crate dependency is still `0.7.0`, so documentation drift may hide upgrade
  gaps. Severity: medium. Likelihood: high. Mitigation: audit every
  Rust-version and `ortho_config` usage claim during the upgrade and update
  inaccurate prose in the same change set.

- Risk: the migration note about `cargo orthohelp` may not apply because this
  crate does not currently expose an `OrthoConfigDocs` root type or generate
  documentation artifacts. Severity: low. Likelihood: high. Mitigation: treat
  this as a decision point; either wire the metadata because there is an
  existing artifact workflow, or explicitly document why it is not part of this
  upgrade.

## Progress

- [x] (2026-03-07 00:00Z) Reviewed repository instructions, the `execplans`
  skill, and the current `ortho_config` footprint.
- [x] (2026-03-07 00:00Z) Confirmed the current state:
  `Cargo.toml` still pins `ortho_config = "0.7.0"`, no `rust-version` is
  declared, `src/cli.rs` uses `load_and_merge_subcommand`, and no
  `ortho_config_macros`, `cli_default_as_absent`, crate aliasing, or
  `cargo orthohelp` workflow is currently present.
- [x] (2026-03-07 00:00Z) Drafted this ExecPlan at
  `docs/execplans/adopt-ortho-config-v0-8-0.md`.
- [x] (2026-03-07 00:00Z) Replaced
  `docs/ortho-config-users-guide.md` with the upstream `ortho-config` `v0.8.0`
  guide text for the changed sections.
- [x] (2026-03-07 00:00Z) Bumped `ortho_config` to `0.8.0`, added
  `rust-version = "1.88"`, and regenerated `Cargo.lock`.
- [x] (2026-03-07 00:00Z) Re-ran the targeted layered configuration tests after
  the bump; both suites passed without source changes.
- [x] (2026-03-07 00:00Z) Updated migration-related documentation, including
  the stale toolchain guidance in `docs/rstest-bdd-users-guide.md`.
- [x] (2026-03-07 00:00Z) Ran `make fmt`, `make check-fmt`, `make lint`,
  `make test`, `make markdownlint`, and `make nixie`; all passed.

## Surprises & Discoveries

- Discovery: the current crate already follows one `0.8.0` migration
  recommendation by importing `figment` through `ortho_config::figment` in
  tests, even though the actual dependency remains at `0.7.0`.

- Discovery: `docs/rstest-bdd-users-guide.md` previously stated that each
  `Cargo.toml` declared `rust-version = "1.85"` and that the repository pinned
  a stable toolchain; both claims needed updating for this crate.

- Discovery: the repository documents `cargo-orthohelp` usage in
  `docs/ortho-config-users-guide.md`, but there is no current
  `[package.metadata.ortho_config]` block, no `OrthoConfigDocs` implementation
  in this crate, and no existing `cargo orthohelp` invocation in the repository.

- Discovery: the local `docs/ortho-config-users-guide.md` was already very
  close to upstream `v0.8.0`; syncing to the tagged source only changed a small
  set of wording and en-GB spelling details rather than replacing the entire
  document body.

- Discovery: the runtime integration in `src/cli.rs` and the existing layered
  test suites were already compatible with `ortho_config` `0.8.0`; the
  dependency bump required no Rust source changes in this crate.

## Decision Log

- Decision: keep this upgrade scoped to compatibility and documentation
  alignment, not a redesign of configuration loading. Rationale: the current
  crate uses explicit subcommand loading helpers in `src/cli.rs`, and the
  migration notes do not require replacing that design. Date/Author: 2026-03-07
  / agent

- Decision: treat the `cargo orthohelp` metadata as conditional rather than
  automatic. Rationale: the migration note says to wire the metadata in only if
  documentation artifacts are generated, and no such workflow currently exists
  in this crate. Date/Author: 2026-03-07 / agent

- Decision: sync the local OrthoConfig guide to the tagged upstream `v0.8.0`
  text instead of hand-editing equivalent prose. Rationale: the user requested
  the upstream guide specifically, and using the tagged source keeps the local
  guide aligned with the published migration guidance. Date/Author: 2026-03-07
  / agent

- Decision: do not add `[package.metadata.ortho_config]` in this change.
  Rationale: this crate does not currently generate `cargo orthohelp`
  artifacts, expose an `OrthoConfigDocs` root type, or document such a build
  step, so the migration note is not applicable here. Date/Author: 2026-03-07 /
  agent

## Outcomes & Retrospective

The migration is complete.

Implemented outcomes:

- `Cargo.toml` now declares `ortho_config = "0.8.0"` and
  `rust-version = "1.88"`.
- `Cargo.lock` was regenerated and now resolves `ortho_config` and
  `ortho_config_macros` at `0.8.0`.
- The existing `src/cli.rs` integration remained source-compatible with
  `ortho_config` `0.8.0`; no runtime Rust code changes were required.
- `docs/rstest-bdd-users-guide.md` now reflects the Rust `1.88` minimum and
  the repository's nightly toolchain pin.
- The local OrthoConfig guide was aligned with the tagged upstream `v0.8.0`
  text in the earlier step of this migration.

Quality validation completed:

- `make fmt`
- `make check-fmt`
- `make lint`
- `make test`
- `make markdownlint`
- `make nixie`

Lessons learned:

- This crate had already adopted the `ortho_config::figment` re-export pattern,
  which reduced the migration to a dependency and documentation update.
- The migration note about `cargo orthohelp` is genuinely conditional; there
  was no value in adding package metadata for a documentation pipeline this
  crate does not use.

## Context and orientation

The files that matter for this upgrade are:

- `Cargo.toml` and `Cargo.lock` for the dependency bump and minimum Rust
  version.
- `rust-toolchain.toml` to verify the pinned compiler is compatible with Rust
  `1.88` or newer.
- `src/cli.rs`, which currently implements layered loading through
  `ortho_config::load_and_merge_subcommand` and
  `ortho_config::subcommand::Prefix`.
- `tests/cli_layering_unit.rs` and `tests/harness_cli_layering_bdd.rs`, which
  are the behavioural contract for layered precedence and subcommand merge
  behaviour.
- `docs/ortho-config-users-guide.md`, which already describes `0.8.0` features
  such as `serde-saphyr`, typed clap defaults for `cli_default_as_absent`, and
  `cargo-orthohelp`.
- `docs/rstest-bdd-users-guide.md`, which currently contains stale toolchain
  guidance.
- `docs/spycatcher-harness-design.md` and `docs/users-guide.md` if the upgrade
  changes any user-facing configuration guidance.

Before implementation, establish the exact baseline with:

```bash
set -o pipefail
rustc --version | tee /tmp/adopt-ortho-config-rustc.log
cargo tree -i ortho_config | tee /tmp/adopt-ortho-config-tree.log
rg -n "ortho[_-]config|cli_default_as_absent|SelectedSubcommandMerge|ortho_config_macros" \
  Cargo.toml src tests docs | tee /tmp/adopt-ortho-config-audit.log
```

Expected observations before any code changes:

```plaintext
- `Cargo.toml` shows `ortho_config = "0.7.0"`.
- No `ortho_config_macros` dependency exists in this crate.
- No current use of `cli_default_as_absent` exists in `src/` or `tests/`.
- `src/cli.rs` is the primary runtime integration point.
```

## Plan of work

### Stage A: baseline capture and red phase

Start by proving what currently works and what the upgrade breaks.

1. Run the focused tests that define the current behaviour:

   ```bash
   set -o pipefail
   cargo test --test cli_layering_unit \
     | tee /tmp/adopt-ortho-config-cli-layering-unit-before.log
   cargo test --test harness_cli_layering_bdd \
     | tee /tmp/adopt-ortho-config-cli-layering-bdd-before.log
   ```

2. Update `Cargo.toml` so the dependency target is `ortho_config = "0.8.0"`
   and add `rust-version = "1.88"`, then refresh `Cargo.lock`.

3. Re-run the two targeted tests immediately. If there are compilation or
   runtime failures, treat those failures as the red phase and fix only what is
   necessary to restore the established behaviour.

Go/no-go:

- Do not broaden the change set until the initial `0.8.0` failures are
  understood and captured in `Surprises & Discoveries`.

### Stage B: source compatibility fixes

Make the smallest source changes required for `0.8.0` compatibility.

1. Audit `src/cli.rs` and any other touched source files for helper signature
   changes or new trait bounds in `load_and_merge_subcommand`, `Prefix`, or
   related APIs.
2. Keep imports on `ortho_config` re-exports where possible. The tests already
   use `ortho_config::figment`; preserve that pattern.
3. If the upgrade introduces any derive macros or a crate alias for
   `ortho_config`, add the required `#[ortho_config(crate = "...")]` attributes
   at every derive site, including any use of `SelectedSubcommandMerge`.
4. Confirm that no field relies on `cli_default_as_absent` with raw
   `default_value = "..."`. If such a field is introduced or discovered during
   refactoring, convert it to `default_value_t` or `default_values_t`.

Go/no-go:

- `cargo test --test cli_layering_unit` and
  `cargo test --test harness_cli_layering_bdd` pass with `ortho_config` `0.8.0`.

### Stage C: configuration-format and parsing audit

Check whether `0.8.0`'s YAML 1.2 parsing change affects this repository.

1. Search tracked configuration examples, fixtures, and docs for YAML snippets
   that rely on YAML 1.1 booleans such as `yes`, `on`, or `off` being parsed as
   strings.
2. If any YAML examples are intended to remain strings, quote them.
3. Remove or fix any duplicate YAML mapping keys if they exist.
4. If no runtime YAML inputs exist in this crate, record that this migration
   note is documentation-only here and avoid speculative code changes.

Go/no-go:

- There are no unreviewed YAML examples or fixtures that would silently change
  meaning under `serde-saphyr`.

### Stage D: documentation alignment

Synchronize the docs with the implemented state.

1. Update any docs that mention the minimum Rust version so they reflect
   `1.88` or newer, especially `docs/rstest-bdd-users-guide.md`.
2. Update `docs/users-guide.md` and `docs/spycatcher-harness-design.md` only if
   the upgrade changes user-facing `ortho_config` behaviour in this crate.
3. Decide whether this crate generates `cargo orthohelp` artifacts:
   - If yes, add `[package.metadata.ortho_config]` to `Cargo.toml`, define the
     appropriate `root_type` and `locales`, and verify the flow with
     `cargo orthohelp`.
   - If no, record that decision in `Decision Log` and keep the upgrade scoped
     to runtime compatibility plus documentation accuracy.

Go/no-go:

- All documentation claims about the toolchain and `ortho_config` usage match
  the implemented code.

### Stage E: full validation and close-out

Run the full repository quality gates, capturing output with `tee` so failures
are inspectable after truncation.

```bash
set -o pipefail
make fmt | tee /tmp/adopt-ortho-config-fmt.log
make check-fmt | tee /tmp/adopt-ortho-config-check-fmt.log
make lint | tee /tmp/adopt-ortho-config-lint.log
make test | tee /tmp/adopt-ortho-config-test.log
make markdownlint | tee /tmp/adopt-ortho-config-markdownlint.log
make nixie | tee /tmp/adopt-ortho-config-nixie.log
```

Expected end state:

```plaintext
- `make lint` passes with no warnings promoted to errors.
- `make test` passes, including the layered CLI unit and BDD suites.
- Markdown and Mermaid validation pass after any documentation edits.
- `git diff --stat` shows a focused compatibility upgrade, not a broad refactor.
```

## Approval gate

This file is the draft plan only. Do not begin implementation until the user
explicitly approves the plan or requests revisions.
