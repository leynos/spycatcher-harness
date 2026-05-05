# Implement chat completions record mode proxying

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

Implementation was explicitly approved by the user on 2026-04-20.

## Purpose / big picture

Task `1.3.1` is the first real HTTP-serving slice of the harness. After this
change, running the harness in record mode will start an actual local HTTP
server, accept `POST /v1/chat/completions` requests with `stream` unset or
`false`, forward those requests to the configured upstream, return the upstream
response to the caller, and append a deterministic cassette entry for the
interaction.

Observable success after delivery:

- Starting the harness in record mode binds a real server socket rather than
  only validating configuration.
- Sending a non-stream chat completions request to the harness returns the
  upstream status, headers, and body bytes to the client.
- The cassette records the incoming request as seen by the harness, plus the
  non-stream upstream response as exact bytes and parsed JSON when valid.
- Configured redaction removes secret headers from persisted cassette data
  without preventing the proxy from authenticating to the upstream.
- `rstest` unit tests and `rstest-bdd` behavioural tests cover happy paths,
  unhappy paths, and edge cases.
- `docs/spycatcher-harness-design.md` records the implementation decisions,
  `docs/users-guide.md` documents the behaviour, and `docs/roadmap.md` marks
  `1.3.1` done only after every gate passes.

## Relevant documentation and skills

Primary documentation for implementation:

- `docs/roadmap.md` - task scope, dependencies, and completion criteria for
  `1.3.1`.
- `docs/spycatcher-harness-design.md` - architecture overview, cassette
  definition, streaming capture and replay, public API surface, and testing
  strategy.
- `docs/users-guide.md` - public behaviour and configuration surface that must
  stay accurate after the change.
- `docs/rust-testing-with-rstest-fixtures.md` - fixture structure and
  parameterized unit test patterns.
- `docs/reliable-testing-in-rust-via-dependency-injection.md` - dependency
  injection guidance for environment and time dependent logic.
- `docs/rstest-bdd-users-guide.md` - feature file structure, `ScenarioState`,
  fixtures, and step-definition conventions.
- `docs/ortho-config-users-guide.md` - command and configuration wording when
  the user's guide is updated.
- `docs/rust-doctest-dry-guide.md` - keep doctest examples valid when public
  API examples change.
- `docs/complexity-antipatterns-and-refactoring-strategies.md` - use when the
  first HTTP slice starts pushing functions or modules past healthy size.

Skills to apply during implementation:

- `execplans` - keep this document current during execution.
- `hexagonal-architecture` - preserve domain versus adapter boundaries.
- `rust-router` - route Rust-specific implementation questions to the smallest
  useful skill.
- `domain-web-services` - for HTTP server lifecycle, routing, and shutdown.
- `rust-errors` - for HTTP and upstream error mapping without leaking
  framework-specific error types into the domain.
- `nextest` - for running and filtering the Rust test suite.
- `leta` - for semantic code navigation before modifying Rust modules.

## Constraints

- This task is limited to the non-stream path of
  `POST /v1/chat/completions`. Requests whose JSON body contains
  `"stream": true` must not be treated as successful non-stream recordings.
  They need an explicit unsupported-path response and zero cassette writes
  until streaming work lands in task `2.1.1`.
- Maintain hexagonal boundaries:
  - HTTP extraction, header filtering, response rendering, and socket
    lifecycle belong in adapter code under `src/server.rs` and submodules.
  - Upstream HTTP client code belongs in `src/upstream.rs` and submodules.
  - Domain-owned cassette types stay in `src/cassette/`.
  - The orchestration that turns an observed HTTP exchange into an
    `Interaction` should depend on ports and domain types, not directly on
    `axum`, `reqwest`, or other framework types.
- Preserve the existing public API contract:
  - `start_harness(cfg)` and `RunningHarness::shutdown()` remain the entry
    points.
  - `HarnessConfig`, `HarnessError`, and existing cassette schema types stay
    source-compatible unless a tolerance threshold is reached and approved.
- The recorded request in the cassette must represent what the client sent to
  the harness, not the enriched outbound request sent upstream. Upstream-only
  authentication and configured `extra_headers` are transport details and must
  not pollute replay matching inputs.
- Persist non-stream responses as exact body bytes. JSON parsing is additive:
  successful `serde_json::from_slice` populates `parsed_json`; parse failure
  leaves `parsed_json` as `None` without changing the stored bytes.
