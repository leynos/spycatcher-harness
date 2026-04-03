# Deliver strict sequential and keyed replay matching modes

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: DRAFT

## Purpose / big picture

Task `1.2.3` delivers the two replay matching modes described in the design
document: **sequential strict** (the default) and **keyed**. After this change,
the replay engine can accept an incoming canonicalized request and decide which
recorded interaction to serve, or reject the request with structured
diagnostics.

Observable success after delivery:

- A `ReplayMatchEngine` (or equivalent domain type) is constructed from a
  loaded `Cassette` and a `MatchMode`. Callers call a `next_match` method with
  the canonical request hash and receive either the matched `Interaction` or a
  structured `MismatchDiagnostic`.
- In **sequential strict mode**, the engine serves interactions in recorded
  order. When the observed request hash does not match the next expected
  interaction's stable hash, the engine returns a `MismatchDiagnostic`
  containing the expected interaction ID (zero-based index), the observed
  request hash, the expected request hash, and a field-level diff summary of
  the two canonical request JSON values. The HTTP adapter (a future task) will
  map this to HTTP 409; the domain layer itself does not produce HTTP responses.
- In **keyed mode**, the engine finds the next unused interaction whose
  `stable_hash` matches the observed request hash and returns it. When no
  unused interaction has the matching hash, the engine returns an appropriate
  diagnostic.
- Unit tests (`rstest`) cover: sequential happy path, sequential mismatch with
  diagnostic content, keyed happy path, keyed with duplicate hashes consuming
  in order, keyed miss (no matching hash), exhausted cassette, and concurrent
  replay order handling.
- Behavioural tests (`rstest-bdd`) cover: sequential mismatch diagnostics
  scenario, keyed out-of-order consumption scenario, and exhausted-cassette
  failure.
- `docs/spycatcher-harness-design.md` records any implementation decisions.
- `docs/users-guide.md` documents the matching modes and the diagnostic shape.
- `docs/roadmap.md` marks task `1.2.3` as done only after all gates pass.

## Constraints

- Maintain hexagonal architecture boundaries:
  - The replay matching engine is **domain logic** and belongs in the
    `src/cassette/` module tree (or a new `src/replay/` domain submodule — see
    Decision Log). It must not depend on HTTP, filesystem, CLI, or
    adapter-layer types.
  - The `MismatchDiagnostic` type is a domain type (a structured value
    carrying expected vs observed hashes and a diff summary). It does not
    reference HTTP status codes.
  - The HTTP adapter layer (task 1.3.2) will later map `MismatchDiagnostic` to
    HTTP 409 responses; this task does not implement HTTP handling.
- Preserve the existing public API surface:
  - `start_harness`, `RunningHarness::shutdown`, `HarnessConfig`, and all
    existing `HarnessError` variants must remain source-compatible.
  - The existing `HarnessError::RequestMismatch` variant has only
    `interaction_id: usize`. This task enriches the variant to also carry
    `expected_hash`, `observed_hash`, and `diff_summary` fields so the adapter
    layer has all the data it needs to build a 409 response body. This is an
    additive change to the variant's fields; existing `matches!()` patterns on
    `HarnessError::RequestMismatch { .. }` remain compatible.
- The `MatchMode` enum (`SequentialStrict`, `Keyed`) already exists in
  `src/config.rs` and must not be duplicated or restructured.
- Tests must use `rstest` for unit coverage and `rstest-bdd` for behavioural
  coverage from the domain boundary.
- Filesystem access must use `cap-std`/`camino`. No `std::fs` or `std::path`.
- Comments and documentation must use en-GB-oxendict spelling.
- No single source file may exceed 400 lines.
- Before completion, run the full commit gates: `make fmt`, `make check-fmt`,
  `make lint`, `make test`, `make markdownlint`, and `make nixie`.

## Tolerances (exception triggers)

- Scope: if implementation requires changes to more than 16 files or 1200 net
  lines, stop and escalate.
