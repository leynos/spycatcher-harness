# Integrate layered configuration loading for all subcommands

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

## Purpose / big picture

Task 1.1.2 introduces deterministic layered configuration loading for `record`,
`replay`, and `verify`, with explicit precedence
`CLI > env > config files > defaults` and subcommand-specific merge support via
`cmds.<subcommand>`. After this change, operators can set stable shared
defaults once, override per subcommand in config files, and still use env and
CLI for higher-priority overrides.

Observable success:

- Unit tests prove precedence and subcommand merge behaviour for happy and
  unhappy paths.
- Behavioural tests (BDD with `rstest-bdd`) prove user-observable CLI outcomes
  for layered configuration and override errors.
- CLI help plus `docs/users-guide.md` describe the merged configuration shape,
  including `cmds.record`, `cmds.replay`, and `cmds.verify`.
- `docs/roadmap.md` entry `1.1.2` is marked done only after all quality gates
  pass.

## Constraints

- Keep hexagonal boundaries explicit:
  - Domain logic remains in library modules (`src/lib.rs`, `src/config.rs`) and
    must not depend on CLI framework or process environment access.
  - CLI/config loading is an adapter concern and must be isolated behind an
    adapter-facing module and mapping into domain `HarnessConfig`.
- Preserve the current public domain API contract from 1.1.1:
  `start_harness(cfg)` and `RunningHarness::shutdown()` remain the entrypoints.
- Configuration precedence must exactly match the roadmap and design document:
  `CLI > env > config files > defaults`.
- Subcommand merge semantics must support `cmds.record`, `cmds.replay`, and
  `cmds.verify` overrides.
- Tests must use `rstest` for unit coverage and `rstest-bdd` for behavioural
  coverage where user-visible behaviour is exercised.
- Avoid direct process-environment mutation in unit tests; prefer deterministic
  layer composition APIs and dependency injection patterns.
- Update design decisions in `docs/spycatcher-harness-design.md` and user-facing
  behaviour in `docs/users-guide.md`.
- Apply all relevant gates before completion:
  `make fmt`, `make check-fmt`, `make lint`, `make test`, `make markdownlint`,
  and `make nixie`.

## Tolerances (exception triggers)

- Scope: if implementation exceeds 18 files or 1200 net lines changed, stop and
  escalate.
- Interface: if meeting this task requires changing the public signature of
  `start_harness` or `RunningHarness::shutdown`, stop and escalate.
- Dependency: if more than four new crates are needed (expected: `ortho_config`,
  `serde`, and only minimal test/support additions), stop and escalate.
- Iteration: if `make lint` or `make test` fails after five fix cycles, stop and
  escalate with failure details.
- Ambiguity: if `cmds.<subcommand>` merge semantics are ambiguous between design
  references and observed OrthoConfig behaviour, stop and record options.

## Risks

- Risk: OrthoConfig derive/merge APIs may differ from assumptions in the design
  narrative. Severity: medium. Likelihood: medium. Mitigation: write red tests
  first against the expected precedence and adapt implementation to observed
  API, not assumptions.

- Risk: configuration loading code could leak CLI or env concerns into domain
  modules. Severity: medium. Likelihood: medium. Mitigation: enforce
  adapter-only loader types and use explicit mapping into `HarnessConfig`.

- Risk: behavioural tests that rely on global env may become flaky.
  Severity: medium. Likelihood: low. Mitigation: use deterministic layer
  composition in unit tests and process-level env setup scoped per spawned
  command in BDD tests.

- Risk: help text/docs drift from effective merged shape.
  Severity: low. Likelihood: medium. Mitigation: add assertions against CLI
  help output and update docs in the same change set.

## Progress

- [x] (2026-03-03 12:46Z) Drafted ExecPlan for roadmap task 1.1.2.
- [x] (2026-03-03 15:40Z) Added unit tests (`rstest`) proving precedence and
  subcommand merge behaviour.
