# Preserve and replay OpenRouter SSE comment frames

This ExecPlan (execution plan) is a living document. The sections `Constraints`,
`Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`,
and `Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

This plan implements roadmap task `2.1.2`. Implementation must not begin until
the user explicitly approves the plan. After approval, work must proceed
milestone by milestone, stopping only when a tolerance threshold is reached or
a constraint would be violated.

## Purpose / big picture

Roadmap task `2.1.1` made the harness record OpenAI-style Server-Sent Events
(SSE) streams to a cassette. Comment frames such as `: OPENROUTER PROCESSING`
are already captured as typed `StreamEvent::Comment` entries, and the raw
transcript bytes are persisted alongside. Replay of a streamed interaction
still returns HTTP 501 `unsupported_stream` because roadmap tasks `2.1.2` and
`2.1.3` had not landed.

Task `2.1.2` ships the first replay path for streamed Chat Completions
cassettes. After this task is delivered:

- A caller can send `POST /v1/chat/completions` with `"stream": true` to a
  replay-mode harness whose cassette contains a `RecordedResponse::Stream`
  interaction, receive a `text/event-stream` response whose body re-emits the
  recorded parsed events (including any recorded comment frames such as
  `: OPENROUTER PROCESSING`) in the order they were observed upstream, and the
  replay harness must not have made any upstream network call.
- A new `StreamCanonicalPolicy` lets cassette consumers (verification tooling,
  future byte-faithful replay) compare two stream-event sequences while
  ignoring comment frames so that mid-stream keep-alive variance does not
  produce spurious mismatches.
- Documentation in `docs/users-guide.md`, `docs/developers-guide.md`, and
  `docs/spycatcher-harness-design.md` describes the new replay surface, the
  canonical-stream mode, and the remaining limitation that byte-faithful
  raw-transcript replay is delivered by task `2.1.3`.

Observable success after delivery:

- The `Replay rejects streaming requests` scenario in
  `tests/features/chat_completions_replay.feature` is replaced by a scenario
  asserting that a recorded OpenRouter stream replays with comments preserved.
- A new behavioural scenario asserts that a cassette whose recorded comment
  text differs from the observed transcript still matches when canonical-
  stream comparison is configured to ignore comments.
- `make check-fmt`, `make lint`, and `make test` succeed locally, with `tee`
  logs captured under `/tmp` per `AGENTS.md`.

## Relevant documentation and skills

Primary project documentation:

- `docs/roadmap.md` — task `2.1.2`, dependencies on `2.1.1`, and success
  criteria.
- `docs/spycatcher-harness-design.md` — sections
  [Streaming capture and replay](../spycatcher-harness-design.md#streaming-capture-and-replay)
  and
  [Known risks and limitations](../spycatcher-harness-design.md#known-risks-and-limitations).
- `docs/execplans/2-1-1-sse-parser-and-recorder-for-open-ai-streams.md` —
  parser and recorder design that this task builds on; reuses cassette schema.
- `docs/users-guide.md` — user-facing behaviour for record and replay modes
  that must be extended for streamed replay.
- `docs/developers-guide.md` — internal module boundaries, record-mode seams,
  cassette schema, and behavioural test conventions.
- `docs/rust-testing-with-rstest-fixtures.md` — `rstest` fixtures and
  parameterized unit tests.
- `docs/reliable-testing-in-rust-via-dependency-injection.md` — injected seams
  for clocks, configuration, upstream behaviour, and matching mode.
- `docs/rust-doctest-dry-guide.md` — keep public examples valid where docs are
  amended.
- `docs/complexity-antipatterns-and-refactoring-strategies.md` — apply when
  replay code pushes a module towards the 400-line cap.
- `docs/ortho-config-users-guide.md` — configuration wording for the new
  canonical-stream knob.
- `docs/rstest-bdd-users-guide.md` — feature files, worlds, scenario state, and
  step-definition structure.
- `docs/documentation-style-guide.md` — en-GB Oxford spelling, table and
  heading wrapping rules.

External protocol and prior-art references resolved during planning:

- OpenRouter API streaming documentation, "Additional Information": the
  service occasionally injects SSE comments such as `: OPENROUTER PROCESSING`
  as keep-alive frames to prevent connection timeouts, and notes that these
  comments are safely ignorable per the WHATWG SSE specification but may be
  used by UIs to drive loading indicators. Mid-stream errors are emitted as
  `data:` frames after HTTP status is committed.
- WHATWG HTML Standard, "Server-sent events": SSE streams are UTF-8,
  line-oriented, use blank lines to dispatch events, and treat any line
  beginning with `:` as a comment that the default `EventSource` consumer
  discards.
- `vcrpy` and `ropensci/vcr` cassette filtering documentation: prior-art
  cassette libraries expose configurable ignore filters to keep matching
  resilient against drifting fields, which is the same shape of policy needed
  for stream comments.
- OpenAI cookbook, "How to stream completions": confirms `[DONE]` terminator
  semantics and that streamed usage chunks appear only when
  `stream_options.include_usage` is enabled.

Skills to apply during implementation:

- `execplans` — keep this document current and honour the approval gate.
- `leta` — use semantic navigation before modifying Rust code; the leta
  workspace for this repository is already configured.
- `hexagonal-architecture` — keep stream replay policy in the domain layer and
  keep Axum body construction at the adapter edge.
- `rust-router` — direct any Rust question to the smallest useful follow-on
  skill.
- `domain-web-services` — keep Axum handlers thin; stream bodies live behind
  `axum::body::Body::from_stream` adapters fed by domain types.
- `rust-async-and-concurrency` — design replay byte emission without holding
  locks across `.await` and without spawning background tasks for the simple
  parsed-event path.
- `rust-errors` — classify stream replay failures (`UnsupportedStream` stays
  available for the raw-transcript mode until task `2.1.3` retires it).
- `rust-types-and-apis` — extend `ReplayResponse` and `MatchMode` without
  leaking adapter detail into the cassette domain.
- `nextest` — use when running the Rust test suite.
- `en-gb-oxendict` — keep documentation and comments in project style.
- `commit-message` — commit gated, atomic changes using a file-based commit
  message.

## Context and orientation

The reader is assumed to have only the current working tree and this plan.

The repository is a single-package Cargo project (library `spycatcher_harness`
plus binary `spycatcher-harness`). Relevant areas:

- `src/sse.rs` — adapter-neutral SSE parser shipped in task `2.1.1`. It emits
  `StreamEvent::Comment { text }` for `:`-prefixed lines and joins `data:`
  frames per the WHATWG SSE rules.
- `src/cassette/mod.rs` — cassette schema. `RecordedResponse::Stream` carries
  `status`, `headers`, `events: Vec<StreamEvent>`, `raw_transcript: Vec<u8>`,
  and an optional `timing: StreamTiming`. The on-disk schema is at
  `format_version = 1`.
- `src/cassette/matching.rs` — request-only matching engine. `MatchMode` today
  has `SequentialStrict` and `Keyed` variants. Comment-aware policy is not yet
  represented.
- `src/cassette/canonical/` — canonical request generation (JSON, query
  string, hashing). Stream-event canonicalization does not yet exist.
- `src/cassette/diff.rs` — produces `canonical_diff_summary` strings; reusable
  for stream-event mismatch reporting if a follow-up needs it.
- `src/replay.rs` — domain replay service. Currently rejects streaming
  request shapes (`reject_unreplayable_request_shape`) and stream cassette
  responses (`response_from_recorded`) with `ReplayError::UnsupportedStream`.
  `ReplayResponse` is a non-stream struct with an eager `Vec<u8>` body.
- `src/server/replay_handler.rs` — Axum handler. Builds `Response<Body>` from
  `ReplayResponse`. The unsupported-stream error has an insta snapshot at
  `src/server/snapshots/spycatcher_harness__server__replay_handler__tests__unsupported_stream.snap`.
- `src/server/record_stream.rs` — record-mode stream proxy. Demonstrates the
  hexagonal pattern this plan should mirror: domain stream representation in a
  focused module, adapter glue (axum `Body::from_stream`,
  `futures_util::TryStream`) at the inbound edge.
- `src/protocol.rs` — `CHAT_COMPLETIONS_PROTOCOL_ID`,
  `CHAT_COMPLETIONS_PATH`, and `is_streaming_chat_completions_request`.
- `tests/features/chat_completions_replay.feature` — currently includes a
  scenario named `Replay rejects streaming requests` (lines 30–41). That
  scenario must be replaced by stream-replay scenarios; the rejection path for
  non-recorded streams becomes a malformed-request edge case rather than the
  headline behaviour.
- `tests/chat_completions_replay/{world,steps,support}.rs` — behavioural
  scaffolding shared with other scenarios.
- `tests/record_mode_proxying/{helpers,stream_steps,world}.rs` — stub
  upstream with `StubUpstream::start_stream`, transcript fragmentation helpers,
  and cassette assertion helpers usable when wiring record-to-replay
  integration scenarios.

Definitions for this plan:

- **Stream event**: a typed entry in `Vec<StreamEvent>` recording either an
  SSE comment payload (`StreamEvent::Comment { text }`) or an SSE data payload
  (`StreamEvent::Data { raw, parsed_json }`).
- **Parsed-event replay**: serializing the recorded `Vec<StreamEvent>` back
  into SSE wire bytes, ordered exactly as recorded. This is what task `2.1.2`
  delivers.
- **Byte-faithful replay**: emitting the recorded `raw_transcript` verbatim,
  preserving original chunk boundaries and header framing. This is reserved for
  task `2.1.3` and is explicitly out of scope here.
- **Canonical-stream matching**: a comparison policy over two stream-event
  sequences that drops `StreamEvent::Comment` entries before equality checking,
  leaving non-comment ordering and content intact. Used by verification and
  equivalence tooling; does not change request matching.

## Constraints

- Do not implement byte-faithful raw-transcript replay in this task. The
  `raw_transcript` field stays untouched on disk and the replay handler must
  not switch into a transcript-pass-through path. Task `2.1.3` owns that
  surface.
- Preserve the existing public lifecycle API:
  `start_harness(cfg) -> HarnessResult<RunningHarness>` and
  `RunningHarness::shutdown(self)` remain source-compatible.
- Preserve the cassette `format_version` value of `1`. The `StreamEvent`,
  `StreamTiming`, and `RecordedResponse::Stream` shapes must continue to
  deserialize existing cassettes round-trip. New configuration is added at the
  harness-config or matching-mode boundary, not by mutating cassette data.
- Maintain hexagonal boundaries:
  - Stream serialization policy (events → SSE bytes) and canonical-stream
    comparison belong in the cassette/replay domain modules and must not
    import `axum` or `reqwest` types.
  - Axum `Response<Body>` construction, `text/event-stream` content-type
    handling, and any `futures_util::TryStream` adapters belong in
    `src/server/`.
  - Request matching policy stays under `src/cassette/matching.rs`; do not
    move request semantics into the server layer.
- Replay must not make outbound network calls. Any test that fails to verify
  this property must be tightened, not relaxed.
- Continue using `camino` and `cap_std` for path and filesystem work, per
  `AGENTS.md`.
- Use `rstest` for unit tests and `rstest-bdd` for behavioural tests. Property
  tests use `proptest` and must hold for canonical-stream invariants.
- Do not mutate process environment in tests. Inject configuration through
  fixtures, world slots, or the existing `EnvProvider` seam.
- No single Rust source file may exceed 400 lines after the change. Where
  adding stream replay would push `src/server/replay_handler.rs` (currently 340
  lines including tests) or `src/replay.rs` (currently 312 lines) past that
  cap, extract a new module rather than expanding the existing file.
- Comments and documentation must use en-GB Oxford spelling and the
  documentation style guide's wrapping rules.
- Run code quality gates sequentially, not in parallel; capture each run's
  output with `tee` under `/tmp` per `AGENTS.md`.
- Use `coderabbit review --agent` after each major milestone and clear all
  actionable concerns before moving to the next milestone.
- Commit after each gated, atomic change. Do not commit work that fails any
  quality gate.

## Tolerances (exception triggers)

- Scope: if implementation requires changes to more than 22 files or
  approximately 1700 net lines of code and documentation, stop and escalate.
- Public API: if `start_harness`, `RunningHarness::shutdown`,
  `HarnessConfig`, or existing cassette public types (`Cassette`, `Interaction`,
  `RecordedRequest`, `RecordedResponse`, `StreamEvent`, `StreamTiming`)
  require a breaking change, stop and escalate.
- Cassette schema: if any cassette field must change shape or a new
  `format_version` is needed to represent comment metadata, stop and present
  the migration trade-off before coding.
- Match-mode API: if comment-aware matching requires more than one new
  `MatchMode` variant or extends `ReplayMatchEngine::new` with new constructor
  arguments beyond an optional policy struct, stop and escalate.
- Dependencies: if a new runtime crate is required (in particular, any new
  SSE encoder/decoder), stop and present the options. `futures-util` and
  `bytes` are already present in `Cargo.lock` and may be reused.
- Streaming semantics: if parsed-event replay cannot reproduce a sequence
  byte-equivalent to the original transcript (within the documented
  canonical-form rewrite for newlines and the colon-space comment prefix), and
  the divergence is detectable by a real SSE client used in tests, stop and
  escalate before relaxing fidelity.
- Cancellation: if replay-mode client disconnect handling would require
  unsupervised background tasks or ambiguous cassette-state semantics, stop and
  escalate.
- Verification: if `make lint` or `make test` still fails after five repair
  cycles, stop and report the failing evidence.
- Review: if CodeRabbit raises an actionable correctness, safety, or test
  gap concern that cannot be addressed within this plan's scope, stop and ask
  for direction.
- Iteration time: if any milestone exceeds eight hours of focused work
  without producing a green gate, stop and escalate.

## Risks

- Risk: parsed-event replay can subtly differ from the recorded transcript
  because the parser strips an optional space after `data:` and may reflow
  multiple `data:` lines. Severity: high. Likelihood: medium. Mitigation:
  document the canonical SSE rewrite rules in `docs/users-guide.md` and the
  developers' guide; assert in unit tests that representative OpenRouter
  fixtures replay byte-equivalent under those rules; defer true verbatim replay
  to task `2.1.3`.

- Risk: introducing a comment-aware match mode could destabilize existing
  `Keyed` and `SequentialStrict` mode tests if the option is wired through too
  widely. Severity: medium. Likelihood: medium. Mitigation: model the
  comment-aware policy as an additive `StreamCanonicalPolicy` value carried
  alongside `MatchMode`, default to `Verbatim`, and assert in tests that
  pre-existing request matching is unchanged.

- Risk: existing snapshot tests (notably
  `spycatcher_harness__server__replay_handler__tests__unsupported_stream.snap`)
  will break when stream replay stops returning 501. Severity: low. Likelihood:
  high. Mitigation: delete the obsolete snapshot during the implementation
  milestone, add new snapshots covering the success body and the new error path
  for cassettes that contain no stream interaction.

- Risk: a recorded stream that ends without `data: [DONE]` (a malformed
  upstream) replays as an unterminated event-stream body. Severity: medium.
  Likelihood: low. Mitigation: rely on the existing recorder policy that
  refuses to persist a stream interaction without a clean parse; document the
  contract and add a unit test that demonstrates how a hand-authored cassette
  without `[DONE]` is still replayed verbatim without synthetic injection of a
  terminator.

- Risk: streaming response bodies can be cancelled mid-flight by the
  client. Severity: medium. Likelihood: medium. Mitigation: design the domain
  serializer as a pure function producing `Vec<u8>` for small recorded streams;
  if and only if total event byte size exceeds a documented soft cap, fall back
  to a `futures::stream::iter` adapter that yields one chunk per event. Keep
  state out of the stream so cancellation has no observable side effect on the
  engine.

- Risk: the new canonical-stream policy could leak into the verify
  subcommand (task `2.2.1`) prematurely. Severity: low. Likelihood: medium.
  Mitigation: expose the policy as a private cassette helper and a single `pub`
  constructor on `ReplayMatchEngine`; do not extend the CLI surface for verify
  in this plan.

## Progress

- [x] 2026-06-04: User explicitly approved implementation of this draft plan
  and requested milestone-by-milestone execution with CodeRabbit reviews after
  deterministic quality gates.
- [x] 2026-06-04: Confirmed the branch is
  `2-1-2-preserve-and-replay-open-router-comment-frames`, so no branch rename
  is required before implementation.
- [x] 2026-06-04: Implemented canonical stream-event policy, replay-domain
  stream responses, Axum stream rendering, and BDD coverage for replaying a
  recorded OpenRouter stream with comment frames preserved.
- [x] 2026-06-04: Ran focused and milestone gates:
  `cargo test stream_canonical --all-features`,
  `cargo test replay --all-features`,
  `cargo test --test chat_completions_replay_bdd --all-features`,
  `make check-fmt`, `make lint`, and `make test`. The final gate logs are
  `/tmp/check-fmt-spycatcher-harness-2-1-2-milestone.out`,
  `/tmp/lint-spycatcher-harness-2-1-2-milestone-rerun2.out`, and
  `/tmp/test-spycatcher-harness-2-1-2-milestone.out`.
- [x] 2026-06-04: Ran `coderabbit review --agent`; CodeRabbit reported six
  minor en-GB spelling issues in comments/test names. Applied those spelling
  fixes in `src/cassette/stream_canonical.rs` and `src/server/replay_stream.rs`.
- [x] 2026-06-04: Addressed four rounds of CodeRabbit findings covering
  documentation spelling, `build_stream_body` Rustdoc, stream chunk allocation,
  realistic property-test data, and stream-body error documentation. Re-ran
  `make check-fmt`, `make lint`, and `make test`; final deterministic gate logs
  are `/tmp/check-fmt-spycatcher-harness-2-1-2-post-coderabbit4.out`,
  `/tmp/lint-spycatcher-harness-2-1-2-post-coderabbit4.out`, and
  `/tmp/test-spycatcher-harness-2-1-2-post-coderabbit4.out`.
- [x] 2026-06-04: A follow-up `coderabbit review --agent` run hit the
  recoverable rate limit. Slept for 20 minutes with `vsleep` as requested,
  retried the review, and CodeRabbit completed with zero findings. Review log:
  `/tmp/coderabbit-spycatcher-harness-2-1-2-post-fixes5.out`.
- [x] 2026-06-04: Committed the implementation milestone as `ffdf864` with
  subject `Replay recorded stream events`.
- [x] 2026-06-04: Updated `docs/users-guide.md`,
  `docs/developers-guide.md`, `docs/spycatcher-harness-design.md`, and
  `docs/roadmap.md` for parsed-event stream replay, comment preservation,
  canonical stream-event comparison, and the deferred byte-faithful replay task.
- [x] 2026-06-04: Ran documentation gates for the documentation milestone:
  `make markdownlint` and `make nixie`. Logs:
  `/tmp/markdownlint-spycatcher-harness-2-1-2-docs.out` and
  `/tmp/nixie-spycatcher-harness-2-1-2-docs.out`.
- [x] 2026-06-04: Ran `coderabbit review --agent` for the documentation
  milestone; CodeRabbit completed with zero findings. Review log:
  `/tmp/coderabbit-spycatcher-harness-2-1-2-docs.out`.
- [x] 2026-06-04: Ran final full quality gates: `make check-fmt`,
  `make lint`, and `make test`. Logs:
  `/tmp/check-fmt-spycatcher-harness-2-1-2-final.out`,
  `/tmp/lint-spycatcher-harness-2-1-2-final.out`, and
  `/tmp/test-spycatcher-harness-2-1-2-final.out`.

## Surprises & discoveries

- 2026-06-04: The repository-level `AGENTS.md` instructions reference
  `docs/contents.md` and `docs/repository-layout.md`, but neither file exists
  in this worktree. This was verified with `sed` failures and
  `find docs -maxdepth 2 -type f`; the impact is that this plan, the roadmap,
  and the present design/user/developer guides are the documentation source of
  truth for orientation.
- 2026-06-04: A streaming request recorded against a non-stream JSON upstream
  response produces no cassette interaction. This was verified by a failing BDD
  run of `replay_rejects_streaming_requests_when_the_cassette_has_no_recording`
  where cassette loading found zero interactions. The scenario was revised to
  assert the actual no-recording replay behaviour: a streaming request against
  a cassette containing only a non-stream recording returns the existing 409
  request-mismatch diagnostic.

## Decision log

- Proposed decision: parsed-event replay re-serializes `Vec<StreamEvent>`
  to SSE bytes using the canonical rewrite rule `data: <raw>\n\n` for data
  frames and `: <text>\n\n` for comment frames, with no additional `event:` or
  `id:` fields. Rationale: the parser already discards `event:` and `id:`
  lines, so reproducing them would be lossy guessing. Byte-faithful
  reproduction of the raw transcript is task `2.1.3`'s scope.

- Implemented decision: introduce
  `StreamCanonicalPolicy { ignore_comments: bool }` as a domain value type,
  default `ignore_comments = false`, and thread it into `ReplayMatchEngine` via
  `ReplayMatchEngine::with_policy(cassette, mode, policy)`. Rationale: keeps
  comment-aware comparison additive and avoids breaking existing match-mode
  wiring.

- Proposed decision: extend `ReplayResponse` into an enum
  `ReplayBody { OneShot(Vec<u8>), Events(Vec<StreamEvent>) }` carried by a
  single `ReplayResponse { status, headers, body }`. Rationale: the Axum layer
  alone decides how to serialize each variant; the domain stays
  framework-agnostic. Alternative considered: keep `ReplayResponse` as a byte
  body and pre-serialize stream events in the domain; rejected because it
  forces the canonical rewrite rules through the request path even when the
  adapter could otherwise stream raw bytes (future-proofing for task `2.1.3`).

- Proposed decision: replace the existing
  `Replay rejects streaming requests` BDD scenario with one that asserts
  successful comment-preserving replay. Add a new scenario for the "cassette
  contains no stream interaction" mismatch path so the previous 501 invariant
  is retained where it still applies.

Record every decision (and every escalation) here as work proceeds.

- Decision: Treat the user's 2026-06-04 request to "proceed with
  implementation" as the explicit approval required by this plan's draft gate.
  Rationale: the request names this plan and asks for implementation, plan
  maintenance, gated CodeRabbit reviews, and frequent commits.
- Decision: Keep `ReplayMatchEngine::new(cassette, mode)` source-compatible
  and add `ReplayMatchEngine::with_policy(cassette, mode, policy)` plus
  `stream_policy()` for explicit canonical-stream policy. Rationale: this makes
  the comment-ignore comparison additive and keeps existing request matching
  unchanged.
- Decision: A streaming request with no matching recorded stream interaction
  remains a request mismatch (`409`) rather than a response-shape error
  (`501`). Rationale: replay cannot inspect a response shape until request
  canonical matching succeeds, and this preserves the existing matching
  contract.

## Outcomes & retrospective

All implementation and documentation milestones completed.

The delivered replay path serves matching `RecordedResponse::Stream`
interactions by serializing recorded parsed `StreamEvent` values as SSE. It
preserves comment frames and data-event ordering, supplies `text/event-stream`
when a stream cassette lacks a content type, and keeps replay independent of
upstream network configuration. The delivered canonical stream-event helper
lets cassette consumers compare streams while ignoring comment-only drift.

Final validation passed with `make check-fmt`, `make lint`, `make test`,
`make markdownlint`, and `make nixie`. CodeRabbit reviewed both the
implementation and documentation milestones; the final documentation review
completed with zero findings.

Follow-up for roadmap task `2.1.3`: byte-faithful stream replay should use the
persisted `raw_transcript` bytes when exact upstream framing matters. Timing
physics remains a later replay-realism concern and was not added here.

## Plan of work

The work is organized into four milestones plus a hardening pass. Each
milestone ends with a focused test run, a CodeRabbit review, and a commit. Do
not skip the review or commit before moving on.

### Milestone 1: Baseline and red tests

Reconfirm starting state:

```sh
git status --short
git branch --show-current
```

Enumerate the touched modules before changing code:

```sh
leta show StreamEvent
leta show RecordedResponse
leta show ReplayMatchEngine
leta show ReplayService.handle_chat_completions
leta show build_replay_response
leta refs StreamEvent
```

Write the failing tests before any production code:

- In `src/cassette/canonical_tests.rs` (or a focused
  `src/cassette/stream_canonical_tests.rs` if the file would exceed 400 lines),
  add `rstest` cases covering:
  - canonical-stream comparison drops `StreamEvent::Comment` entries while
    preserving the order of `StreamEvent::Data` entries,
  - canonical-stream comparison is the identity when no comments are
    present,
  - canonical-stream comparison handles empty sequences without panic.
- In a new `src/server/replay_stream_tests.rs` (or under
  `src/server/replay_handler.rs`'s existing test module if size permits) add
  `rstest` cases covering:
  - parsed-event replay emits `data: <raw>\n\n` for each `Data` event,
  - parsed-event replay emits `: <text>\n\n` for each `Comment` event in
    recorded order,
  - parsed-event replay forwards recorded headers verbatim and forces
    `content-type: text/event-stream` when the cassette omits it,
  - parsed-event replay propagates the recorded status code,
  - empty event lists produce an empty body and a 200 status without
    synthesising `[DONE]`.
- In `tests/features/chat_completions_replay.feature` replace the
  `Replay rejects streaming requests` scenario with two scenarios:
  - `Replay emits a recorded OpenRouter stream including comment frames`,
  - `Replay rejects streaming requests when the cassette has no recording`,
  and add a third scenario:
  - `Canonical-stream matching ignores comment-only drift`.
- In `tests/chat_completions_replay/steps.rs` and `world.rs`, add the new
  step definitions and `Slot` fields. Reuse `StubUpstream::start_stream` and
  `stream_response` from `tests/record_mode_proxying/helpers.rs` to drive a
  comment-laden record cassette.
- Add a `proptest!` block in the canonical-stream test module asserting
  idempotence over `Vec<StreamEvent>` values:

  ```rust
  canonicalize_events(&canonicalize_events(&events, policy), policy)
      == canonicalize_events(&events, policy)
  ```

  Also assert that the `Data`-only subsequence is preserved.

Run focused failing suites with `tee` logs:

```sh
cargo test --no-run --all-features 2>&1 \
  | tee /tmp/build-spycatcher-harness-2-1-2-red.out
