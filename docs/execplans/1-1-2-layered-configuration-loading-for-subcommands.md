# Integrate layered configuration loading for subcommands (1.1.2)

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: DRAFT

## Purpose / big picture

Task 1.1.2 introduces real CLI configuration loading for the harness and
replaces the current placeholder binary behaviour (start with defaults, then
shutdown immediately). After this work, `record`, `replay`, and `verify`
subcommands all load configuration deterministically using OrthoConfig with the
required precedence:

1. CLI flags (highest)
2. Environment variables
3. Config files
4. Struct defaults (lowest)

Observable success means a developer can run subcommands with any combination
of defaults, TOML config, environment variables, and CLI flags and get stable,
test-proven merged results. Help output and user-facing docs will explain the
`cmds.<subcommand>` shape and precedence rules.

## Context and orientation

The current state after task 1.1.1 is:

- `src/bin/spycatcher_harness.rs` builds a Tokio runtime, uses
  `HarnessConfig::default()`, starts the harness, then shuts down.
- `src/config.rs` contains domain configuration types and defaults, but no
  `clap` or OrthoConfig integration yet.
- `tests/harness_startup_bdd.rs` covers startup/shutdown scenarios only.
- `docs/users-guide.md` explicitly says CLI parsing/subcommands are future work.
- `docs/roadmap.md` has task `1.1.2` unchecked.

Implementation must keep hexagonal boundaries clear:

- Domain logic remains in library domain types (`HarnessConfig` and related
  structs).
- CLI/config loading mechanics remain in adapter/application layers.
- Adapters map merged CLI/config data into domain types before invoking
  `start_harness`.

Reference documents used for this plan:

- `docs/roadmap.md`
- `docs/spycatcher-harness-design.md` (Configuration via OrthoConfig; CLI
  integration and configuration)
- `docs/ortho-config-users-guide.md`
- `docs/rust-testing-with-rstest-fixtures.md`
- `docs/reliable-testing-in-rust-via-dependency-injection.md`
- `docs/rust-doctest-dry-guide.md`
- `docs/rstest-bdd-users-guide.md`
- `docs/complexity-antipatterns-and-refactoring-strategies.md`

`docs/corbusier-design.md` is referenced in the request but is not present in
this repository. This is logged in `Surprises & Discoveries`.

## Constraints

- Preserve configuration precedence exactly as
  `CLI > env > config files > defaults`.
- Prove precedence and subcommand merge behaviour through automated tests.
- Cover both happy and unhappy paths via:
  - Unit tests using `rstest`
  - Behavioural tests using `rstest-bdd`
- Keep domain and adapters separated:
  - No direct `clap`/OrthoConfig concerns inside domain entities or domain
    invariants.
  - Mapping from adapter DTOs to domain configuration happens at the adapter
    boundary.
- Keep public library API typed; opaque errors stay at binary/app boundary.
- Keep module and Rustdoc standards from `AGENTS.md`:
  - `//!` module docs
  - `///` docs on public API
  - no file above 400 lines
- Avoid direct unguarded process environment mutation in tests.
- Update documentation:
  - `docs/users-guide.md` for user-visible behaviour
  - `docs/spycatcher-harness-design.md` for any new decisions
- On full implementation completion, mark roadmap item `1.1.2` done in
  `docs/roadmap.md`.

## Tolerances

- Scope tolerance: if implementation needs more than 15 files or roughly 1000
  net lines, pause and reassess decomposition before continuing.
- Dependency tolerance: if additional crates beyond expected CLI/config needs
  (`clap`, `serde`, `ortho_config`) are required, record rationale in decision
  log before adding.
- Behaviour tolerance: if `record`, `replay`, and `verify` require materially
  different configuration models than documented in the design, stop and update
  design decisions first.
- Quality tolerance: do not conclude work until all required gates pass:
  `make check-fmt`, `make lint`, `make test`, `make markdownlint`, `make nixie`.
- Architecture tolerance: if any domain module must import adapter framework
  types (`clap`, OrthoConfig CLI parsing machinery), stop and refactor at the
  boundary.