- [x] (2026-03-03 15:55Z) Implemented adapter-side layered loader in
  `src/cli.rs` and wired binary subcommands.
- [x] (2026-03-03 16:02Z) Added behavioural tests (`rstest-bdd`) for replay,
  record, verify, and invalid env-value paths.
- [x] (2026-03-03 16:10Z) Updated CLI help text and `docs/users-guide.md` to
  describe merged `cmds.<subcommand>` shape.
- [x] (2026-03-03 16:12Z) Recorded task 1.1.2 design decisions in
  `docs/spycatcher-harness-design.md`.
- [x] (2026-03-03 16:14Z) Marked roadmap entry `1.1.2` as done.
- [x] (2026-03-04 00:00Z) Ran all quality gates and resolved failures.

## Surprises & discoveries

- Observation: OrthoConfig's subcommand loader does not honour
  `<PREFIX>CONFIG_PATH`; it discovers `. <prefix>.toml` candidates instead.
  Evidence: failing tests when only `SPYCATCHER_HARNESS_CONFIG_PATH` was set;
  passing tests once `.spycatcher_harness.toml` was written in the jailed
  working directory. Impact: tests and docs now rely on discovered
  `.spycatcher_harness.toml` behaviour for subcommand merge scenarios.

- Observation: `rstest-bdd` step captures included surrounding quotes in this
  suite for string placeholders. Evidence: BDD failures showed TOML values
  rendered as `""from_file""`. Impact: BDD steps now normalize placeholder
  inputs with `trim_surrounding_quotes` before writing config content.

## Decision log

- Decision: keep domain configuration types free of CLI/framework concerns and
  implement OrthoConfig loading in an adapter-focused module with explicit
  mapping into `HarnessConfig`. Rationale: this satisfies the hexagonal
  dependency rule and keeps domain invariants testable without process-global
  state. Date/Author: 2026-03-03 / agent

- Decision: require red-green validation for precedence
  (`CLI > env > file > defaults`) before writing loader implementation.
  Rationale: precedence defects are subtle and regress easily; tests must lock
  behaviour first. Date/Author: 2026-03-03 / agent

- Decision: use `ortho_config::load_and_merge_subcommand` with explicit
  `Prefix` rather than deriving `OrthoConfig` on flattened command structs.
  Rationale: derive-based loading produced clap parser-generation conflicts for
  flattened nested fields; the explicit helper keeps the loader deterministic
  and still provides required precedence and `cmds.<subcommand>` semantics.
  Date/Author: 2026-03-03 / agent

## Outcomes & retrospective

Task 1.1.2 is complete. Outcomes against roadmap criteria:

- Configuration precedence is proven in unit tests:
  `CLI > env > config files > defaults`.
- `record`, `replay`, and `verify` each load `cmds.<subcommand>` values with
  test coverage for override behaviour.
- CLI help and `docs/users-guide.md` now describe the merged configuration
  shape and environment namespace.

Quality validation completed for this implementation:

- `make test`
- `make check-fmt`
- `make lint`
- `make markdownlint`
- `make nixie`

Lessons learned:

- Keep subcommand loader assumptions grounded in actual crate behaviour
  (`candidate_paths`) rather than generalized docs.
- BDD string capture quirks can silently affect config fixture generation;
  normalize step input values early.

## Context and orientation

Current repository state relevant to this task:

- `src/config.rs` remains domain-focused with defaults and no CLI framework
  dependencies.
- `src/cli.rs` now contains the adapter-side layered subcommand loader and
  mapping into `HarnessConfig`.
- `src/bin/spycatcher_harness.rs` dispatches `record`, `replay`, and `verify`
  via merged layered configuration.
- `tests/harness_cli_layering_bdd.rs` and
  `tests/features/harness_cli_layering.feature` provide behavioural coverage
  for merged loading.
- `docs/roadmap.md` item `1.1.2` is marked done.

Design references for this implementation:

- `docs/spycatcher-harness-design.md`:
  - `#configuration-via-orthoconfig`
  - `#cli-integration-and-configuration`
