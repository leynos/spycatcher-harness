# Implement library and binary crate skeleton for harness startup

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

## Purpose / big picture

The Spycatcher harness project currently contains a single `src/main.rs` stub
that prints a placeholder message. Task 1.1.1 restructures this into a proper
library-plus-binary crate so that the public API contract for harness startup
and shutdown is established, compile-checked, and tested.

After this change a developer can:

- Import `spycatcher_harness` as a library and call `start_harness(cfg).await`
  to obtain a `RunningHarness`, then call `harness.shutdown().await` to tear it
  down.
- Run `cargo run --bin spycatcher-harness` to execute a CLI binary that
  delegates entirely to the library entry points.
- Run `cargo test --workspace --all-targets --all-features` and see baseline
  unit tests (via `rstest`) and behavioural tests (via `rstest-bdd`) for
  startup and shutdown passing.
- Inspect typed error enums (`HarnessError`) returned by the public API,
  without encountering opaque error types.

Observable success: `make all` passes cleanly (formatting, linting, tests, doc
generation).

## Constraints

Hard invariants that must hold throughout implementation. Violation requires
escalation, not workarounds.

- The library crate name must be `spycatcher_harness` (Cargo's automatic
  underscore conversion from `spycatcher-harness`). The binary must be named
  `spycatcher-harness`.
- Public library APIs must return `HarnessError` (a typed enum); opaque error
  types (`eyre::Report`, `anyhow::Error`) must not appear in the library's
  public surface.
- All Clippy lints defined in `Cargo.toml` must pass. No `#[allow]` attributes
  are permitted; use `#[expect(..., reason = "...")]` only as a last resort.
- Every public item must have `///` documentation. Every module must start with
  `//!` documentation. (`missing_docs = "deny"`,
  `missing_crate_level_docs = "deny"` are enforced.)
- Path fields must use `camino::Utf8PathBuf`, not `std::path::PathBuf`, per
  `AGENTS.md`.
- Dependencies must use caret requirements (e.g., `thiserror = "2.0.18"`).
  Wildcard and open-ended specifiers are forbidden.
- No single source file may exceed 400 lines.
- en-GB-oxendict spelling in all comments and documentation.
- The `Makefile` variable `TARGET ?= spycatcher-harness` must continue to
  resolve correctly after restructuring.

## Tolerances (exception triggers)

Thresholds that trigger escalation when breached.

- Scope: if implementation requires changes to more than 20 files or 1500
  lines of code (net), stop and escalate.
- Interface: the public API shape (`start_harness`, `RunningHarness::shutdown`,
  `HarnessError`, `HarnessConfig`) is defined by the design document. If it
  must change, stop and escalate.
- Dependencies: the plan adds `thiserror`, `tokio`, `camino` as runtime
  dependencies and `rstest`, `rstest-bdd`, `rstest-bdd-macros` as
  dev-dependencies. If additional dependencies are required, stop and escalate.
- Iterations: if `make all` still fails after 5 fix-up attempts, stop and
  escalate.
- Ambiguity: if multiple valid interpretations exist for a design element and
  the choice materially affects the public API, stop and present options.

## Risks

- Risk: `rstest-bdd` async scenario support may have rough edges with
  `tokio::test`. Severity: low Likelihood: low Mitigation: the user's guide
  documents the exact pattern (`#[tokio::test(flavor = "current_thread")]`
  before `#[scenario]`). Fall back to synchronous scenarios with `block_on` if
  async macro expansion fails.

- Risk: strict Clippy lints (`unused_async`, `must_use_candidate`,
  `missing_const_for_fn`) will flag the skeleton's placeholder implementations.
  Severity: low Likelihood: high Mitigation: use
  `#[expect(..., reason = "...")]` with clear justifications for each
  suppression. The skeleton's `shutdown()` is async by API contract but
  performs no async work yet.