## Risks

- Risk: Subcommand merge API details in OrthoConfig may differ from design-time
  assumptions. Mitigation: implement a thin adapter module first and lock
  behaviour with red tests before wiring runtime flow.

- Risk: Testing env precedence can introduce flaky cross-test interference.
  Mitigation: prefer layer-composition tests; where live env is required, use
  guarded helpers with shared mutex in test utilities.

- Risk: CLI defaults can accidentally mask file/env values.
  Mitigation: use `Option<T>` command fields where appropriate and explicitly
  test unset-vs-set behaviour for every precedence layer.

- Risk: Help text drifts from actual merged configuration shape.
  Mitigation: add behavioural assertions against rendered help output and keep
  `docs/users-guide.md` examples aligned to tested config samples.

## Plan of work

### Milestone 0: Baseline and architecture seam

Create a small seam that isolates adapter concerns from domain types.

1. Add a configuration-loading adapter module (for example,
   `src/config/loading.rs` or an equivalent adapter-focused module) that owns:
   - CLI argument structs and subcommand enum
   - OrthoConfig derivations and merge calls
   - mapping functions into domain `HarnessConfig` and any subcommand-specific
     runtime config structs
2. Keep `src/config.rs` domain data definitions as framework-agnostic as
   possible.
3. Update binary entrypoint to delegate parse/load/dispatch instead of creating
   default config directly.

Acceptance checkpoint:

- Build passes with no behaviour change yet beyond code movement.
- `record`, `replay`, and `verify` command surfaces exist in CLI parsing.

### Milestone 1: Red tests for precedence and merges

Write failing tests first.

Unit tests (`rstest`) should cover:

1. Global precedence for a representative field:
   defaults < config file < env < CLI.
2. `cmds.record` merge behaviour (example: upstream base URL and cassette
   naming).
3. `cmds.replay` merge behaviour (example: replay timing fields).
4. `cmds.verify` merge behaviour (example: cassette selection and strictness
   toggles, based on implemented verify-args shape).
5. Explicit CLI override winning over `cmds.<subcommand>` and env values.
6. Unhappy paths:
   - malformed config file
   - invalid field value type in config/env
   - record mode missing required upstream after merge

Behavioural tests (`rstest-bdd`) should cover user-observable flows:

1. `record` resolves values from `[cmds.record]`.
2. `replay` resolves values from `[cmds.replay]`.
3. `verify` resolves values from `[cmds.verify]`.
4. Environment overrides config file defaults.
5. CLI flags override env/config.
6. `--help` output explains precedence and where subcommand defaults live.

Files likely added:

- `tests/features/config_layering.feature`
- `tests/config_layering_bdd.rs`

### Milestone 2: Implement deterministic layered loading

Implement real merge logic using OrthoConfig in the adapter module.

1. Define root CLI and subcommand structures with clear command-specific
   fields.
2. Use OrthoConfig merge facilities for subcommands (`cmds.<subcommand>`
   namespace and associated env prefixes).
3. Convert merged adapter structs into domain configuration structs.
4. Dispatch:
   - `record` and `replay` call library startup with merged domain config
   - `verify` loads merged config and routes to current verify execution path
     (placeholder or concrete, depending on current task boundary)

Acceptance checkpoint:

- All red tests from Milestone 1 now pass.
- Domain modules still do not depend on CLI framework concerns.

### Milestone 3: Help text and user documentation

Update user-visible documentation to match implemented behaviour.

1. Improve CLI help strings to document:
   - precedence order
   - `cmds.<subcommand>` configuration sections
   - expected environment naming pattern
2. Update `docs/users-guide.md` with:
   - subcommand list and usage examples
   - merged config file examples for `record`, `replay`, `verify`
   - precedence explanation with concrete examples
3. Update `docs/spycatcher-harness-design.md` implementation decisions section
   for any deviation or clarification taken during implementation.

Acceptance checkpoint:

- BDD help/documentation scenario passes.
- Docs reflect actual tested config shape and naming.

### Milestone 4: Quality gates and completion updates

Run full repository gates with captured logs, then mark roadmap completion.