- Interface: if satisfying this task requires changing the signature of
  `start_harness` or `RunningHarness::shutdown`, stop and escalate.
- Dependencies: if a new external crate dependency is required, stop and
  escalate. All necessary primitives (`serde_json::Value` diff, `HashMap`,
  `sha2`) are already available.
- Iteration: if `make lint` or `make test` still fails after five repair
  cycles, stop and escalate with the failing evidence.
- Ambiguity: if the diff summary format or the diagnostic type shape has
  multiple reasonable interpretations, stop and present options before writing
  more code.

## Risks

- Risk: the `HarnessError::RequestMismatch` variant currently has a single
  `interaction_id: usize` field. Adding new fields is a backwards-compatible
  change for pattern matches using `{ .. }`, but any test or code matching the
  variant with all fields named will need updating.
  Severity: low. Likelihood: medium. Mitigation: search for all existing
  uses of `RequestMismatch` and update them when the variant is enriched.

- Risk: generating a useful "field-level diff summary" of two
  `serde_json::Value` canonical requests without pulling in a heavy diff
  library. Severity: medium. Likelihood: low. Mitigation: implement a
  minimal JSON value diff that walks two `Value` trees recursively and reports
  keys that are added, removed, or changed. This is a domain utility, not a
  general-purpose diff engine.

- Risk: keyed mode with concurrent access could require interior mutability
  or synchronization. Severity: low. Likelihood: medium. Mitigation: the
  engine is designed as a single-owner mutable type (`&mut self`); the adapter
  layer that later wraps it in `Arc<Mutex<_>>` or similar is out of scope for
  this task.

- Risk: the 400-line file limit may be tight for a module containing the
  matching engine, diff logic, and tests. Severity: low. Likelihood: medium.
  Mitigation: split matching logic, diff logic, and tests into separate files
  within the module tree (for example `src/cassette/matching.rs`,
  `src/cassette/matching/diff.rs`, `src/cassette/matching_tests.rs`).

## Progress

- [ ] Drafted ExecPlan for roadmap task `1.2.3`.
- [ ] Enriched `HarnessError::RequestMismatch` with diagnostic fields.
- [ ] Implemented `MismatchDiagnostic` domain type and field-level canonical
      JSON diff.
- [ ] Implemented `ReplayMatchEngine` with sequential strict mode.
- [ ] Implemented `ReplayMatchEngine` with keyed mode.
- [ ] Added unit tests for both modes, covering happy and unhappy paths.
- [ ] Added BDD feature file and step definitions for matching mode scenarios.
- [ ] Updated design document, user guide, and roadmap.
- [ ] Ran full validation gates and all passed.

## Surprises & discoveries

(None yet — to be populated during implementation.)

## Decision log

(To be populated during implementation.)

## Outcomes & retrospective

(To be populated on completion.)

## Context and orientation

### Repository layout relevant to this task

The repository is a single Rust package (edition 2024, Rust 1.88) with a
library crate (`src/lib.rs`) and a binary target
(`src/bin/spycatcher_harness.rs`).

Key files and their roles:

- `src/cassette/mod.rs` (295 lines) — defines the cassette domain model:
  `Cassette`, `CassetteFormatVersion`, `Interaction`, `RecordedRequest`,
  `RecordedResponse`, `StreamEvent`, `StreamTiming`, `InteractionMetadata`,
  and the `CassetteReader`/`CassetteAppender` trait ports. Re-exports
  canonical types from the `canonical` submodule.

- `src/cassette/canonical/mod.rs` (224 lines) — pure domain logic for
  request canonicalization and SHA-256 hashing. Exports `CanonicalRequest`,
  `IgnorePathConfig`, `CanonicalError`, `canonicalize`, `stable_hash`, and
  `RecordedRequest::populate_canonical_fields`.

- `src/cassette/filesystem.rs` (351 lines) — filesystem adapter implementing
  `CassetteReader` and `CassetteAppender` via `FilesystemCassetteStore`.

