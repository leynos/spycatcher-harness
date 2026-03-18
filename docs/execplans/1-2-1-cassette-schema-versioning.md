# Implement cassette schema versioning and append-only persistence

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

## Purpose / big picture

Task `1.2.1` is the first real delivery slice for cassette storage. After this
change, the harness will have a concrete, versioned cassette schema that can
round-trip both non-stream and stream interactions without loss, record mode
will persist interactions using append-only semantics, and replay mode will
open the cassette read-only and fail fast when the on-disk `format_version` is
unsupported.

Observable success:

- A stored cassette contains `format_version`, ordered interactions,
  protocol identifiers, and the metadata described in
  `docs/spycatcher-harness-design.md#cassette-definition`.
- Unit tests built with `rstest` prove lossless schema round-trips for
  non-stream and stream interactions, append-only ordering, and unsupported
  version rejection.
- Behavioural tests built with `rstest-bdd` cover the user-visible replay
  startup paths that are observable now: supported cassette startup and
  unsupported cassette rejection.
- `docs/spycatcher-harness-design.md` records the final schema and persistence
  decisions, `docs/users-guide.md` describes the new replay behaviour and error
  cases, and `docs/roadmap.md` marks `1.2.1` done only after all gates pass.

## Constraints

- Maintain hexagonal boundaries:
  - cassette schema types, version validation rules, and store traits belong
    to the library's domain-facing `cassette` module;
  - filesystem persistence is an adapter concern and must not leak filesystem
    handles, `cap_std` types, or JSON parser details into the public library
    API;
  - `start_harness(cfg)` remains the composition root that wires domain logic
    to adapters.
- Preserve the existing public entrypoints
  `spycatcher_harness::start_harness` and
  `spycatcher_harness::RunningHarness::shutdown`.
- Preserve the current cassette naming contract unless the user explicitly
  approves a breaking change: `HarnessConfig.cassette_name` is currently a
  logical name and `RunningHarness.cassette_path` is currently
  `cassette_dir.join(cassette_name)`. The design text mentioning
  `cassette.json` conflicts with the implemented API and must be resolved by
  documentation and implementation decisions, not by a silent contract change.
- Replay mode must never require write access to the cassette file.
- Record-mode persistence must be append-only in behaviour: adding a new
  interaction may not reorder, remove, or mutate earlier interactions.
- Tests must use `rstest` for unit coverage and `rstest-bdd` where behaviour is
  observable from the harness boundary.
- Filesystem access must use capability-oriented and UTF-8-safe path handling
  (`cap_std::fs_utf8` and `camino`) rather than `std::fs` and `std::path`.
- Comments and documentation must use en-GB-oxendict spelling.
- Before completion, run the full commit gates with `set -o pipefail` and
  `tee`: `make fmt`, `make check-fmt`, `make lint`, `make test`,
  `make markdownlint`, and `make nixie`.

## Tolerances (exception triggers)

- Scope: if implementation requires changes to more than 16 files or 1200 net
  lines, stop and escalate.
- Interface: if satisfying this task requires changing the signature of
  `start_harness`, `RunningHarness::shutdown`, or the meaning of
  `cassette_name`, stop and escalate.
- Format: if true byte-level append-only writes are required to satisfy the
  roadmap wording, and that requirement conflicts with a single-file JSON
  cassette representation, stop and record the options before proceeding.
- Dependencies: if more than four new crates are needed, stop and escalate.
  Likely additions are `serde_json`, `cap-std`, and one byte-encoding helper
  crate if `Vec<u8>` arrays are rejected as too poor for the on-disk format.
- Iteration: if `make lint` or `make test` still fails after five repair
  cycles, stop and escalate with the failing evidence.
- Ambiguity: if the design references leave stream-interaction metadata too
  vague to define a stable schema for round-trip tests, stop and write the
  options into `Decision Log` before more code is added.

## Risks

- Risk: the current code and docs use extension-free cassette names, while the
  design prose mentions `cassette.json`. Severity: medium. Likelihood: high.
  Mitigation: make the file naming decision explicit in
  `docs/spycatcher-harness-design.md`, keep the current contract unless the
  user approves otherwise, and update `docs/users-guide.md` to match.

- Risk: "append-only persistence" can mean either logical append-only
  behaviour or physical `O_APPEND`-style file writes. Severity: medium.
  Likelihood: medium. Mitigation: lock the meaning down in the design document
  before implementation proceeds beyond red tests.