- Risk: `camino` path types may conflict with design document code samples
  that show `std::path::PathBuf`. Severity: low Likelihood: medium Mitigation:
  this is a deliberate decision documented in the design doc.
  `camino::Utf8PathBuf` is the project standard per `AGENTS.md`.

## Progress

- [x] Write ExecPlan document.
- [x] Update `Cargo.toml` with lib/bin sections and dependencies.
- [x] Create `src/error.rs` with `HarnessError` and `HarnessResult`.
- [x] Create `src/config.rs` with `HarnessConfig` and sub-types.
- [x] Create stub modules (`i18n`, `protocol`, `cassette`, `server`,
  `upstream`, `replay`).
- [x] Create `src/lib.rs` with `start_harness`, `RunningHarness`, re-exports.
- [x] Create `src/bin/spycatcher_harness.rs` as binary entry point.
- [x] Delete `src/main.rs`.
- [x] Write unit tests in `src/error.rs`, `src/config.rs`, `src/lib.rs`.
- [x] Create `tests/features/harness_startup.feature` (BDD feature file).
- [x] Create `tests/harness_startup_bdd.rs` (BDD step definitions).
- [x] Run `make all` and fix any issues.
- [x] Record design decisions in `docs/spycatcher-harness-design.md`.
- [x] Create `docs/users-guide.md` with public API surface documentation.
- [x] Mark task 1.1.1 as done in `docs/roadmap.md`.

## Surprises & discoveries

- Observation: `allow-expect-in-tests = true` in `clippy.toml` only covers
  `#[cfg(test)]` modules, not integration test files under `tests/`. Evidence:
  Clippy raised `expect_used` errors in the BDD integration test file despite
  the clippy.toml setting. Impact: integration test files that use `expect()`
  need a crate-level `#[expect(clippy::expect_used)]` attribute.

- Observation: `rstest-bdd`'s `#[derive(ScenarioState)]` does not
  auto-derive `Default`; both `#[derive(Default, ScenarioState)]` are required.
  Evidence: compiler error "the trait `Default` is not implemented". Impact:
  future BDD world fixtures must derive both traits explicitly.

- Observation: `rstest-bdd` scenario function parameters must match
  fixture names exactly; the underscore-prefixed naming convention
  (`_harness_world`) breaks fixture resolution. Evidence: runtime panic
  "requires fixtures harness_world, but the following are missing:
  harness_world. Available fixtures: _harness_world". Impact: scenario binding
  functions must use the exact fixture name without underscore prefix, even if
  the value is unused in the body.

## Decision log

- Decision: use single package with `[lib]` + `[[bin]]` sections rather than a
  Cargo workspace. Rationale: the design document specifies
  `spycatcher_harness` (library) and `spycatcher-harness` (binary). A single
  package achieves this naturally via Cargo's hyphen-to-underscore convention.
  A workspace adds unnecessary `Cargo.toml` duplication and cross-crate wiring
  for a single logical unit. The roadmap shows no need for additional packages
  yet. Date/Author: 2026-03-01 / agent

- Decision: use `camino::Utf8PathBuf` for all path fields instead of
  `std::path::PathBuf`. Rationale: `AGENTS.md` explicitly mandates `camino`
  over `std::path`. The design document's pseudo-code shows `PathBuf` but
  `AGENTS.md` takes precedence as the binding coding standard. Using
  `Utf8PathBuf` from the start avoids a breaking change later. Date/Author:
  2026-03-01 / agent

- Decision: skeleton `start_harness` validates config and returns a
  `RunningHarness` with the configured listen address but does not bind an HTTP
  server. Rationale: HTTP server binding is task 1.3.1. The skeleton proves the
  API shape compiles, the error types work, and the startup/shutdown lifecycle
  is testable. The address in `RunningHarness` reflects the configured listen
  address, which will become the actual bound address once the server is
  implemented. Date/Author: 2026-03-01 / agent