- `docs/ortho-config-users-guide.md` for layer composition and subcommand merge
  mechanics.
- `docs/rust-testing-with-rstest-fixtures.md` and
  `docs/rstest-bdd-users-guide.md` for test structure.
- `docs/reliable-testing-in-rust-via-dependency-injection.md` for avoiding
  brittle global-state tests.

## Plan of work

### Stage A: baseline and failing tests (red)

Create test coverage that fails before implementation:

- Unit tests (`rstest`) for precedence and merge semantics in a dedicated
  configuration-loader adapter test module.
- Behavioural tests (`rstest-bdd`) that exercise user-visible command behaviour
  and help text for merged configuration shape.

Required red cases:

- Defaults only produce expected baseline command config.
- Config file values override defaults.
- Env values override file values.
- CLI values override env values.
- `cmds.record`, `cmds.replay`, and `cmds.verify` merge into the active
  subcommand while preserving global values not overridden in the subcommand.
- Invalid layered values (for example invalid enum strings or malformed address)
  fail with actionable configuration/parse diagnostics.

Go/no-go: do not implement loader logic until these tests fail with clear,
expected failure reasons.

### Stage B: adapter scaffolding and boundary-preserving model mapping

Introduce adapter-specific configuration loading types and mapping:

- Add OrthoConfig/CLI adapter structures for global options and subcommands.
- Keep domain `HarnessConfig` free from CLI/env parsing concerns.
- Add explicit conversion from adapter-loaded config to domain config, including
  validation context for record/replay/verify command expectations.

Go/no-go: compile with adapter scaffolding and keep existing domain tests green.

### Stage C: layered loading and subcommand merge implementation (green)

Implement layered loading with explicit precedence and subcommand merge:

- Load defaults, then configuration file layers, then env, then CLI.
- Apply `cmds.<subcommand>` overlays for the selected subcommand (`record`,
  `replay`, `verify`) beneath direct CLI flags.
- Wire binary subcommand execution to load effective config before delegating to
  existing library entrypoints.

Go/no-go: unit tests from Stage A pass and prove precedence plus merge
semantics.

### Stage D: behavioural coverage and unhappy paths

Complete BDD scenarios for user-visible behaviour:

- Happy path: each subcommand resolves merged values correctly from layered
  inputs.
- Unhappy paths: conflicting or malformed layered values produce deterministic,
  localized CLI errors.
- Help path: `--help` output clearly documents merged config shape, including
  `cmds.<subcommand>` sections.

Go/no-go: BDD scenarios pass, and failures are clear when scenarios are broken.

### Stage E: documentation and roadmap closure

Update and align docs with implemented behaviour:

- `docs/users-guide.md`: precedence model, merged shape examples, and subcommand
  override examples.
- `docs/spycatcher-harness-design.md`: append concrete design decisions made
  during implementation.
- `docs/roadmap.md`: mark task `1.1.2` done only after all quality gates pass.

Go/no-go: docs are consistent with CLI help and tests.

## Concrete steps

All commands run from repository root `/home/user/project`.

1. Capture baseline and create red tests.

```bash
set -o pipefail
make test 2>&1 | tee /tmp/1-1-2-baseline-test.log
```

Expected transcript excerpt:

```plaintext
... existing startup tests pass ...
... new precedence/subcommand tests fail (red) ...
```

1. Implement adapter loader and subcommand merge wiring.

```bash
set -o pipefail
cargo test --all-targets --all-features 2>&1 | tee /tmp/1-1-2-dev-test.log
```

Expected transcript excerpt:

```plaintext
... precedence tests now pass ...
... bdd scenarios for config layering pass ...
```

1. Run formatting and lint/test quality gates.

```bash
set -o pipefail
make fmt 2>&1 | tee /tmp/1-1-2-fmt.log
set -o pipefail
make check-fmt 2>&1 | tee /tmp/1-1-2-check-fmt.log
set -o pipefail
make lint 2>&1 | tee /tmp/1-1-2-lint.log
set -o pipefail
make test 2>&1 | tee /tmp/1-1-2-test.log
```