- Risk: stream support is only partially designed today because runtime
  Server-Sent Events (SSE) capture arrives in later roadmap tasks. Severity:
  medium. Likelihood: medium. Mitigation: define a stream schema that can
  faithfully round-trip synthetic stream interactions now without claiming that
  live SSE parsing is finished.

- Risk: replay startup currently does not open cassette files at all.
  Severity: medium. Likelihood: high. Mitigation: keep startup integration for
  this task narrow: load and validate the cassette in replay mode, initialize a
  persistence adapter in record mode, and defer actual HTTP request handling to
  later tasks.

## Progress

- [x] (2026-03-09) Drafted ExecPlan for roadmap task `1.2.1`.
- [x] (2026-03-10) Resolved the cassette path and extension decision in the
  design document.
- [x] (2026-03-10) Added `rstest` unit tests for schema round-trips,
  append-only ordering, and unsupported `format_version` handling.
- [x] (2026-03-10) Introduced cassette domain types and version validation.
- [x] (2026-03-10) Implemented the filesystem-backed cassette store adapter.
- [x] (2026-03-10) Wired replay startup to open and validate cassettes
  read-only.
- [x] (2026-03-10) Added `rstest-bdd` behavioural tests for supported and
  unsupported replay startup.
- [x] (2026-03-10) Updated `docs/spycatcher-harness-design.md`,
  `docs/users-guide.md`, and `docs/roadmap.md`.
- [x] (2026-03-10) Ran `make fmt`, `make check-fmt`, `make lint`,
  `make test`, `make markdownlint`, and `make nixie`.

## Surprises & Discoveries

- Observation: `src/cassette.rs` and `src/replay.rs` are still stubs, so this
  task establishes the first real cassette domain model and persistence
  boundary. Evidence: the modules currently contain only module-level comments.

- Observation: the existing startup tests assert that
  `RunningHarness.cassette_path` is exactly `cassette_dir.join(cassette_name)`.
  Evidence: `src/lib.rs` unit tests and `tests/harness_startup_bdd.rs` both
  assert that contract today. Impact: file naming changes are contract changes,
  not implementation details.

- Observation: repeated test runs reused previously written cassette files when
  generated test names were only counter-based. Evidence: the append-order
  filesystem test loaded duplicate interactions from a prior run until the test
  helpers included the process ID in generated cassette names. Impact: cassette
  persistence tests must generate cross-run-unique paths, not merely
  per-process counters.

- Observation: once `src/cassette.rs` gained a child module
  (`src/cassette/filesystem.rs`), Whitaker's `self_named_module_files` rule
  required the parent module to move to `src/cassette/mod.rs`. Evidence:
  `make lint` failed until the module was moved. Impact: future cassette
  submodules must keep the parent module in directory form.

## Decision log

- Decision: implement the cassette schema as domain-owned Rust types plus
  `serde` serialization, and keep filesystem persistence behind a dedicated
  adapter. Rationale: this follows the hexagonal dependency rule and keeps
  schema tests independent of file I/O. Date/Author: 2026-03-09 / agent

- Decision: scope behavioural tests to replay startup and replay rejection,
  not low-level append internals. Rationale: `rstest-bdd` should exercise
  observable behaviour. Append semantics, serialization details, and lossless
  round-trips are better locked down with `rstest` unit tests. Date/Author:
  2026-03-09 / agent

- Decision: treat the current `cassette_name` path semantics as the default
  compatibility constraint for this plan. Rationale: the implementation, unit
  tests, behavioural tests, and user's guide already agree on that contract.
  Any move to an implicit `.json` suffix should be treated as a conscious
  breaking change. Date/Author: 2026-03-09 / agent

- Decision: implement append-only persistence as a logical guarantee backed by
  full-document JSON rewrites in the filesystem adapter. Rationale: this keeps
  the on-disk schema simple and versioned while still ensuring that record mode
  only grows the interaction list in order. Date/Author: 2026-03-10 / agent

## Outcomes & retrospective

Task `1.2.1` is complete. The delivered behaviour is:

- cassettes are stored as versioned JSON documents with ordered interactions,
  protocol IDs, and metadata;
- replay startup loads cassettes read-only and rejects unsupported
  `format_version` values with typed errors;
- unit tests cover non-stream and stream schema round-trips plus append
  ordering;
- behavioural tests cover supported and unsupported replay startup;
- the design doc, user's guide, roadmap, and this ExecPlan all reflect the
  final behaviour.

Validation completed:

- `make fmt`
- `make check-fmt`
- `make lint`
- `make test`
- `make markdownlint`
- `make nixie`

## Context and orientation

Current repository state relevant to this task:

- `src/cassette/mod.rs` defines `Cassette`, `Interaction`, `RecordedRequest`,
  `RecordedResponse`, `StreamEvent`, `StreamTiming`, and `InteractionMetadata`
  types with serde serialization; provides `from_reader` and `write_to` for
  JSON persistence; implements `validate()` for version checking; defines
  `CassetteReader` and `CassetteAppender` traits for hexagonal boundaries.
- `src/cassette/filesystem.rs` implements `FilesystemCassetteStore` with
  `open_for_replay` and `open_or_create_for_record`; provides read-only replay
  access and append-only record-mode writes using cap-std.
- `src/replay.rs` remains an empty stub; replay startup validation occurs in
  `src/lib.rs::prepare_cassette` instead.
- `src/lib.rs::prepare_cassette` loads and validates cassettes for both record
  and replay modes via `FilesystemCassetteStore`; `start_harness` returns
  `RunningHarness` with validated cassette path.
- `src/error.rs` defines `CassetteNotFound`, `InvalidCassette`, and
  `UnsupportedCassetteFormatVersion` variants alongside `Io` for filesystem
  errors.
- Test coverage includes unit tests for cassette round-trips, append-only
  persistence, version rejection, and BDD scenarios for replay startup with
  supported/unsupported cassettes.

Key design references to keep open while implementing:

- `docs/spycatcher-harness-design.md#cassette-definition`
- `docs/spycatcher-harness-design.md#architecture-overview`
- `docs/spycatcher-harness-design.md#public-library-api-surface`
- `docs/rust-testing-with-rstest-fixtures.md`
- `docs/reliable-testing-in-rust-via-dependency-injection.md`
- `docs/rstest-bdd-users-guide.md`
- `docs/rust-doctest-dry-guide.md`
- `docs/complexity-antipatterns-and-refactoring-strategies.md`

Key terms for this task:

- **Cassette**: one persisted agent session containing an ordered list of
  recorded interactions.
- **Interaction**: one request/response exchange plus metadata such as protocol
  identifier, upstream identifier, and timestamps.
- **Format version**: a top-level schema discriminator used to reject
  unsupported on-disk formats before replay proceeds.
- **Append-only persistence**: record mode may only add interactions to the end
  of the cassette; it may not rewrite earlier interaction content as part of
  normal operation.

The target cassette shape should be explicit before code lands. A concise
example is below. This is not a copy-paste implementation contract, but it is
the minimum observable data shape the implementation must preserve.

```json
{
  "format_version": 1,
  "interactions": [
    {
      "request": {
        "method": "POST",
        "path": "/v1/chat/completions",
        "query": "",
        "headers": {
          "content-type": "application/json"
        },
        "body": "<encoded bytes>",
        "parsed_json": {
          "model": "openai/gpt-4o-mini",
          "stream": false
        }
      },
      "response": {
        "kind": "non_stream",
        "status": 200,
        "headers": {
          "content-type": "application/json"
        },
        "body": "<encoded bytes>"
      },
      "metadata": {
        "protocol_id": "openai.chat_completions.v1",
        "upstream_id": "openrouter",
        "recorded_at": "2026-03-09T00:00:00Z",
        "relative_offset_ms": 0
      }
    }
  ]
}
```

For stream interactions, the `response` variant must be able to preserve both
parsed events and raw transcript bytes so that later streaming tasks can reuse
the same schema without a migration.

## Plan of work

### Stage A: lock behaviour with failing tests (red)

Start with tests before implementation so the schema and persistence rules are
not guessed at informally.

Add unit tests with `rstest` in a new cassette-focused test module. Use small,
reusable fixtures for sample non-stream interactions, sample stream
interactions, and temporary cassette directories. Cover at least these cases:

- non-stream interaction serializes and deserializes without losing bytes,
  parsed JSON, headers, or metadata;
- stream interaction serializes and deserializes without losing parsed events,
  raw transcript bytes, or timing metadata;
- appending two interactions preserves insertion order;
- replay loading rejects an unsupported `format_version` with an actionable
  error that mentions the observed version and the supported version range;
- replay loading rejects malformed cassettes that omit required top-level
  fields.

Add behavioural coverage with `rstest-bdd` only for what is externally
observable now:

- replay startup succeeds when a supported cassette already exists;
- replay startup fails with a typed, user-visible error when
  `format_version` is unsupported.

If practical without brittle permission assumptions, add one more scenario:
replay startup succeeds when the cassette file itself is read-only, proving
that replay only reads.

Go/no-go: the new tests must fail for the expected reasons before production
code is added.

