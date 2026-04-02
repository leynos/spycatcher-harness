# Implement canonical request generation and stable hashing

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

## Purpose / big picture

Task `1.2.2` introduces deterministic canonical request generation and stable
hashing for recorded HTTP interactions. After this change, every
`RecordedRequest` in a cassette can be reduced to a canonical form and a
SHA-256 hash that is stable across runs, regardless of JSON key ordering,
insignificant whitespace, query parameter ordering, or metadata fields
configured as "ignore paths". This capability is the prerequisite for keyed
replay matching (task `1.2.3`) and mismatch diagnostics (task `2.2.2`).

Observable success after delivery:

- Calling `canonicalize(&request, &ignore_paths)` produces a
  `CanonicalRequest` value that normalizes query parameters (sorted by key then
  value), normalizes JSON key ordering and whitespace, and drops configured
  ignore paths.
- Calling `stable_hash(&canonical_request)` produces a hex-encoded SHA-256
  string.
- Two `RecordedRequest` values that differ only in JSON key ordering,
  insignificant whitespace, query parameter ordering, or ignored metadata paths
  produce identical hashes.
- Two `RecordedRequest` values that differ in method, path, non-ignored JSON
  fields, or non-ignored query parameters produce different hashes.
- The `canonical_request` and `stable_hash` fields on `RecordedRequest`
  (reserved since task `1.2.1`) are populated by the canonicalization pipeline.
- Fixture tests built with `rstest` confirm all of the above.
- Behavioural tests built with `rstest-bdd` confirm that equivalent requests
  produce identical hashes when exercised through the harness boundary.
- `docs/spycatcher-harness-design.md` records the canonicalization decisions.
- `docs/users-guide.md` documents the ignore-path configuration surface.
- `docs/roadmap.md` marks task `1.2.2` as done only after all gates pass.

## Constraints

- Maintain hexagonal architecture boundaries:
  - Canonicalization and hashing are domain logic and belong in the
    `src/cassette/` module tree, not in adapter code.
  - The `CanonicalRequest` type, `canonicalize` function, and `stable_hash`
    function must not depend on filesystem, HTTP, or CLI types.
  - Ignore-path configuration is a domain concern; it is a list of JSON
    Pointer strings (RFC 6901) that the canonicalization pipeline removes
    before hashing.
- Preserve the existing public API surface:
  - `start_harness`, `RunningHarness::shutdown`, `HarnessConfig`, and all
    existing `HarnessError` variants must remain source-compatible.
  - The existing `RecordedRequest` fields `canonical_request` and
    `stable_hash` are `Option<Value>` and `Option<String>` respectively; the
    implementation populates these fields but does not change their types.
- The SHA-256 hash must be computed over a deterministic byte string. The
  hash input format must be documented in the design document so that future
  implementations in other languages can reproduce it.
- Tests must use `rstest` for unit coverage and `rstest-bdd` for
  behaviour observable from the harness boundary.
- Filesystem access must use `cap-std`/`camino`. No `std::fs` or `std::path`.
- Comments and documentation must use en-GB-oxendict spelling.
- No single source file may exceed 400 lines.
- Before completion, run the full commit gates: `make fmt`, `make check-fmt`,
  `make lint`, `make test`, `make markdownlint`, and `make nixie`.

## Tolerances (exception triggers)

- Scope: if implementation requires changes to more than 14 files or 1000 net
  lines, stop and escalate.
- Interface: if satisfying this task requires changing the signature of
  `start_harness`, `RunningHarness::shutdown`, or the types of the reserved
  `canonical_request` / `stable_hash` fields, stop and escalate.
- Dependencies: if more than two new crates are needed beyond `sha2` (for
  SHA-256) and `hex` (or equivalent for hex encoding), stop and escalate. Note:
  `sha2` may bundle hex encoding via its `Digest` trait, reducing this to one
  new crate.
- Iteration: if `make lint` or `make test` still fails after five repair
  cycles, stop and escalate with the failing evidence.