- Decision: define the full `HarnessConfig` struct shape from the design
  document with `Default` implementations for all sub-types. Rationale: task
  1.1.2 (OrthoConfig integration) needs stable types to decorate with
  `clap`/OrthoConfig attributes. Defining the full shape now avoids a breaking
  restructuring in 1.1.2. Fields that are not exercised yet get sensible
  defaults. Date/Author: 2026-03-01 / agent

- Decision: use `Box<dyn std::error::Error>` as the binary's return type
  rather than `eyre::Report`. Rationale: adding `eyre` as a dependency is
  outside the planned dependency set for this task.
  `Box<dyn std::error::Error>` is idiomatic for the app boundary. Task 1.1.2
  will introduce `eyre` alongside OrthoConfig. Date/Author: 2026-03-01 / agent

## Outcomes & retrospective

All success criteria from the roadmap are met:

- `spycatcher_harness` exposes `start_harness(cfg)` and
  `RunningHarness::shutdown()` as compile-checked public APIs.
- Public library entry points return `HarnessResult<T>` backed by the
  typed `HarnessError` enum — no opaque error types.
- `spycatcher-harness` CLI binary delegates to library entry points.
- `cargo test --workspace --all-targets --all-features` passes with 21
  tests (17 unit, 4 BDD).
- Design decisions recorded in `docs/spycatcher-harness-design.md`.
- `docs/users-guide.md` created with public API surface documentation.
- Roadmap entry marked as done.

Key learnings:

1. Clippy's `allow-expect-in-tests` only covers `#[cfg(test)]` modules,
   not integration test files under `tests/`. Future BDD test files need
   crate-level `#[expect(clippy::expect_used)]`.
2. `rstest-bdd`'s `ScenarioState` derive does not imply `Default`;
   always derive both explicitly.
3. Scenario function parameter names must match fixture names exactly —
   no underscore prefixes.
4. `tokio::main` defaults to `multi_thread` flavour; when only `rt`
   (not `rt-multi-thread`) is enabled, specify
   `#[tokio::main(flavor = "current_thread")]` explicitly.

## Context and orientation

The Spycatcher harness is an LLM API recording and replay framework. The
project lives in a single Git repository with the following relevant structure:

```plaintext
Cargo.toml          — package "spycatcher-harness", edition 2024, no deps yet
Cargo.lock          — minimal lock file
clippy.toml         — strict thresholds (cognitive: 9, args: 4, lines: 70)
Makefile            — targets: all, build, test, lint, fmt, check-fmt
rust-toolchain.toml — nightly-2026-02-26
AGENTS.md           — coding standards and commit gating requirements
src/
  main.rs           — stub: println!("Hello from Spycatcher!")
docs/
  spycatcher-harness-design.md — full design document
  roadmap.md                   — implementation roadmap with task 1.1.1
  rstest-bdd-users-guide.md    — rstest-bdd usage guide
  ...                          — other reference documents
```

Key terms:

- **Harness**: the Spycatcher HTTP server that records LLM API interactions
  (record mode) or replays them deterministically (replay mode).
- **Cassette**: a recorded session file containing ordered request/response
  interactions.
- **`start_harness(cfg)`**: the library entry point that validates
  configuration and starts the harness.
- **`RunningHarness`**: a handle to a running harness instance, exposing the
  bound address and cassette path, with a `shutdown()` method.
- **`HarnessError`**: a typed error enum covering configuration errors,
  cassette errors, request mismatches, upstream failures, and I/O errors.
- **`HarnessConfig`**: the top-level configuration struct controlling listen
  address, mode, protocol, matching, cassette location, upstream settings,
  redaction, replay timing, and localisation.

The design document (`docs/spycatcher-harness-design.md`) defines the public
API surface at section "Public library API surface" and the module layout at
section "Crate layout". This ExecPlan implements the subset needed for task
1.1.1.

## Plan of work

### Stage A: scaffolding (Cargo.toml and file structure)