cargo test sse --all-features 2>&1 \
  | tee /tmp/test-sse-spycatcher-harness-2-1-2-red.out
cargo test --test chat_completions_replay_bdd --all-features 2>&1 \
  | tee /tmp/test-bdd-spycatcher-harness-2-1-2-red.out
```

Acceptance: each new test fails for the documented missing behaviour, not for
unrelated compilation errors. Existing non-stream replay scenarios still pass.

### Milestone 2: Canonical-stream policy and matching wiring

Add the domain policy first because it is dependency-free:

- In a new `src/cassette/stream_canonical.rs` (kept under 400 lines), define
  `pub struct StreamCanonicalPolicy { pub ignore_comments: bool }` with a
  `Default` impl that sets `ignore_comments = false`, plus a
  `canonicalize_events(events: &[StreamEvent], policy: StreamCanonicalPolicy)`
  function returning `Vec<StreamEvent>`. Re-export from `src/cassette/mod.rs`.
- Extend `ReplayMatchEngine` with a constructor that accepts the policy
  (favouring `ReplayMatchEngine::with_policy(cassette, mode, policy)` over
  changing the existing `new`). Confirm the new constructor leaves the existing
  `new` signature unchanged.
- Add unit tests under the new module covering at least: empty stream,
  comment-only stream, comment-followed-by-data, repeated comments, and
  data-only stream.

If the team prefers a discoverable knob, register the policy as a new
`MatchMode` variant during the approval discussion before coding. Update this
section after the user's choice and update the Decision Log.

Run focused tests:

```sh
cargo test stream_canonical --all-features 2>&1 \
  | tee /tmp/test-stream-canonical-spycatcher-harness-2-1-2.out
