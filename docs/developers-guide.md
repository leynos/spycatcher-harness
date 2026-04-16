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

| Module     | Purpose                                                                       |
| ---------- | ----------------------------------------------------------------------------- |
| `cassette` | Schema, canonicalization, hashing, matching, diff, and filesystem persistence |
| `cli`      | CLI argument parsing via `clap`                                               |
| `config`   | `HarnessConfig` and related structures                                        |
| `error`    | `HarnessError` enum and `HarnessResult` alias                                 |
| `i18n`     | Internationalization via Fluent                                               |
| `protocol` | Protocol identifier definitions                                               |
| `replay`   | Replay mode logic (placeholder)                                               |
| `server`   | HTTP server logic (placeholder)                                               |
| `upstream` | Upstream target configuration                                                 |

_Table 1: Top-level library modules._

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