Update `Cargo.toml` to declare both a library target (`src/lib.rs`) and a
binary target (`src/bin/spycatcher_harness.rs`). Add runtime dependencies
(`thiserror`, `tokio`, `camino`) and dev-dependencies (`rstest`, `rstest-bdd`,
`rstest-bdd-macros`). Create the directory `src/bin/`.

Create all source files as stubs so the crate compiles: `src/lib.rs`,
`src/error.rs`, `src/config.rs`, the six stub modules, and
`src/bin/spycatcher_harness.rs`. Delete `src/main.rs`.

Validation: `cargo check --all-targets` passes.

### Stage B: error and config types

Populate `src/error.rs` with the `HarnessError` enum (five variants from the
design document) and the `HarnessResult<T>` type alias. All variants and fields
are documented with `///`.

Populate `src/config.rs` with all configuration types from the design:
`HarnessConfig`, `ListenAddr`, `Mode`, `Protocol`, `MatchMode`,
`UpstreamConfig`, `UpstreamKind`, `LocalizationConfig`, `RedactionConfig`,
`ReplayConfig`. All types derive `Debug` and `Clone`. `HarnessConfig` has a
`Default` implementation with sensible defaults (listen `127.0.0.1:8787`, mode
`Replay`, match mode `SequentialStrict`, protocol `OpenAiChatCompletions`,
cassette dir `fixtures/llm`, cassette name `default`).

Validation: `cargo check --all-targets` passes, `cargo doc --no-deps` builds
without warnings.

### Stage C: library API and binary

Populate `src/lib.rs` with:

- `pub mod` declarations for all submodules.
- Re-exports: `HarnessConfig`, `HarnessError`, `HarnessResult`.
- `pub struct RunningHarness` with `addr: SocketAddr` and
  `cassette_path: Utf8PathBuf`.
- `pub async fn start_harness(cfg: HarnessConfig) -> HarnessResult<RunningHarness>`
  that validates config (rejects empty cassette name) and returns a
  `RunningHarness`.
- `impl RunningHarness` with
  `pub async fn shutdown(self) -> HarnessResult<()>` (no-op returning `Ok(())`).
- A private `fn validate_config(cfg: &HarnessConfig) -> HarnessResult<()>`.

Populate `src/bin/spycatcher_harness.rs` with a `#[tokio::main] async fn main`
that constructs a default `HarnessConfig`, calls `start_harness`, and calls
`shutdown`. Return type is `Result<(), Box<dyn std::error::Error>>`.

Delete `src/main.rs`.

Validation: `cargo build --all-targets` passes,
`cargo run --bin spycatcher-harness` exits cleanly.

### Stage D: unit tests

Add `#[cfg(test)]` modules to `src/error.rs`, `src/config.rs`, and `src/lib.rs`
with `rstest`-based tests.

In `src/error.rs`:

- Parameterised test for all five error variant display strings using
  `#[rstest]` with `#[case]`.
- Test that `HarnessError` implements `std::error::Error`.

In `src/config.rs`:

- Test that `HarnessConfig::default()` produces a config with non-empty
  cassette name.
- Tests for default values of `ListenAddr`, `Mode`, `MatchMode`.

In `src/lib.rs`:

- Async test: `start_harness` with valid config returns `Ok`.
- Async test: `start_harness` with empty cassette name returns
  `Err(InvalidConfig)`.
- Async test: cassette path is `cassette_dir.join(cassette_name)`.
- Async test: `shutdown()` returns `Ok(())`.
- Async test: returned address matches configured listen address.

All async tests use `#[rstest]` combined with `#[tokio::test]`.

Validation: `cargo test --workspace --all-targets --all-features` passes.

### Stage E: BDD scenarios

Create `tests/features/harness_startup.feature` with four scenarios:

1. Start harness with valid configuration (happy path).
2. Start harness with empty cassette name fails (unhappy path).
3. Shutdown a running harness (happy path).
4. Start harness preserves configured listen address (edge case).

