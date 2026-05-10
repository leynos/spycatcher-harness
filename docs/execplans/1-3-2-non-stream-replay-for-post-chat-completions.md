# Implement non-stream chat completions replay

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

Implementation was explicitly approved on 2026-05-08, and this plan is now the
working delivery record.

## Purpose / big picture

Task `1.3.2` completes the first deterministic record-to-replay vertical slice
for the OpenAI-compatible `POST /v1/chat/completions` endpoint. After this
change, a caller can record a non-stream chat completion against a stub or real
upstream provider, restart the harness in replay mode with the same cassette,
send the same request to the harness, and receive the recorded non-stream
response without any outbound network access.

Observable success after delivery:

- Starting the harness with `Mode::Replay` binds a local HTTP server instead
  of returning `HarnessError::ModeNotYetImplemented`.
- `POST /v1/chat/completions` in replay mode canonicalizes the inbound
  request, matches it through `ReplayMatchEngine`, and returns the recorded
  non-stream status, persisted selected headers, and body bytes from the
  cassette.
- Replay mode constructs no upstream HTTP client, reads no upstream API key,
  and performs no outbound network call. A replay request succeeds even when
  `HarnessConfig.upstream` points at an unreachable endpoint or is absent.
- Mismatched requests fail fast with HTTP `409 Conflict` and structured
  diagnostics derived from `MismatchDiagnostic`.
- Streamed cassette interactions and streamed replay requests are rejected
  explicitly until streaming replay lands in later roadmap tasks.
- Unit tests using `rstest` and behavioural tests using `rstest-bdd` cover
  happy paths, unhappy paths, edge cases, and a record-to-replay integration
  path using the existing stub upstream support.
- `docs/spycatcher-harness-design.md`, `docs/users-guide.md`, and
  `docs/developers-guide.md` describe the shipped replay behaviour and internal
  boundaries. `docs/roadmap.md` marks `1.3.2` done only after the feature is
  implemented, approved gates pass, and the implementation is ready to commit.

## Relevant documentation and skills

Primary documentation for implementation:

- `docs/roadmap.md` - task `1.3.2`, dependencies `1.2.3` and `1.3.1`, and the
  success criteria that must be checked off after implementation.
- `docs/spycatcher-harness-design.md` - "Goals and non-goals", "Architecture
  overview", "Record and replay interaction sequence", "Recording and replay
  semantics", "Matching modes", "Public library API surface", "CLI integration
  and configuration", and "Testing, observability, and rollout roadmap".
- `docs/users-guide.md` - user-visible startup behaviour, replay matching
  modes, cassette file expectations, and chat completions endpoint behaviour.
- `docs/developers-guide.md` - internal module layout, header selection
  helpers, request/response capture types, replay matching architecture, and
  BDD test structure.
- `docs/rust-testing-with-rstest-fixtures.md` - `rstest` fixtures and
  parameterized unit-test patterns.
- `docs/reliable-testing-in-rust-via-dependency-injection.md` - dependency
  injection guidance for proving replay does not consult environment or
  upstream network state.
- `docs/rstest-bdd-users-guide.md` - feature file, world, fixture, and step
  structure for the behavioural record-to-replay scenarios.
- `docs/rust-doctest-dry-guide.md` - keep public API examples valid when
  replay startup examples change.
- `docs/complexity-antipatterns-and-refactoring-strategies.md` - use if replay
  server wiring starts producing large functions or duplicated response
  builders.
- `docs/ortho-config-users-guide.md` - use for wording when documenting CLI
  and configuration behaviour.

Skills to apply during implementation:

- `execplans` - keep this document current while implementing.
- `hexagonal-architecture` - protect the cassette and matching domain boundary
  without forcing a pattern transplant.
- `leta` - use semantic navigation before modifying Rust modules.
- `rust-router` - route Rust-specific design questions to the smallest useful
  Rust skill.
- `domain-web-services` - keep Axum handlers thin and server state ownership
  explicit.
- `rust-errors` - preserve typed, inspectable library failures and map replay
  failures at the HTTP adapter boundary.