- Ambiguity: if the ignore-path semantics are unclear (for example, whether
  wildcards or array indexing are needed), stop and present options before
  writing more code.

## Risks

- Risk: the `sha2` crate may trigger new Clippy warnings under the project's
  strict lint configuration. Severity: low. Likelihood: medium. Mitigation: pin
  a recent stable version of `sha2` and check `make lint` immediately after
  adding the dependency.

- Risk: canonical JSON serialization using `serde_json` may not produce
  deterministic output for all `Value` variants (for example, floating-point
  formatting). Severity: medium. Likelihood: medium. Mitigation: implement a
  custom canonical serializer that walks `serde_json::Value` recursively,
  sorting object keys and using a stable float-to-string conversion. Document
  the format explicitly.

- Risk: the ignore-path feature using JSON Pointer (RFC 6901) may be
  insufficient for deeply nested or array-indexed fields. Severity: low.
  Likelihood: low. Mitigation: start with simple pointer paths (for example
  `/metadata/run_id`), document the limitation, and defer wildcard or
  array-index support to a follow-up if needed.

- Risk: the `RecordedRequest.body` field is `Vec<u8>` and may contain
  non-JSON content. Severity: low. Likelihood: medium. Mitigation:
  canonicalization falls back to hashing raw body bytes when `parsed_json` is
  `None`.

## Progress

- [x] Drafted ExecPlan for roadmap task `1.2.2`.
- [x] Added `sha2` and implemented pure canonicalization and hashing in
      `src/cassette/canonical/mod.rs` with private helper submodules.
- [x] Added `RecordedRequest::populate_canonical_fields` so callers can fill
      the reserved cassette fields without adapter dependencies.
- [x] Added unit coverage for canonical query ordering, JSON key sorting,
      ignore-path removal, stable hashes, and non-JSON request bodies.
- [x] Added BDD coverage for equivalent requests, divergent requests, and
      ignore-path stability.
- [x] Updated the design document, user guide, and roadmap.

## Surprises & discoveries

- Adding ignore-path fields to `HarnessConfig` or `ReplayConfig` would break
  source compatibility for external callers constructing those public structs
  with struct literals. The implementation therefore exposes ignore-path
  configuration through the additive `IgnorePathConfig` domain type in this
  task and leaves harness-startup threading for follow-on work.

## Decision log

- Chose a hand-rolled query parser/encoder instead of the `url` crate to keep
  the dependency graph narrow and avoid the heavy `idna`/ICU stack for this
  pure canonicalization feature.
- Kept canonicalization as pure domain logic under `src/cassette/` and did
  not thread configuration through startup yet because public struct
  compatibility outweighed the planned config expansion.

## Outcomes & retrospective

- Delivery adds deterministic request canonicalization and SHA-256 hashing to
  the cassette domain API without changing the existing harness startup API.
- `IgnorePathConfig` is the public configuration surface for ignored JSON
  paths in this release.
- The implementation supports JSON Pointer removal for nested object paths and
  numeric array indices.

## Context and orientation

### Repository layout relevant to this task

The repository is a single Rust package (edition 2024, Rust 1.88) with a
library crate (`src/lib.rs`) and a binary target
(`src/bin/spycatcher_harness.rs`).

Key files and their roles:

- `src/cassette/mod.rs` (289 lines) — defines the cassette domain model:
  `Cassette`, `CassetteFormatVersion`, `Interaction`, `RecordedRequest`,
  `RecordedResponse`, `StreamEvent`, `StreamTiming`, `InteractionMetadata`, and
  the `CassetteReader`/`CassetteAppender` trait ports. The `RecordedRequest`
  type already has two reserved `Option` fields:
  `canonical_request: Option<Value>` and `stable_hash: Option<String>`.

- `src/cassette/filesystem.rs` (351 lines) — filesystem adapter implementing
  `CassetteReader` and `CassetteAppender` via `FilesystemCassetteStore`.

- `src/cassette/tests.rs` (125 lines) — `rstest` unit tests for schema
  round-trips, including fixtures `sample_non_stream_interaction` and
  `sample_stream_interaction`.