Create `tests/harness_startup_bdd.rs` with:

- A `HarnessWorld` struct using `Slot<T>` for `config`, `result`, and
  `shutdown_result` fields, deriving `ScenarioState`.
- An `rstest::fixture` producing `HarnessWorld::default()`.
- Step definitions (`#[given]`, `#[when]`, `#[then]`) implementing each
  Gherkin step.
- `#[scenario]` + `#[tokio::test(flavor = "current_thread")]` bindings for
  each scenario.

Validation: `cargo test --workspace --all-targets --all-features` passes,
including all BDD scenarios.

### Stage F: commit gating and documentation

Run `make all` (`check-fmt`, `lint`, `test`). Fix any remaining issues.

Update `docs/spycatcher-harness-design.md` to record design decisions (path
types, crate structure).

Create `docs/users-guide.md` documenting the public API surface.

Mark task 1.1.1 as done in `docs/roadmap.md`.

Validation: `make all` passes. All documentation builds cleanly.

## Concrete steps

All commands run from the repository root `/home/user/project`.

### 1. Update Cargo.toml

Add `[lib]` and `[[bin]]` sections. Add dependencies. The `[lints]` sections
remain unchanged.

### 2. Create source files

Create the following files (contents detailed in "Interfaces and dependencies"):

```plaintext
src/lib.rs
src/error.rs
src/config.rs
src/i18n.rs
src/protocol.rs
src/cassette.rs
src/server.rs
src/upstream.rs
src/replay.rs
src/bin/spycatcher_harness.rs
```

### 3. Delete src/main.rs

```sh
rm src/main.rs
```

### 4. Verify compilation

```sh
cargo check --all-targets
```

Expected: no errors or warnings.

### 5. Write unit tests

Add `#[cfg(test)]` modules to `src/error.rs`, `src/config.rs`, `src/lib.rs`.

### 6. Write BDD tests

Create:

```plaintext
tests/features/harness_startup.feature
tests/harness_startup_bdd.rs
```

### 7. Run full validation

```sh
set -o pipefail && make all 2>&1 | tee /tmp/make-all.log
```

Expected output (key lines):

```plaintext
test result: ok. <N> passed; 0 failed; ...
```

### 8. Update documentation

Edit `docs/spycatcher-harness-design.md`, create `docs/users-guide.md`, update
`docs/roadmap.md`.

## Validation and acceptance

Quality criteria (what "done" means):

- Tests: `cargo test --workspace --all-targets --all-features` passes with all
  unit tests and BDD scenarios green.
- Lint/typecheck: `cargo clippy --workspace --all-targets --all-features -- -D
  warnings` produces zero warnings. `cargo doc
  --no-deps` builds without warnings.
- Formatting: `cargo fmt --all -- --check` reports no differences.
- API surface: `start_harness(cfg)` and `RunningHarness::shutdown()` are
  callable from external code. `HarnessError` is the only error type in the
  public API.

Quality method:

```sh
make all
```

This single command runs `check-fmt`, `lint`, and `test` in sequence, covering
all quality criteria.

## Idempotence and recovery

Every step can be re-run safely. Source files are created via overwrite (not
append). `cargo check`, `cargo test`, and `make all` are idempotent. If a step
fails, fix the issue and re-run from that step.

The only destructive action is deleting `src/main.rs`. This is safe because the
file contains only the placeholder stub and its replacement
(`src/bin/spycatcher_harness.rs`) is created before deletion.

## Artifacts and notes

### Error enum shape

```rust
#[derive(Debug, Error)]
pub enum HarnessError {
    #[error("invalid configuration: {message}")]
    InvalidConfig { message: String },
    #[error("cassette not found: {cassette_name}")]
    CassetteNotFound { cassette_name: String },
    #[error("request mismatch at interaction {interaction_id}")]
    RequestMismatch { interaction_id: usize },
    #[error("upstream request failed")]
    UpstreamRequestFailed,
    #[error("io failure")]
    Io,
}
```