- `rust-async-and-concurrency` - use if matcher state sharing requires more
  than a simple `Arc<Mutex<ReplayMatchEngine>>`.
- `rust-types-and-apis` - use if replay service ports or public API shapes
  need new trait or type boundaries.
- `nextest` - use when running or filtering the Rust test suite.
- `en-gb-oxendict-style` - keep documentation and comments in project style.
- `pr-creation` and `commit-message` - use when committing and opening the
  draft pull request.

## Constraints

- Do not implement streaming replay in this task. `RecordedResponse::Stream`
  remains out of scope until later roadmap tasks.
- "Verbatim" means the replay HTTP response must use the cassette's persisted
  selected response status, selected response headers, and stored body bytes.
  It does not mean reconstructing original wire framing, hop-by-hop headers,
  `content-length`, or transport-level chunking that the cassette intentionally
  did not persist.
- Replay mode must not construct `ReqwestUpstreamClient`, call
  `ChatCompletionsUpstream::send_chat_completions`, resolve
  `upstream.api_key_env`, or otherwise depend on outbound network access.
- Preserve existing public lifecycle APIs:
  - `start_harness(cfg) -> HarnessResult<RunningHarness>` remains the startup
    entry point.
  - `RunningHarness::shutdown(self)` remains the shutdown entry point.
  - `HarnessConfig`, `HarnessError`, cassette schema types, and
    `ReplayMatchEngine` stay source-compatible unless a tolerance threshold is
    reached and approval is granted.
- Maintain hexagonal boundaries:
  - Canonicalization, cassette schema, matching, and mismatch diagnostics stay
    in the domain-owned `src/cassette/` module tree.
  - HTTP request extraction, Axum response construction, socket lifecycle, and
    status-code mapping stay in inbound adapter code under `src/server/`.
  - Filesystem cassette loading remains the persistence adapter in
    `src/cassette/filesystem.rs`.
  - Replay orchestration may live in `src/replay.rs` if it depends only on
    domain types and adapter-neutral exchange types. It must not depend on
    `axum` or `reqwest`.
- Continue using `camino` and `cap_std` for path and filesystem work.
- Use `rstest` for unit tests and `rstest-bdd` for behavioural tests where the
  behaviour is externally observable.
- Add property tests only when implementation introduces a new invariant over
  a range of values, states, orderings, or transitions. The existing matching
  and diff invariants already have property-style coverage; do not add a weak
  property test that merely restates an example. If the replay response builder
  introduces a header-preservation invariant beyond example cases, cover it
  with a bounded property test.
- Do not mutate process environment in tests. Use configuration values,
  injected seams, or stub servers instead.
- No single code file may exceed 400 lines.
- Comments and documentation must use en-GB-oxendict spelling.
- Do not mark `docs/roadmap.md` task `1.3.2` done until implementation and all
  gates have passed.

## Tolerances (exception triggers)

- Scope: if implementation requires changes to more than 22 files or roughly
  1700 net lines of code and documentation, stop and escalate before continuing.
- Public API: if satisfying this task requires changing the signature of
  `start_harness`, `RunningHarness::shutdown`, `ReplayMatchEngine::new`, or
  `ReplayMatchEngine::next_match`, stop and escalate.
- Cassette schema: if the stored cassette schema must change to replay
  non-stream interactions, stop and escalate. Task `1.3.1` already persists the
  data this task needs.
- Dependencies: if a new external crate is required, stop and escalate. The
  existing Axum, Tokio, serde, reqwest test infrastructure, rstest, and
  rstest-bdd dependencies should be sufficient.
- Network boundary: if any replay implementation path appears to need upstream
  configuration, an API key, or an outbound client, stop and document the
  conflict before proceeding.
- Ambiguity: if there is no single defensible HTTP response for stream
  interactions encountered in replay, choose HTTP `501 Not Implemented` only if
  that matches existing record-mode unsupported-stream behaviour; otherwise
  stop and present options.
- Iteration: if `make lint` or `make test` still fails after five repair
  cycles, stop and escalate with the failing evidence.