- `src/config.rs` (272 lines) — `HarnessConfig` and related configuration
  types. Currently has no ignore-path configuration field.

- `src/error.rs` (138 lines) — `HarnessError` enum and `HarnessResult<T>`
  alias.

- `src/lib.rs` (367 lines) — library entry point; `start_harness`,
  `validate_config`, `prepare_cassette`, `RunningHarness`.

- `tests/` — integration and BDD tests using `rstest-bdd`.

- `Cargo.toml` — current production dependencies include `serde`,
  `serde_json`, `camino`, `cap-std`, `clap`, `eyre`, `ortho_config`,
  `thiserror`, `tokio`. Dev dependencies include `rstest`, `rstest-bdd`,
  `rstest-bdd-macros`, `uuid`.

### Key terms

- **Canonical request**: a normalized representation of a `RecordedRequest`
  that is invariant under JSON key reordering, insignificant whitespace
  changes, query parameter reordering, and configured ignore-path removal.
  Stored as `serde_json::Value` in the `canonical_request` field.

- **Stable hash**: a hex-encoded SHA-256 digest computed over a deterministic
  byte string derived from the canonical request. Stored as `String` in the
  `stable_hash` field.

- **Ignore paths**: a list of JSON Pointer strings (RFC 6901, for example
  `/metadata/run_id`) identifying fields in the request body JSON that should
  be removed before canonicalization. This allows metadata that drifts between
  runs (timestamps, trace IDs) to not affect hash stability.

- **Canonical query**: the query string after parsing into key-value pairs,
  sorting by key then value (preserving duplicate keys), and re-encoding in a
  consistent form.

### Design document references

The design document at `docs/spycatcher-harness-design.md`, section
"Canonicalization and hashing" (lines 231–257), specifies the recommended
canonicalization pipeline:

1. Canonicalize query parameters: parse, sort by key then value, preserving
   repeated keys, re-encode.
2. Parse JSON body into `serde_json::Value`.
3. Apply a protocol-specific normalization pass: drop configured paths,
   optionally coerce numeric types.
4. Serialize with a canonical serializer: sorted object keys, stable float
   formatting.
5. Hash (SHA-256) over: `method + path + canonical_query + canonical_json`.

## Plan of work

### Stage A: add the `sha2` dependency and verify lint compatibility

Add `sha2` to `[dependencies]` in `Cargo.toml`. Run `make lint` to verify that
the new dependency does not introduce Clippy warnings under the project's
strict lint profile. If `sha2` bundles hex encoding (via the `Digest` trait's
`finalize` returning a `GenericArray` that can be hex-formatted), no separate
`hex` crate is needed; otherwise add `hex` as well.

Go/no-go: `make lint` passes with the new dependency. No code changes beyond
`Cargo.toml`.

### Stage B: define domain types and the canonicalization interface

Implement the canonicalization submodule at `src/cassette/canonical/mod.rs`
with private helpers in sibling files (`hex.rs`, `json.rs`, `query.rs`). This
module contains the pure domain functions for request normalization and hashing
with no adapter dependencies.

Expose the following public items from `src/cassette/mod.rs` via the canonical
submodule:

```rust
use serde_json::Value;

/// Configuration controlling which JSON body paths are excluded
/// from canonicalization.
///
/// Paths use JSON Pointer syntax (RFC 6901), for example
/// `/metadata/run_id`. Matching paths are removed from the
/// parsed JSON before canonical serialization.
#[derive(Debug, Clone, Default)]
pub struct IgnorePathConfig {
    /// JSON Pointer paths to remove from the request body before
    /// hashing.
    pub ignored_body_paths: Vec<String>,
}

/// A normalized request representation used for deterministic
/// matching and hashing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalRequest {
    /// HTTP method in uppercase.
    pub method: String,
    /// Request path without query string.
    pub path: String,
    /// Query parameters sorted by key then value, re-encoded.
    pub canonical_query: String,
    /// Normalized JSON body with sorted keys, removed ignore
    /// paths, and stable formatting. `None` when the request
    /// body is not JSON.
    pub canonical_body: Option<Value>,
}
```