Expected transcript excerpt:

```plaintext
... check-fmt passes ...
... clippy/whitaker/doc pass with no warnings ...
... nextest and doctests pass ...
```

1. Validate documentation changes.

```bash
set -o pipefail
make markdownlint 2>&1 | tee /tmp/1-1-2-markdownlint.log
set -o pipefail
make nixie 2>&1 | tee /tmp/1-1-2-nixie.log
```

Expected transcript excerpt:

```plaintext
... markdownlint passes ...
... nixie passes ...
```

1. Final verification before closing the task.

```bash
git status --short
rg -n "1\.1\.2" docs/roadmap.md
```

Expected transcript excerpt:

```plaintext
M docs/roadmap.md
... shows item 1.1.2 marked [x] ...
```

## Validation and acceptance

Acceptance criteria mapped to roadmap 1.1.2:

- Precedence proof: unit tests assert
  `CLI > env > config files > defaults` for representative scalar, enum, and
  nested values.
- Subcommand merge proof: unit tests assert `cmds.record`, `cmds.replay`, and
  `cmds.verify` overlays for both override and pass-through cases.
- Behavioural proof: `rstest-bdd` scenarios assert user-visible outcomes for
  happy and unhappy layered inputs.
- Documentation proof: CLI `--help` and `docs/users-guide.md` both document
  merged configuration shape and subcommand override namespace.

Quality criteria:

- Tests: `make test` passes.
- Lint/docs/type quality: `make lint` passes.
- Formatting: `make fmt` then `make check-fmt` passes.
- Markdown/diagram validation: `make markdownlint` and `make nixie` pass.

## Idempotence and recovery

- The test and validation commands are safe to rerun.
- If formatting changes files unexpectedly, rerun `make fmt` followed by
  `make check-fmt`.
- If merge logic work-in-progress leaves partial failures, use red/green cycles
  by rerunning only targeted tests first, then full gates.
- Do not mark roadmap item `1.1.2` done until all gates pass in the same final
  revision.

## Artifacts and notes

Expected new or modified artifacts during implementation:

- Adapter-side configuration loader and mapper module(s).
- Unit test module(s) for layered precedence and `cmds.<subcommand>` merge
  semantics using `rstest` fixtures.
- Behavioural feature file and step definitions for configuration-loading
  behaviour using `rstest-bdd`.
- Updated docs in `docs/users-guide.md`, `docs/spycatcher-harness-design.md`,
  and `docs/roadmap.md`.

Record concise command transcripts in `/tmp/1-1-2-*.log` files as execution
proof.

## Interfaces and dependencies

Prescriptive interface outcomes for this milestone:

- A dedicated adapter loader function must exist that returns the selected
  subcommand plus effective domain config. Example target shape:

```rust
pub(crate) fn load_effective_command_config(
    argv: impl IntoIterator<Item = std::ffi::OsString>,
) -> eyre::Result<LoadedCommandConfig>
```

- A command-selection type must represent `record`, `replay`, and `verify`
  after merging. Example target shape:

```rust
pub(crate) enum LoadedCommandConfig {
    Record(spycatcher_harness::HarnessConfig),
    Replay(spycatcher_harness::HarnessConfig),
    Verify(spycatcher_harness::HarnessConfig),
}
```

- Mapping from adapter-level config DTOs into domain `HarnessConfig` must be
  explicit and test-covered.
- Dependencies should remain minimal. Expected additions are `ortho_config`
  and `serde` (derive support), plus only necessary test utilities.

## Revision note

Initial draft was created on 2026-03-03 for roadmap task 1.1.2.

Revision on 2026-03-03:

- Updated status to `COMPLETE`.
- Recorded implementation progress, discoveries, and final decisions.
- Replaced pending outcome text with delivered behaviour and validation
  evidence.