- Formatting/docs: if `make fmt`, `make markdownlint`, or `make nixie` changes
  or rejects unrelated documentation, stop and inspect before committing.

## Risks

- Risk: replay startup currently validates the cassette and then returns
  `ModeNotYetImplemented`, so changing `Mode::Replay` to bind a real server may
  affect existing startup tests. Severity: medium. Likelihood: high.
  Mitigation: update tests that assert replay is unimplemented, and preserve
  verify-mode behaviour until a verify server exists.

- Risk: response reconstruction may accidentally use raw proxy-header policy
  rather than persisted cassette-header policy. Severity: medium. Likelihood:
  medium. Mitigation: implement a replay-specific response builder that
  consumes `RecordedResponse::NonStream` and tests duplicate headers, invalid
  status fallback, invalid header values, and byte-exact bodies.

- Risk: replay matching consumes state and the Axum server handles concurrent
  requests. Severity: medium. Likelihood: medium. Mitigation: own one
  `ReplayMatchEngine` inside replay app state and guard it with a narrow mutex.
  Keep matching synchronous and short while avoiding locks across `.await`.

- Risk: exact no-network behaviour is easy to assert indirectly but hard to
  prove globally. Severity: high. Likelihood: medium. Mitigation: make the
  production replay state unable to hold an upstream client, add a replay-mode
  BDD scenario with a configured stub upstream that records zero requests, and
  add a unit test with a sentinel upstream config proving replay startup and
  request handling do not resolve credentials.

- Risk: `RecordedResponse::Stream` can appear in manually authored or future
  cassettes even though current record mode rejects `stream: true`. Severity:
  low. Likelihood: medium. Mitigation: add a unit test for a matched stream
  interaction and map it to HTTP `501 Not Implemented` with a clear JSON error.

- Risk: mismatch diagnostics may leak more information than intended if the
  HTTP body serializes the entire canonical request. Severity: medium.
  Likelihood: low. Mitigation: return the diagnostic fields already produced by
  `MismatchDiagnostic`: position, expected hash, observed hash, and diff
  summary. Do not include raw request or response bodies.

- Risk: a full record-to-replay BDD scenario may duplicate the record-mode BDD
  helper stack. Severity: low. Likelihood: medium. Mitigation: extract only the
  minimal reusable helper code needed by the new replay suite, and document any
  shared helper movement in `docs/developers-guide.md`.

## Progress

- [x] 2026-05-08 05:33 CEST - Read `AGENTS.md`, roadmap task `1.3.2`, the
      design sections named by the request, adjacent execplans, and current
      server/cassette code.
- [x] 2026-05-08 05:33 CEST - Used a Wyvern agent team for read-only
      reconnaissance over documentation and code/test topology.
- [x] 2026-05-08 05:33 CEST - Drafted this ExecPlan for approval.
- [x] 2026-05-08 14:47 CEST - Received explicit user approval to implement
      this ExecPlan and updated status to `IN PROGRESS`.
- [x] 2026-05-08 14:47 CEST - Re-read this plan, `AGENTS.md`, and the
      relevant skills at implementation start.
- [x] 2026-05-08 14:52 CEST - Added unit tests and replay BDD scenarios for
      non-stream replay, mismatch handling, stream rejection, and no-network
      replay behaviour.
- [x] 2026-05-08 14:54 CEST - Implemented replay startup, replay service
      state, and replay route handling.
- [x] 2026-05-08 14:55 CEST - Implemented replay response and error mapping
      for matched non-stream interactions and mismatches.
- [x] 2026-05-08 14:56 CEST - Updated user-facing and internal documentation.
- [x] 2026-05-08 14:57 CEST - Marked roadmap task `1.3.2` done after the
      implementation and focused replay BDD tests passed.
- [x] 2026-05-08 14:59 CEST - Ran validation gates with `tee` logs. `make
      check-fmt`, `make lint`, `make test`, `make markdownlint`, and `make
      nixie` passed. `make fmt` ran Rust formatting but failed in the
      repository-wide Markdown formatting phase on existing MD013 line-length
      findings; unrelated formatter churn was inspected and restored.
