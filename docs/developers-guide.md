# Developer's guide

This guide documents build configuration choices, dependency rationale,
internal module layout, and test structure patterns for contributors working on
the Spycatcher harness.

## Build configuration

### `serde_json` `preserve_order` feature

The `serde_json` dependency is compiled with the `preserve_order` feature
enabled:

```toml
serde_json = { version = "1.0.143", features = ["preserve_order"] }
```

This feature causes `serde_json::Map` to use an insertion-ordered map
(`IndexMap`) instead of a `BTreeMap`. It is **required** for two reasons:

1. **Deterministic canonical hashing.** The canonical request module
   normalizes JSON objects before hashing. Without `preserve_order`,
   round-tripping through `serde_json` could silently reorder keys, producing
   different hashes for semantically identical requests.
2. **Stable diff output.** The field-level diff utility iterates over
   JSON objects in key order. Deterministic iteration order ensures that diff
   summaries are reproducible across runs, which the property-based tests in
   `diff_tests` explicitly verify (see
   `diff_key_ordering_does_not_affect_output`).

Removing this feature flag would break both the hashing invariant and the diff
determinism property tests.

## Dev-dependencies

### `insta` — snapshot testing

[Insta](https://insta.rs/) captures the string output of a function and
compares it against a stored reference value (a "snapshot"). It is used in
`src/cassette/diff_tests.rs` to pin the exact format of diff summaries via
inline snapshots:

```rust
insta::assert_snapshot!(diff, @r#"changed: method: "POST" -> "GET""#);
```

Snapshots are reviewed and updated with:

```sh
cargo insta review
```

This prevents accidental regressions in human-readable diagnostic output
without requiring handwritten expected strings for every format variation.

### `proptest` — property-based testing

[Proptest](https://proptest-rs.github.io/proptest/) generates random inputs to
verify that properties hold across a broad input space. It is used in
`src/cassette/diff_tests.rs` alongside a custom `arb_json_object()` strategy
that produces small JSON objects with random keys and scalar values. Four
property tests validate diff invariants:

- `identical_values_always_produce_empty_diff` — diffing a value
  against itself always yields an empty string.
- `diff_is_deterministic_across_invocations` — repeated calls with the
  same inputs produce identical output.
- `every_differing_key_is_mentioned` — every key that differs between
  expected and observed appears in the summary.
- `diff_key_ordering_does_not_affect_output` — reversing key insertion
  order does not change the diff result.

These properties complement the example-based `#[rstest]` cases and snapshot
tests, catching edge cases that handwritten examples might miss.

### `rstest` and `rstest-bdd` — fixtures and BDD scenarios

[rstest](https://docs.rs/rstest/) provides parameterized test cases (`#[case]`)
and shared fixtures injected by function signature.
[rstest-bdd](https://docs.rs/rstest-bdd/) extends this with Gherkin-style step
definitions (`#[given]`, `#[when]`, `#[then]`) driven by `.feature` files.

### `uuid` — unique cassette names

The `uuid` crate (with the `v4` feature) generates unique cassette filenames in
integration tests, preventing collisions when tests run in parallel.

## Internal module layout

The library crate (`src/lib.rs`) exposes the following public modules:

| Module     | Purpose                                                                    |
| ---------- | -------------------------------------------------------------------------- |
| `cassette` | Schema, canonicalization, hashing, matching, diff, filesystem persistence  |
| `cli`      | CLI argument parsing via `clap`                                            |
| `config`   | `HarnessConfig`, `UpstreamConfig`, `RedactionConfig`, and related types    |
| `error`    | `HarnessError` enum and `HarnessResult` alias                              |
| `i18n`     | Internationalization via Fluent                                            |
| `protocol` | Capture and redaction helpers: header selection, hop-by-hop filtering      |
| `replay`   | Replay mode logic                                                          |
| `server`   | Axum record-mode HTTP server: routing, handler, graceful shutdown          |
| `upstream` | Outbound HTTP adapter: URL construction, secret resolution, reqwest client |

_Table 1: Top-level library modules._

The crate root re-exports the public entry points `start_harness`,
`RunningHarness`, and `shutdown` (from `RunningHarness`) as well as
`HarnessConfig`, `HarnessError`, and `HarnessResult`.

The upstream adapter returns `ObservedResponse` values carrying the HTTP status
code, raw header byte pairs for proxying as `Vec<(String, Vec<u8>)>`, and exact
response body bytes. Header value percent-encoding happens only at the
persistence boundary when cassette-safe string headers are derived.

The `cassette` module contains several submodules:

| Submodule    | Purpose                                     |
| ------------ | ------------------------------------------- |
| `canonical`  | Request normalization and `stable_hash`     |
| `diff`       | Field-level JSON diff summaries             |
| `filesystem` | `FilesystemCassetteStore` (reader/appender) |
| `matching`   | `ReplayMatchEngine` and match outcome types |

_Table 2: Cassette submodules._

The binary crate (`src/bin/spycatcher_harness.rs`) delegates all behaviour to
the library entry points.

## Test structure

### Unit tests

Unit tests live alongside their production code under `#[cfg(test)]` modules.
Two patterns are used:

- **Inline test module.** A `mod tests { ... }` block at the bottom of
  the source file (e.g. `src/lib.rs`, `src/cassette/filesystem.rs`).
- **Dedicated test file.** A sibling `*_tests.rs` file imported under
  `#[cfg(test)]` (e.g. `src/cassette/diff_tests.rs`,
  `src/cassette/canonical_tests.rs`). When the test module grows large it may
  be split into a directory with submodules (e.g.
  `src/cassette/matching_tests/` with `construction.rs`, `diagnostic.rs`,
  `fixtures.rs`, `keyed.rs`, `sequential.rs`).

### Integration tests (BDD)

Integration tests reside in the `tests/` directory and follow rstest-bdd
conventions:

```plaintext
tests/
  features/                      Gherkin .feature files
    replay_matching_modes.feature
    ...
  replay_matching_modes/         Step definitions and world
    fixtures.rs
    helpers.rs
    steps.rs
    world.rs
  replay_matching_modes_bdd.rs   Entrypoint wiring features to steps
  support/                       Shared BDD fixtures and utilities
    bdd_fixtures.rs
    test_utils.rs
```

Each BDD test suite has:

1. A `.feature` file in `tests/features/` defining scenarios in
   Gherkin syntax.
2. An entrypoint `*_bdd.rs` file in `tests/` that wires the feature
   file to step definitions.
3. Optional subdirectory (e.g. `tests/replay_matching_modes/`) holding
   step definitions (`steps.rs`), a world struct (`world.rs`), fixtures
   (`fixtures.rs`), and helpers (`helpers.rs`).

### Running tests

All tests are run via the Makefile:

```sh
make test       # nextest + doctests
make lint       # cargo doc + clippy + whitaker
make check-fmt  # formatting check (no modification)
make fmt        # apply formatting (Rust + Markdown)
```

See `AGENTS.md` for the full command expansions and commit gating requirements.

## Replay matching architecture

The replay matching subsystem lives in the `cassette::matching` module and
decides which recorded interaction to serve for each incoming request during
replay mode. It is built around four core types:

### `MatchMode` (defined in `config`)

An enum selecting the matching strategy:

| Variant            | Behaviour                                                                                          |
| ------------------ | -------------------------------------------------------------------------------------------------- |
| `SequentialStrict` | Default. Expects requests in recorded order; mismatches fail fast.                                 |
| `Keyed`            | Matches by request hash, consuming the next unused interaction with that hash regardless of order. |

_Table 3: Matching mode variants._

### `ReplayMatchEngine`

The stateful matching engine, constructed from a loaded `Cassette` and a
`MatchMode`. Construction validates that every interaction carries a
`stable_hash`, returning `HarnessError::InvalidCassette` if any is missing.

The single public entry point is
`next_match(observed_hash, observed_canonical)` which returns a `MatchOutcome`.
Internally it dispatches to one of two private strategies:

- **Sequential strict** — maintains a cursor tracking the next expected
  index. The incoming hash must equal the hash at the cursor position; on
  success the cursor advances, on failure a `MismatchDiagnostic` is returned
  and the cursor stays put (allowing retry at the application level).
- **Keyed** — builds a `HashMap<String, Vec<usize>>` at construction
  time mapping each hash to its interaction indices. On each call it finds the
  first unconsumed index for the observed hash, marks it consumed, and returns
  the interaction. When no index exists or all matching indices are consumed, a
  `MismatchDiagnostic` is returned.

### `MatchOutcome`

The return type of `next_match`:

- `Matched(&Interaction)` — the request matched; carries a borrow of the
  recorded interaction for the caller to replay.
- `Mismatch(MismatchDiagnostic)` — no match found; carries structured
  diagnostics.

### `MismatchDiagnostic`

A plain struct carrying everything the adapter layer needs to build an HTTP 409
response without coupling the domain to HTTP types:

| Field           | Type                  | Purpose                                                                 |
| --------------- | --------------------- | ----------------------------------------------------------------------- |
| `position`      | `InteractionPosition` | Identifies which interaction (or bound) the mismatch relates to.        |
| `expected_hash` | `String`              | Stable hash of the expected request (sequential) or empty (keyed miss). |
| `observed_hash` | `String`              | Stable hash of the incoming request.                                    |
| `diff_summary`  | `String`              | Field-level diff produced by the `diff` module.                         |

_Table 4: `MismatchDiagnostic` fields._

### `InteractionPosition`

Disambiguates the positional semantics carried inside a `MismatchDiagnostic`:

| Variant        | Mode(s)    | Payload meaning                                                        |
| -------------- | ---------- | ---------------------------------------------------------------------- |
| `Expected(n)`  | Sequential | Zero-based index of the next expected interaction.                     |
| `Exhausted(n)` | Sequential | Cassette is exhausted; `n` is the total interaction count.             |
| `KeyedMiss(n)` | Keyed      | No unconsumed interaction matched; `n` is the total interaction count. |

_Table 5: `InteractionPosition` variants._

### Diagnostic constants

Three sentinel strings identify the mismatch category in the `diff_summary`
field:

- `DIAGNOSTIC_EXHAUSTED` (`"cassette-exhausted"`) — no more
  interactions available.
- `DIAGNOSTIC_NO_MATCH` (`"no-matching-interaction"`) — keyed mode
  found no interaction with the observed hash.
- `DIAGNOSTIC_CONSUMED` (`"interaction-already-consumed"`) — keyed
  mode found interactions but all are already consumed.

### Relationship to `HarnessError::RequestMismatch`

`MismatchDiagnostic` is a domain-internal type that does not leave the
`cassette` module boundary as an error. The adapter layer (HTTP server) maps a
`MatchOutcome::Mismatch` into a `HarnessError::RequestMismatch` when surfacing
the failure to callers. `RequestMismatch` mirrors the diagnostic fields
(`interaction_id`, `expected_hash`, `observed_hash`, `diff_summary`) so the
error can be formatted for logging and HTTP responses without re-importing
matching internals.

### Supporting module: `diff`

The `diff` module (`cassette::diff`) provides `canonical_diff_summary`, which
compares two `serde_json::Value` trees and produces a human-readable summary of
field-level differences. The matching engine calls this function when building
a `MismatchDiagnostic` to populate the `diff_summary` field. Output format uses
newline-separated change lines:

- `added: <path>: <value>` — field present in observed but not expected.
- `removed: <path>` — field present in expected but not observed.
- `changed: <path>: <expected> -> <observed>` — differing values.

Determinism of this output depends on the `serde_json` `preserve_order` feature
documented in the build configuration section above.

## Extension guidelines

When adding new modules or test files:

- Place unit tests in a `#[cfg(test)]` module within the source file
  or in a sibling `*_tests.rs` file.
- For BDD scenarios, add a `.feature` file in `tests/features/` and
  wire it through a `*_bdd.rs` entrypoint.
- Dependency versions in `Cargo.toml` must use implicit semver caret
  versioning (e.g. `"1.2.3"`); an explicit `'^'` must not appear (e.g. not
  `"^1.2.3"`). New dev-dependencies must be documented in this guide with their
  rationale.
- New `serde_json` feature flags or build configuration changes must
  be documented here with the invariant they support.