- Apply header redaction before persistence, using case-insensitive header-name
  matching while preserving the observed order and duplicates of retained
  headers.
- Continue using `camino` and `cap_std` rather than `std::path` or `std::fs`.
- Avoid direct environment mutation or `std::env::var` calls inside logic that
  needs unit coverage. Use dependency injection for environment access and, if
  time values become observable in tests, for clock access as well.
- No single source file may exceed 400 lines. Split the first HTTP slice into
  submodules rather than building one oversized `server.rs`.
- Comments and documentation must use en-GB-oxendict spelling.
- Completion requires the applicable quality gates with `tee` logging:
  `make fmt`, `make check-fmt`, `make lint`, `make test`, `make markdownlint`,
  and `make nixie`.

## Tolerances (exception triggers)

- Scope: if implementing this task requires changes to more than 20 files or
  roughly 1600 net lines, stop and escalate before continuing.
- Public API: if satisfying this task requires changing the signature of
  `start_harness`, `RunningHarness::shutdown`, or the shape of
  `RecordedRequest` / `RecordedResponse`, stop and escalate.
- Dependencies: adding one inbound HTTP stack and one outbound HTTP client is
  expected. If implementation appears to need a second server/client stack or
  more than five new crates in total, stop and escalate.
- Ambiguity: if the selected-header policy, unsupported-stream behaviour, or
  upstream URL construction still has multiple defensible interpretations after
  consulting the design and current docs, stop and present options before
  coding further.
- Iteration: if `make lint` or `make test` still fails after five repair
  cycles, stop and escalate with the failing evidence.

## Risks

- Risk: this task is larger than a route handler because the repository still
  has placeholder `server`, `protocol`, and `upstream` modules. It is also the
  first task that makes `start_harness` genuinely asynchronous. Mitigation:
  treat server binding, request handling, upstream forwarding, and cassette
  append as separate milestones with their own tests.
- Risk: selected request-header semantics are underspecified in the roadmap.
  Forwarding too many headers creates noise or protocol bugs; forwarding too
  few breaks compatibility. Mitigation: adopt a documented filter that drops
  hop-by-hop and framing headers, persists the filtered inbound set after
  redaction, and covers the behaviour with unit tests.
- Risk: upstream authentication depends on environment state, which is brittle
  under test. Mitigation: use dependency injection for environment lookup and
  keep tests on in-memory fakes.
- Risk: exact-byte capture can be accidentally altered by convenience helpers
  that parse and reserialize JSON. Mitigation: always store the original byte
  buffer first, then attempt parse from that buffer.
- Risk: record-mode concurrency now becomes real because the server can accept
  multiple requests while the cassette appender currently requires `&mut self`.
  Mitigation: serialize record writes behind shared state in the adapter, and
  document the ordering behaviour.

## Progress

- [x] Read the roadmap item, referenced design sections, current module layout,
      and prior execplans for adjacent tasks.
- [x] Identified the current gap: `src/server.rs`, `src/protocol.rs`, and
      `src/upstream.rs` are placeholders, so task `1.3.1` introduces the first
      real server and proxy path.
- [x] Drafted this ExecPlan.
- [x] Obtain explicit user approval for the plan.
- [x] Reconfirm repository guidance, implementation constraints, and available
      tools at execution start.
- [x] Implement server binding and graceful shutdown for the harness runtime.
- [x] Implement non-stream record-mode proxying for
      `POST /v1/chat/completions`.
- [x] Add unit and behavioural tests.
- [x] Update the design document, user's guide, and roadmap.
- [x] Run all required validation gates and record outcomes here.
- [x] Address post-review fixes for deterministic metadata timestamps and raw
      `Connection` header token parsing.
- [x] Address post-review fixes for configurable tool paths, BDD helper
      cardinality checks, and safe proxy-header warning logs.

## Surprises & Discoveries

- The current `start_harness` implementation only validates configuration and
  prepares cassette access. Task `1.3.1` is therefore the first milestone that
  turns the public async API into a genuinely async server lifecycle.
- The cassette domain is already ready for this slice: `RecordedRequest`,
  `RecordedResponse::NonStream`, `InteractionMetadata`,
  `RecordedRequest::populate_canonical_fields`, and append-only persistence are
  implemented.
- `IgnorePathConfig` exists only as an additive domain API from task `1.2.2`;
  it is not yet threaded through `HarnessConfig`. Record mode therefore needs a
  deliberate, documented default rather than pretending configuration exists.