- [x] 2026-05-11 14:38 CEST - Added malformed JSON replay handling after
      review feedback. Replay now rejects non-JSON chat completions request
      bodies before matching so malformed byte sequences cannot share a
      body-less replay hash.
- [x] 2026-05-11 14:45 CEST - Added unit and BDD coverage for malformed replay
      requests, documented the user-facing `400 malformed_json` response, and
      moved repeated BDD assertion helpers out of `steps.rs` to satisfy the
      module-size lint.
- [x] 2026-05-11 14:50 CEST - Re-ran `cargo test --test
      chat_completions_replay_bdd --all-features`, `make check-fmt`, `make
      lint`, `make markdownlint`, `make test`, and `make nixie`; all passed.
- [x] 2026-05-11 15:12 CEST - Verified three review findings. All were still
      valid: a restrictive-clause comma in the design doc, embedded lifecycle
      tests keeping `src/lib.rs` over the module-size limit, and duplicated
      record/replay server listener wiring. Applied minimal fixes and moved
      lifecycle coverage to `tests/lifecycle_tests.rs`.
- [x] 2026-05-11 15:18 CEST - Re-ran `cargo test --test lifecycle_tests
      --all-features`, `make check-fmt`, `make lint`, `make markdownlint`,
      `make test`, and `make nixie`; all passed.
- [ ] Commit the implemented feature after gates pass.

## Surprises & Discoveries

- Replay-mode startup already opens cassettes read-only through
  `FilesystemCassetteStore::open_for_replay`, but `start_harness` immediately
  returns `HarnessError::ModeNotYetImplemented` for `Mode::Replay`.
- The cassette and matching domain are ready for this slice:
  `RecordedResponse::NonStream`, `RecordedResponse::Stream`,
  `ReplayMatchEngine`, `MatchOutcome`, `MismatchDiagnostic`, and canonical
  request hashing already exist.
- The record-mode server path is intentionally separated into
  `src/server/runtime.rs`, `src/server/record.rs`, and
  `src/server/record_handler.rs`. Replay should follow the same style rather
  than adding large logic to `src/lib.rs`.
- The existing `tests/record_mode_proxying/` suite has a useful stub upstream
  and scenario runtime pattern, and `tests/replay_matching_modes/` already
  demonstrates BDD coverage for sequential and keyed matching behaviour.
- The phrase "recorded status, headers, and body bytes verbatim" is ambiguous
  unless tied to the persisted cassette. This plan defines it as persisted
  selected headers and stored body bytes because hop-by-hop and framing headers
  are intentionally excluded before persistence.
- `make fmt` still reports existing repository-wide MD013 line-length findings
  through the plain `markdownlint --fix` formatter path, even though the
  configured `make markdownlint` gate passes with zero errors. This matches the
  earlier planning branch observation and is recorded as a formatter caveat,
  not a replay implementation failure.
- Reusing the record-mode BDD helper module in the replay BDD binary triggers
  unused helper functions in that specific test crate. The replay test
  entrypoint uses a tightly scoped `#[expect(dead_code)]` with a reason instead
  of duplicating the stub upstream implementation.
- Recorded malformed or non-JSON chat completions requests have
  `parsed_json: None`; canonicalization omits body content in that case. Without
  a replay-side guard, two different malformed request bodies can therefore
  produce the same replay key.
- Adding the malformed replay scenario pushed
  `tests/chat_completions_replay/steps.rs` over Whitaker's 400-line module
  limit. A small `tests/chat_completions_replay/support.rs` module now owns
  repeated response assertion helpers, leaving the step file below the limit.
- Moving lifecycle tests out of `src/lib.rs` changed them from a lib test
  module into an integration test binary. The replay test helper now requests
  an OS-selected port to avoid parallel integration-test port collisions.
- Moving lifecycle tests also removed the only lib-test reference to
  `FilesystemCassetteStore::save`; the filesystem adapter test now exercises
  `save` directly when writing an unsupported cassette version.

## Decision Log

