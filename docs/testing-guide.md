# Testing guide

This guide records test dependency rationale and test layout conventions for
the Spycatcher harness.

## Dev-dependencies

- `insta` pins human-readable diagnostics and cassette JSON snapshots. Review
  snapshot updates with `cargo insta review`.
- `tracing-test` captures tracing events in unit tests, including checks that
  request logs use paths rather than full URIs containing query secrets.
- `proptest` covers invariants in modules such as `cassette::diff`, including
  deterministic output, empty self-diffs, and key-order independence.
- `rstest` provides parameterized test cases and fixtures.
- `rstest-bdd` connects Gherkin feature files to Rust step definitions.
- `uuid` creates unique cassette names for parallel integration tests.
- `tempfile` provides self-cleaning cassette directories for tests that still
  exercise filesystem-backed record services.

Do not replace temporary cassette fixtures with fixed paths. A future in-memory
cassette store can remove the temporary filesystem dependency once
`RecordService` accepts cassette reader/appender traits generically.

## Test structure

Unit tests live beside production code under `#[cfg(test)]` modules. Use an
inline `mod tests` block for small suites and sibling `*_tests.rs` files when a
module needs richer helpers. Large suites may split into directories such as
`src/cassette/matching_tests/`.

Integration tests live in `tests/` and follow rstest-bdd conventions:

```plaintext
tests/
  features/                      Gherkin .feature files
  replay_matching_modes/         Step definitions and world
  replay_matching_modes_bdd.rs   Entrypoint wiring features to steps
  support/                       Shared BDD fixtures and utilities
```

Each BDD suite has a `.feature` file in `tests/features/`, a `*_bdd.rs`
entrypoint, and optional suite-local step, world, fixture, and helper modules.

## Running tests

Use Makefile targets for local gates:

```sh
make test       # nextest + doctests
make lint       # cargo doc + clippy + whitaker
make check-fmt  # formatting check
make fmt        # apply Rust and Markdown formatting
```

See `AGENTS.md` for the full command expansions and commit gating
requirements.