```

Acceptance: the new unit and property tests pass, and pre-existing
`replay_matching_modes` tests are still green. Run `coderabbit review --agent`,
clear actionable findings, commit.

### Milestone 3: Replay response domain extension

Refactor `ReplayResponse` to carry either a one-shot byte body or a parsed
event sequence. The domain remains framework-agnostic:

- In `src/replay.rs`, change `ReplayResponse` to hold a `ReplayBody` enum
  (`OneShot(Vec<u8>)` for non-stream, `Events(Vec<StreamEvent>)` for stream)
  plus the existing `status` and `headers`. Update `response_from_recorded` so a
  `RecordedResponse::Stream` branch yields `ReplayBody::Events(events)` rather
  than `ReplayError::UnsupportedStream`.
- Update `reject_unreplayable_request_shape` to allow streaming requests
  (`is_streaming_chat_completions_request` returning `true`) when the matched
  interaction is itself a stream. The shape rejection only stays active when
  the cassette interaction is non-stream.
- Add an enum variant `ReplayError::StreamCassetteRequiredForStreamRequest`
  (working name) for the explicit "stream requested but cassette has no stream
  interaction" path so the 501 invariant survives. Wire it into a unit test.
- Keep `ReplayError::UnsupportedStream` to preserve any callers that still
  rely on it; mark it as reserved for task `2.1.3`'s raw-transcript path so it
  is not accidentally removed.

Run focused tests:

```sh
cargo test replay --all-features 2>&1 \
  | tee /tmp/test-replay-spycatcher-harness-2-1-2.out