Command pattern (required for long outputs):

```bash
set -o pipefail
make check-fmt 2>&1 | tee /tmp/1-1-2-check-fmt.log
make lint 2>&1 | tee /tmp/1-1-2-lint.log
make test 2>&1 | tee /tmp/1-1-2-test.log
make markdownlint 2>&1 | tee /tmp/1-1-2-markdownlint.log
make nixie 2>&1 | tee /tmp/1-1-2-nixie.log
```

After all gates pass:

1. Mark roadmap item `1.1.2` as done in `docs/roadmap.md`.
2. Update this ExecPlan status to `COMPLETE`.
3. Complete `Outcomes & Retrospective` with final evidence.

## Detailed test matrix

Unit tests (`rstest`) matrix:

1. Happy path:
   - config file only
   - env only
   - CLI only
   - each subcommand reading its `cmds.<subcommand>` block
2. Precedence assertions:
   - every layer combination proving final winner deterministically
3. Edge cases:
   - missing optional values
   - partially specified nested config where lower layers fill omissions
4. Unhappy path:
   - parse failures from invalid TOML
   - invalid env value coercion
   - semantic validation failures after merge

Behaviour tests (`rstest-bdd`) matrix:

1. User-level merge narrative (`Given` config, `When` command loads config,
   `Then` merged view reflects precedence).
2. Command-specific scenario for `record`, `replay`, and `verify`.
3. Help text scenario verifying precedence guidance appears.
4. Failure scenario with user-facing error for bad configuration source.

## Implementation file map (expected)

Expected touched files (exact names may vary slightly during implementation):

- `src/bin/spycatcher_harness.rs` (delegate to real CLI dispatch)
- `src/config.rs` (domain-preserving adjustments only, if required)
- `src/...` new adapter/config-loading module(s) with OrthoConfig logic
- `tests/features/config_layering.feature`
- `tests/config_layering_bdd.rs`
- existing/new unit test modules around configuration loading
- `docs/users-guide.md`
- `docs/spycatcher-harness-design.md`
- `docs/roadmap.md`
- this ExecPlan file

## Progress

- [x] 2026-03-03 00:00Z: Gathered roadmap, design, and testing references for
      task 1.1.2.
- [x] 2026-03-03 00:00Z: Drafted this ExecPlan in
      `docs/execplans/1-1-2-layered-configuration-loading-for-subcommands.md`.
- [ ] 2026-03-03 00:00Z: Implement Milestone 0 architecture seam.
- [ ] 2026-03-03 00:00Z: Add failing unit and BDD tests for precedence and
      subcommand merges.
- [ ] 2026-03-03 00:00Z: Implement merge logic and pass tests.
- [ ] 2026-03-03 00:00Z: Update CLI help, design decisions, and user guide.
- [ ] 2026-03-03 00:00Z: Run all quality gates and review logs.
- [ ] 2026-03-03 00:00Z: Mark roadmap item 1.1.2 done.

## Surprises & Discoveries

- `docs/corbusier-design.md` was requested as a reference but is not present in
  this repository. Planning therefore uses `docs/spycatcher-harness-design.md`
  as the authoritative design reference.
- Project memory MCP tools (`qdrant-find` / `qdrant-store`) are not available
  in the current tool listing, so project-memory retrieval could not be
  executed in this session.

## Decision Log

- Decision: keep adapter concerns (CLI parsing and OrthoConfig merge mechanics)
  out of domain core types wherever possible, with explicit mapping into domain
  config structs. Rationale: preserves the hexagonal dependency rule while
  still meeting roadmap requirements for layered CLI configuration loading.
  Date/Author: 2026-03-03 / agent.

- Decision: enforce precedence behaviour through both unit tests and
  behavioural tests before implementation completion. Rationale: roadmap
  success criteria explicitly require proof by tests and user-visible
  command/documentation alignment. Date/Author: 2026-03-03 / agent.

## Outcomes & Retrospective

Not yet executed. This section will be completed when implementation is done,
gates pass, and roadmap item 1.1.2 is marked complete.