- The current `Cargo.toml` has no HTTP server or client crate. The plan must
  account for inbound and outbound HTTP dependencies explicitly rather than
  assuming routing is already present.
- Existing BDD tests use `rstest-bdd` worlds plus helper modules under
  `tests/`, which is the right pattern to reuse for an end-to-end record-mode
  flow with a stub upstream.
- The project-scoped qdrant memory protocol is documented in `AGENTS.md`, but
  the qdrant MCP tools are not exposed in this session. Local repository
  documents and the existing ExecPlan therefore serve as the working memory
  substitute for this turn.
- BDD support had to change once record mode became real: step-local Tokio
  runtimes were sufficient for the placeholder harness, but they dropped the
  real server task between steps. The scenario worlds now retain a shared
  runtime for the full scenario lifetime.
- Post-review comments referred to pre-rebase files such as `src/server.rs`,
  but the equivalent implementation now lives in focused modules under
  `src/server/` and `src/http_exchange.rs`.

## Decision Log

- Confirmed decision: use an Axum/Hyper-based inbound server adapter and a
  single Reqwest-based outbound HTTP client adapter, keeping both confined to
  adapter modules. This matches the design document's crate layout and avoids a
  custom HTTP stack for the first slice.
- Proposed decision: mount the actual server in all modes once task `1.3.1`
  lands, but only record mode will have a working `POST /v1/chat/completions`
  path in this slice. Replay-mode request handling remains for task `1.3.2`.
- Confirmed decision: keep replay and verify on the existing startup-only path
  in this slice, and bind a real listener only for record mode. This trims the
  first serving change to the minimum needed for `1.3.1` without weakening the
  public API contract.
- Confirmed decision: reject `stream: true` requests with a clear
  "not implemented yet" adapter response and do not append to the cassette.
  Record this temporary limitation in both the design document and user's guide.
- Confirmed decision: construct the upstream URL for this endpoint by joining
  `HarnessConfig.upstream.base_url` with `/chat/completions`, not by naively
  concatenating the inbound `/v1/chat/completions` path, because the current
  default base URL already includes `/api/v1`.
- Confirmed decision: persist the filtered inbound request headers after
  redaction, but forward an enriched outbound request that also includes the
  configured upstream API key and `extra_headers`.
- Confirmed decision: populate canonical request fields during recording with
  `IgnorePathConfig::default()` until a later task introduces configuration for
  ignore paths at startup.
- Confirmed decision: treat hop-by-hop and framing headers as non-selected for
  both forwarding and persistence. Preserve the observed order and duplicates
  of the retained headers, then apply case-insensitive redaction immediately
  before persistence.
- Confirmed decision: disable Reqwest content decoding in the upstream adapter
  for this slice so the recorded response body remains the exact byte payload
  read from the upstream transport.
- Confirmed decision: parse `Connection` header tokens from raw header bytes so
  one non-UTF-8 token does not suppress valid hop-by-hop tokens in the same
  header value.
- Confirmed decision: proxy response header parse failures may log header names
  and byte lengths, but not raw header values, because response headers can
  carry sensitive data.

## Outcomes & Retrospective

Implementation completed. Success for this task means:

- a real harness server starts and shuts down cleanly;
- non-stream chat completions requests proxy successfully in record mode;
- cassette persistence stores deterministic request/response data with header
  redaction;
- unit and behavioural tests pass;
- documentation and roadmap status match the shipped behaviour.

Retrospective notes and final command results must be added here during
execution.

Interim evidence captured during implementation:

- `cargo test --lib` passes with new unit coverage for:
  - request-header filtering and case-insensitive redaction;
  - upstream URL construction;
  - unsupported `stream: true` rejection without cassette writes;
  - missing API key rejection without cassette writes;
  - invalid JSON response capture preserving exact bytes with
    `parsed_json: None`.
- `cargo test --test harness_startup_bdd -- --nocapture` passes after updating
  the startup BDD suite for real record-mode server binding and graceful
  shutdown.
- `cargo test --test record_mode_proxying_bdd -- --nocapture` passes with
  end-to-end scenarios proving successful proxying, redaction, unsupported
  streaming rejection, and upstream failure handling.
- Final validation commands all passed and their logs were captured under
  `/tmp/1-3-1-*.log`:
  - `make fmt`
  - `make check-fmt`
  - `make lint`
  - `make test`
  - `make markdownlint`
  - `make nixie`

## Context and orientation

Relevant repository paths at the start of this task:

- `src/lib.rs` - current public entry point. Validates config and prepares the
  cassette store, but does not yet bind an HTTP server.
- `src/config.rs` - `HarnessConfig`, `UpstreamConfig`, `RedactionConfig`, and
  replay settings.
- `src/error.rs` - typed error surface, including `UpstreamRequestFailed`.
- `src/cassette/mod.rs` - cassette schema and domain types already needed by
  this task.
- `src/cassette/filesystem.rs` - append-only filesystem-backed
  `CassetteAppender`.
- `src/server.rs` - placeholder module comment only.
- `src/protocol.rs` - placeholder module comment only.
- `src/upstream.rs` - placeholder module comment only.
- `tests/harness_startup_bdd.rs` and `tests/replay_matching_modes_bdd.rs` -
  working BDD patterns to follow.
- `tests/support/test_utils.rs` - current runtime helper for BDD tests.
- `docs/users-guide.md` - needs new record-mode behaviour documented.
- `docs/spycatcher-harness-design.md` - must capture the approved design
  decisions taken while implementing this task.

Key terms:

- Recorded request: the client request as observed at the harness boundary,
  stored for future canonical matching and replay diagnostics.
- Outbound upstream request: the enriched proxy request sent from the harness
  to the configured upstream, including authentication and configured extra
  headers.
- Selected headers: the retained non-hop-by-hop HTTP headers that the proxy
  forwards and that the cassette may persist after redaction.
- Exact bytes: the unmodified response body byte buffer returned by the
  upstream for a non-stream interaction.

## Plan of work

### Stage A: establish the runtime boundary and server lifecycle

Expand the placeholder runtime into a real server without collapsing adapter
and domain concerns. The top-level goal of this stage is that
`start_harness(cfg).await` binds a listener, spawns the server task, and
returns a `RunningHarness` that can stop the server cleanly.

Implementation guidance:

1. Add the inbound and outbound HTTP dependencies required for the approved
   approach in `Cargo.toml`.
2. Turn `src/server.rs` into a module root with small submodules for:
   listener/startup, shared application state, route registration, and shutdown
   handling.
3. Extend `RunningHarness` so it owns whatever shutdown signal and join handle
   the runtime needs, while preserving the existing public method shape.
4. Keep mode-independent startup concerns in `src/lib.rs`; push HTTP details
   into `src/server`.
5. Update startup tests so they assert a genuinely bound address and graceful
   shutdown instead of only the configured socket value.

Acceptance for Stage A:

- Record mode starts a live server and returns the actual bound address.
- Shutdown stops the server task cleanly.
- Existing startup tests are updated rather than left asserting the old
  placeholder behaviour.

### Stage B: define the record-mode orchestration boundary

Create a thin application-facing boundary between HTTP adapters and cassette
domain logic. The handler should not directly manipulate filesystem stores,
environment lookups, canonicalization, timestamp generation, and upstream HTTP
calls all in one function.

Implementation guidance:

1. Define the minimal orchestration API needed for one record-mode exchange.
   The input should be adapter-neutral request data; the output should be an
   adapter-neutral proxied response plus any persisted interaction data.
2. Keep `RecordedRequest`, `RecordedResponse`, `Interaction`, and
   `HarnessError` as the shared domain vocabulary.
3. Introduce narrow injected ports for at least:
   - upstream execution;
   - environment lookup for `api_key_env`;
   - timestamp / relative-offset generation if those values become observable
     in tests.
4. Use
   `RecordedRequest::populate_canonical_fields(&IgnorePathConfig::default())`
   so recorded interactions are immediately usable by the existing canonical
   matching machinery.
5. Serialize access to the shared `CassetteAppender` in adapter state so
   append ordering remains deterministic.

Acceptance for Stage B:

- The route handler delegates orchestration to a small service boundary.
- Domain-owned cassette data is produced without `axum` or client-library
  types leaking into cassette modules.
- Environment-dependent logic is unit-testable without mutating process-global
  state.

### Stage C: implement request forwarding and cassette capture

Implement the actual record-mode path for `POST /v1/chat/completions` with the
non-stream scope enforced explicitly.

Implementation guidance:

1. Mount only the required route for this task:
   `POST /v1/chat/completions`.
2. Parse the incoming body once into bytes. Attempt JSON parsing from those
   same bytes so the stored body stays byte-faithful.
3. Detect the unsupported streaming path. If the JSON body sets `stream` to
   `true`, return the approved unsupported response and do not write to the
   cassette.