```

Acceptance: domain replay tests prove that a stream cassette yields a
`ReplayResponse` carrying a `Vec<StreamEvent>` in observed order, with header
and status passthrough. Existing non-stream tests still pass. Run
`coderabbit review --agent`, clear findings, commit.

### Milestone 4: Axum adapter and behavioural coverage

Render the new domain stream body at the inbound edge:

- Create `src/server/replay_stream.rs` (mirroring `record_stream.rs`)
  exposing a helper that turns `Vec<StreamEvent>` into an `axum::body::Body`
  backed by either an eager `Bytes` buffer (when total size is below a
  documented soft cap, currently 64 KiB) or a `futures_util::stream::iter`
  adapter. Force `content-type: text/event-stream` if the recorded headers omit
  it.
- Update `src/server/replay_handler.rs`'s `build_replay_response` to
  dispatch on `ReplayBody`. Keep the existing one-shot path source- compatible.
  Remove the
  `spycatcher_harness__server__replay_handler__tests__unsupported_stream.snap`
  snapshot only after the success snapshot lands.
- Add new insta snapshots for the success body and the cassette-shape
  mismatch.

Run the behavioural suite with `tee` logs:

```sh
cargo test --test chat_completions_replay_bdd --all-features 2>&1 \
  | tee /tmp/test-bdd-spycatcher-harness-2-1-2-green.out