- `src/config.rs` (272 lines) — `HarnessConfig` and constituent types
  including `MatchMode` (enum with `SequentialStrict` and `Keyed` variants).

- `src/error.rs` (108 lines) — `HarnessError` enum with the existing
  `RequestMismatch { interaction_id: usize }` variant and `HarnessResult<T>`
  alias.

- `src/replay.rs` (6 lines) — placeholder module-level doc comment only. No
  implementation yet.

- `src/lib.rs` (367 lines) — library entry point; `start_harness`,
  `validate_config`, `prepare_cassette`, `RunningHarness`.

- `tests/` — integration and BDD tests using `rstest-bdd`. Existing BDD test
  files follow a consistent pattern: a `ScenarioState` struct, `#[fixture]`
  for the world, step definitions as `#[given]`/`#[when]`/`#[then]` functions,
  and `#[scenario]` bindings referencing a `.feature` file.

- `tests/support/bdd_fixtures.rs` — shared `unique_cassette_name` helper.

- `tests/support/test_utils.rs` — `build_runtime()` helper for synchronous
  BDD steps.

### Key terms

- **Sequential strict mode**: the default replay matching strategy. The engine
  maintains a cursor over the cassette's `interactions` vector. Each incoming
  request is expected to match the interaction at the current cursor position.
  A mismatch produces a `MismatchDiagnostic` (which the HTTP adapter will later
  render as HTTP 409). On match, the cursor advances.

- **Keyed mode**: an alternative replay matching strategy. The engine maintains
  a set of unconsumed interaction indices, indexed by `stable_hash`. Each
  incoming request is matched by its hash against the next unconsumed
  interaction with that hash. On match, the interaction is marked consumed and
  returned. When no unconsumed interaction matches, a diagnostic is produced.

- **`MismatchDiagnostic`**: a domain value type carrying the expected
  interaction ID, expected hash, observed hash, and a field-level diff summary
  comparing two canonical request JSON values. This is the structured
  diagnostic that the adapter layer maps to HTTP 409.

- **Field-level diff summary**: a concise representation of differences between
  two `serde_json::Value` canonical requests. Reports keys that were added,
  removed, or whose values changed. This is a domain utility for diagnostics,
  not a general-purpose JSON diff.

### Design document references

The design document at `docs/spycatcher-harness-design.md`, section "Matching
modes" (lines 210–230), specifies:

- Sequential strict mode expects the next incoming request to match the next
  recorded interaction. Any mismatch fails fast with a diagnostic response
  (409) containing expected interaction ID, observed request hash, and a diff
  summary of canonical request JSON.
- Keyed mode matches by request hash and consumes the next unused interaction
  with that hash. Supports limited reordering and concurrent requests.

## Plan of work

### Stage A: enrich `HarnessError::RequestMismatch` (no new logic)

Extend the `HarnessError::RequestMismatch` variant in `src/error.rs` to
carry the diagnostic fields needed by the matching engine:

```rust
RequestMismatch {
    /// Zero-based index of the expected interaction.
    interaction_id: usize,
    /// Stable hash of the expected canonical request.
    expected_hash: String,
    /// Stable hash of the observed incoming request.
    observed_hash: String,
    /// Field-level diff summary of expected vs observed canonical JSON.
    diff_summary: String,
}
```

Update the `Display` implementation (via `thiserror`) to include the new
fields in a human-readable message. Update the existing test case in
`src/error.rs::tests` that constructs `RequestMismatch` to include the new
fields. Search the entire codebase for any other references to
`RequestMismatch` and update them.

Go/no-go: `make test` passes. Existing tests remain green.

### Stage B: implement the field-level canonical JSON diff utility

Create a new file `src/cassette/diff.rs` containing a pure function:

```rust
/// Produces a human-readable field-level diff summary comparing two
/// canonical request JSON values.
///
/// Reports keys that are present in only one value (added/removed) and
/// keys whose values differ (changed). Nested objects are compared
/// recursively with dotted path notation.
pub(crate) fn canonical_diff_summary(
    expected: &serde_json::Value,
    observed: &serde_json::Value,
) -> String
```

