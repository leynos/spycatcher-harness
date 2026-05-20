# Add SSE parser and recorder for OpenAI-style streams

This ExecPlan (execution plan) is a living document. The sections `Constraints`,
 `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`,
and `Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

Implementation was explicitly approved on 2026-05-20. The earlier approval gate
is now satisfied; implementation must continue milestone by milestone and stop
only if a tolerance threshold is reached.

## Purpose / big picture

Task `2.1.1` removes the current record-mode limitation for streamed
OpenAI-compatible Chat Completions responses. After the approved
implementation, a caller can send `POST /v1/chat/completions` with
`"stream": true` to the harness in record mode, receive the upstream
Server-Sent Events (SSE) response as it arrives, and inspect a cassette entry
containing both the raw transcript bytes and the parsed stream events.

This task is about recording and parser fidelity. It does not make recorded
stream interactions replayable; replay remains an explicit
`501 Not Implemented` path until tasks `2.1.2` and `2.1.3` deliver
comment-aware and byte-faithful replay.

Observable success after delivery:

- A streamed record-mode request no longer returns `501 Not Implemented`.
- The upstream SSE bytes are proxied downstream without intentional
  reserialization.
- The cassette stores a `RecordedResponse::Stream` with status, selected
  headers, parsed events, raw transcript bytes, and timing metadata.
- Parsed events include OpenAI-style `data:` chunks, `stream_options` usage
  final chunks, and terminal `[DONE]` markers.
- Parser tests prove fragmented byte boundaries, line-ending variants, malformed
  frames, malformed JSON payloads, and invalid UTF-8 are handled without panics
  or parser state corruption.
- Behavioural tests prove the record-mode streaming workflow through a stub
  upstream and verify the persisted cassette shape.

## Relevant documentation and skills

Primary project documentation:

- `docs/roadmap.md` - task `2.1.1`, dependencies, and success criteria.
- `docs/spycatcher-harness-design.md` - architecture overview, cassette
  definition, streaming capture and replay, OpenRouter comments, and replay
  boundaries.
- `docs/users-guide.md` - user-visible behaviour for record and replay modes.
- `docs/developers-guide.md` - internal module boundaries, record-mode seams,
  cassette schema, and behavioural test conventions.
- `docs/rust-testing-with-rstest-fixtures.md` - `rstest` fixtures and
  parameterized unit tests.
- `docs/reliable-testing-in-rust-via-dependency-injection.md` - injected seams
  for time, environment, and upstream behaviour.
- `docs/rust-doctest-dry-guide.md` - public examples must remain valid.
- `docs/complexity-antipatterns-and-refactoring-strategies.md` - use when
  streaming code pushes functions or modules past healthy size.
- `docs/ortho-config-users-guide.md` - configuration wording when documenting
  unchanged record/replay configuration.
- `docs/rstest-bdd-users-guide.md` - feature files, worlds, scenario state, and
  step-definition structure.

External protocol and prior-art references resolved during planning:

- OpenAI cookbook, "How to stream completions": Chat Completions streams are
  data-only SSE chunks; streamed usage appears in an extra final chunk when
  `stream_options.include_usage` is enabled.
- WHATWG HTML Standard, "Server-sent events": SSE streams are UTF-8,
  line-oriented, use blank lines to dispatch events, ignore comment lines for
  normal EventSource consumers, and discard incomplete final events.
- OpenRouter streaming documentation: OpenRouter sends SSE comments such as
  `: OPENROUTER PROCESSING`, can include usage data in the final chunk, and can
  report mid-stream errors as SSE data chunks after HTTP headers are committed.
- `eventsource-stream` on docs.rs: Rust prior art exists for converting byte
  streams into SSE events, but this plan treats a new dependency as an
  escalation point because the harness needs raw transcript preservation in
  addition to parsed events.

Skills to apply during implementation:

- `execplans` - keep this document current and honour the approval gate.
- `leta` - use semantic navigation before modifying Rust code.
- `hexagonal-architecture` - keep parser/domain policy separate from HTTP and
  upstream adapters.
- `rust-router` - route Rust questions to the smallest useful Rust skill.
- `domain-web-services` - keep Axum handlers thin and streaming response
  ownership explicit.
- `rust-async-and-concurrency` - design byte-stream ownership, cancellation,
  and append timing without holding locks across `.await`.
- `rust-errors` - classify parser, upstream, cancellation, and persistence
  failures with typed errors.
- `rust-types-and-apis` - shape parser output and stream transcript types
  without leaking adapter implementation details.
- `nextest` - use when running the Rust test suite.
- `en-gb-oxendict-style` - keep documentation and comments in project style.
- `commit-message` - commit gated, atomic changes using a file-based commit
  message.

## Constraints

- Do not implement streaming replay in this task. Replay of stream requests and
  manually authored `RecordedResponse::Stream` cassettes must continue to
  return the existing unsupported-stream response until later roadmap tasks
  change that behaviour.
- Do not mark `docs/roadmap.md` task `2.1.1` done until implementation,
  documentation, CodeRabbit review, and all quality gates pass.
- Preserve the existing public lifecycle API:
  `start_harness(cfg) -> HarnessResult<RunningHarness>` and
  `RunningHarness::shutdown(self)` remain source-compatible.
- Preserve cassette format compatibility where possible. The existing
  `RecordedResponse::Stream`, `StreamEvent`, and `StreamTiming` types should be
  reused unless implementation discovers they cannot represent terminal markers
  or parser failures safely.
- Maintain hexagonal boundaries:
  - SSE parsing policy and parser state should be adapter-neutral and should
    return cassette/domain types or narrow parser output types.
  - Axum request extraction, response construction, and downstream streaming
    belong in `src/server/`.
  - Reqwest upstream streaming and upstream URL/header construction belong in
    `src/upstream.rs` or focused upstream submodules.
  - Cassette persistence remains behind `CassetteAppender` and
    `FilesystemCassetteStore`.
- The raw transcript must preserve the upstream byte order exactly as observed
  after reqwest content decoding policy has been finalized. If exact transport
  bytes conflict with reqwest behaviour, document the boundary precisely and
  escalate before weakening the claim.
- Do not intentionally reserialize SSE frames before sending them to the
  downstream client in record mode. The default path should pass through the
  received byte chunks while also recording them.
- Store parsed JSON for `data:` payloads when valid. Invalid JSON data frames
  should still be recorded as raw data events unless the frame is malformed at
  the SSE framing layer.
- Treat `[DONE]` as a protocol terminal data marker for OpenAI-style streams and
  record it as a typed or raw data event according to the final schema decision.
- Capture OpenRouter comment lines without changing their relative order. Later
  task `2.1.2` will decide replay matching and deterministic emission semantics
  for comments.
- Continue using `camino` and `cap_std` for path and filesystem work.
- Use `rstest` for unit tests and `rstest-bdd` for behavioural tests.
- Add property tests for parser fragmentation invariants. Kani or Verus should
  be used only if substantive invariants cannot be covered adequately with
  property tests and the necessary harness can be introduced without exceeding
  dependency or scope tolerances.
- Do not mutate process environment in tests. Use injected config, stub
  upstreams, or existing helper seams.
- No single Rust source file may exceed 400 lines after the change.
- Comments and documentation must use en-GB-oxendict spelling.
- Run code quality gates sequentially, not in parallel, and capture output with
  `tee` logs under `/tmp`.
- Use `coderabbit review --agent` after each major milestone and clear all
  actionable concerns before moving on.
- Commit after each gated, atomic change. Do not commit failing work.

## Tolerances (exception triggers)

- Scope: if implementation requires changes to more than 24 files or roughly
  1900 net lines of code and documentation, stop and escalate.
- Public API: if `start_harness`, `RunningHarness::shutdown`,
  `HarnessConfig`, or existing cassette public types require a breaking change,
  stop and escalate.
- Cassette schema: if the existing `RecordedResponse::Stream` shape cannot
  represent terminal markers, malformed events, or timing data without schema
  changes, document the options and ask for approval before changing the
  persisted format.
- Dependencies: if a new runtime dependency is required, stop and present the
  options. A parser crate may be considered only if it can preserve raw
  transcripts and malformed-frame diagnostics without undermining local parser
  control.
- Streaming fidelity: if downstream byte pass-through cannot be implemented
  while recording parsed events, stop and present the trade-off between delayed
  buffering and lower-fidelity proxying.
- Error policy: if malformed upstream SSE should either fail the HTTP exchange
  or record a partial stream, and both choices remain defensible after tests
  and docs are reviewed, stop and ask for a product decision.
- Cancellation: if client disconnect handling would require unsupervised
  background tasks or ambiguous billing/recording semantics, stop and escalate.
- Verification: if `make lint` or `make test` still fails after five repair
  cycles, stop and report the failing evidence.
- Review: if CodeRabbit raises an actionable correctness, safety, or test gap
  concern that cannot be addressed within this plan's scope, stop and ask for
  direction.

## Risks

- Risk: parser correctness depends on byte boundaries that do not align with SSE
  frame boundaries. Severity: high. Likelihood: high. Mitigation: implement an
  incremental parser with explicit pending-buffer state and property tests that
  compare fragmented and unfragmented inputs.

- Risk: preserving raw transcript bytes while parsing incrementally can tempt a
  design that buffers the entire upstream before sending anything downstream.
  Severity: high. Likelihood: medium. Mitigation: design a streaming recorder
  that appends raw bytes and parser events while forwarding chunks promptly. If
  the architecture cannot do this safely, escalate under the streaming fidelity
  tolerance.

- Risk: OpenAI `[DONE]`, usage final chunks, OpenRouter comments, and
  OpenRouter mid-stream error chunks are similar at the wire level but matter
  differently to downstream tooling. Severity: medium. Likelihood: high.
  Mitigation: keep the SSE parser generic, then layer OpenAI-style event
  classification at the protocol adapter boundary.

- Risk: replay remains unsupported after this task, so users may record streams
  they cannot replay yet. Severity: medium. Likelihood: high. Mitigation:
  document the exact limitation in `docs/users-guide.md` and keep replay tests
  asserting the unsupported response until `2.1.3`.

- Risk: large streamed responses increase memory pressure because cassettes
  persist both raw transcript bytes and parsed events. Severity: medium.
  Likelihood: medium. Mitigation: capture only the required bytes/events in
  memory for this slice, document the limitation, and leave spill-to-disk or
  bounded transcript policies out of scope unless memory use becomes a concrete
  test failure.

- Risk: upstream or downstream cancellation can leave partial transcripts.
  Severity: medium. Likelihood: medium. Mitigation: define whether partial
  streams are appended only after a clean terminal marker or are recorded with
  a typed incomplete-stream error. Escalate if product semantics are unclear.

- Risk: adding async streaming support can spread locks or mutable state through
  handlers. Severity: medium. Likelihood: medium. Mitigation: keep ownership
  local to one request path, avoid locks across `.await`, and use
  `spawn_blocking` only for cassette writes.

## Progress

- [x] 2026-05-19 00:25 CEST - Read `AGENTS.md`, branch state, roadmap task
      `2.1.1`, the streaming design section, documentation style guide, and
      adjacent record/replay ExecPlans.
- [x] 2026-05-19 00:35 CEST - Created context pack `pk_3f2fgzhk` for the
      Wyvern agent team with roadmap, design, record service, and cassette
      stream schema references.
- [x] 2026-05-19 00:45 CEST - Used Wyvern agents for read-only topology,
      testing, and protocol-prior-art planning. Findings are reflected in this
      draft.
- [x] 2026-05-19 00:50 CEST - Used Firecrawl to verify OpenAI streaming usage
      chunks, WHATWG SSE parsing rules, OpenRouter comments and mid-stream
      errors, and Rust SSE parser prior art.
- [x] 2026-05-19 01:05 CEST - Drafted this approval-gated ExecPlan.
- [x] 2026-05-20 14:30 CEST - User explicitly approved proceeding with the
      implementation from this ExecPlan.
- [x] 2026-05-20 14:35 CEST - Re-read this plan, `AGENTS.md`, and relevant
      skills at implementation
      start.
- [ ] Establish failing tests for parser and record-mode streaming behaviour.
- [x] 2026-05-20 15:25 CEST - Added and passed focused parser tests covering
      JSON data events, multiline data, comments, `[DONE]`, usage chunks,
      line-ending variants, fragmented boundaries, invalid UTF-8, incomplete
      final events, unknown fields, and a `proptest` fragmentation invariant.
- [x] 2026-05-20 15:50 CEST - Replaced the previous record-mode streaming
      rejection BDD scenario with a streamed proxying scenario that verifies
      downstream bytes, upstream request body, `RecordedResponse::Stream`,
      raw transcript, ordered events, parsed usage, and timing shape.
- [x] Implement adapter-neutral SSE parser and OpenAI-style event
      classification.
- [x] Implement streaming upstream capture and record-mode downstream
      pass-through.
- [x] Persist `RecordedResponse::Stream` entries with raw transcript, parsed
      events, and timing.
- [x] 2026-05-20 16:15 CEST - Ran CodeRabbit after the implementation
      milestone. It reported three concerns: duplicate event-stream header
      checks in BDD steps, a nested `Option<Result<...>>` clarity issue, and a
      missing diagnostic log for an impossible stream request state. All three
      were fixed, focused BDD was rerun, and a second CodeRabbit review
      returned zero findings.
- [x] 2026-05-20 16:35 CEST - Updated the user's guide, developer's guide,
      design document, and roadmap to describe shipped stream recording and
      the still-unsupported stream replay boundary.
- [x] Run CodeRabbit review and clear all actionable concerns.
- [ ] Run `make check-fmt`, `make lint`, and `make test` sequentially with
      `/tmp` `tee` logs.
- [x] 2026-05-20 17:20 CEST - Ran final gates successfully:
      `make check-fmt`, `make lint`, `make test`, `make markdownlint`, and
      `make nixie`, all with `/tmp` logs.
- [x] 2026-05-20 17:35 CEST - Ran final `coderabbit review --agent`; it
      returned zero findings.
- [x] Confirm roadmap task `2.1.1` remains marked done after final gates pass.
- [ ] Commit the completed, gated change.

## Surprises & Discoveries

- The cassette schema already has `RecordedResponse::Stream`, `StreamEvent`,
  and `StreamTiming`, plus round-trip tests for stream-shaped interactions. The
  task can likely reuse the schema rather than introduce a new cassette version.
- Record mode currently rejects `stream: true` before resolving the upstream API
  key, so the implementation must deliberately replace that guardrail with a
  branch into streaming capture.
- The outbound `ReqwestUpstreamClient` currently reads `response.bytes().await`
  and returns a fully materialized `ObservedResponse`; this is the main adapter
  change needed for record-mode streaming.
- OpenRouter documents comment frames and mid-stream error data chunks. The
  parser must not assume every `data:` payload is a normal completion chunk.
- The WHATWG SSE rules discard incomplete final events at end-of-file. The plan
  needs an explicit malformed or incomplete-stream policy for recorder
  behaviour rather than relying on silent parser behaviour.
- `eventsource-stream` exists as Rust prior art, but it exposes parsed events
  from byte streams and does not by itself satisfy the harness requirement to
  preserve raw transcript bytes and malformed frame evidence.
- Existing behavioural tests already contain a scenario named "Streaming
  requests are rejected until streaming support lands"; this should become the
  red test that is replaced by stream-recording assertions.
- Adding reqwest's `stream` feature would have expanded `Cargo.lock` with extra
  packages. The implementation instead uses `reqwest::Response::chunk()` inside
  a local `futures-util` stream wrapper, so only a direct dependency on the
  already-present `futures-util` crate is needed.
- The first streaming implementation pushed `src/server/record.rs` and BDD
  step files beyond the 400-line file limit. Stream recording now lives in
  `src/server/record_stream.rs`, and stream-specific BDD steps live in
  `tests/record_mode_proxying/stream_steps.rs`.

## Decision Log

- Proposed decision: implement an adapter-neutral incremental SSE parser owned
  by the library rather than adding a runtime parser dependency in the first
  implementation. Rationale: the harness must preserve raw transcript bytes,
  classify malformed frames, and support future OpenAI Responses and Anthropic
  streaming without binding parser semantics to a client-only crate.

- Proposed decision: keep OpenAI-specific semantics outside the generic SSE
  parser. The generic parser should understand comments, fields, blank-line
  dispatch, UTF-8, and malformed framing. The OpenAI Chat Completions adapter
  should classify `data: [DONE]`, JSON completion chunks, usage final chunks,
  and mid-stream error payloads.

- Proposed decision: record `[DONE]` in the ordered event list, not only in raw
  transcript bytes. Rationale: success criteria require handling end markers,
  and later replay/matching tasks need a stable terminal signal.

- Proposed decision: malformed JSON inside a syntactically valid `data:` frame
  is not a malformed SSE frame. Record it as
  `StreamEvent::Data { raw, parsed_json: None }` and let higher-level
  verification decide whether the provider payload was acceptable.

- Proposed decision: invalid UTF-8, unterminated events at upstream EOF, or
  field lines that violate the accepted parser grammar should produce typed
  parser diagnostics. If any bytes have already been sent downstream, the
  recorder cannot change the HTTP status; it should avoid appending a
  successful cassette entry unless the incomplete-stream policy is explicitly
  approved.

- Proposed decision: capture coarse timing in this task using the existing
  `StreamTiming` fields: time-to-first-token (`ttft_ms`) and relative event
  offsets (`chunk_offsets_ms`). Detailed replay physics remains task `3.1.1`.

- Proposed decision: keep replay unsupported for streams. Add or retain unit
  and behavioural tests proving recorded stream cassettes still fail with the
  current unsupported-stream response in replay mode.

- Decision: malformed UTF-8 or incomplete final SSE events cause the stream
  recorder to avoid appending a successful cassette entry and increment the
  record-mode failure counter. Rationale: bytes may already have been proxied
  downstream, so the HTTP status cannot be changed reliably after commitment,
  but the cassette must not contain a successful stream whose parser state is
  known to be corrupt.

- Decision: use `futures-util` as a direct dependency because Axum response
  bodies and reqwest chunk polling both need a `TryStream` adapter, and
  `futures-util` was already present in the lockfile. Rationale: this avoids a
  new parser or streaming runtime package while making the stream body explicit
  and testable.

## Implementation plan

### Milestone 1: Baseline and red tests

Reconfirm the working tree is clean enough for a documentation-to-code change:

```sh
git status --short
git branch --show-current
```

Read the current stream guardrail and test entry points:

```sh
leta show RecordService.handle_chat_completions
leta show RecordService.check_not_streaming
leta show ReqwestUpstreamClient.send_chat_completions
leta show RecordedResponse
```

Add failing unit tests before implementation. The parser tests should live near
the parser module, for example `src/protocol/sse.rs` or `src/sse.rs` with a
matching test module. Use `rstest` parameterized cases for:

- one JSON `data:` event ending in `\n\n`,
- multiple `data:` lines combined into one event,
- comment-only frames,
- OpenRouter-style comment followed by JSON data,
- `data: [DONE]`,
- usage final chunk with `choices: []` and non-null `usage`,
- line endings `\n`, `\r\n`, and `\r`,
- valid frames split across every possible byte boundary,
- malformed UTF-8,
- incomplete final event at EOF,
- unknown fields ignored according to SSE rules.

Add a property test using `proptest` that generates valid SSE transcripts and
random fragmentation points. The invariant is that feeding the transcript as
one byte slice or as many arbitrary fragments produces identical parsed events
and identical raw transcript bytes.

Add or update behavioural red tests in
`tests/features/record_mode_proxying.feature` and `tests/record_mode_proxying/`
so a stub upstream emits a chunked SSE response. The scenario should assert
that the client receives the upstream stream bytes, the upstream receives the
request body unchanged, and the cassette contains one
`RecordedResponse::Stream` interaction with the expected raw transcript and
events.

Run focused failing tests and record the expected failure in this plan:

```sh
cargo test sse --all-features 2>&1 | tee /tmp/test-spycatcher-harness-feat-sse-execplan-for-streams.out
cargo test --test record_mode_proxying_bdd --all-features 2>&1 | tee /tmp/test-bdd-spycatcher-harness-feat-sse-execplan-for-streams.out
```

Acceptance for this milestone: tests fail because the parser or streaming
recording behaviour is missing, not because of unrelated compilation errors. Run
 `coderabbit review --agent` if the red-test diff is substantial enough to
benefit from review, then clear any actionable concerns. Commit only if the
repository policy accepts a red-test commit; otherwise keep the red tests
unstaged until the green implementation commit.

### Milestone 2: Parser and event classification

Implement the incremental SSE parser as a small adapter-neutral module. The
module should expose a narrow API such as a parser state object with a method
that accepts `&[u8]` fragments and returns newly completed events plus typed
diagnostics. Keep raw byte accumulation separate from semantic event output so
the recorder can preserve the transcript exactly.

Map generic parsed SSE records into cassette events:

- comments become `StreamEvent::Comment { text }`;
- `data:` payloads become `StreamEvent::Data { raw, parsed_json }`;
- `[DONE]` must be distinguishable, either by extending `StreamEvent` after
  approval if needed or by preserving it as a data event with documented
  terminal semantics.

If `StreamEvent` must change, update cassette serde tests and the design
document in the same milestone. If the change is not backward-compatible,
escalate under the cassette schema tolerance before coding it.

Run parser tests and property tests:

```sh
cargo test sse --all-features 2>&1 | tee /tmp/test-sse-parser-spycatcher-harness-feat-sse-execplan-for-streams.out
```

Acceptance for this milestone: parser unit tests and property tests pass, and
the parser has no dependency on Axum, Reqwest, filesystem adapters, or process
environment. Run `coderabbit review --agent`, resolve actionable concerns, then
commit this parser milestone with a focused commit message.

### Milestone 3: Streaming upstream capture and record orchestration

Extend the outbound upstream port and production Reqwest adapter to support a
streaming response path. Keep non-stream behaviour source-compatible for
existing tests. The implementation may introduce a new return type for
streaming exchanges if it remains crate-internal and does not break public
library APIs.

Replace `RecordService::check_not_streaming` with request classification. The
non-stream branch should keep the current materialized `ObservedResponse`
recording path. The stream branch should:

1. Resolve the upstream API key through the existing `EnvProvider`.
2. Send the request upstream with existing header and URL policy.
3. Forward upstream byte chunks to the downstream client as they arrive.
4. Append each raw chunk to the transcript buffer.
5. Feed each raw chunk into the SSE parser.
6. Capture timing offsets for completed events.
7. Append one cassette interaction only after the stream completes according
   to the approved success policy.

Avoid holding the cassette-store mutex across `.await`. If the stream
implementation needs a channel between the upstream task and downstream
response body, document task ownership and cancellation in `Decision Log`
before finalizing the code.

Run focused record-mode unit tests:

```sh
cargo test record --all-features 2>&1 | tee /tmp/test-record-stream-spycatcher-harness-feat-sse-execplan-for-streams.out
```

Acceptance for this milestone: non-stream record-mode tests still pass, stream
requests are no longer rejected in record mode, malformed upstream stream tests
produce the documented failure behaviour, and no replay behaviour has changed.
Run `coderabbit review --agent`, resolve concerns, then commit.

### Milestone 4: Behavioural and integration coverage

Extend the stub upstream in `tests/record_mode_proxying/helpers.rs` or a
focused streaming helper module so it can emit `text/event-stream` responses
with controlled byte fragments. Include at least one transcript with:

```plaintext
: OPENROUTER PROCESSING