```

Acceptance: the replay BDD scenarios pass, the
`Stub upstream saw no replay request` assertions hold, comment frames appear in
the recorded order, and canonical-stream matching makes the comment-drift
scenario green. Run `coderabbit review --agent`, clear findings, commit.

### Milestone 5: Documentation, roadmap, and gates

- Update `docs/spycatcher-harness-design.md` to describe parsed-event
  replay, canonical-stream matching, and the deferred byte-faithful replay (task
  `2.1.3`). Cross-reference the new modules.
- Update `docs/users-guide.md` to describe how stream replay behaves, that
  comments survive, and that internal `StreamCanonicalPolicy` handles
  comment-aware matching without exposing a CLI toggle in this milestone.
- Update `docs/developers-guide.md` with the new module boundaries
  (`src/cassette/stream_canonical.rs`, `src/server/replay_stream.rs`) and the
  hexagonal seam between `ReplayBody` and the Axum adapter.
- Mark `docs/roadmap.md` task `2.1.2` as `[x]` and tick its success
  criteria boxes only after every gate has passed.

Run final gates sequentially with `tee` logs per `AGENTS.md`:

```sh
make check-fmt 2>&1 \
  | tee /tmp/check-fmt-spycatcher-harness-2-1-2.out
make lint 2>&1 \
  | tee /tmp/lint-spycatcher-harness-2-1-2.out