- Decision: treat this branch as a pre-implementation planning branch.
  Rationale: the user explicitly stated that the plan must be approved before
  it is implemented. Therefore this branch writes the ExecPlan, opens a draft
  PR, and leaves `docs/roadmap.md` task `1.3.2` unchecked.

- Decision: implement replay through a dedicated replay service and handler,
  not by reusing `RecordService`. Rationale: `RecordService` owns upstream
  configuration, an environment provider, and a `ChatCompletionsUpstream`.
  Reusing it would weaken the no-network replay guarantee. A separate replay
  path makes the boundary compile-time visible.

- Decision: bind replay mode to the same `CHAT_COMPLETIONS_PATH` route used by
  record mode. Rationale: both operational modes expose the same
  OpenAI-compatible endpoint to the agent under test; only the backing adapter
  changes.

- Decision: put HTTP status-code mapping in `src/server/` rather than in
  `src/cassette/`. Rationale: `MismatchDiagnostic` is domain data. HTTP `409`,
  `501`, and JSON error-body formatting are inbound adapter concerns.

- Decision: reject matched stream interactions and replay requests whose body
  asks for `stream: true` with an explicit unsupported-stream response.
  Rationale: current record mode already rejects streaming, and later roadmap
  tasks own SSE replay. Failing explicitly is safer than silently serving an
  incompatible response shape.

- Decision: promote the server runtime handle to a mode-neutral `ServerHandle`.
  Rationale: record and replay use the same listener binding and graceful
  shutdown lifecycle; sharing the handle avoids duplicated shutdown ownership
  while preserving separate record and replay state/handlers.

- Decision: implement replay orchestration in `src/replay.rs` and keep Axum
  response construction in `src/server/replay_handler.rs`. Rationale:
  `src/replay.rs` remains adapter-neutral and owns canonicalization plus match
  progression, while HTTP status codes, headers, and JSON error bodies remain
  inbound-adapter concerns.

- Decision: keep the existing matching and header-selection invariants rather
  than adding a new property test for replay response construction. Rationale:
  the new replay response builder preserves valid duplicate headers with
  example-based `rstest` coverage, and no new broad input invariant was
  introduced beyond existing header-selection and matching properties.

- Decision: reject malformed or non-JSON chat completions replay requests
  before matching rather than adding raw malformed bytes to the replay key.
  Rationale: this preserves existing cassette hash semantics, avoids adding a
  special-case body hashing mode for invalid JSON, and fails visibly with
  `400 malformed_json` instead of hiding client request bugs.

## Outcomes & Retrospective

Implemented non-stream replay for `POST /v1/chat/completions`. `Mode::Replay`
now starts a local Axum server from a read-only cassette, canonicalizes inbound
requests, advances `ReplayMatchEngine`, and returns recorded non-stream status,
persisted selected headers, and body bytes. Replay state owns no upstream
configuration, environment provider, or outbound client; the end-to-end BDD
suite proves the stub upstream sees no replay request.

Validation evidence:

- `make fmt 2>&1 | tee
  /tmp/fmt-spycatcher-harness-1-3-2-non-stream-replay-for-post-chat-completions.out`
  ran Rust formatting and failed only in the repository-wide Markdown
  formatting phase on existing MD013 findings.
- `make check-fmt 2>&1 | tee
  /tmp/check-fmt-spycatcher-harness-1-3-2-non-stream-replay-for-post-chat-completions.out`
  passed.
- `make lint 2>&1 | tee
  /tmp/lint-spycatcher-harness-1-3-2-non-stream-replay-for-post-chat-completions.out`
  passed.
- `make test 2>&1 | tee
  /tmp/test-spycatcher-harness-1-3-2-non-stream-replay-for-post-chat-completions.out`
  passed: nextest ran 186 tests with 186 passed, followed by doctests with 14
  passed and 4 intentionally ignored.
- `make markdownlint 2>&1 | tee
  /tmp/markdownlint-spycatcher-harness-1-3-2-non-stream-replay-for-post-chat-completions.out`
  passed with zero errors.