The diff walks two `serde_json::Value` trees:

- For two objects: iterate the union of keys. Report keys present only in
  `expected` as "removed", keys present only in `observed` as "added", and
  keys present in both with different values as "changed" (recursing into
  nested objects).
- For two arrays: compare element-by-element, reporting index-level changes.
- For scalar mismatches: report the path and the two differing values.
- For type mismatches: report the path and the two types.

The output format is a newline-separated list of change descriptions, for
example:

```plaintext
changed: method: "POST" -> "GET"
removed: canonical_body.metadata.run_id
added: canonical_body.extra_field: "value"
changed: canonical_query: "a=1&b=2" -> "a=1&c=3"
```

This file should remain under 200 lines. Register it in
`src/cassette/mod.rs` as `mod diff;` (private).

Add unit tests for the diff utility in `src/cassette/diff_tests.rs`:

- Two identical values produce an empty summary.
- An added top-level key is reported.
- A removed top-level key is reported.
- A changed scalar value is reported with both old and new values.
- Nested object differences use dotted path notation.
- Type mismatches are reported.

Go/no-go: `make test` passes with new diff tests green.

### Stage C: implement `ReplayMatchEngine` with sequential strict mode

Create a new file `src/cassette/matching.rs` containing the core replay
matching engine. This is domain logic with no adapter dependencies.

```rust
use crate::cassette::{Cassette, Interaction, CanonicalRequest, stable_hash};
use crate::cassette::canonical::canonical_request_value;
use crate::cassette::diff::canonical_diff_summary;
use crate::config::MatchMode;

/// Structured diagnostic for a replay mismatch.
///
/// Carries all information needed by the adapter layer to build an
/// HTTP 409 response body without coupling the domain to HTTP types.
#[derive(Debug, Clone, PartialEq)]
pub struct MismatchDiagnostic {
    /// Zero-based index of the expected interaction.
    pub interaction_id: usize,
    /// Stable hash of the expected canonical request.
    pub expected_hash: String,
    /// Stable hash of the observed incoming request.
    pub observed_hash: String,
    /// Field-level diff summary of canonical request JSON.
    pub diff_summary: String,
}

/// Outcome of a replay match attempt.
#[derive(Debug)]
pub enum MatchOutcome<'a> {
    /// The incoming request matched a recorded interaction.
    Matched(&'a Interaction),
    /// No match was found; diagnostics explain why.
    Mismatch(MismatchDiagnostic),
}

/// Replay matching engine that consumes cassette interactions
/// according to the configured match mode.
pub struct ReplayMatchEngine {
    // Internal state varies by mode — see implementation.
}

impl ReplayMatchEngine {
    /// Creates a new engine from a loaded cassette and match mode.
    pub fn new(cassette: &Cassette, match_mode: MatchMode) -> Self;

    /// Attempts to match an incoming request against the cassette.
    ///
    /// In sequential strict mode, the request must match the next
    /// recorded interaction in order. In keyed mode, the request
    /// matches the next unconsumed interaction with the same hash.
    pub fn next_match(
        &mut self,
        observed_hash: &str,
        observed_canonical: &CanonicalRequest,
    ) -> MatchOutcome<'_>;
}
```

The engine stores a reference to the cassette's interactions (or an owned
copy of the data it needs — stable hashes and canonical request values — to
avoid lifetime entanglement). Internal state:

- **Sequential strict**: a `cursor: usize` tracking the next expected
  interaction index.
- **Keyed**: a `Vec<bool>` or `BitVec` tracking consumed interactions, plus
  a `HashMap<String, Vec<usize>>` mapping stable hashes to interaction
  indices for efficient lookup.

For sequential strict `next_match`:

1. If `cursor >= interactions.len()`, return a `Mismatch` diagnostic
   indicating the cassette is exhausted.