make test 2>&1 \
  | tee /tmp/test-spycatcher-harness-2-1-2.out
make markdownlint 2>&1 \
  | tee /tmp/markdownlint-spycatcher-harness-2-1-2.out
make nixie 2>&1 \
  | tee /tmp/nixie-spycatcher-harness-2-1-2.out
```

Run a final `coderabbit review --agent` after the gates succeed and ensure zero
findings before opening the PR for review.

## Concrete steps

The commands above are the canonical concrete steps. When this plan is in
implementation, update each step with the actual output captured under `/tmp`
so a novice can compare expected to observed evidence.

## Validation and acceptance

A reader who can only see the working tree should be able to verify success by:

1. Running `make check-fmt`, `make lint`, and `make test` and observing
   each pass.
2. Running `cargo test --test chat_completions_replay_bdd --all-features`
   and seeing the new
   `Replay emits a recorded OpenRouter stream including comment frames`
   scenario pass without any upstream request.
3. Running `cargo test stream_canonical --all-features` and seeing the
   canonical-stream unit and property tests pass.
4. Reading `docs/users-guide.md` and `docs/spycatcher-harness-design.md`
   and finding the new stream-replay behaviour described, with the explicit
   note that task `2.1.3` ships byte-faithful replay.
5. Reading `docs/roadmap.md` and finding task `2.1.2` marked complete.

Quality criteria:

- Tests: every `cargo test --workspace` target passes; the new BDD
  scenarios pass; pre-existing scenarios remain green.
- Lint and format: `make lint` and `make check-fmt` pass with no warnings.
- Markdown: `make markdownlint` and `make nixie` pass.
- Review: `coderabbit review --agent` returns zero actionable findings at
  the conclusion of the work.

## Idempotence and recovery

- Each milestone is independently testable and can be reverted with a
  focused `git revert`. Avoid stacking refactors into a single commit so
  rollback stays surgical.
- Snapshot updates use `cargo insta accept` only after manual review of the
  diff. Do not run `cargo insta accept` over uninspected diffs.
- The `/tmp` log filename template
  `${ACTION}-spycatcher-harness-2-1-2.out` keeps repeated runs from shadowing
  earlier evidence; rerun with a fresh suffix when iterating.

## Artifacts and notes

Populate with the most informative transcripts during execution. Capture
expected and observed output for each milestone so the next reader can verify
the run without re-deriving the commands.

## Interfaces and dependencies

By the end of this plan the following symbols must exist with the indicated
shapes:

- In `src/cassette/stream_canonical.rs`:

  ```rust
  pub struct StreamCanonicalPolicy {
      pub ignore_comments: bool,
  }

  impl Default for StreamCanonicalPolicy { /* ignore_comments = false */ }

  pub fn canonicalize_events(
      events: &[StreamEvent],
      policy: StreamCanonicalPolicy,
  ) -> Vec<StreamEvent>;
  ```

  re-exported from `crate::cassette`.

- In `src/cassette/matching.rs`:

  ```rust
  impl ReplayMatchEngine {
      pub fn with_policy(
          cassette: Cassette,
          mode: MatchMode,
          policy: StreamCanonicalPolicy,
      ) -> HarnessResult<Self>;
  }
  ```

  preserving the existing `ReplayMatchEngine::new(cassette, mode)` for source
  compatibility.

- In `src/replay.rs`:

  ```rust
  pub(crate) enum ReplayBody {
      OneShot(Vec<u8>),
      Events(Vec<StreamEvent>),
  }

  pub(crate) struct ReplayResponse {
      pub status: u16,
      pub headers: Vec<(String, String)>,
      pub body: ReplayBody,
  }
  ```

- In `src/server/replay_stream.rs`:

  ```rust
  pub(crate) fn build_stream_body(
      events: Vec<StreamEvent>,
  ) -> axum::body::Body;
  ```

  consumed by `src/server/replay_handler.rs::build_replay_response`.

No new runtime dependencies are introduced. Reuse `futures-util` and `bytes`,
which the lockfile already contains.

## Revision note

This plan was initially authored on `2026-05-29` and is now implementation
complete for roadmap task `2.1.2`, pending final review closure. Revisions
since the draft include completed milestone notes, the selected
`StreamCanonicalPolicy` contract, parsed-event stream replay implementation
details, canonical-stream property-test guidance, replay-handler error
coverage, and final validation gate records.