data: {"id":"chatcmpl-test","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}],"usage":null}

data: {"id":"chatcmpl-test","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}],"usage":null}

data: {"id":"chatcmpl-test","object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":null}

data: {"id":"chatcmpl-test","object":"chat.completion.chunk","choices":[],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}

data: [DONE]

```

Update behavioural assertions so the cassette snapshot or structural assertion
proves:

- `response.kind == "stream"`,
- selected response headers include `content-type: text/event-stream`,
- `raw_transcript` equals the stub bytes,
- comment and data events are ordered correctly,
- valid JSON payloads populate `parsed_json`,
- `[DONE]` is present,
- timing offsets are deterministic enough for assertions after redaction.

Keep replay BDD tests explicit: a streamed replay request or matched stream
cassette still returns the unsupported-stream response until task `2.1.3`.

Run behavioural tests:

```sh
cargo test --test record_mode_proxying_bdd --all-features 2>&1 | tee /tmp/test-record-bdd-spycatcher-harness-feat-sse-execplan-for-streams.out
cargo test --test chat_completions_replay_bdd --all-features 2>&1 | tee /tmp/test-replay-bdd-spycatcher-harness-feat-sse-execplan-for-streams.out
```

Acceptance for this milestone: record-mode BDD proves streamed recording, and
replay BDD proves the replay limitation remains deliberate. Run
`coderabbit review --agent`, resolve concerns, then commit.

### Milestone 5: Documentation and roadmap

Update `docs/spycatcher-harness-design.md` to record the implemented parser
contract, terminal marker policy, malformed stream policy, and record/replay
boundary. If the terminal marker or incomplete-stream policy is substantive,
add an Architectural Decision Record (ADR) using the naming and section rules in
 `docs/documentation-style-guide.md`, then link it from the design document.

Update `docs/users-guide.md` so users know:

- record mode supports `stream: true` for OpenAI-style Chat Completions;
- cassettes now persist raw stream transcript bytes and parsed stream events;
- replay of stream interactions remains unsupported until later roadmap tasks;
- malformed upstream streams are handled according to the implemented policy.

Update `docs/developers-guide.md` with internal conventions:

- where the parser lives,
- which layer owns OpenAI-style classification,
- how raw transcript preservation relates to downstream proxying,
- what tests should be added for future streaming protocols.

Update `docs/roadmap.md` only at the end of the implementation, after all gates
pass, changing task `2.1.1` and its success criteria to checked.

Run documentation validation:

```sh
make markdownlint 2>&1 | tee /tmp/markdownlint-spycatcher-harness-feat-sse-execplan-for-streams.out
make nixie 2>&1 | tee /tmp/nixie-spycatcher-harness-feat-sse-execplan-for-streams.out
```

Acceptance for this milestone: documentation describes the actual shipped
behaviour without promising streaming replay. Run `coderabbit review --agent`,
resolve concerns, then commit.

### Milestone 6: Full gates and final commit

Run the required project gates sequentially:

```sh
make check-fmt 2>&1 | tee /tmp/check-fmt-spycatcher-harness-feat-sse-execplan-for-streams.out
make lint 2>&1 | tee /tmp/lint-spycatcher-harness-feat-sse-execplan-for-streams.out
make test 2>&1 | tee /tmp/test-spycatcher-harness-feat-sse-execplan-for-streams.out
```

If documentation changed and the previous milestone did not already run them
after the final docs edit, also run:

```sh
make markdownlint 2>&1 | tee /tmp/markdownlint-spycatcher-harness-feat-sse-execplan-for-streams.out
make nixie 2>&1 | tee /tmp/nixie-spycatcher-harness-feat-sse-execplan-for-streams.out
```

Run a final `coderabbit review --agent` and clear all actionable concerns.
Inspect the final diff:

```sh
git status --short
git diff --stat
git diff -- docs/roadmap.md docs/spycatcher-harness-design.md \
  docs/users-guide.md docs/developers-guide.md
```

Acceptance for this milestone: all required gates pass, CodeRabbit has no open
actionable concerns, `docs/roadmap.md` marks `2.1.1` done, and this ExecPlan's
`Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` are current. Commit the final gated changes.

## Validation plan

Required commands before the implementation is considered complete:

```sh
make check-fmt 2>&1 | tee /tmp/check-fmt-spycatcher-harness-feat-sse-execplan-for-streams.out
make lint 2>&1 | tee /tmp/lint-spycatcher-harness-feat-sse-execplan-for-streams.out
make test 2>&1 | tee /tmp/test-spycatcher-harness-feat-sse-execplan-for-streams.out
```

Documentation gates when docs change:

```sh
make markdownlint 2>&1 | tee /tmp/markdownlint-spycatcher-harness-feat-sse-execplan-for-streams.out
make nixie 2>&1 | tee /tmp/nixie-spycatcher-harness-feat-sse-execplan-for-streams.out
```

Focused checks expected during development:

```sh
cargo test sse --all-features
cargo test record --all-features
cargo test --test record_mode_proxying_bdd --all-features
cargo test --test chat_completions_replay_bdd --all-features
```

Each long-running command must be run sequentially and should use `tee` to a
`/tmp` log. If a command fails, inspect the log before making the next change.

## Outcomes & Retrospective

Task `2.1.1` shipped record-mode support for OpenAI-style SSE streams. The
implementation adds an incremental SSE parser in `src/sse.rs`, a streaming
upstream path in `src/upstream.rs`, and record-mode stream orchestration in
`src/server/record_stream.rs`. Stream requests now proxy upstream bytes to the
client and append `RecordedResponse::Stream` entries containing selected
headers, ordered comments and data events, raw transcript bytes, and coarse
timing metadata.

Replay of streams remains deliberately unsupported. Replay still returns
`501 Not Implemented` for `stream: true` requests and matched stream cassette
responses, which preserves the boundary for roadmap tasks `2.1.2` and
`2.1.3`.

The main implementation deviation was dependency handling. Enabling reqwest's
`stream` feature would have expanded the resolved package graph, so the final
implementation uses `reqwest::Response::chunk()` plus a direct dependency on
the already-present `futures-util` crate. This kept streaming explicit without
adding a new parser or runtime package.

CodeRabbit found three actionable issues after the implementation milestone:
duplicated event-stream header checks in BDD steps, a nested
`Option<Result<...>>` expression that should use `transpose`, and a missing
diagnostic log for an impossible missing-request state. All were fixed. The
subsequent milestone review and final review both returned zero findings.

Final validation passed with these logs:

- `/tmp/check-fmt-spycatcher-harness-2-1-1-sse-parser-and-recorder-for-open-ai-streams.out`
- `/tmp/lint-spycatcher-harness-2-1-1-sse-parser-and-recorder-for-open-ai-streams.out`
- `/tmp/test-spycatcher-harness-2-1-1-sse-parser-and-recorder-for-open-ai-streams.out`
- `/tmp/markdownlint-spycatcher-harness-2-1-1-sse-parser-and-recorder-for-open-ai-streams.out`
- `/tmp/nixie-spycatcher-harness-2-1-1-sse-parser-and-recorder-for-open-ai-streams.out`
- `/tmp/coderabbit-spycatcher-harness-2-1-1-sse-parser-and-recorder-for-open-ai-streams.out`

The malformed-stream policy is sufficient for this slice: invalid UTF-8 or an
incomplete final event prevents a successful cassette append. Later replay work
can rely on stream cassettes representing syntactically complete SSE
transcripts.