2. Retrieve the expected interaction at `interactions[cursor]`.
3. Compare `observed_hash` against the expected interaction's `stable_hash`.
4. On match: advance `cursor`, return `Matched(&interaction)`.
5. On mismatch: build a `MismatchDiagnostic` with the expected interaction
   ID (`cursor`), expected hash, observed hash, and a field-level diff
   summary by calling `canonical_diff_summary` on the canonical request
   JSON values. Return `Mismatch(diagnostic)`.

Register `matching.rs` in `src/cassette/mod.rs` as `pub(crate) mod matching;`
and re-export `MismatchDiagnostic`, `MatchOutcome`, and `ReplayMatchEngine`
from `src/cassette/mod.rs`.

Go/no-go: the module compiles (`cargo check`). No tests yet.

### Stage D: implement keyed mode in `ReplayMatchEngine`

Extend the `ReplayMatchEngine::next_match` implementation to handle
`MatchMode::Keyed`:

1. Look up the observed hash in the hash-to-indices map.
2. Find the first unconsumed index in the list for that hash.
3. On match: mark the index as consumed, return `Matched(&interaction)`.
4. On miss (no unconsumed interaction with that hash): return a
   `Mismatch` diagnostic. In keyed mode the "expected interaction ID" in the
   diagnostic is set to the total number of interactions (indicating no
   specific expected position), and the diff summary reports that no
   interaction with the given hash was found.

Go/no-go: `cargo check` passes. Proceed to tests.

### Stage E: add unit tests (red then green)

Add unit tests in `src/cassette/matching_tests.rs` using `rstest`. The test
file is registered in `src/cassette/mod.rs` as
`#[cfg(test)] mod matching_tests;`.

Helper fixtures:

- `sample_cassette()` — a `Cassette` with three interactions whose stable
  hashes are `"hash_a"`, `"hash_b"`, `"hash_c"` (predetermined for
  testability).
- `duplicate_hash_cassette()` — a `Cassette` with two interactions sharing
  hash `"hash_a"` and one with `"hash_b"`.
- `canonical_for_hash(hash: &str)` — a minimal `CanonicalRequest` that
  produces the given hash when passed to `stable_hash`. (Alternatively, the
  engine can accept pre-computed hashes directly, avoiding the need to
  reverse-engineer inputs that produce specific hashes.)

Design decision: the engine accepts the observed hash as a pre-computed
`&str` rather than computing it internally. This makes testing
straightforward and keeps hash computation as a separate concern (already
implemented in the canonical module).

Test cases for sequential strict mode:

1. Three requests with correct hashes in order all return `Matched` and
   return the correct interaction for each position.
2. First request with wrong hash returns `Mismatch` with `interaction_id: 0`,
   the expected hash, the observed hash, and a non-empty diff summary.
3. First correct, second wrong returns `Mismatch` with `interaction_id: 1`.
4. All three consumed, a fourth request returns `Mismatch` indicating
   cassette exhaustion.

Test cases for keyed mode:

1. Three requests with correct hashes in recorded order all return `Matched`.
2. Three requests with correct hashes in reversed order all return `Matched`
   (keyed mode permits reordering).
3. Two interactions with the same hash: the first request consumes the first
   occurrence, the second request consumes the second occurrence.
4. A request with a hash not present in the cassette returns `Mismatch`.
5. After all interactions are consumed, any request returns `Mismatch`
   indicating exhaustion.

Test cases for diagnostic content:

1. Sequential mismatch diagnostic contains the expected interaction ID.
2. Sequential mismatch diagnostic contains both expected and observed hashes.
3. Sequential mismatch diagnostic diff summary mentions the field that
   differs.

Go/no-go: `make test` passes with all new tests green. Existing tests
remain green.

### Stage F: add BDD behavioural tests

Create a Gherkin feature file at
`tests/features/replay_matching_modes.feature`:

```gherkin
Feature: Replay matching modes

  Scenario: Sequential strict mode serves interactions in order
    Given a cassette with three recorded interactions
    And the replay engine is in sequential strict mode
    When three requests arrive with matching hashes in recorded order
    Then all three requests receive the corresponding recorded interaction

  Scenario: Sequential strict mode rejects a mismatched request
    Given a cassette with three recorded interactions
    And the replay engine is in sequential strict mode
    When a request arrives with a hash that does not match the next interaction
    Then the engine returns a mismatch diagnostic
    And the diagnostic contains the expected interaction ID
    And the diagnostic contains the expected and observed hashes
    And the diagnostic contains a field-level diff summary

  Scenario: Keyed mode permits out-of-order requests
    Given a cassette with three recorded interactions with distinct hashes
    And the replay engine is in keyed mode
    When three requests arrive with matching hashes in reversed order
    Then all three requests receive the corresponding recorded interaction

  Scenario: Keyed mode consumes duplicate hashes in recorded order
    Given a cassette with two interactions sharing the same hash
    And the replay engine is in keyed mode
    When two requests arrive with the shared hash
    Then the first request receives the first recorded interaction
    And the second request receives the second recorded interaction

  Scenario: Replay engine rejects requests after cassette exhaustion
    Given a cassette with one recorded interaction
    And the replay engine is in sequential strict mode
    When the first request matches and consumes the interaction
    And a second request arrives
    Then the engine returns a mismatch diagnostic indicating exhaustion
```

Create the BDD step definitions at
`tests/replay_matching_modes_bdd.rs`. The test driver constructs
cassettes with known interactions, creates a `ReplayMatchEngine`, and
exercises it through the step definitions.

Go/no-go: `make test` passes with all BDD scenarios green.

### Stage G: update documentation

Update `docs/spycatcher-harness-design.md`:

- In the "Matching modes" section, record any implementation decisions:
  - The `ReplayMatchEngine` type and its `next_match` API.
  - The `MismatchDiagnostic` domain type and its field-level diff summary
    format.
  - The decision to keep the matching engine as a domain type that the adapter
    layer wraps.

Update `docs/users-guide.md`:

- Add or expand the "Replay matching modes" section describing:
  - Sequential strict mode: default, requests must arrive in recorded order,
    mismatches produce a diagnostic with interaction ID, expected/observed
    hashes, and field-level diff.
  - Keyed mode: matches by request hash, permits reordering, consumes
    interactions in recorded order for duplicate hashes.
  - Configuration: the `match_mode` field on `HarnessConfig` selects the
    mode (`SequentialStrict` or `Keyed`).

Update `docs/roadmap.md`:

- Mark task `1.2.3` as done (change `- [ ]` to `- [x]` for the task and its
  sub-criteria).

Go/no-go: documentation is complete and accurate.

### Stage H: run full validation with logged evidence

Run every required gate through `tee` with `set -o pipefail` so truncated
terminal output does not hide failures:

```bash
set -o pipefail
make fmt 2>&1 | tee /tmp/1-2-3-fmt.log
make check-fmt 2>&1 | tee /tmp/1-2-3-check-fmt.log
make lint 2>&1 | tee /tmp/1-2-3-lint.log
make test 2>&1 | tee /tmp/1-2-3-test.log
make markdownlint 2>&1 | tee /tmp/1-2-3-markdownlint.log
make nixie 2>&1 | tee /tmp/1-2-3-nixie.log
```

Expected end state:

- `make lint` finishes without Clippy, Rustdoc, or Whitaker warnings.
- `make test` passes all unit tests, behavioural tests, and doctests.
- Markdown validation passes after documentation updates.
- The roadmap item is checked off only after every command above exits zero.

## Concrete steps

All commands are run from the repository root (`/home/user/project`).

Stage A:

```bash
# Edit src/error.rs to enrich RequestMismatch
# Search for all existing references to RequestMismatch
set -o pipefail
cargo check 2>&1 | tee /tmp/1-2-3-check-a.log
make test 2>&1 | tee /tmp/1-2-3-test-a.log
```

Stage B:

```bash
# Create src/cassette/diff.rs and src/cassette/diff_tests.rs
set -o pipefail
cargo check 2>&1 | tee /tmp/1-2-3-check-b.log
make test 2>&1 | tee /tmp/1-2-3-test-b.log
```

Stage C and D:

```bash
# Create src/cassette/matching.rs
set -o pipefail
cargo check 2>&1 | tee /tmp/1-2-3-check-cd.log
```

Stage E:

```bash
# Create src/cassette/matching_tests.rs
set -o pipefail
make test 2>&1 | tee /tmp/1-2-3-test-e.log
```

Stage F:

```bash
# Create tests/features/replay_matching_modes.feature
# Create tests/replay_matching_modes_bdd.rs
set -o pipefail
make test 2>&1 | tee /tmp/1-2-3-test-f.log
```

Stage G:

```bash
# Update docs/spycatcher-harness-design.md
# Update docs/users-guide.md
# Update docs/roadmap.md
set -o pipefail
make markdownlint 2>&1 | tee /tmp/1-2-3-markdownlint-g.log
make nixie 2>&1 | tee /tmp/1-2-3-nixie-g.log
```

Stage H:

```bash
set -o pipefail
make fmt 2>&1 | tee /tmp/1-2-3-fmt.log
make check-fmt 2>&1 | tee /tmp/1-2-3-check-fmt.log
make lint 2>&1 | tee /tmp/1-2-3-lint.log
make test 2>&1 | tee /tmp/1-2-3-test.log
make markdownlint 2>&1 | tee /tmp/1-2-3-markdownlint.log
make nixie 2>&1 | tee /tmp/1-2-3-nixie.log
```

## Validation and acceptance

Quality criteria (what "done" means):

- Tests: `make test` passes all unit, BDD, and doc tests.
  - The new `matching_tests` module contains at least 12 test cases covering
    sequential strict mode (happy path, mismatch, exhaustion), keyed mode
    (happy path, reordering, duplicate hashes, miss, exhaustion), and
    diagnostic content.
  - The new `diff_tests` module contains at least 6 test cases covering
    identical values, added keys, removed keys, changed values, nested objects,
    and type mismatches.
  - BDD scenarios in `tests/replay_matching_modes_bdd.rs` cover sequential
    mismatch diagnostics, keyed out-of-order consumption, duplicate-hash
    consumption, and cassette exhaustion.
- Lint: `make lint` passes without warnings.
- Format: `make check-fmt` passes.
- Docs: `make markdownlint` and `make nixie` pass.
- Roadmap: task `1.2.3` is marked done in `docs/roadmap.md`.

Quality method (how to check):

```bash
set -o pipefail
make fmt 2>&1 | tee /tmp/1-2-3-fmt.log
make check-fmt 2>&1 | tee /tmp/1-2-3-check-fmt.log
make lint 2>&1 | tee /tmp/1-2-3-lint.log
make test 2>&1 | tee /tmp/1-2-3-test.log
make markdownlint 2>&1 | tee /tmp/1-2-3-markdownlint.log
make nixie 2>&1 | tee /tmp/1-2-3-nixie.log
```

## Idempotence and recovery

All stages are safe to repeat. The matching engine module is additive; it
does not modify existing types beyond the `HarnessError::RequestMismatch`
enrichment (which is backwards-compatible). If a stage fails partway through,
re-running from that stage's starting point is safe.

Test fixtures use deterministic, hardcoded cassettes with known hashes,
avoiding cross-run interference.

## Artifacts and notes

### `MismatchDiagnostic` field specification

```plaintext
interaction_id: usize    — zero-based index of the expected interaction
                           (sequential) or total interaction count (keyed miss)
expected_hash:  String   — SHA-256 hex hash of the expected canonical request
                           (sequential) or empty string (keyed miss)
observed_hash:  String   — SHA-256 hex hash of the observed incoming request
diff_summary:   String   — newline-separated field-level diff, for example:
                           "changed: method: \"POST\" -> \"GET\"\n
                            removed: canonical_body.metadata.run_id"
```