### Stage B: define the cassette domain model

Replace the `src/cassette.rs` stub with real domain types. If the file grows
too quickly, split early into a `src/cassette/` module tree such as `mod.rs`,
`model.rs`, and `store.rs` to stay below the project's 400-line file limit.

Define the top-level types needed for this task:

- `Cassette`
- `CassetteFormatVersion` or equivalent explicit version type
- `Interaction`
- `RecordedRequest`
- `RecordedResponse` with distinct non-stream and stream variants
- `InteractionMetadata`
- `StreamEvent` and any stream transcript wrapper needed for lossless
  round-tripping

Keep the domain model free of filesystem or CLI concerns. It is acceptable for
the domain model to derive `Serialize` and `Deserialize`, because serialization
is part of the cassette schema contract itself.

Add format validation helpers that reject unsupported versions before replay
logic consumes the cassette. The error surface should be semantic and typed.
Extending `HarnessError` with a cassette-format-specific variant is expected.

Go/no-go: unit tests for in-memory schema round-trips pass without any
filesystem adapter involved.

### Stage C: define ports and implement the filesystem adapter

Add a domain-owned store interface in the cassette module. Keep the port small
and mode-specific rather than one large "do everything" trait. One reasonable
shape is:

- a writer capability for record mode that can initialize a cassette and append
  one interaction at a time;
- a reader capability for replay mode that can load and validate an existing
  cassette without any write surface.

Implement the filesystem adapter using `cap_std::fs_utf8` and `camino`. The
adapter may keep a full in-memory `Cassette` value and rewrite the serialized
document atomically after each append if, and only if, the team records that
this satisfies "append-only" at the behavioural level for `1.2.1`. If the
roadmap wording is interpreted as requiring physical append-only writes, stop
and escalate rather than silently substituting a different guarantee.

The adapter must:

- create or initialize an empty versioned cassette for record mode;
- preserve interaction ordering across multiple appends;
- open replay cassettes read-only;
- reject unsupported `format_version` values with actionable errors;
- return typed `HarnessResult` failures rather than panicking.

Go/no-go: adapter-level unit tests prove append ordering, load validation, and
read-only replay semantics.

### Stage D: wire startup and replay validation through the composition root

Integrate the new cassette adapter into the existing startup path in
`src/lib.rs` without changing the public API.

Expected wiring for this task:

- `Mode::Replay`: resolve `cassette_path`, open the cassette through the
  read-only adapter, and fail fast on missing files, malformed files, or
  unsupported `format_version`.
- `Mode::Record`: initialize the append-capable cassette store so the runtime
  has a ready persistence foundation for later record-mode HTTP work.
- `Mode::Verify`: if the current code path touches cassette loading here, keep
  it aligned with replay's version validation. If verify remains CLI-only for
  now, record that limitation explicitly in the design document.

Do not expand this task into HTTP routing, request proxying, or replay
matching. Those belong to later roadmap items.

Go/no-go: startup-oriented unit tests and BDD scenarios pass without changing
the public library surface.

### Stage E: document the final behaviour and close the roadmap item

Update `docs/spycatcher-harness-design.md` with the decisions that become real
in code. At minimum record:

- the on-disk cassette shape and supported `format_version`;
- the meaning of "append-only persistence" for this release;
- the cassette file naming decision relative to `cassette_name`;
- the domain/adapter split for cassette storage.

Update `docs/users-guide.md` with user-visible changes:

- replay now opens and validates cassettes at startup;
- unsupported cassette versions fail fast with actionable diagnostics;
- any clarified cassette naming rules or file examples.

Only after code, tests, and docs are complete should `docs/roadmap.md` mark
task `1.2.1` as done.

### Stage F: run full validation with logged evidence

Run every required gate through `tee` so truncated terminal output does not
hide failures. Use one log file per command.

```bash
set -o pipefail
make fmt 2>&1 | tee /tmp/1-2-1-fmt.log
make check-fmt 2>&1 | tee /tmp/1-2-1-check-fmt.log
make lint 2>&1 | tee /tmp/1-2-1-lint.log
make test 2>&1 | tee /tmp/1-2-1-test.log
make markdownlint 2>&1 | tee /tmp/1-2-1-markdownlint.log
make nixie 2>&1 | tee /tmp/1-2-1-nixie.log
```

Expected end state:

- `make lint` finishes without Clippy, Rustdoc, or Whitaker warnings;
- `make test` passes unit tests, behavioural tests, and doctests;
- Markdown validation passes after documentation updates;
- the roadmap item is checked off only after every command above exits zero.