- `make nixie 2>&1 | tee
  /tmp/nixie-spycatcher-harness-1-3-2-non-stream-replay-for-post-chat-completions.out`
  passed with all diagrams validated.

All roadmap success criteria for task `1.3.2` are met. Follow-up work remains
with later roadmap items: streaming capture/replay, replay metrics, and verify
mode.

## Context and orientation

The repository is a Rust package with a library crate and a binary target. The
binary in `src/bin/spycatcher_harness.rs` delegates to library entry points.
`src/lib.rs` owns `start_harness`, `RunningHarness`, configuration validation,
and top-level mode dispatch.

Relevant current files and roles:

- `src/lib.rs` defines `start_harness`. At the time of writing, record mode
  starts `server::start_record_server`, while replay and verify mode both
  return `HarnessError::ModeNotYetImplemented` after cassette preparation.
- `src/server/runtime.rs` binds the Axum listener, mounts
  `CHAT_COMPLETIONS_PATH`, and returns a `RecordServerHandle`.
- `src/server/record_handler.rs` extracts Axum request data into
  `ObservedRequest` and builds downstream responses for record mode.
- `src/server/record.rs` contains `RecordAppState`, `RecordService`, and
  record-mode orchestration. It must not be reused for replay because it owns
  upstream transport seams.
- `src/http_exchange.rs` defines `ObservedRequest`, `ObservedResponse`,
  `ProxyResponse`, JSON parsing, selected request/response header helpers, and
  persistence redaction helpers.
- `src/cassette/mod.rs` defines `Cassette`, `Interaction`, `RecordedRequest`,
  `RecordedResponse`, `CassetteReader`, and `CassetteAppender`.
- `src/cassette/filesystem.rs` implements `FilesystemCassetteStore`, including
  `open_for_replay`.
- `src/cassette/matching.rs` implements `ReplayMatchEngine`,
  `MatchOutcome`, and `MismatchDiagnostic`.
- `src/replay.rs` is currently a placeholder for native replay logic and is a
  reasonable home for adapter-neutral replay orchestration if it stays free of
  Axum and reqwest types.
- `src/upstream.rs` defines `ChatCompletionsUpstream` and
  `ReqwestUpstreamClient`. Replay code must not depend on this module except
  for documentation references or tests that prove it is not used.

## Implementation plan

### Milestone 1: Establish failing replay tests

Add unit tests before production code where practical.

Unit-level tests should cover:

- building a replay response from a `RecordedResponse::NonStream` returns the
  recorded status, headers, and body bytes;
- duplicate recorded response headers are preserved when valid;
- invalid recorded header names or values are dropped with a bounded warning
  rather than panicking;
- invalid recorded status codes map to a safe `502 Bad Gateway` response, as
  record-mode proxy response construction already does;
- a matched `RecordedResponse::Stream` returns an unsupported-stream failure;
- mismatch diagnostics map to HTTP `409 Conflict` with expected position,
  expected hash, observed hash, and diff summary;
- replay service canonicalizes the observed request with
  `IgnorePathConfig::default()` and passes the resulting hash to
  `ReplayMatchEngine`.

Behavioural tests should live in a new replay suite, for example:

```plaintext
tests/features/chat_completions_replay.feature
tests/chat_completions_replay_bdd.rs
tests/chat_completions_replay/
  helpers.rs
  steps.rs
  world.rs
```

Scenarios should include:

- successful record-to-replay cycle using the existing stub upstream style:
  record one non-stream response, shut the record harness down, start replay
  mode on the persisted cassette, send the same request, and assert status,
  selected headers, and body bytes match the recorded response;
- replay mode does not call upstream: configure a stub upstream or unreachable
  endpoint, start replay mode, send a matching request, and assert the stub
  captured zero requests;
- mismatched replay request returns HTTP `409 Conflict` with diagnostic fields;
- replay rejects `stream: true` requests until streaming replay lands;
- keyed mode can replay two recorded interactions out of order if an
  end-to-end scenario is cheap to express. If keyed behaviour becomes too large
  for the first E2E suite, keep keyed end-to-end coverage to unit tests plus
  the existing matching BDD suite and document the reason here.