### Diff summary format

The diff summary is a newline-separated list of change lines. Each line has
one of three prefixes:

- `added: <path>: <value>` — a field present in the observed request but not
  in the expected request.
- `removed: <path>` — a field present in the expected request but not in the
  observed request.
- `changed: <path>: <expected_value> -> <observed_value>` — a field present
  in both with different values.

Paths use dotted notation for nested objects (for example
`canonical_body.metadata.run_id`). Array elements use bracket notation (for
example `canonical_body.messages[0].role`).

### File layout after completion

```plaintext
src/cassette/
  mod.rs              — adds `mod diff;` and `pub(crate) mod matching;`
                        re-exports matching types
  diff.rs             — canonical JSON diff utility (new, ~120 lines)
  matching.rs         — ReplayMatchEngine, MismatchDiagnostic, MatchOutcome
                        (new, ~250 lines)

src/cassette/ (test modules, compiled under #[cfg(test)])
  diff_tests.rs       — unit tests for diff utility (new, ~120 lines)
  matching_tests.rs   — unit tests for matching engine (new, ~300 lines)

src/error.rs          — enriched RequestMismatch variant

tests/features/
  replay_matching_modes.feature  — Gherkin scenarios (new)

tests/
  replay_matching_modes_bdd.rs   — BDD step definitions (new)
```

### Estimated line counts

- `src/cassette/diff.rs`: ~120 lines
- `src/cassette/diff_tests.rs`: ~120 lines
- `src/cassette/matching.rs`: ~250 lines
- `src/cassette/matching_tests.rs`: ~300 lines
- `tests/features/replay_matching_modes.feature`: ~40 lines
- `tests/replay_matching_modes_bdd.rs`: ~250 lines
- Changes to existing files: ~50 lines net

Total new code: ~1130 lines across 6 new files and modifications to ~4
existing files. This is within the 16-file / 1200-line tolerance.

## Interfaces and dependencies

### No new production dependencies

All required functionality is available from existing dependencies:
`serde_json` (Value comparison), `sha2` (already present), standard library
collections (`HashMap`, `Vec`).

### New types and functions (domain layer)

In `src/cassette/diff.rs`:

```rust
/// Produces a field-level diff summary of two canonical request JSON
/// values.
pub(crate) fn canonical_diff_summary(
    expected: &serde_json::Value,
    observed: &serde_json::Value,
) -> String;
```

In `src/cassette/matching.rs`:

```rust
use crate::cassette::{Cassette, Interaction, CanonicalRequest};
use crate::config::MatchMode;

/// Structured diagnostic for a replay mismatch.
#[derive(Debug, Clone, PartialEq)]
pub struct MismatchDiagnostic {
    pub interaction_id: usize,
    pub expected_hash: String,
    pub observed_hash: String,
    pub diff_summary: String,
}

/// Outcome of a replay match attempt.
#[derive(Debug)]
pub enum MatchOutcome<'a> {
    Matched(&'a Interaction),
    Mismatch(MismatchDiagnostic),
}

/// Replay matching engine.
pub struct ReplayMatchEngine { /* ... */ }

impl ReplayMatchEngine {
    pub fn new(cassette: &Cassette, match_mode: MatchMode) -> Self;
    pub fn next_match(
        &mut self,
        observed_hash: &str,
        observed_canonical: &CanonicalRequest,
    ) -> MatchOutcome<'_>;
}
```

### Modified types

In `src/error.rs`, the `RequestMismatch` variant gains three new fields:

```rust
RequestMismatch {
    interaction_id: usize,
    expected_hash: String,
    observed_hash: String,
    diff_summary: String,
}
```

### Module registration

In `src/cassette/mod.rs`:

```rust
mod diff;
pub(crate) mod matching;

#[cfg(test)]
mod diff_tests;
#[cfg(test)]
mod matching_tests;

pub use matching::{MatchOutcome, MismatchDiagnostic, ReplayMatchEngine};
```