Define the following public functions and methods:

- `canonicalize(...)` returns a `CanonicalRequest` by normalizing the query
  string and parsed JSON body.
- `stable_hash(canonical: &CanonicalRequest) -> String` — produces a
  hex-encoded SHA-256 hash over a deterministic byte representation of the
  canonical request.
- `RecordedRequest::populate_canonical_fields(&IgnorePathConfig)` stores the
  derived canonical request and stable hash back onto the request model.

Internal helpers (private):

- `canonicalize_query(raw_query: &str) -> String` — parses query pairs, sorts
  by key then value, percent-decodes triplets, preserves literal `+`, and
  re-encodes non-unreserved bytes with uppercase hex escapes.
- `canonicalize_body(value: Value, ignore_paths: &[String]) -> Value` —
  removes configured JSON Pointer paths, then recursively sorts object keys.
- `serialize_json_canonical(value: &Value) -> String` — serializes a
  canonicalized `serde_json::Value` into a compact, deterministic JSON string.
- `remove_pointer(value: &mut Value, tokens: &[String])` — removes a single
  parsed JSON Pointer path from a mutable `Value`.

The implemented hash input byte string is the labeled UTF-8 sequence:

```plaintext
METHOD\n{method}\nPATH\n{path}\nQUERY\n{canonical_query}\nBODY\n{canonical_json}
```

When `canonical_body` is `None` (non-JSON request), the body component is
empty. This ensures that non-JSON requests still produce stable hashes based on
method, path, and query.

Go/no-go: the module compiles (`cargo check`) without errors. No tests yet.

### Stage C: implement the canonicalization pipeline

Implement the functions defined in Stage B. The implementation details:

1. `canonicalize_query`: split the raw query string on `&`, then split each
   pair on the first `=`. Collect into a `Vec<(String, String)>`, sort by key
   then by value (lexicographic, byte-order). Re-encode as
   `key1=value1&key2=value2`. Empty query strings produce an empty string.
   Percent-encoding is preserved as-is; decoding and re-encoding is out of
   scope for this task (the design document does not require URL normalization
   beyond ordering).

2. `normalize_json`: clone the input `Value`, then for each ignore path call
   `remove_json_pointer`. The pointer removal walks the `Value` tree and
   removes the leaf. If the pointer does not match, do nothing (no error).

3. `remove_json_pointer`: parse the RFC 6901 pointer (split on `/`, unescape
   `~1` → `/` and `~0` → `~`), walk the `Value` tree following object keys, and
   remove the final key from its parent object. Array indices are not supported
   in this initial implementation; document this limitation.

4. `canonical_json_bytes`: write the `Value` to a byte buffer using a custom
   recursive serializer that:
   - emits object keys in sorted (lexicographic, byte-order) order,
   - emits no insignificant whitespace (compact form),
   - formats numbers using `serde_json`'s default numeric formatting (which is
     stable for integers and finite floats),
   - emits `null`, `true`, `false` as literals.

   The simplest correct approach: recursively walk the `Value`, and for
   `Value::Object` entries, sort the keys before writing. This avoids depending
   on `serde_json::to_string` key ordering guarantees (which are
   insertion-order for `Map`, not sorted).

5. `stable_hash`: construct the hash input byte string as specified, compute
   SHA-256 using `sha2::Sha256`, and format the digest as lowercase hexadecimal.

Go/no-go: the module compiles. Proceed to Stage D.

### Stage D: lock behaviour with failing tests (red)

Write `rstest` unit tests in `src/cassette/canonical/mod.rs` (in a
`#[cfg(test)] mod tests` block) or in a sibling
`src/cassette/canonical_tests.rs` if the tests would push the file over 400
lines. Use `rstest` fixtures and parameterized cases.

Test cases for `canonicalize_query`:

1. Empty query string produces empty string.
2. Single parameter passes through unchanged.
3. Two parameters in reverse order are sorted.
4. Duplicate keys are preserved and sorted by value.
5. Percent-encoded values are preserved verbatim.

Test cases for `normalize_json`:

1. A JSON object with keys in non-sorted order produces the same normalized
   value as the sorted version.
2. Removing an ignore path (`/metadata/run_id`) drops the nested field.
3. Removing a non-existent ignore path leaves the value unchanged.
4. Multiple ignore paths are all removed.

Test cases for `canonical_json_bytes`:

1. An object with keys `{"b": 1, "a": 2}` serializes as `{"a":2,"b":1}`.
2. Nested objects have their keys sorted recursively.
3. Arrays preserve element order.
4. Whitespace-only differences in input produce identical byte output.

Test cases for `canonicalize`:

1. Two `RecordedRequest` values that differ only in JSON key ordering produce
   equal `CanonicalRequest` values.
2. Two `RecordedRequest` values that differ only in query parameter ordering
   produce equal `CanonicalRequest` values.
3. Two `RecordedRequest` values that differ in an ignored path produce equal
   `CanonicalRequest` values.
4. Two `RecordedRequest` values that differ in method produce different
   `CanonicalRequest` values.
5. Two `RecordedRequest` values that differ in a non-ignored JSON field
   produce different `CanonicalRequest` values.
6. A `RecordedRequest` with no `parsed_json` (non-JSON body) produces a
   `CanonicalRequest` with `canonical_body: None`.

Test cases for `stable_hash`:

1. Two identical canonical requests produce the same hash.
2. Two materially different canonical requests produce different hashes.
3. The hash output is a 64-character lowercase hex string (SHA-256).
4. A known test vector with fixed input produces the expected hash string
   (golden test to guard against accidental algorithm changes).

Go/no-go: all new tests fail for the expected reasons (functions not yet
returning correct values or not yet implemented). Existing tests still pass.

### Stage E: make the tests pass (green)

Implement the function bodies to satisfy all tests from Stage D. This is the
main implementation effort.

Go/no-go: `make test` passes with all new and existing tests green.

### Stage F: add ignore-path configuration to `HarnessConfig`

Add an `ignore_paths` field to `HarnessConfig` in `src/config.rs`:

```rust
/// Canonicalization settings for request matching.
pub canonicalization: CanonicalizationConfig,
```

Define `CanonicalizationConfig` in `src/config.rs`:

```rust
/// Configuration for request canonicalization and matching.
#[derive(Debug, Clone, Default)]
pub struct CanonicalizationConfig {
    /// JSON Pointer paths (RFC 6901) to exclude from canonical
    /// request generation. Matching paths are removed from the
    /// request body JSON before hashing. This supports metadata
    /// drift (timestamps, trace IDs) without affecting hash
    /// stability.
    pub ignore_body_paths: Vec<String>,
}
```

Add a default and a unit test for the new field. Update the `HarnessConfig`
default to include `CanonicalizationConfig::default()` (empty ignore list).

Go/no-go: `make test` passes. `make lint` passes.

### Stage G: add BDD behavioural tests

Add a Gherkin feature file at
`tests/features/canonical_request_hashing.feature` that describes the
observable behaviour from the harness boundary. Example scenarios:

```gherkin
Feature: Canonical request generation and stable hashing

  Scenario: Equivalent requests produce identical hashes
    Given a recorded request with JSON body keys in order "model,stream"
    And a recorded request with JSON body keys in order "stream,model"
    When both requests are canonicalized
    Then both stable hashes are identical

  Scenario: Different requests produce different hashes
    Given a recorded request with method "POST"
    And a recorded request with method "PUT"
    When both requests are canonicalized
    Then the stable hashes differ

  Scenario: Ignored paths do not affect hash stability
    Given a recorded request with field "/metadata/run_id" set to "abc"
    And the same request with field "/metadata/run_id" set to "xyz"
    And ignore paths configured as "/metadata/run_id"
    When both requests are canonicalized
    Then both stable hashes are identical
```