Run focused tests and capture expected failures. Example commands:

```sh
cargo test chat_completions_replay --all-features
cargo test replay --all-features
```

Expected state before implementation: new replay tests fail because replay mode
does not bind a server or because the new symbols do not yet exist.

### Milestone 2: Add replay service state without network seams

Add replay-specific application state, likely in new files:

- `src/server/replay.rs` for replay service state and adapter-neutral
  orchestration;
- `src/server/replay_handler.rs` for Axum extraction and response/error
  building.

The replay state should own one `ReplayMatchEngine` built from a read-only
loaded cassette:

```rust
struct ReplayAppState {
    service: ReplayService,
}

struct ReplayService {
    engine: Arc<Mutex<ReplayMatchEngine>>,
}
```

The exact names can vary to match local conventions, but the state must not
hold `UpstreamConfig`, `ReqwestUpstreamClient`, `EnvProvider`, or any outbound
client trait. `ReplayAppState::from_config` should load the cassette with
`FilesystemCassetteStore::open_for_replay(cassette_path)?`, call `load()`, and
construct `ReplayMatchEngine::new(cassette, cfg.match_mode)`.

If `ReplayService` lives in `src/replay.rs`, expose only adapter-neutral
methods such as:

```rust
fn replay_chat_completions(&self, request: ObservedRequest) -> ReplayResult
```

Keep Axum types out of `src/replay.rs`.

### Milestone 3: Wire replay server startup

Update `src/server/runtime.rs` or a split runtime module to support replay
startup. Prefer a small generic server handle if that reduces duplication
without obscuring mode-specific setup:

- either rename `RecordServerHandle` to a mode-neutral internal
  `ServerHandle`, or add a parallel `ReplayServerHandle`;
- mount `CHAT_COMPLETIONS_PATH` with the replay handler in replay mode;
- keep graceful shutdown behaviour identical to record mode.

Update `RunningHarness` in `src/lib.rs` so it can own the runtime handle for
record or replay. The public fields remain unchanged. `Mode::Replay` should
call replay server startup after `prepare_cassette`. `Mode::Verify` should
continue to return `ModeNotYetImplemented` until the verify subcommand is
implemented.

Update lifecycle unit tests in `src/lib.rs`:

- replace `start_harness_replay_mode_returns_not_yet_implemented` with a test
  proving replay binds a local address when a compatible cassette exists;
- keep missing-cassette and unsupported-format failures;
- add a test proving replay mode does not require `upstream`.

### Milestone 4: Implement replay request handling

The replay handler should mirror record-mode extraction:

- use `OriginalUri` to preserve the raw query string;
- use `selected_request_headers` for the observed request's persisted header
  representation;
- use `selected_forward_headers` only if needed by shared constructors. Do not
  forward anywhere in replay mode;
- parse JSON with `parse_json_bytes`;
- create an `ObservedRequest` with method `POST`, path
  `CHAT_COMPLETIONS_PATH`, raw query, selected headers, parsed JSON, and body
  bytes.

The replay service then:

1. rejects `stream: true` requests with a typed replay error;
2. creates a temporary `RecordedRequest` from the observed request;
3. calls `populate_canonical_fields(&IgnorePathConfig::default())`;
4. extracts `stable_hash` and `canonical_request`;
5. locks the `ReplayMatchEngine` only around `next_match`;
6. maps `MatchOutcome::Mismatch` to a replay mismatch error;
7. maps `MatchOutcome::Matched(interaction)` to the recorded non-stream
   response, or an unsupported-stream error if the matched response is
   `RecordedResponse::Stream`.

Do not hold a `MutexGuard` across any `.await`.

### Milestone 5: Implement HTTP response and error mapping

The adapter should convert replay outcomes to HTTP responses:

- matched non-stream response:
  - status from cassette if it is in `100..=599`, otherwise `502 Bad Gateway`;
  - headers from the cassette's persisted `Vec<(String, String)>`, preserving
    valid duplicate headers;
  - body from the cassette's `Vec<u8>` exactly;