### Public API signatures

```rust
pub type HarnessResult<T> = std::result::Result<T, HarnessError>;

pub async fn start_harness(
    cfg: HarnessConfig,
) -> HarnessResult<RunningHarness>;

impl RunningHarness {
    pub async fn shutdown(self) -> HarnessResult<()>;
}
```

### BDD feature file

```gherkin
Feature: Harness startup and shutdown

  Scenario: Start harness with valid configuration
    Given a valid harness configuration
    When the harness is started
    Then the harness is running
    And the cassette path matches the configured directory and name

  Scenario: Start harness with empty cassette name fails
    Given a harness configuration with an empty cassette name
    When the harness is started
    Then the startup fails with an invalid configuration error
    And the error message mentions the cassette name

  Scenario: Shutdown a running harness
    Given a valid harness configuration
    And the harness has been started
    When the harness is shut down
    Then the shutdown succeeds

  Scenario: Start harness preserves listen address
    Given a harness configuration with listen address 127.0.0.1:9090
    When the harness is started
    Then the harness address is 127.0.0.1:9090
```

## Interfaces and dependencies

### Runtime dependencies

- `thiserror = "2.0.18"` — derive `std::error::Error` for `HarnessError`.
- `tokio = { version = "1.43.0", features = ["rt", "macros", "net"] }` — async
  runtime for `start_harness` and `shutdown`.
- `camino = "1.1.9"` — `Utf8PathBuf` for path fields per `AGENTS.md`.

### Dev-dependencies

- `rstest = "0.25.0"` — fixtures and parameterised tests.
- `rstest-bdd = "0.1.0"` — BDD scenario framework (`Slot`, `ScenarioState`).
- `rstest-bdd-macros = "0.1.0"` — procedural macros (`#[given]`, `#[when]`,
  `#[then]`, `#[scenario]`, `ScenarioState`).

### Library API surface (src/lib.rs)

```rust
pub use config::HarnessConfig;
pub use error::{HarnessError, HarnessResult};

pub struct RunningHarness {
    pub addr: SocketAddr,
    pub cassette_path: Utf8PathBuf,
}

impl RunningHarness {
    pub async fn shutdown(self) -> HarnessResult<()>;
}

pub async fn start_harness(
    cfg: HarnessConfig,
) -> HarnessResult<RunningHarness>;
```

### Configuration types (src/config.rs)

```rust
pub struct HarnessConfig {
    pub listen: ListenAddr,
    pub mode: Mode,
    pub localization: LocalizationConfig,
    pub protocol: Protocol,
    pub match_mode: MatchMode,
    pub cassette_dir: Utf8PathBuf,
    pub cassette_name: String,
    pub upstream: Option<UpstreamConfig>,
    pub redaction: RedactionConfig,
    pub replay: ReplayConfig,
}

pub struct ListenAddr(SocketAddr);
pub enum Mode { Record, Replay }
pub enum Protocol { OpenAiChatCompletions }
pub enum MatchMode { SequentialStrict, Keyed }

pub struct UpstreamConfig {
    pub kind: UpstreamKind,
    pub base_url: String,
    pub api_key_env: String,
    pub extra_headers: BTreeMap<String, String>,
}

pub enum UpstreamKind { OpenRouter }

pub struct LocalizationConfig {
    pub locale: Option<String>,
    pub fallback_locale: String,
}

pub struct RedactionConfig {
    pub drop_headers: Vec<String>,
}

pub struct ReplayConfig {
    pub simulate_timing: bool,
    pub ttft_ms: u64,
    pub tps: u64,
}
```

### Binary entry point (src/bin/spycatcher_harness.rs)

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = HarnessConfig::default();
    let harness = start_harness(cfg).await?;
    harness.shutdown().await?;
    Ok(())
}
```