Create the BDD test driver at `tests/canonical_request_hashing_bdd.rs` using
`rstest-bdd` step definitions. The BDD tests exercise the public `canonicalize`
and `stable_hash` functions from the cassette module.

Go/no-go: `make test` passes with all BDD scenarios green.

### Stage H: document the final behaviour

Update `docs/spycatcher-harness-design.md`:

- In the "Canonicalization and hashing" section, record the final hash input
  format
  (`method + "\n" + path + "\n" + canonical_query + "\n" + canonical_json_bytes`).
- Record the ignore-path semantics (JSON Pointer, no array index support in
  this release).
- Record the canonical JSON serialization rules (sorted keys, compact form,
  stable numeric formatting).

Update `docs/users-guide.md`:

- Add a section on canonicalization configuration describing the
  `canonicalization.ignore_body_paths` field.
- Show a TOML example:

  ```toml
  [canonicalization]
  ignore_body_paths = ["/metadata/run_id", "/metadata/trace_id"]
  ```

- Explain the effect: fields at these paths are removed from the request body
  before hashing, so metadata that drifts between runs does not cause hash
  mismatches.

Update `docs/roadmap.md`:

- Mark task `1.2.2` as done (change `- [ ]` to `- [x]` for the task and its
  sub-items).

Go/no-go: documentation is complete and accurate.

### Stage I: run full validation with logged evidence

Run every required gate through `tee` with `set -o pipefail` so truncated
terminal output does not hide failures:

```bash
set -o pipefail
make fmt 2>&1 | tee /tmp/1-2-2-fmt.log
make check-fmt 2>&1 | tee /tmp/1-2-2-check-fmt.log
make lint 2>&1 | tee /tmp/1-2-2-lint.log
make test 2>&1 | tee /tmp/1-2-2-test.log
make markdownlint 2>&1 | tee /tmp/1-2-2-markdownlint.log
make nixie 2>&1 | tee /tmp/1-2-2-nixie.log
```

Expected end state:

- `make lint` finishes without Clippy, Rustdoc, or Whitaker warnings.
- `make test` passes all unit tests, behavioural tests, and doctests.
- Markdown validation passes after documentation updates.
- The roadmap item is checked off only after every command above exits zero.

## Interfaces and dependencies

### New production dependency

Add to `Cargo.toml` `[dependencies]`:

```toml
sha2 = "0.10.9"
```

The `sha2` crate provides the SHA-256 implementation. The `Digest` trait's
`finalize()` returns a `GenericArray<u8, U32>` which can be formatted as hex
using the `LowerHex` implementation or by iterating bytes with
`format!("{:02x}")`.

### New types and functions (domain layer)

In `src/cassette/canonical/mod.rs`:

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Configuration controlling which JSON body paths are excluded from
/// canonicalization.
#[derive(Debug, Clone, Default)]
pub struct IgnorePathConfig {
    pub ignored_body_paths: Vec<String>,
}

/// A normalized request representation for deterministic matching.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CanonicalRequest {
    pub method: String,
    pub path: String,
    pub canonical_query: String,
    pub canonical_body: Option<Value>,
}

/// Produces a canonical request from a recorded request.
pub fn canonicalize(
    request: &RecordedRequest,
    ignore_config: &IgnorePathConfig,
) -> CanonicalRequest;

/// Produces a hex-encoded SHA-256 hash of the canonical request.
pub fn stable_hash(canonical: &CanonicalRequest) -> String;

/// Computes and stores canonical request fields on a recorded request.
impl RecordedRequest {
    pub fn populate_canonical_fields(&mut self, ignore_config: &IgnorePathConfig);
}
```

```rust
/// Configuration for request canonicalization and matching.
#[derive(Debug, Clone, Default)]
pub struct CanonicalizationConfig {
    pub ignore_body_paths: Vec<String>,
}
```

Added as a field on `HarnessConfig`:

No `HarnessConfig` field was added. To preserve source compatibility for
existing callers that construct `HarnessConfig` with struct literals, the
implementation exposes `IgnorePathConfig` directly as an additive cassette
domain type and leaves harness startup configuration unchanged.

### Module registration

In `src/cassette/mod.rs`, keep the canonicalization module private and
re-export the supported API surface:

```rust
mod canonical;