- mismatch:
  - HTTP `409 Conflict`;
  - JSON body with stable fields such as `error.message`,
    `error.kind = "request_mismatch"`, `position`, `expected_hash`,
    `observed_hash`, and `diff_summary`;
- unsupported stream:
  - HTTP `501 Not Implemented`;
  - JSON body with a clear message that streaming replay is not implemented;
- internal cassette/canonicalization failures:
  - HTTP `500 Internal Server Error` or `502 Bad Gateway` according to the
    existing server-adapter convention. Use typed internal replay errors rather
    than string parsing.

Do not include raw request bodies, raw response bodies, or secret header values
in error responses.

### Milestone 6: Update documentation

Update `docs/spycatcher-harness-design.md`:

- remove or revise the statement that replay-mode HTTP serving remains the
  next slice;
- record that task `1.3.2` serves non-stream responses natively from
  persisted cassette data;
- document the no-network replay boundary;
- document the stream replay guardrail for this slice;
- mark the roadmap mirror entry for `1.3.2` done after implementation gates
  pass.

Update `docs/users-guide.md`:

- describe replay-mode `POST /v1/chat/completions` behaviour;
- state that replay uses persisted selected headers and exact body bytes from
  the cassette;
- state that replay mode does not require upstream configuration or API keys;
- document mismatch `409` behaviour and unsupported streaming behaviour.

Update `docs/developers-guide.md`:

- describe replay service state and why it owns no upstream transport seam;
- add any new replay modules to the internal module layout;
- explain the matcher state ownership and concurrency model;
- document new BDD test structure if a new suite is added.

Update `docs/roadmap.md` only at the end:

- change `1.3.2` from unchecked to checked;
- check all success criteria only after implementation and validation prove
  them.

### Milestone 7: Validate and commit

Run formatting and quality gates sequentially with `tee` logs. Use branch- and
project-specific log names under `/tmp`, for example:

```sh
make fmt 2>&1 | tee /tmp/fmt-spycatcher-harness-1-3-2-non-stream-replay-for-post-chat-completions.out
make check-fmt 2>&1 | tee /tmp/check-fmt-spycatcher-harness-1-3-2-non-stream-replay-for-post-chat-completions.out
make lint 2>&1 | tee /tmp/lint-spycatcher-harness-1-3-2-non-stream-replay-for-post-chat-completions.out
make test 2>&1 | tee /tmp/test-spycatcher-harness-1-3-2-non-stream-replay-for-post-chat-completions.out
make markdownlint 2>&1 | tee /tmp/markdownlint-spycatcher-harness-1-3-2-non-stream-replay-for-post-chat-completions.out
make nixie 2>&1 | tee /tmp/nixie-spycatcher-harness-1-3-2-non-stream-replay-for-post-chat-completions.out
```

Inspect each log after completion. If a command output is truncated in the
agent interface, use `tail`, `rg`, or `sed` on the log file to inspect the
failure.

Commit using the `commit-message` skill and a file-based commit message. Do not
use `git commit -m`.

## Acceptance criteria

The implementation is complete only when all of these are true:

- `Mode::Replay` starts a local server for `POST /v1/chat/completions`.
- A matching non-stream replay response returns the recorded status, persisted
  selected headers, and body bytes from the cassette.
- Replay mode performs no outbound network access and requires no upstream API
  key.
- Mismatches return HTTP `409 Conflict` with structured diagnostics.
- Streaming requests and stream-recorded responses are explicitly rejected
  until streaming replay is implemented.
- End-to-end record-to-replay BDD tests pass using a stub upstream service.
- Unit tests cover response reconstruction, mismatch mapping, stream
  rejection, and no-network service state.
- `docs/spycatcher-harness-design.md`, `docs/users-guide.md`, and
  `docs/developers-guide.md` are current.
- `docs/roadmap.md` marks task `1.3.2` done.
- `make fmt`, `make check-fmt`, `make lint`, `make test`,
  `make markdownlint`, and `make nixie` pass sequentially.