4. Build the outbound upstream request by:
   - targeting the approved upstream URL;
   - forwarding the selected inbound headers;
   - adding the upstream bearer token from `api_key_env`;
   - adding configured `extra_headers`.
5. Capture the upstream response status, selected response headers, and exact
   body bytes. Populate `parsed_json` only when parsing succeeds.
6. Persist an `Interaction` containing:
   - the inbound request after redaction-safe header capture;
   - a `RecordedResponse::NonStream` with exact bytes and optional parsed JSON;
   - `InteractionMetadata` values derived from the injected time/session
     context.
7. Return the upstream status, headers, and exact body bytes to the client.

Acceptance for Stage C:

- Successful non-stream requests are proxied and persisted.
- Redacted headers are absent from the cassette.
- Invalid JSON responses still preserve exact bytes with `parsed_json: None`.
- Failed upstream exchanges do not append partial interactions.

### Stage D: cover the behaviour with unit tests

Add focused unit tests with `rstest` around the small seams introduced above.
Prefer pure fixtures and injected fakes over spinning up sockets when a unit
boundary exists.

Minimum unit coverage:

- upstream URL construction from the configured base URL;
- request-header filtering and case-insensitive redaction;
- exact-byte response capture with valid JSON parsing;
- exact-byte response capture with invalid or non-JSON bodies leaving
  `parsed_json` as `None`;
- unsupported `stream: true` request path performs no append;
- upstream transport failure or missing API key produces the expected failure
  and leaves the cassette unchanged;
- metadata and canonical-field population behave deterministically under test
  doubles.

Prefer `#[rstest]` fixtures for fake environment providers, fake clocks, and
sample request/response payloads.

### Stage E: add behavioural tests with `rstest-bdd`

Add end-to-end scenarios that exercise the actual harness server against a
local stub upstream service. Reuse the repository's existing BDD structure:
feature file under `tests/features/`, scenario bindings in a top-level test
file, and helper/world modules in a sibling directory.

Minimum behavioural scenarios:

1. Successful non-stream proxying records one interaction and returns the
   upstream JSON response unchanged.
2. A secret inbound header is forwarded as required for proxy behaviour but is
   absent from the persisted cassette after redaction.
3. An unsupported `stream: true` request fails clearly and leaves the cassette
   untouched.
4. An upstream failure surfaces as an error response and leaves the cassette
   untouched.

The BDD checks should inspect the persisted cassette via the filesystem
adapter, not by peeking into adapter internals.

### Stage F: update documentation and roadmap status

Once the implementation and tests pass, update the documentation that users and
future implementers rely on.

Required documentation updates:

- `docs/spycatcher-harness-design.md`
  - record the final decisions on server/runtime shape, unsupported stream
    handling, header selection/redaction semantics, upstream URL mapping, and
    any environment/time injection ports introduced.
- `docs/users-guide.md`
  - document that record mode now serves a real local endpoint;
  - explain how the upstream API key is sourced;
  - explain which headers are persisted versus redacted;
  - document the temporary unsupported status of `stream: true`.
- `docs/roadmap.md`
  - mark `1.3.1` as done only after all validation commands pass.

If public examples in Rustdoc are touched while updating docs, keep them
doctest-safe per `docs/rust-doctest-dry-guide.md`.

## Validation and evidence

Run all gates with `tee` so full output survives truncation:

```bash
set -o pipefail
make fmt 2>&1 | tee /tmp/1-3-1-fmt.log
set -o pipefail
make check-fmt 2>&1 | tee /tmp/1-3-1-check-fmt.log
set -o pipefail
make lint 2>&1 | tee /tmp/1-3-1-lint.log
set -o pipefail
make test 2>&1 | tee /tmp/1-3-1-test.log
set -o pipefail
make markdownlint 2>&1 | tee /tmp/1-3-1-markdownlint.log
set -o pipefail
make nixie 2>&1 | tee /tmp/1-3-1-nixie.log
```

During implementation, add concise evidence here:

- which new unit tests fail before the change and pass after it;
- which BDD scenarios prove proxying, redaction, and failure handling;
- any manual smoke test transcript if one is needed beyond automated coverage.

Expected completion signal:

- every command above exits `0`;
- the new unit and BDD tests pass;
- the roadmap item is checked off;
- this ExecPlan status moves from `DRAFT` to `COMPLETE` via `APPROVED` and
  `IN PROGRESS` as work proceeds.
