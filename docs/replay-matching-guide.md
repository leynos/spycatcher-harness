# Replay matching guide

This guide describes the replay matching subsystem and extension conventions
that were split out of the developer's guide to keep both documents scannable.

## Replay matching architecture

The replay matching subsystem lives in `cassette::matching` and decides which
recorded interaction to serve for each incoming replay request.

### `MatchMode`

`MatchMode` selects the matching strategy:

| Variant            | Behaviour                                                                                   |
| ------------------ | ------------------------------------------------------------------------------------------- |
| `SequentialStrict` | Default. Expects requests in recorded order; mismatches fail fast.                          |
| `Keyed`            | Matches by request hash, consuming the next unused interaction with that hash in any order. |

### `ReplayMatchEngine`

`ReplayMatchEngine` is constructed from a loaded `Cassette` and a `MatchMode`.
Construction validates that every interaction carries a `stable_hash`, returning
`HarnessError::InvalidCassette` if any is missing.

`next_match(observed_hash, observed_canonical)` returns a consuming
`MatchOutcome`. `peek_match(observed_hash, observed_canonical)` returns the
same kind of outcome without advancing the sequential cursor or keyed consumed
flags, allowing replay orchestration to validate response shape before
committing match state.

Sequential strict mode keeps a cursor for the next expected interaction. The
incoming hash must equal the hash at the cursor; success advances the cursor,
while failure returns a `MismatchDiagnostic` and leaves the cursor unchanged.

Keyed mode builds a `HashMap<String, Vec<usize>>` at construction time. Each
match consumes the first unconsumed interaction for the observed hash. Missing
or exhausted hashes return `MismatchDiagnostic`.

### `MatchOutcome`

- `Matched { interaction_id, interaction }` carries the recorded interaction
  to replay.
- `Mismatch(MismatchDiagnostic)` carries structured mismatch details.

### `MismatchDiagnostic`

`MismatchDiagnostic` is domain-internal and contains the fields needed by the
HTTP adapter to build a `409 Conflict` response:

| Field           | Purpose                                                                                       |
| --------------- | --------------------------------------------------------------------------------------------- |
| `position`      | Identifies which interaction or bound the mismatch relates to.                                |
| `expected_hash` | Stable hash of the expected request, or empty when no single expected interaction exists.     |
| `observed_hash` | Stable hash of the incoming request.                                                          |
| `diff_summary`  | Field-level diff from `cassette::diff`, or a stable sentinel for exhaustion and keyed misses. |

`InteractionPosition` disambiguates the mismatch location:

| Variant        | Meaning                                                                 |
| -------------- | ----------------------------------------------------------------------- |
| `Expected(n)`  | Sequential mode expected interaction index.                             |
| `Exhausted(n)` | Sequential mode has no more interactions; `n` is the interaction count. |
| `KeyedMiss(n)` | Keyed mode has no unconsumed matching interaction.                      |

### Diagnostic constants

- `DIAGNOSTIC_EXHAUSTED`: no more interactions are available.
- `DIAGNOSTIC_NO_MATCH`: keyed mode found no interaction for the hash.
- `DIAGNOSTIC_CONSUMED`: keyed mode found only consumed interactions.

### Relationship to HTTP errors

The matching domain returns `MatchOutcome::Mismatch`; it does not know about
HTTP. The replay adapter maps mismatches to request-mismatch responses and maps
stream-shaped requests matched to non-stream cassette entries to the dedicated
`stream_cassette_required` 501 path after a non-consuming peek.

### Supporting module: `diff`

`cassette::diff::canonical_diff_summary` compares two `serde_json::Value` trees
and produces newline-separated change lines:

- `added: <path>: <value>`
- `removed: <path>`
- `changed: <path>: <expected> -> <observed>`

Diff determinism depends on the `serde_json` `preserve_order` feature described
in [`developers-guide.md`](developers-guide.md).

## Extension guidelines

- Place unit tests in a `#[cfg(test)]` module or sibling `*_tests.rs` file.
- For BDD scenarios, add a `.feature` file in `tests/features/` and wire it
  through a `*_bdd.rs` entrypoint.
- Dependency versions in `Cargo.toml` must use implicit semver caret versioning
  such as `"1.2.3"`; do not write an explicit `'^'`.
- New dev-dependencies belong in [`testing-guide.md`](testing-guide.md) with
  their rationale.
- New `serde_json` feature flags or build configuration changes belong in
  [`developers-guide.md`](developers-guide.md) with the invariant they support.