pub use canonical::{CanonicalRequest, IgnorePathConfig, canonicalize, stable_hash};
```

## Validation and acceptance

Quality criteria (what "done" means):

- Tests: `make test` passes all unit, BDD, and doc tests. The new test
  module `cassette::canonical::tests` contains at least 15 test cases covering
  query normalization, JSON normalization, ignore-path removal,
  canonicalization equivalence, canonicalization divergence, and hash stability.
- BDD: `tests/canonical_request_hashing_bdd.rs` scenarios pass, covering
  equivalent hashes, divergent hashes, and ignore-path stability.
- Lint: `make lint` passes without warnings.
- Format: `make check-fmt` passes.
- Docs: `make markdownlint` and `make nixie` pass.
- Roadmap: task `1.2.2` is marked done in `docs/roadmap.md`.

Quality method (how to check):

```bash
set -o pipefail
make fmt 2>&1 | tee /tmp/1-2-2-fmt.log
make check-fmt 2>&1 | tee /tmp/1-2-2-check-fmt.log
make lint 2>&1 | tee /tmp/1-2-2-lint.log
make test 2>&1 | tee /tmp/1-2-2-test.log
make markdownlint 2>&1 | tee /tmp/1-2-2-markdownlint.log
make nixie 2>&1 | tee /tmp/1-2-2-nixie.log
```

## Idempotence and recovery

All stages are safe to repeat. The canonicalization module is additive; it does
not modify existing types or behaviour. If a stage fails partway through,
re-running from that stage's starting point is safe.

The `sha2` dependency addition is idempotent (Cargo deduplicates). Test
fixtures use unique cassette names with UUIDs, preventing cross-run
interference.

## Artifacts and notes

### Hash input format specification

The SHA-256 hash input is the byte concatenation of:

```plaintext
<METHOD>\n<PATH>\n<CANONICAL_QUERY>\n<CANONICAL_JSON_BYTES>
```

Where:

- `<METHOD>` is the HTTP method in uppercase ASCII (for example `POST`).
- `<PATH>` is the request path without query string (for example
  `/v1/chat/completions`).
- `<CANONICAL_QUERY>` is the query string after parsing, sorting by key then
  value, and re-encoding (for example `model=gpt-4&stream=true`). Empty if no
  query parameters.
- `<CANONICAL_JSON_BYTES>` is the compact JSON serialization of the
  normalized request body with sorted keys, or empty (zero bytes) if the
  request body is not JSON.

Each component is separated by a single newline byte (`0x0A`). The trailing
newline before `<CANONICAL_JSON_BYTES>` is always present even when the JSON
component is empty, ensuring that the hash input for a non-JSON request is
distinguishable from a request whose JSON body serializes to an empty string.

### Canonical JSON serialization rules

1. Object keys are emitted in sorted lexicographic (byte-order) order.
2. No whitespace is emitted between tokens (compact form).
3. Integers are formatted as decimal without leading zeros.
4. Floating-point numbers use `serde_json`'s default formatting (which
   produces the shortest representation that round-trips).
5. Strings are JSON-escaped per RFC 8259.
6. `null`, `true`, `false` are emitted as literals.
7. Arrays preserve element order.

### Ignore-path semantics

- Paths use JSON Pointer syntax (RFC 6901).
- Only object keys are addressable; array indices are not supported in this
  release.
- A path that does not match the request body is silently ignored (no error).
- Ignore paths are applied before canonical serialization, so removed fields
  do not participate in the hash.
- Example: with ignore path `/metadata/run_id`, the JSON body
  `{"model": "gpt-4", "metadata": {"run_id": "abc123"}}` is canonicalized as
  `{"metadata":{},"model":"gpt-4"}`.
