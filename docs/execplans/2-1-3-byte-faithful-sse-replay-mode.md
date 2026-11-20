# 2.1.3 Implement byte-faithful SSE replay mode

This ExecPlan (execution plan) is a living document. The sections `Constraints`,
`Tolerances`, `Risks`, `Progress`, `Surprises & discoveries`, `Decision log`,
`Outcomes & retrospective`, `Conformance basis`, and `Verification plan` must
be kept up to date as work proceeds.

Status: DRAFT (expert design review applied; awaiting user approval)

Branch: `2-1-3-byte-faithful-sse-replay-mode`

Roadmap item: `2.1.3` in `docs/roadmap.md`.

Implementation must not begin until the user explicitly approves this plan.
After approval, work proceeds milestone by milestone and stops only when a
tolerance threshold is reached, a constraint would be violated, or an
architecture deviation needs acceptance.

## Purpose / big picture

The Spycatcher harness records a conversation between an agent and a Large
Language Model (LLM) provider into a JSON file called a cassette, then replays
that cassette in continuous integration (CI) without network access. Providers
such as OpenRouter stream chat completions as Server-Sent Events (SSE): a
`text/event-stream` HTTP body made of text lines such as `data: {...}` and
`: OPENROUTER PROCESSING`, in which a blank line ends ("dispatches") one event.

Roadmap tasks `2.1.1` and `2.1.2` made the harness record every streamed
interaction twice: as a list of parsed `StreamEvent` values and as the exact
upstream bytes in `raw_transcript`. Replay today uses only the parsed events.
It writes them back out in one canonical form: every line ends in a line feed,
every data line starts with `data:` and one space, every comment line starts
with `:` and one space, and fields the parser ignores (`event:`, `id:`,
`retry:`) are lost. That is deliberate and documented as "not byte-faithful".

For example, an upstream that sent this transcript (shown with escapes):

```plaintext
event: chunk\r\ndata:{"choices":[]}\r\n\r\n: OPENROUTER PROCESSING\r\n\r\ndata: [DONE]\r\n\r\n
```

is replayed today as:

```plaintext
data: {"choices":[]}\n\n: OPENROUTER PROCESSING\n\ndata: [DONE]\n\n
```

After this change, a user can select the stream replay mode through
configuration:

- `parsed_events` (the default) keeps today's behaviour exactly.
- `raw_transcript` replays the recorded `raw_transcript` bytes verbatim, so the
  same client receives the first transcript above byte for byte, including the
  carriage returns, the `event:` field, and the missing space after `data:`.
  The harness writes those bytes as a sequence of HTTP body chunks, each of
  which ends exactly where an SSE event is dispatched ("event-aligned frames").

The mode is selected in `[cmds.replay]` of the configuration file as
`stream_mode = "raw_transcript"`, by the environment variable
`SPYCATCHER_HARNESS_CMDS_REPLAY_STREAM_MODE=raw_transcript`, by the flag
`spycatcher-harness replay --stream-mode raw_transcript`, or by library callers
through `HarnessConfig.replay.stream_mode`.

Preserved behaviour: matching, cursor commitment, status and recorded-header
replay, the `text/event-stream` default content type, non-stream replay, record
mode, the cassette schema (`format_version` 1), and every parsed-events byte.
Deliberate changes:

- In `raw_transcript` mode, a matched stream interaction that has recorded
  events but an empty `raw_transcript` (only possible in a hand-authored
  cassette) is answered with HTTP `501 Not Implemented` and error kind
  `unsupported_stream`. The interaction is not consumed. The message of that
  error changes from "not implemented yet" to an actionable explanation.
- In `raw_transcript` mode, replay start-up logs one bounded warning per
  stream interaction whose `raw_transcript` does not parse to its recorded
  events, because edits to `events` have no effect in this mode.
- Raw-mode responses always use chunked transfer encoding; parsed-mode
  responses keep today's framing.

Raw mode reproduces upstream quirks faithfully, including ones that break some
clients and that parsed mode would have smoothed over (for example a final
event terminated only by a lone carriage return). Users choose raw mode when
they need the exact wire bytes; parsed mode remains the robust default.

This closes the deferral recorded in `docs/spycatcher-harness-design.md`
("Byte-faithful replay of exact upstream framing remains roadmap task `2.1.3`")
and the mitigation promised under "Known risks and limitations" ("record raw
byte transcript and replay it verbatim as an option; test against
representative SSE clients"). The frame ends are also candidate insertion
points for the timing delays of task `3.1.1`; how frames map to the per-event
`chunk_offsets_ms` is left to that task.

## Relevant documentation and skills

Read these before starting. Each is cited where it matters below.

- `docs/roadmap.md`, item `2.1.3`, for the success criteria.
- `docs/spycatcher-harness-design.md`, sections "Cassette definition",
  "Streaming capture and replay", "Known risks and limitations", and
  "Implementation decisions".
- `docs/developers-guide.md`, sections "Rust lint baseline", "SSE parser and
  stream recorder", "Shared test infrastructure", "Module layout", and the
  replay metrics label list.
- `docs/users-guide.md`, sections "Configuration", "Replay behaviour for chat
  completions", "Common CLI flags", "Configuration file shape", and
  "Environment variable shape".
- `docs/testing-guide.md` for dev-dependency rationale and test layout.
- `docs/rust-testing-with-rstest-fixtures.md` for fixtures and `#[case]`
  tables.
- `docs/rstest-bdd-users-guide.md` for `rstest-bdd` 0.6 scenarios and steps.
- `docs/reliable-testing-in-rust-via-dependency-injection.md` for the
  environment-injection rule (`EnvProvider`, `MapEnv`, `read_ambient`).
- `docs/rust-doctest-dry-guide.md` for doctest style on new public items.
- `docs/complexity-antipatterns-and-refactoring-strategies.md` for keeping new
  functions small and files under 400 lines.
- `docs/ortho-config-users-guide.md`, "Let files override clap defaults", for
  why the new flag must not carry a clap default.
- `docs/scripting-standards.md` for the Python `uv` script conventions used by
  the OpenAI Python client check and the Verus mutation gate.
- `docs/documentation-style-guide.md` for ADR format, Markdown wrapping, and
  en-GB-oxendict spelling.
- Skills: `execplans` (this format); `rust-router`, then `rust-unit-testing`,
  `proptest`, `rust-verification`, and `verus`; `hexagonal-architecture` for
  the domain and adapter split; `domain-web-services` for the Axum body; and
  `codegraph-mcp` for navigation (for example `codegraph_get_callers` on
  `response_from_recorded` and `build_replay_response`).

## Constraints

- The cassette schema must not change. `CassetteFormatVersion::SUPPORTED`
  stays `1`, `RecordedResponse::Stream` keeps its fields and serde shape, and
  `raw_transcript` stays a required `Vec<u8>` serialized as a JSON integer
  array. Any change requires escalation.
- `parsed_events` must remain the default and must stay byte-identical to the
  current output. The existing assertions in
  `src/server/replay_stream_tests.rs`, `src/server/replay_stream_properties.rs`,
  `src/replay_tests/`, and `tests/chat_completions_replay_bdd.rs` must pass
  unchanged. Tests may move between modules, and exhaustive `match`es on
  `ReplayBody` gain one arm, but no expected value changes.
- Record mode, request matching, canonicalization, and cursor semantics must
  not change. The replay engine must still peek, build the response, and only
  then commit (`match_recorded_response` in `src/replay.rs`), so a rejected
  response never consumes an interaction.
- `raw_transcript` mode must emit exactly the recorded bytes: no line-ending
  normalization, no comment stripping (`StreamCanonicalPolicy` is a matching
  concern and must not be applied to emitted bytes), no synthetic terminator,
  and no byte-order-mark (BOM) handling. It must not clone `events`, and frame
  ends must not be computed while the replay engine's mutex is held.
- Hexagonal boundaries: the frame-boundary kernel (`src/sse_frames.rs`) is pure
  domain code with no I/O, Axum, `bytes`, or `tokio` imports. The domain
  (`src/replay.rs`, `src/replay_types.rs`) decides the body shape; the Axum
  adapter (`src/server/`) turns that shape into an HTTP body.
- Configuration layering must follow the existing pattern (`ReplayArgs` in
  `src/cli/mod.rs`, merged by `load_and_merge_subcommand`, mapped in
  `src/cli/config_build.rs`). The new flag must not have a clap
  `default_value`, which would make the command-line layer always win. Tests
  that need configuration must inject sources (`load_with_sources`, `MapEnv`,
  or subprocess `Command::env`); no in-process environment mutation and no
  `serial_test` or global locks.
- Repository rules from `AGENTS.md` and `Cargo.toml`/`clippy.toml`: no file over
  400 lines; every module starts with `//!`; every assertion has a message; no
  `.expect()` or `.unwrap()` outside tests; `clippy::indexing_slicing` and
  `option_if_let_else` are denied; cognitive complexity at most 9 and nesting
  at most 4; lint suppressions only as item-scoped
  `#[expect(..., reason = "...")]`; en-GB-oxendict spelling.
  `src/replay_tests/mod.rs` (399 lines) and `src/cassette/filesystem.rs` (399
  lines) must stay at or under 400 lines.
- The `#[expect(dead_code, ...)]` on `ReplayError::UnsupportedStream` in
  `src/replay_types.rs` must be removed in the same commit that first
  constructs the variant. The kernel must land in the same milestone as its
  first production caller so it never needs a `dead_code` expectation.
- Verus proofs must be about the production kernel body included by
  `#[path]`, not a parallel reimplementation. Forbidden in the proof and in the
  kernel: `assume`, `admit`, `#[verifier::external_body]`,
  `#[verifier::external]` on anything the kernel reaches,
  `assume_specification`, `#[verifier::exec_allows_no_decreases_clause]`, any
  `cfg(not(verus_keep_ghost))` code inside the kernel body, and `VERUS_FLAGS`
  that skip modules.
- Authorized new dependencies, all dev-only with caret requirements:
  `googletest = "0.14"`, `pretty_assertions = "1.4"`, and
  `eventsource-stream = "0.2.3"`. No new runtime dependency. The `reqwest`
  feature set must not change (the client tests use `Response::chunk()`, which
  needs no `stream` feature). `http-body-util` must not become a direct
  dependency; body frames are read with `axum::body::Body::into_data_stream()`
  and `futures_util::StreamExt::next`.
- Authorized new tooling: Verus pinned through `rust-prover-tools`
  (`tools/verus/VERSION`, `tools/verus/SHA256SUMS`,
  `tools/rust-prover-tools/REF`); Makefile targets `verus-install`, `verus`,
  `verus-selftest`, `verus-mutation`, and `test-sse-clients`; a new
  `.github/workflows/verus.yml`; a `.gitattributes` entry marking SSE fixtures
  as binary; and a uv cache step in `.github/workflows/ci.yml`. `make verus`
  stays out of `make all`, `make test`, and `make lint`, but its workflow runs
  on every pull request without a path filter.
- The binary's lifecycle must not change. `spycatcher-harness replay` starts
  and immediately shuts down today (`run_harness` in
  `src/bin/spycatcher_harness.rs`), so tests that need a serving harness use
  the library's `start_harness` in-process; subprocess tests only check
  configuration wiring. Making the binary long-running is out of scope.
- Historical documents must remain untouched: every completed plan under
  `docs/execplans/` and the accepted text of ADR 0001 and ADR 0002.
- The public library change is additive: a new
  `#[non_exhaustive] pub enum config::StreamReplayMode` and a new
  `pub stream_mode` field on `config::ReplayConfig`. The crate is pre-1.0 and
  every in-repository `HarnessConfig` literal uses
  `..HarnessConfig::default()`, so no compatibility shim is needed or permitted.

## Tolerances (exception triggers)

Counting base: `git diff --numstat c3e6d76...HEAD`, excluding
`docs/execplans/2-1-3-byte-faithful-sse-replay-mode.md`. Each limit measures
net lines (additions minus deletions) and is enforced independently; spare
capacity in one category never offsets another.

Classification is by whole file:

- Code: `src/**`, `tests/**` (including `.feature` files, `.sse` fixtures,
  insta `.snap` files, and Python test scripts), `verus/**`, `scripts/**`, and
  `Makefile`.
- Configuration: `Cargo.toml`, `.gitattributes`, `.gitignore`,
  `.github/workflows/**`, `tools/**`, and `typos.local.toml`. `Cargo.lock` and
  the generated `typos.toml` are excluded from line counts; instead, the
  lockfile may add at most 12 new `[[package]]` entries (count with
  `git diff c3e6d76 -- Cargo.lock | grep -c '^+name = '`).
- Documentation: `docs/**` except this ExecPlan, plus `README.md`.

Budget survey: the comparable plans `2-1-1` (24 files or about 1,900 net lines),
`2-1-2` (22 files or about 1,700), `1-3-2` (22 files or about 1,700), and
`1-2-3` (16 files or 1,200) used one combined budget, recorded no revisions,
and grew only through repository constraints (file splits for the 400-line
limit and avoiding the `reqwest` `stream` feature). This task adds work those
plans did not have: exhaustive small-scope tests, a Verus proof with its
tooling and contract tests, a wire-level framing test, and third-party client
compatibility checks in two languages. The estimates below come from the
file-level plan in `Plan of work`.

- Files: stop and escalate if more than 60 non-ExecPlan files change.
- Code: stop and escalate above 3,600 net lines. Estimate: about 1,250 lines
  of production and unit-test Rust for the kernel, domain, adapter,
  observability, and configuration; about 900 lines of behaviour-driven,
  wire-level, and end-to-end tests and fixtures; about 550 lines of Verus
  proof, macros, and tooling contract tests; about 400 lines for the Python
  client check, the mutation gate script and its tests, and Makefile targets.
  Contingency is roughly 15 per cent.
- Configuration: stop and escalate above 260 net lines, or above 12 new
  lockfile packages. Estimate: about 10 lines in `Cargo.toml`, 80 in
  `verus.yml`, 30 in `ci.yml`, 10 in `tools/`, and 5 in `.gitattributes` and
  `.gitignore`.
- Documentation: stop and escalate above 800 net lines. Estimate: two ADRs
  (about 320), the design document (about 100), the developers' guide (about
  160), the users' guide (about 100), the testing guide (about 50), and roadmap
  and contents updates (about 15).
- Proof: escalate if Verus proof code (everything under `verus/` plus the
  kernel macro file) exceeds 350 lines, if `--rlimit` must be raised, if a
  trigger warning would have to be ignored, or if `make verus` still fails 8
  working hours after `make verus-selftest` first passes. The fallback to
  present is "drop `EP-M4`; the exhaustive, property, and example evidence of
  `EP-M1` stands", not a weaker proof.
- Interface: escalate if any public item beyond `config::StreamReplayMode`
  and `ReplayConfig::stream_mode` must change, or if `ReplayBody`,
  `ReplayError`, or `StreamDeliveryMode` need more than one new variant each.
- Dependencies: escalate if a dependency beyond the three authorized dev
  dependencies is needed, including `async-openai`, `reqwest-eventsource`,
  `http-body-util`, or a `reqwest` feature change.
- Architecture: escalate if raw replay appears to need a cassette schema
  change, recorded upstream chunk boundaries, changes to record mode, or a
  long-running binary.
- Iterations: escalate if a focused test still fails after 5 fix attempts.
- Time: escalate if any milestone exceeds 8 working hours.
- Ambiguity: escalate if a representative client disagrees with the WHATWG
  rules in a way that changes what "event boundary" means for this plan.

## Risks

- Risk: the Verus bridge does not accept a kernel that also passes this
  repository's lints (`indexing_slicing` denied, complexity limits, no
  let-chains in the Verus rustc). Severity: medium. Likelihood: medium.
  Mitigation: `EP-M0` probes the real kernel shape under full `make lint` and
  Verus; the proof is the last implementation milestone and delivery does not
  depend on it; the fallback is the exhaustive small-scope evidence.
- Risk: real clients disagree with WHATWG. Known from source reading:
  `eventsource-stream` 0.2.3 never delivers a final event whose blank line is a
  lone carriage return at end of stream (its parser waits for a possible line
  feed); the OpenAI Python SDK keeps the last event ID across events and so
  turns a blank line after any `id:` into an empty event that fails JSON
  parsing; openai-node delivers an unterminated final event. Severity: medium.
  Likelihood: high. Mitigation: per-fixture, per-client expectation tables that
  check transparency, content, and mode contrast separately; fixtures that pin
  each quirk; users' guide notes that raw mode reproduces upstream quirks.
- Risk: lone-carriage-return line endings. A frame ending in a lone carriage
  return cannot be recognized as complete by a streaming parser until the next
  byte arrives, and at end of stream some parsers drop it. Severity: medium.
  Likelihood: low (CR-only upstreams are rare). Mitigation: the kernel never
  splits a carriage return from its line feed; the behaviour is faithful to the
  upstream; it is documented and pinned by a fixture.
- Risk: cassette size and load time. `raw_transcript` is pretty-printed as a
  JSON integer array (about 14 bytes on disk per transcript byte), and replay
  start-up reads the cassette unbuffered and twice. This exists today, but raw
  mode makes users depend on the field. Severity: medium. Likelihood: medium.
  Mitigation: `EP-M0` measures a 1 MiB transcript; ADR 0003 records the
  measurement and a trigger for a later `format_version` 2 (for example base64
  or a sidecar `.sse` file); buffered reading is recorded as a follow-up, not
  done here.
- Risk: users scrub or edit `events` but not the `raw_transcript` array, so raw
  mode replays unredacted or stale bytes. Severity: medium. Likelihood: medium.
  Mitigation: the start-up consistency warning; users' guide states that
  `raw_transcript` is authoritative in raw mode; the design document's risks
  section ties redaction of `raw_transcript` to the `verify` task (`2.2.1`); a
  roadmap amendment is proposed for the user's decision.
- Risk: the Python client check needs network access to fetch `openai` and its
  dependencies on first run, and a transitive release could break it. Severity:
  low. Likelihood: medium. Mitigation: it runs from a dedicated
  `make test-sse-clients` target in CI, not in `make test`; the uv script pins
  `openai` exactly and sets `exclude-newer`; CI caches uv downloads.
- Risk: the new dev-dependencies fail `make msrv-check`, which compiles all
  targets (including tests) on Rust 1.89.0. Severity: low. Likelihood: low.
  Mitigation: `EP-M0` runs `make msrv-check` with all three added.
- Risk: the Verus workflow becomes flaky and is waived, and later edits to the
  kernel by task `3.1.1` break the proof unnoticed. Severity: medium.
  Likelihood: medium. Mitigation: cache by pinned version, 15-minute job
  timeout, run on every pull request without a path filter, and an ADR 0004
  rule that a failing proof blocks merge.
- Risk: the 400-line limit forces splits of `src/replay.rs` (338 lines) or
  `src/cli/mod.rs` (343 lines). Severity: low. Likelihood: medium. Mitigation:
  new logic goes into new focused modules (`src/sse_frames.rs`,
  `src/server/replay_transcript.rs`, `src/replay_consistency.rs`, and
  `src/cli/replay_args.rs` if needed).

## Progress

- [x] (2026-11-15 15:30Z) Reconnaissance with a Wyvern team: replay stream
  path, configuration surface, test and proof infrastructure, and prior
  ExecPlan budgets.
- [x] (2026-11-15 15:40Z) External research: WHATWG SSE parsing rules, OpenAI
  Python SDK `SSEDecoder`, `eventsource-stream` 0.2.3 as used by `async-openai`
  0.43, and the estate Verus pattern in `mdtablefix`.
- [x] (2026-11-15 16:30Z) Initial draft written and committed.
- [x] (2026-11-15 17:45Z) Community-of-experts design review (structure and
  contracts; failure modes and operations; verification and test strategy; SSE
  clients and alternatives) applied; see `Decision log` D10 to D18.
- [ ] Plan approved by the user.
- [ ] `EP-M0` feasibility probe.
- [ ] `EP-M1` raw-transcript replay in the library, with the frame kernel.
- [ ] `EP-M2` configuration layering and binary wiring.
- [ ] `EP-M3` representative streaming client compatibility.
- [ ] `EP-M4` Verus proof of the frame kernel.
- [ ] `EP-M5` documentation, ADRs, and roadmap closure.

## Surprises & discoveries

- Observation: the existing byte-equality stream test cannot distinguish the
  two modes. Evidence: `tests/chat_completions_replay/stream_steps.rs` compares
  the replayed body with `sample_stream_transcript()`, which is already in
  canonical form. Impact: raw-mode tests must use non-canonical transcripts.
- Observation: parsed events are lossy in order as well as framing. Evidence:
  `src/sse.rs` emits a comment as soon as its line is read, so
  `data: a\n: c\ndata: b\n\n` yields `[Comment c, Data "a\nb"]`. Impact: raw
  mode must take bytes from `raw_transcript`, never from events.
- Observation: no replay option is configurable from the CLI, environment, or
  file today; `match_mode` and `replay.*` are library-only. Evidence:
  `ReplayArgs` in `src/cli/mod.rs` and `to_replay_config` in
  `src/cli/config_build.rs`. Impact: `stream_mode` is the first layered replay
  option.
- Observation: the binary starts the harness and shuts it down immediately and
  never prints its address. Evidence: `run_harness` in
  `src/bin/spycatcher_harness.rs` calls `start_harness` then `shutdown`.
  Impact: no subprocess test can send requests to it (D12).
- Observation: `src/server/snapshots/*unsupported_stream*.snap` exists but no
  test case references it, and its message ("not implemented yet") will be
  false after this task. Impact: `EP-M1` changes the message and re-attaches
  the snapshot deliberately.
- Observation: error bodies use `{"error": {"kind": ..., "message": ...}}`
  (`json_error_body` in `src/server/replay_handler.rs`), not `code`.
- Observation: the upstream request strips `accept-encoding`
  (`src/upstream.rs`, `src/http_exchange.rs`), so recorded transcripts are
  identity-encoded SSE text, not compressed bytes.
- Observation: `make msrv-check` compiles all targets, including tests and
  dev-dependencies (`Makefile`, `--all-targets`).
- Observation: hyper's client-side chunked decoder can hand a reader part of a
  chunk, so client-observed read boundaries can split frames as well as merge
  them. Impact: frame alignment is asserted on the server side and on the raw
  HTTP chunk framing, never on client read boundaries (D11).

## Decision log

- D1. Decision: two modes, `parsed_events` (default) and `raw_transcript`,
  spelt in `snake_case` in files, environment, and CLI, and the enum is
  `#[non_exhaustive]`. Rationale: keeps the default byte-identical for existing
  users; matches the repository's enum precedent (`RecordUpstreamKind`,
  `rename_all = "snake_case"`) and the cassette field name; leaves room for a
  later `auto` value. Rejected: raw mode as default, which would silently
  change output for existing suites and break hand-authored cassettes with an
  empty transcript. Date/Author: 2026-11-15, planning agent.
- D2. Decision: the setting lives on `ReplayConfig` as `stream_mode`, on
  `ReplayArgs` as `stream_mode: Option<StreamModeArg>`, and on `ReplayService`
  as its own field. Rationale: `ReplayConfig` owns replay output shaping;
  `ReplayArgs` is the established route to file, environment, and CLI layering;
  `ReplayContext` is documented as observability context, so behaviour must not
  hide there. Date/Author: 2026-11-15, revised after review.
- D3. Decision: raw mode writes event-aligned frames: one HTTP body chunk per
  SSE dispatch boundary, computed by a pure kernel over the recorded bytes.
  Rationale: upstream chunk boundaries were never persisted (`2.1.1` appends
  chunks to one buffer) and TCP does not preserve them, so the strongest
  boundary property available is that every chunk the harness writes ends where
  a conformant client dispatches, and no chunk splits a carriage return from
  its line feed or a multi-byte UTF-8 character from itself. It protects naive
  clients that parse each read separately and gives task `3.1.1` candidate
  timing insertion points. The roadmap requirement itself is semantic and is
  met by verbatim bytes plus client transparency (D11); D3 is the design choice
  layered on it (`REQ-RAW-FRAMING`). Rejected: one eager body (no alignment, no
  timing hook); re-deriving chunks from parsed events (lossy).
- D4. Decision: raw mode always streams frames and never uses the 64 KiB eager
  path; delivery is recorded with a new `StreamDeliveryMode::RawFramed`
  (`"raw_framed"`), and the existing event histogram receives the recorded
  event count so its meaning is unchanged. Rationale: frame alignment is the
  point of the mode; a distinct bounded label keeps the existing `eager` and
  `streamed` semantics.
- D5. Decision: an empty `raw_transcript` with non-empty `events` returns
  `ReplayError::UnsupportedStream` (HTTP 501, kind `unsupported_stream`) during
  the peek, before commit, with a new actionable message; an empty transcript
  with no events replays an empty body. Rationale: explicit failure instead of
  silent fallback to parsed events; reuses the variant `2.1.2` reserved for
  this task. Rejected: refusing to start, which would block cassettes where the
  interaction is never reached and duplicate `2.2.1`.
- D6. Decision: in raw mode, `ReplayAppState::from_config` checks each stream
  interaction once at start-up by parsing `raw_transcript` with `SseParser` and
  comparing with `events`, and logs one `warn!` per inconsistent interaction
  (first 10 only, fixed reason categories: `empty_transcript`,
  `incomplete_transcript`, `invalid_utf8`, `events_differ`) plus a summary
  count. It never rejects the cassette. Rationale: edits and redactions to
  `events` silently stop mattering in raw mode; a load-time warning catches
  that without moving `2.2.1`'s verification policy into replay.
- D7. Decision: the frame kernel's correctness is established by exhaustive
  small-scope enumeration, property tests, and named examples in `EP-M1`, then
  proved for all lengths with Verus in `EP-M4`. Delivery of the roadmap
  criteria does not depend on `EP-M4`. Rationale: the kernel is the single new
  piece of contractual logic, a small pure loop over arbitrary byte sequences,
  which is the case the testing hierarchy reserves for an unbounded proof; the
  exhaustive check covers every local pattern but cannot exclude
  length-dependent scanner bugs. Rejected: Kani (bounded, and needs tooling of
  similar cost); Verus over a spec mirror (proves nothing about production
  code); dropping Verus entirely (recommended by one reviewer as cheaper; kept
  because the request asks for Verus where warranted and the kernel is the
  warranted case; the user may choose to drop `EP-M4` at approval).
- D8. Decision: representative clients are `eventsource-stream` 0.2.3
  (in-process, part of `make test`) and the official OpenAI Python SDK (via a
  `uv` script driven by an ignored Rust test, run by `make test-sse-clients` in
  CI). Rationale: `eventsource-stream` is the SSE decoder used by
  `async-openai` 0.43 and `reqwest-eventsource`; the OpenAI Python SDK has its
  own decoder and is the client most agent frameworks use (LangChain uses it,
  and `httpx-sse` shares its decoder's origin). Rejected: `async-openai` (large
  dependency exercising the same decoder); browser `EventSource` (GET only,
  cannot post a chat completion); openai-node through bun (bun is not in CI;
  better suited to tasks `4.1.1` and `4.2.1`, whose Anthropic SDKs share its
  decoder design).
- D9. Decision: record the mode and framing decisions in ADR 0003 and the
  Verus adoption in ADR 0004. ADR 0002's "Amended by ADR-0004" banner refers to
  concordat's ADR-0004, an external document; the local ADR 0004's title names
  Verus and its text states that it is unrelated to concordat ADR-0004.
- D10. Decision: milestone order is library replay with kernel, then
  configuration, then client compatibility, then the Verus proof, then
  documentation. Rationale (review): the kernel lands with its caller so no
  `dead_code` expectation is needed, and the roadmap criteria do not wait on
  prover tooling.
- D11. Decision: the client-compatibility oracle has three parts per fixture
  and client: transparent (the client's view through raw replay equals its view
  of the recorded upstream), content (the client yields the payloads the
  recorded events predict, or a named expected error), and mode contrast (the
  documented difference between modes). Frame alignment is asserted on the
  server side and on raw HTTP/1.1 chunk framing over a `TcpStream`, never on
  client read boundaries. Rationale (review): comparing a client only with
  itself hides failures, and client read boundaries are not deterministic.
- D12. Decision: tests that need a serving harness use `start_harness`
  in-process; the Python check is spawned by a Rust test that owns the harness
  and a stub upstream. Rationale (review): the binary exits immediately after
  start-up.
- D13. Decision: the Python check performs a record-then-replay round trip.
  The Rust driver starts a stub upstream serving a fixture's bytes, starts a
  record-mode harness, runs the SDK through it (so the cassette holds exactly
  the request the pinned SDK sends), then starts a raw-mode replay harness on
  that cassette and runs the SDK again. The two SDK outputs must be equal.
  Rationale (review): strict matching compares the canonical request, so a
  handwritten cassette would have to guess the SDK's exact body.
- D14. Decision: the kernel's postconditions are stated in gap form (ordered,
  terminated, sound, gap-free), each as a named `spec fn`, with completeness
  and uniqueness derived as lemmas. Rationale (review): gap form is cheaper for
  the solver, and named predicates make mutation failures attributable.
- D15. Decision: the kernel indexes the byte slice directly, under an
  item-scoped `#[expect(clippy::indexing_slicing, reason = "...")]` whose
  reason cites the Verus proof, inside a small local dual-expansion macro
  (modelled on `mdtablefix`'s `verified_kernel_function!`, trimmed to one arm
  that accepts exactly that attribute). `EP-M0` may replace this with
  `cfg_attr(verus_keep_ghost, verus_spec(...))` if that verifies and lints.
  Rationale (review): `get`-and-`match` code is pushed by clippy towards
  closures that Verus handles poorly.
- D16. Decision: the error message for `unsupported_stream` becomes "raw
  transcript stream replay requires a recorded raw_transcript; the matched
  interaction has events but an empty raw_transcript; re-record it or use
  stream_mode = parsed_events". Rationale (review): the old message would
  mislead a CI user into thinking streaming replay is broken.
- D17. Decision: keep `googletest` and `pretty_assertions` as requested. Use
  `googletest` matchers where they add diagnostics (`elements_are!` for frame
  vectors, `contains_substring` for error messages, `matches_pattern!` for
  `ReplayBody` variants) and `pretty_assertions::assert_eq!` on
  `escape_ascii()` strings rather than raw `Vec<u8>`, so diffs show `\r\n`.
- D18. Decision: out of scope, recorded as follow-ups: buffered cassette
  reading and the double parse at start-up; a long-running binary; sending
  `x-should-retry: false` on deterministic replay errors; warning that
  `simulate_timing` has no effect until `3.1.1`; recording upstream chunk
  boundaries (natural to revisit with the schema change `4.2.1` is likely to
  need for `event:` names).

## Outcomes & retrospective

Not started. Complete at each milestone and at the end, including the
reconciliation of discoveries with the design document, ADRs, and roadmap.

## Context and orientation

Inspected base revision: `c3e6d76` on `main`. Line numbers below refer to that
revision; relocate symbols with `rg -n '<symbol>' src tests`.

The crate is a single package with a library (`src/lib.rs`) and a binary
(`src/bin/spycatcher_harness.rs`). Replay starts in `start_harness`
(`src/lib.rs`), which calls `server::start_replay_server`
(`src/server/runtime.rs`). That builds `ReplayAppState::from_config`
(`src/server/replay.rs`, 54 lines), the only place configuration reaches the
replay service: it creates a `ReplayMatchEngine` with `cfg.match_mode` and a
`ReplayService` with a `ReplayContext` (`src/replay_types.rs`, fields
`cassette` and `protocol`, documented as observability context).

The Axum handler `replay_chat_completions_handler`
(`src/server/replay_handler.rs`, 260 lines) calls
`ReplayService::handle_chat_completions` (`src/replay.rs`). Matching locks the
engine, peeks the next interaction, calls `response_from_recorded`, and commits
only if that succeeds; the `?` returns before `commit_match`. For
`RecordedResponse::Stream` it returns `ReplayBody::Events(events.clone())` and
ignores `raw_transcript` and `timing`. `build_replay_response` turns
`ReplayBody::OneShot` into a plain body and `ReplayBody::Events` into
`build_stream_body` (`src/server/replay_stream.rs`, 229 lines), which
serializes each event and either concatenates everything (total up to
`EAGER_STREAM_LIMIT_BYTES`, 64 KiB, which hyper sends with `content-length`) or
streams one chunk per event (chunked). When the body is a stream and no
`content-type` was recorded it inserts `text/event-stream`. `ReplayError` maps
to HTTP in `build_replay_error_response`; error bodies are
`{"error": {"kind", "message"}}`. `UnsupportedStream` exists but is unused,
carries `#[expect(dead_code, reason = "reserved for task 2.1.3 ...")]`, and
maps to 501 with message "streaming chat completions replay is not implemented
yet". Rejections pass through `log_rejection`, which feeds the `requests.total`
and `rejections.total` counters. `record_stream_delivery`
(`src/replay_observability.rs`) records the `delivery` label and an event count
histogram (`spycatcher.replay.stream.events`).

The cassette model is in `src/cassette/mod.rs`:
`RecordedResponse::Stream {
status, headers, events, raw_transcript: Vec<u8>, timing: Option<StreamTiming>
}`,
with `StreamEvent::{Comment { text }, Data { raw, parsed_json }}`. Record mode
(`src/server/record_stream/state.rs`, `observe_chunk`) appends every upstream
chunk to `raw_transcript` and persists only after `SseParser::finish` succeeds,
so recorded transcripts parse cleanly and are consistent with their events by
construction. `Cassette::write_to` pretty-prints, and `Cassette::from_reader`
reads without buffering.

The SSE parser `SseParser` (`src/sse.rs`, 375 lines, `pub(crate)`) handles LF,
CR, and CRLF, defers a trailing CR until the next chunk or `finish`, emits
comments immediately, joins `data` lines with a line feed, ignores `event`,
`id`, `retry`, and unknown fields, keeps no state across events, does not strip
a BOM, and returns `IncompleteEvent` from `finish` if bytes or data remain.

Configuration: `HarnessConfig` and `ReplayConfig` live in `src/config.rs` (290
lines). `ReplayConfig` holds `simulate_timing`, `ttft_ms`, and `tps`, none of
which replay reads yet; its doc comment and the `HarnessConfig.replay` field
doc say "timing controls". The `replay` subcommand's `ReplayArgs`
(`src/cli/mod.rs`, 343 lines) is a plain clap and serde struct merged by
`load_and_merge_subcommand`, which drops absent command-line values before
merging. It has `listen`, `cassette_dir`, `cassette_name`, `locale`,
`fallback_locale`, and `localization`; `to_replay_config`
(`src/cli/config_build.rs`) maps it onto `HarnessConfig::default()`. Files use
`[cmds.replay]`; environment variables use
`SPYCATCHER_HARNESS_CMDS_REPLAY_<FIELD>`. Option help is plain clap doc
comments, not Fluent messages, so no `.ftl` change is required, and the help
snapshots in `tests/snapshots/` cover root help only.

Tests: `rstest` 0.28, `rstest-bdd` 0.6, `proptest` 1.11, and `insta` 1.47 (with
`json`) are dev-dependencies; `googletest`, `pretty_assertions`, and any SSE
client are not. `make test` runs
`cargo nextest run --all-targets --all-features` and then doctests. Relevant
suites: `src/sse.rs` tests; `src/server/replay_stream_tests.rs` (337 lines,
with the private `StreamMetricRecorder`) and `replay_stream_properties.rs`;
`src/server/replay_handler_tests.rs` (error-body table at its end, snapshots in
`src/server/snapshots/`); `src/replay_tests/` (`mod.rs` is 399 lines and matches
`ReplayBody` exhaustively in
`concurrent_sequential_replay_consumes_duplicate_hashes_once_each`, as does
`concurrent_stream.rs`); `tests/chat_completions_replay_bdd.rs` with
`tests/features/chat_completions_replay.feature` and its stub upstream in
`tests/chat_completions_replay/stream_steps.rs`; the layering suites
`tests/harness_cli_layering_bdd.rs` and `tests/cli_layering/`; the subprocess
suite `tests/binary_cli_layering_e2e.rs`; and the contract-test precedent
`tests/build_standard_contract.rs`. Integration tests read streams with
`reqwest` `.bytes()`, so they never see chunk boundaries. The only Python is
`tests/test_make_all.py` (a `unittest` script run by `uv`); `interrogate`
requires docstrings on everything under `INTERROGATE_TARGETS` (`tests`).

There is no Verus or Kani set-up in this repository and no `.gitattributes`.
The estate precedent is `leynos/mdtablefix`: `verus/lib.rs` includes production
kernels with `#[path = "../src/..."]`; kernels gate ghost code on
`cfg(verus_keep_ghost)` through dual-expansion macros
(`src/verified_kernel_macros.rs`); `verus/smoke.rs` must fail
(`make verus-selftest`); mutation targets must fail with the expected predicate
named; and `.github/workflows/verus.yml` runs them on pull requests with a
`.verus/<version>` cache. That repository does not deny
`clippy::indexing_slicing`; this one does.

CI (`.github/workflows/ci.yml`) installs `uv` (cache disabled), runs
`make check-fmt`, `make lint`, `make msrv-check` (Rust 1.89.0,
`--all-targets`), and the tests through the shared coverage action rather than
`make test`.

External facts gathered for this plan (see `Verification plan`, axioms):

- WHATWG HTML "Server-sent events", "Parsing an event stream": lines end with
  CRLF, lone CR, or LF; a blank line dispatches; a line starting with `:` is
  ignored; `data` appends the value plus LF; at end of file an incomplete event
  is discarded; streams are UTF-8 decoded and one leading BOM is stripped; the
  last event ID persists across events.
- OpenAI Python SDK (`src/openai/_streaming.py`): `SSEDecoder` splits on CR,
  LF, or CRLF, ignores comment lines, dispatches on a blank line once any of
  event, data, ID, or retry is set (the ID is never cleared), and
  `Stream.__stream__` stops when `data` starts with `[DONE]`; it ignores
  `event:` names on chat streams except `error` and `thread.*`. Default timeout
  600 s and two retries on status 500 and above.
- `async-openai` 0.43 depends on `eventsource-stream` 0.2 (latest 0.2.3,
  2022); its nom parser accepts CRLF, CR, and LF, uses a streaming `\r\n` tag
  (so a final lone CR never completes), and does not deliver an unterminated
  final event.

## Conformance basis

Upstream artefacts, all at revision `c3e6d76`:

- `docs/roadmap.md`, item `2.1.3` and its three success criteria.
- `docs/spycatcher-harness-design.md`, "Cassette definition", "Streaming
  capture and replay", "Known risks and limitations" ("Streaming fidelity edge
  cases"), and "Implementation decisions".
- ADR 0001 and ADR 0002 (not affected).
- `AGENTS.md`, `Cargo.toml` lint tables, `clippy.toml`, and
  `docs/developers-guide.md` "Rust lint baseline" as governing standards.
- No Terms of Reference document exists for this project.

Requirements:

- `REQ-MODE-CONFIG`: replay supports parsed-event and raw-transcript stream
  modes selected by configuration (roadmap `2.1.3` criterion 1).
- `REQ-RAW-BOUNDARIES`: raw-transcript mode preserves the event boundaries SSE
  clients need: every client sees the same dispatch points and field values
  through replay as from the recorded upstream (criterion 2).
- `REQ-RAW-FRAMING`: the chunks the harness writes in raw mode end only at
  dispatch boundaries (or at the end of the transcript) and never split a CRLF
  pair (design choice D3, supporting criterion 2).
- `REQ-CLIENT-COMPAT`: integration tests validate compatibility with
  representative streaming clients (criterion 3; design "Known risks").
- `REQ-RAW-VERBATIM`: raw mode replays recorded bytes verbatim (design "Known
  risks": "replay it verbatim as an option").
- `REQ-PARSED-PRESERVED`: parsed-event replay semantics in the design are
  unchanged and remain the default.
- `REQ-NO-SCHEMA-CHANGE`: cassette `format_version` 1 and the stream variant
  are unchanged.
- `REQ-EXPLICIT-FAILURE`: unusable raw data is reported, not silently
  replaced (`AGENTS.md`, "Architecture and operational boundaries").
- `REQ-OBSERVABLE`: raw-mode replay produces distinguishable, low-cardinality
  telemetry through the real collection path (`AGENTS.md`, "Observability").

Gaps observed at the base revision:

- `GAP-RAW-UNREAD`: nothing on the replay side reads `raw_transcript`.
- `GAP-NO-LAYERED-REPLAY-OPTION`: no replay option is reachable from CLI,
  environment, or file.
- `GAP-NO-CLIENT-TESTS`: no test consumes a replayed stream with an SSE
  client or observes chunk framing.
- `GAP-NO-PROOF-TOOLING`: no Verus pins, targets, workflow, or contract tests.
- `GAP-RESERVED-ERROR`: `ReplayError::UnsupportedStream` is reserved, its
  message is stale, and its snapshot is orphaned.
- `GAP-DOCS-DEFERRAL`: the design document and users' guide say byte-faithful
  replay "remains roadmap task `2.1.3`".

Trace links:

```plaintext
ROADMAP-2.1.3-SC1 -> REQ-MODE-CONFIG -> EP-M1, EP-M2
  -> replay_tests::stream_modes, cli_layering::stream_mode, harness_cli_layering_bdd, binary_stream_mode_e2e
ROADMAP-2.1.3-SC2 -> REQ-RAW-BOUNDARIES -> EP-M1, EP-M3
  -> sse_frames alignment properties, sse_client_compat (transparent and content checks)
ROADMAP-2.1.3-SC2 -> REQ-RAW-FRAMING -> EP-M1, EP-M3, EP-M4
  -> sse_frames exhaustive and properties, replay_transcript_tests, raw_transcript_wire_framing, verus/lib.rs
ROADMAP-2.1.3-SC2 -> REQ-RAW-VERBATIM -> EP-M1 -> replay_transcript_tests, stream_replay_modes_bdd
ROADMAP-2.1.3-SC3 -> REQ-CLIENT-COMPAT -> EP-M3 -> sse_client_compat, sse_python_client (make test-sse-clients)
DESIGN-STREAMING -> REQ-PARSED-PRESERVED -> EP-M1 -> existing replay_stream tests unchanged
DESIGN-SCHEMA -> REQ-NO-SCHEMA-CHANGE -> all milestones -> cassette tests unchanged
AGENTS-ERRORS -> REQ-EXPLICIT-FAILURE -> EP-M1 -> INV-EMPTY-TRANSCRIPT tests, start-up consistency tests
AGENTS-OBSERVABILITY -> REQ-OBSERVABLE -> EP-M1 -> metric recorder tests
GAP-NO-PROOF-TOOLING -> EP-M4 -> make verus, make verus-selftest, make verus-mutation, verus_tooling_contract
GAP-DOCS-DEFERRAL -> EP-M5 -> design, users' guide, ADR 0003, ADR 0004
```

## Verification plan

### Abstract model

Let `s` be a finite byte sequence of length `n` (the recorded
`raw_transcript`). Bytes are indexed `s[0]` to `s[n-1]`. Write `LF` for 0x0A and
`CR` for 0x0D. All predicates below are defined only for positions with
`1 <= p <= n`.

A position `p` is an end-of-line end (`eol_end(s, p)`) when either
`s[p-1] == LF`, or `s[p-1] == CR` and (`p == n` or `s[p] != LF`). This is the
WHATWG `end-of-line = cr lf / cr / lf` rule with the greedy reading: a CR
immediately followed by LF belongs to one CRLF terminator.

For an end-of-line end `p`, its start `eol_start(s, p)` is `p - 2` when
`s[p-1] == LF`, `p >= 2`, and `s[p-2] == CR`; otherwise it is `p - 1`.

A dispatch end (`dispatch_end(s, p)`) is an end-of-line end whose line is empty:
`eol_start(s, p) == 0`, or `eol_end(s, eol_start(s, p))`. It is the position
just after the blank line that makes a WHATWG parser dispatch. Let `D(s)` be
the set of dispatch ends of `s`.

The kernel returns a sequence `ends`. Let `len = ends.len()`, `prev(0) = 0`, and
`prev(k) = ends[k-1]`. The frames are `s[prev(k)..ends[k]]`.

Preconditions: none. Every byte sequence, including the empty sequence, invalid
UTF-8, and transcripts with an incomplete final event, is admissible. Malformed
input is not rejected; byte faithfulness requires replaying it as recorded, and
its final frame is the remainder.

Postconditions, each a named `spec fn`, for all `s`:

1. `frames_ordered`: for all `k < len`, `prev(k) < ends[k] <= n`.
2. `frames_terminated`: `n == 0` if and only if `len == 0`, and if `n > 0`
   then `ends[len-1] == n`.
3. `frames_sound`: for all `k < len - 1`, `dispatch_end(s, ends[k])`.
4. `frames_gap_free`: for all `k < len` and all `p` with
   `prev(k) < p < ends[k]`, not `dispatch_end(s, p)`.

Known deviation from WHATWG: the model does not strip a leading BOM, matching
`src/sse.rs`. For a transcript starting with a BOM followed by a blank line,
one WHATWG dispatch point (a no-op, because the data buffer is empty) is not a
frame end. Soundness with respect to WHATWG is unaffected.

Representation relationship: the production kernel
`crate::sse_frames::event_frame_ends(&[u8]) -> Vec<usize>` is the verified
function itself, included into `verus/lib.rs` by `#[path]`; its slice and `Vec`
are viewed through `vstd` as `Seq<u8>` and `Seq<usize>`. The adapter
(`src/server/replay_transcript.rs`) maps `ends` to `Bytes::slice` ranges; that
step is tested, not proved.

### Invariants and lemmas

- `INV-FRAME-ORDERED`, `INV-FRAME-TERMINATED`, `INV-FRAME-SOUND`,
  `INV-FRAME-GAP-FREE`: postconditions 1 to 4 (`REQ-RAW-FRAMING`).
- `L-FRAME-COMPLETE`: every `p` in `D(s)` is an element of `ends`. Derived
  from 1 to 4.
- `L-FRAME-UNIQUE`: postconditions 1 to 4 determine `ends` uniquely as the
  sorted elements of `D(s)` together with `n` when `n > 0`, and as the empty
  sequence when `n == 0`. This licenses exact-equality oracles and makes
  exec-level witnesses meaningful.
- `L-FRAME-LINE-ALIGNED`: every frame starts at 0 or at an end-of-line end,
  and the last line of every non-final frame is empty. Consequently no frame
  boundary splits a CRLF pair or a multi-byte UTF-8 sequence (whose bytes are
  never CR or LF). The CRLF part is a direct corollary of soundness and the
  definition of `eol_end`, and is documented rather than counted as separate
  evidence.
- `L-MODEL-SEQUENTIAL`: `dispatch_end` agrees with a sequential
  transliteration of the WHATWG "collect a line, dispatch on empty line"
  procedure over bytes. This makes the specification faithful, not merely
  self-consistent.
- `L-FRAME-ALIGNMENT` (refinement to the repository parser): for every
  non-final frame, a fresh `SseParser` fed the frame and then `finish`ed
  succeeds; the concatenation of per-frame events equals the events of the
  whole transcript; for the final frame, `finish` succeeds if and only if it
  succeeds for the whole transcript. A single parser fed frame by frame carries
  no pending event across a frame end, except one deferred dispatch for a frame
  ending in a lone CR. Fresh-parser-per-frame is valid only because `SseParser`
  keeps no state across events; WHATWG parsers keep the last event ID, so this
  lemma is specific to the repository parser.
- `L-CLIENT-TRANSPARENT`: for each representative client and fixture, the
  client's view of a raw-mode replay equals its view of the recorded upstream
  (`REQ-RAW-BOUNDARIES`, `REQ-CLIENT-COMPAT`).
- `INV-RAW-VERBATIM`: in raw mode the HTTP body bytes equal `raw_transcript`
  exactly, for empty, small, larger-than-64-KiB, and pathological (1 MiB of
  line feeds) transcripts (`REQ-RAW-VERBATIM`).
- `INV-PARSED-UNCHANGED`: in parsed mode the body bytes equal the pre-change
  serialization (`REQ-PARSED-PRESERVED`).
- `INV-MODE-SELECT`: the configured mode, from library, file, environment, or
  CLI with precedence CLI over environment over file over default, determines
  the body source; the default is `parsed_events` (`REQ-MODE-CONFIG`).
- `INV-SHARED-ENVELOPE`: status, recorded headers, and the
  `text/event-stream` default are identical in both modes. Framing headers
  (`content-length`, `transfer-encoding`) are excluded: raw mode is always
  chunked.
- `INV-EMPTY-TRANSCRIPT`: in raw mode, empty transcript with non-empty events
  yields 501 `unsupported_stream` and leaves the cursor unchanged (a second
  identical request also yields 501); empty transcript with no events yields
  200 with an empty body (`REQ-EXPLICIT-FAILURE`).
- `INV-STARTUP-CONSISTENCY`: in raw mode, start-up logs one warning per
  inconsistent stream interaction, at most 10, each with the interaction index
  and one fixed reason category, plus one summary; in parsed mode it logs
  nothing new; start-up never fails because of it.
- `INV-RAW-TELEMETRY`: a raw-mode replay increments the stream delivery
  counter with `delivery="raw_framed"` and records the recorded event count; an
  `unsupported_stream` rejection increments `rejections.total` with that
  outcome; no transcript bytes appear in logs or spans (`REQ-OBSERVABLE`).

### Axioms (trusted, not verified here)

- `AXIOM-WHATWG-EOL`: conformant SSE parsers split lines on CRLF, lone CR, and
  LF, with CR followed by LF read as one terminator, and dispatch on a blank
  line (WHATWG HTML, "Parsing an event stream"). Checked against the model by
  `L-MODEL-SEQUENTIAL` and against `src/sse.rs` by `L-FRAME-ALIGNMENT`.
- `AXIOM-UTF8-ASCII-TRANSPARENT`: the WHATWG UTF-8 decoder never consumes
  0x0A or 0x0D as part of an invalid sequence, so byte-level line ends equal
  code-point-level line ends. This justifies "Preconditions: none".
- `AXIOM-CLIENT-DECODERS`: `eventsource-stream` 0.2.3 and the pinned OpenAI
  Python SDK behave as their source shows. Their internals are not verified;
  each is checked for chunk invariance per fixture before being used as an
  oracle, and any disagreement is recorded here.
- `AXIOM-HYPER-BODY`: hyper encodes each non-empty body frame the harness
  yields as one HTTP/1.1 chunk, in order. Client libraries may merge or split
  chunks when reading. `EP-M0` probes this, and the wire-level test relies on
  it.
- `AXIOM-RECORDED-IDENTITY`: recorded transcripts are identity-encoded because
  the recorder strips `accept-encoding` from upstream requests.
- `AXIOM-VERUS`: the pinned Verus release and its `vstd` specifications of
  slices and `Vec` are sound.

### Methods per obligation

- Obligation: `INV-FRAME-*`, `L-FRAME-COMPLETE`, `L-FRAME-UNIQUE`,
  `L-FRAME-LINE-ALIGNED`, `L-MODEL-SEQUENTIAL` (executable evidence).
  - Method: exhaustive small-scope enumeration, a byte-class invariance
    property, a long-input property, and named `rstest` examples, all in
    `make test`.
  - Rationale: `dispatch_end(s, p)` depends only on `s[p-3..=p]`, and the
    kernel treats every byte other than CR and LF alike, so enumerating every
    string over `{CR, LF, 'a'}` up to length 10 (about 88,000 inputs, well
    under a second) exercises every local pattern; the byte-class property
    justifies the three-letter alphabet; the long-input property samples
    scanner state far from the start.
  - Domain: all strings over `{CR, LF, 'a'}` of length 0 to 10 (exhaustive);
    arbitrary bytes up to 4 KiB (property); named cases listed in `EP-M1`.
  - Artefacts: `src/sse_frames_tests.rs` (named cases and exhaustive loop),
    `src/sse_frames_properties.rs` (properties), and the two test-only
    oracles in `src/sse_frames_oracles.rs`: `local_oracle`, which computes
    the sorted `D(s)` plus `n` directly from the definitions (quadratic), and
    `sequential_oracle`, a line-by-line transliteration of the WHATWG
    procedure written without reference to `eol_start`.
  - Evidence: `cargo nextest run sse_frames` passes; the exhaustive test
    asserts three-way exact equality of kernel, `local_oracle`, and
    `sequential_oracle`, and asserts its own case count.
  - Non-vacuity: exact equality against `sorted(D(s) ∪ {n})` is licensed by
    `L-FRAME-UNIQUE`; the two oracles share no code with the kernel or each
    other, so a specification error must also fool the sequential
    transliteration. Permanent negative controls: a test-only function
    `frame_violation(s, ends) -> Option<String>` is checked against in-test
    mutant kernels (`every_line_end`, `crlf_split`, `missing_final_end`) and
    must return `Some` with the expected predicate name on named inputs.
    Each equivalence class (empty, LF, CRLF, lone CR, mixed, incomplete final
    event, leading blank line, BOM) is guaranteed by a named case rather than
    by measured generator rates.
- Obligation: `L-FRAME-ALIGNMENT`.
  - Method: `proptest` over structurally generated transcripts plus named
    examples.
  - Domain: each transcript picks one terminator style (LF, CRLF, CR, or
    mixed) and a sequence of events, each a sequence of lines (`data`,
    comment, `event`, `id`, `retry`, unknown field) ended by a blank line,
    optionally followed by one trailing line that is a comment, `event:`, or
    `id:`. Generation rules: a blank line terminated by LF never follows a
    CR-terminated line (that would fuse into one CRLF); the final line of the
    transcript is terminated by LF or CRLF only, because `SseParser::finish`
    reports a CR-terminated trailing non-blank line as incomplete.
  - Artefact: `src/sse_frames_properties.rs`; regressions in
    `src/sse_frames_properties.proptest-regressions`.
  - Evidence: `cargo nextest run sse_frames` passes. Each terminator style is
    a separate strategy branch, so every class is generated by construction;
    multi-frame cases are guaranteed by a minimum event count of two in one
    branch.
  - Non-vacuity: the whole-transcript `finish` must succeed for every
    generated case (asserted, not assumed); the streaming variant asserts the
    exact deferred-dispatch count for CR-terminated frames; the in-test mutant
    `every_line_end` must make the property fail on a two-line event (a
    permanent named test).
- Obligation: `INV-FRAME-*`, `L-FRAME-COMPLETE`, `L-FRAME-UNIQUE` for all
  lengths.
  - Method: Verus deductive proof of the production kernel (`EP-M4`).
  - Rationale: removes the residual risk of length-dependent scanner bugs that
    small-scope enumeration cannot exclude.
  - Artefacts: `verus/lib.rs`, `verus/sse_frames_spec.rs`,
    `verus/sse_frames_witnesses.rs`, `verus/smoke.rs`, `src/sse_frames_macros.rs`,
    `scripts/verus_frame_mutation.py` with `scripts/tests/`, and
    `tests/verus_tooling_contract.rs`.
  - Evidence: `make verus` prints a `verification results::` line with zero
    errors and a verified count that includes `event_frame_ends`;
    `make verus-selftest` passes because `verus/smoke.rs` fails verification
    and the output shows the verifier ran; `make verus-mutation` passes because
    each of three mutants fails, naming its predicate.
  - Non-vacuity: the proof has no `requires`, so every input is in scope; the
    loop has a `decreases` clause; exec-level witnesses call
    `event_frame_ends` and prove exact results (for example `\n\n` gives
    `[1, 2]`, which would not follow without `frames_gap_free`; `\r\r\n` gives
    `[1, 3]`; `data: a\r\n\r\n` gives `[11]`); the three mutants are "drop
    the CRLF look-ahead" (must fail `frames_sound`), "push only after
    LF-terminated blank lines" (must fail `frames_gap_free`), and "drop the
    final push of `n`" (must fail `frames_terminated`); each is matched on the
    predicate name in either an invariant or an `ensures` failure, and the
    gate also requires a `verification results::` line so a compile error
    cannot pass it.
- Obligation: `INV-RAW-VERBATIM`, `INV-PARSED-UNCHANGED`,
  `INV-SHARED-ENVELOPE`, `INV-EMPTY-TRANSCRIPT`, `INV-RAW-TELEMETRY`.
  - Method: `rstest` tables with `googletest` matchers at domain and adapter
    level; a `proptest` that adapter frames concatenate to the input and are
    each non-empty and equal to the kernel's slices; an `insta` snapshot
    contrasting both modes for one non-canonical cassette, recording each
    frame as an `escape_ascii()` string, built at the response-builder level so
    no `Date` header appears; metric assertions through the shared
    `StreamMetricRecorder`.
  - Artefacts: `src/replay_tests/stream_modes.rs`,
    `src/server/replay_transcript_tests.rs`,
    `src/server/replay_handler_tests.rs`, `src/server/test_metrics.rs`, and
    `src/server/snapshots/*stream_mode_contrast*.snap`.
  - Non-vacuity: transcripts are non-canonical, so a mode switch that
    accidentally serializes events fails the byte comparison; the 70 KiB case
    exercises the size the eager path would otherwise take; the cursor test
    sends a second identical request and asserts 501 again; metric tests
    assert non-zero values.
- Obligation: `INV-STARTUP-CONSISTENCY`.
  - Method: `rstest` table over cassettes containing each reason category and
    a consistent cassette, using `tracing-test` to capture logs.
  - Non-vacuity: the consistent cassette must log no warning and the
    eleven-inconsistency cassette must log exactly ten warnings and one
    summary.
- Obligation: `INV-MODE-SELECT`.
  - Method: `rstest` precedence table through `load_with_sources`; a
    behaviour scenario in `tests/features/harness_cli_layering.feature`; and
    subprocess tests of the binary with `Command::env`.
  - Domain: default; file only; and the pairwise cases file raw with
    environment parsed, environment raw with CLI parsed, and file raw with CLI
    parsed (each expecting the higher layer's value); CLI spelling
    `--stream-mode raw_transcript`; invalid value `verbatim` from file,
    environment, and CLI.
  - Non-vacuity: in every pairwise row the higher layer sets the opposite
    value, so any wrong ordering selects a different mode; the invalid value
    must produce a non-zero exit whose stderr names `stream_mode`.
- Obligation: `L-CLIENT-TRANSPARENT` and `REQ-RAW-FRAMING` on the wire.
  - Method: integration tests, detailed in `EP-M3`, with the per-fixture,
    per-client expectations table; a wire-level test that sends a handwritten
    HTTP/1.1 request over a `TcpStream`, decodes `Transfer-Encoding: chunked`
    in a small helper, and asserts the chunk payloads equal literal expected
    frames.
  - Non-vacuity: every fixture is also replayed in parsed mode, and the
    expectations table names at least one client-visible difference per client
    (for `eventsource-stream`, the `event` field; for the Python SDK, the
    `[DONE]`-only and quirk fixtures); `eventsource-stream` is first checked
    for chunk invariance per fixture (one chunk equals single-byte chunks).

### Formal verification decision

Verus is warranted for the frame kernel only (D7). It is the single new piece
of contractual logic: its postconditions must hold for transcripts of any
length, and a wrong boundary silently changes every client's view of a stream.
The kernel is small, pure, and stable. The exhaustive small-scope check is
primary evidence because it is cheap and runs in `make test`; the proof removes
the remaining length-dependent risk and guards later edits by task `3.1.1`.

Verus is deliberately not used for mode selection, configuration precedence,
header handling, the 501 rule, the start-up consistency check, or the Axum body
adapter. These are finite control flow or third-party plumbing, where named
tests and tables give complete coverage and a proof would only restate the
code. Kani is not used: it would bound the transcript length, which the
exhaustive check already covers, and it would add tooling of similar cost.
`L-FRAME-ALIGNMENT` stays a property test because `SseParser` uses `String`,
`serde_json`, and `Result` plumbing outside the Verus subset; modelling it in
Verus would create a second parser to keep in step.

This deviates from the `verus` skill's on-ramp advice that a prover arrives
only after other tools: this repository has no prover yet, and the proof gates
pull requests from its first day. ADR 0004 records the deviation. Running the
Verus workflow on every pull request without a path filter also serves as the
check that the `#[path]` include still resolves.

The proof plan is a sketch. The likely shape is a loop invariant over the scan
position `i`, the start of the current line, and the last pushed end (tracked
in a local rather than read back from the `Vec`): every dispatch end at or
before the current line start has been pushed, every pushed value is a dispatch
end, and no dispatch end lies strictly between consecutive pushed values. A
small lemma that a position strictly inside a non-empty line is not a dispatch
end is likely. Implementation may refine or split these. `L-FRAME-COMPLETE` and
`L-FRAME-UNIQUE` are proved from the gap form as separate `proof fn`s with
`///` comments. `L-MODEL-SEQUENTIAL` stays a test unless a spec-level proof is
cheap. Auxiliary sequence lemmas, triggers, and `broadcast use` directives are
deferred until `EP-M4` exposes the actual verification conditions.

### Suggested BDD features

Proposed path: `tests/features/stream_replay_modes.feature`, run by
`tests/stream_replay_modes_bdd.rs` with steps in `tests/stream_replay_modes/`.
Each scenario creates its own cassette.

```gherkin
Feature: Stream replay modes
  Replay can re-emit parsed stream events or the recorded upstream bytes.

  Scenario: Raw-transcript mode replays the recorded bytes exactly
    Given a stream cassette whose raw transcript uses CRLF line endings and an event field
    And the replay harness uses the "raw_transcript" stream mode
    When a client requests the streamed chat completion
    Then the response status is 200
    And the response content type is "text/event-stream"
    And the response body equals the recorded raw transcript

  Scenario: Parsed-events mode remains the default
    Given a stream cassette whose raw transcript uses CRLF line endings and an event field
    And the replay harness uses the default stream mode
    When a client requests the streamed chat completion
    Then the response body equals the canonical serialization of the recorded events

  Scenario: An SSE client sees the recorded events through raw-transcript replay
    Given a stream cassette whose raw transcript uses CRLF line endings and an event field
    And the replay harness uses the "raw_transcript" stream mode
    When an SSE client reads the streamed chat completion
    Then the client sees the same events as it sees in the recorded raw transcript

  Scenario: Raw-transcript mode rejects a stream recording without a transcript
    Given a stream cassette whose raw transcript is empty but whose events are not
    And the replay harness uses the "raw_transcript" stream mode
    When a client requests the streamed chat completion twice
    Then both responses have status 501
    And both responses have error kind "unsupported_stream"
```

Addition to `tests/features/harness_cli_layering.feature` (one scenario per
pairwise row, shown for one):

```gherkin
  Scenario: Command-line stream mode overrides the environment
    Given environment sets replay stream_mode to "raw_transcript"
    And the command line sets replay stream_mode to "parsed_events"
    When the layered command configuration is loaded
    Then replay stream_mode is "parsed_events"
```

Runners: `cargo nextest run --test stream_replay_modes_bdd` and
`cargo nextest run --test harness_cli_layering_bdd`. The orphaned step "the
replay client receives an unsupported streaming response" in
`tests/chat_completions_replay/steps.rs` is reused or moved, not duplicated.

## Plan of work

### `EP-M0` feasibility probe (prototype, deleted)

Work in a scratch worktree (`git worktree add ../sh-2-1-3-probe`), never on the
plan branch. Record transcripts in `Artefacts and notes`, then delete the
worktree.

Questions:

1. Verus bridge: with Verus installed by `prover-tools verus install`, does a
   draft of the real kernel (`&[u8]` in, `Vec<usize>` out, direct indexing,
   `while` loop with `decreases`, `Vec::push`, last end tracked in a local),
   wrapped in a trimmed one-arm dual-expansion macro that carries
   `#[expect(clippy::indexing_slicing, reason = "...")]`, verify a trivial
   `ensures` under Verus and pass the full `make lint` (rustdoc, clippy,
   Whitaker, interrogate) and `make msrv-check` under Cargo? Does
   `cfg_attr(verus_keep_ghost, verus_spec(...))` work as a simpler alternative?
   Does `make verus`, run from the repository root, ignore the nightly
   `rust-toolchain.toml` and the `-Z` flags in `.cargo/config.toml`?
2. Does `reqwest` 0.13 `Response::chunk()` work without the `stream` feature,
   and does `eventsource-stream` 0.2.3 decode a stream built from those chunks?
   Confirm its behaviour on a final lone-CR blank line and an unterminated
   final event.
3. Do `googletest`, `pretty_assertions`, and `eventsource-stream` add at most
   12 lockfile packages, and does `make msrv-check` pass with them?
4. Does hyper, behind Axum, emit exactly one HTTP/1.1 chunk per non-empty body
   frame from `Body::from_stream` (`AXIOM-HYPER-BODY`)?
5. How large is a cassette holding one 1 MiB transcript, and how long does
   `start_harness` take to load it?
6. Does the pinned OpenAI Python SDK, under `uv run --script` with
   `exclude-newer`, stream from a local server with `base_url` ending in `/v1`,
   `api_key="test"`, `timeout=10`, `max_retries=0`, and
   `http_client=DefaultHttpxClient(trust_env=False)`?

Go: questions 2 to 4 and 6 answered yes and question 3 within tolerance. No-go
on 1 moves `EP-M4` to the proof fallback in `Tolerances` (presented to the
user) and does not block `EP-M1` to `EP-M3`; the kernel then keeps the
macro-free shape that passes lint. No-go on 2, 3, or 6 triggers the dependency
escalation. No-go on 4 replaces the wire-level test with server-side frame
assertions only and is recorded as a decision. Question 5 only feeds ADR 0003.

### `EP-M1` raw-transcript replay in the library, with the frame kernel

Requirements: `REQ-MODE-CONFIG` (library), `REQ-RAW-VERBATIM`,
`REQ-RAW-FRAMING`, `REQ-RAW-BOUNDARIES` (parser refinement),
`REQ-PARSED-PRESERVED`, `REQ-EXPLICIT-FAILURE`, `REQ-OBSERVABLE`,
`GAP-RAW-UNREAD`, `GAP-RESERVED-ERROR`.

Preparatory refactor (its own commit, behaviour unchanged): move
`concurrent_sequential_replay_consumes_duplicate_hashes_once_each` from
`src/replay_tests/mod.rs` into a sibling module, and move
`StreamMetricRecorder` from `src/server/replay_stream_tests.rs` into a shared
`#[cfg(test)]` module `src/server/test_metrics.rs`. Run the affected tests
before and after.

Add `googletest` and `pretty_assertions` to `[dev-dependencies]`.

Red, kernel: add `src/sse_frames_tests.rs` with an `rstest` table of named
cases asserting the exact `ends` vector: empty input; one LF event; one CRLF
event; CR-only events; `\r\r\n` (`[1, 3]`); `\n` (`[1]`); `\n\r` (`[1, 2]`);
`a\n\r\n` (`[4]`); `\n\n` (`[1, 2]`); a leading blank line; two events; a
trailing comment without a blank line; an incomplete final event (`data: a\r`
gives `[8]`); CR at the very end (`data: a\n\r` gives `[9]`); invalid UTF-8
bytes; the BOM case `b"\xEF\xBB\xBF\n\ndata: a\n\n"` (`[5, 14]`). Add the
exhaustive test, the oracles, the properties, and the permanent negative
controls described in `Verification plan`. Add `mod sse_frames;` with a stub
returning an empty vector; run and observe the failures.

Green, kernel: implement `event_frame_ends` as a single left-to-right scan that
tracks the start of the current line and the last pushed end, recognizes CRLF
before lone CR, pushes the end of every empty line's terminator, and finally
pushes `n` if it is not already last. Mark it `#[must_use]`. Keep it within the
complexity limits; extract a small `eol_end_at` helper if needed. If `EP-M0`
question 1 succeeded, use the macro shape it validated; otherwise use the plain
shape that passes lint.

Red, domain: in a new `src/replay_tests/stream_modes.rs`, add tests that a
`ReplayService` with `StreamReplayMode::RawTranscript` returns
`ReplayBody::Transcript` carrying the recorded bytes and event count; that the
default returns `ReplayBody::Events`; both `INV-EMPTY-TRANSCRIPT` cases,
including that a second identical request is also rejected; and that raw mode
does not consult `events` (a cassette whose `events` differ from its transcript
still replays the transcript).

Red, start-up: add `src/replay_consistency_tests.rs` for
`INV-STARTUP-CONSISTENCY`.

Red, adapter: add `src/server/replay_transcript_tests.rs` asserting, by draining
`Body::into_data_stream()`, that frames equal the kernel's slices and
concatenate to the input for empty, small, 70 KiB, and 1 MiB-of-line-feeds
inputs, and that delivery is recorded as `raw_framed` with the event count.
Extend `src/server/replay_handler_tests.rs` with the shared-envelope table, the
`unsupported_stream` case in the error-body table (accepting the new message in
its snapshot deliberately), and the `stream_mode_contrast` snapshot.

Green, library:

1. `src/config.rs`: add the `#[non_exhaustive]` enum `StreamReplayMode`
   with variants `ParsedEvents` (the `#[default]`) and `RawTranscript`, deriving
   `Debug, Clone, Copy, Default, PartialEq, Eq`, with a doctest; add
   `pub stream_mode: StreamReplayMode` to `ReplayConfig` and its `Default`;
   update the `ReplayConfig` and `HarnessConfig.replay` doc comments, which now
   cover more than timing.
2. `src/replay_types.rs`: add `ReplayBody::Transcript(RawTranscriptBody)` with
   `pub(crate) struct RawTranscriptBody { bytes: Vec<u8>, event_count: usize }`;
   remove the `expect` and `FIXME` from `UnsupportedStream` and rewrite its
   doc comment.
3. `src/replay.rs`: add a `stream_mode` field to `ReplayService` and its
   constructor; in the `RecordedResponse::Stream` arm, choose the body by mode
   through a small helper; in raw mode, return `UnsupportedStream` (after
   `log_rejection` with reason `unsupported_stream`) for the empty-transcript
   case; otherwise clone only `raw_transcript`. Add `body_source` and
   `transcript_len` to the existing debug event. Update the exhaustive
   `ReplayBody` matches in `src/replay_tests/`.
4. `src/replay_consistency.rs`:
   `check_raw_transcripts(cassette) -> ConsistencyReport` (pure; parses each
   stream interaction's transcript with `SseParser`) and a logging function
   that emits the bounded warnings.
5. `src/server/replay.rs`: pass `cfg.replay.stream_mode` into the service; in
   raw mode run the consistency check and log its report; add `stream_mode` to
   the "cassette loaded" log line.
6. `src/sse_frames.rs`: the kernel, with its abstract model in the module
   comment.
7. `src/server/replay_transcript.rs`: `build_transcript_body` (signature in
   `Interfaces and dependencies`), which converts the bytes to `Bytes` once (no
   copy), computes `event_frame_ends` outside any lock, and streams
   `bytes.slice(start..end)` per frame with `futures_util::stream::iter`,
   inside a `replay_transcript_render` span with `byte_len`, `frame_count`, and
   `final_frame_is_dispatch` fields and no transcript content.
8. `src/replay_observability.rs`: add `StreamDeliveryMode::RawFramed`.
9. `src/server/replay_handler.rs`: route `ReplayBody::Transcript` to
   `build_transcript_body`, treat it as a stream for the content-type default,
   and change the `unsupported_stream` message (D16).

Refactor: confirm no file exceeds 400 lines and rerun the existing stream
suites unchanged.

### `EP-M2` configuration layering and binary wiring

Requirements: `REQ-MODE-CONFIG`, `GAP-NO-LAYERED-REPLAY-OPTION`.

Red: add `tests/cli_layering/stream_mode.rs` with the `rstest` precedence table
and invalid-value cases from `Verification plan`, using `load_with_sources`;
add the pairwise layering scenarios to
`tests/features/harness_cli_layering.feature` with their steps; add
`tests/binary_stream_mode_e2e.rs`, following
`tests/binary_cli_layering_e2e.rs`, which runs the binary with
`Command::env_clear` plus forwarded ambient values: with
`SPYCATCHER_HARNESS_CMDS_REPLAY_STREAM_MODE=verbatim` it must exit non-zero
with stderr naming `stream_mode`; with `raw_transcript` and a valid temporary
cassette it must start and shut down successfully; with
`--stream-mode verbatim` it must fail with a clap error.

Green: add a private `StreamModeArg` enum beside `RecordUpstreamKind` in
`src/cli_args.rs` with `serde` `rename_all = "snake_case"`, clap `ValueEnum`
with `#[value(rename_all = "snake_case")]`, and `From` into
`config::StreamReplayMode`; add
`#[arg(long, value_enum)] stream_mode: Option<StreamModeArg>` (no
`default_value`) to `ReplayArgs`; map `merged_args.stream_mode` in
`to_replay_config` directly, not through the shared overrides macro. If
`src/cli/mod.rs` would pass 400 lines, move `ReplayArgs` to
`src/cli/replay_args.rs`. Confirm the help snapshots are unchanged.

### `EP-M3` representative streaming client compatibility

Requirements: `REQ-CLIENT-COMPAT`, `REQ-RAW-BOUNDARIES`, `REQ-RAW-FRAMING`
(wire), `GAP-NO-CLIENT-TESTS`.

Add `eventsource-stream = "0.2.3"` to `[dev-dependencies]`. Add
`tests/fixtures/sse_transcripts/*.sse -text` to a new `.gitattributes` so Git
never normalizes the carriage returns. Add fixtures with a short `README.md`
describing each one:

- `openai_lf.sse`: OpenAI-style LF stream with a usage-including final chunk
  and `data: [DONE]`.
- `openrouter_comments.sse`: `: OPENROUTER PROCESSING` comments and a
  mid-stream error chunk.
- `crlf_event_field.sse`: CRLF endings, `event: chunk`, and `data:` without a
  space.
- `cr_only.sse`: lone-CR endings throughout.
- `multiline_interleaved_comment.sse`: two `data` lines with a comment
  between them.
- `sticky_id.sse`: an `id:` line followed later by a comment block.
- `bom.sse`: a leading BOM.
- `unicode_separators.sse`: JSON strings containing U+2028 and U+0085.
- `large.sse`: generated at test time to exceed 64 KiB.

Add `tests/sse_client_compat.rs`. For each fixture it starts a raw-mode and a
parsed-mode harness in-process with `start_harness` on `127.0.0.1:0`, reads
each response with `reqwest` `Response::chunk()` under
`tokio::time::timeout(Duration::from_secs(10), ...)`, feeds the chunks to
`eventsource_stream::Eventsource::eventsource()`, and checks the three parts of
D11 against an expectations table in the test source, for example:

```plaintext
fixture                         eventsource-stream (raw)        OpenAI Python SDK (raw)
openai_lf                       same as upstream; content ok    same as upstream; content ok
crlf_event_field                same; event field "chunk"       same; event name ignored
cr_only                         same; final event lost          same; content ok
sticky_id                       same; content ok                same; JSONDecodeError in both
```

The final table is filled in from `EP-M0` evidence and kept in the test file
and the testing guide. Before using `eventsource-stream` as an oracle for a
fixture, assert that feeding the fixture as one chunk and as single-byte chunks
gives the same events; a disagreement is recorded as an axiom.

Add `tests/raw_transcript_wire_framing.rs`: it starts a raw-mode harness,
writes a literal HTTP/1.1 request over `tokio::net::TcpStream`, decodes the
chunked body with a small helper, and asserts the chunk payloads equal literal
expected frames for `crlf_event_field.sse` and
`multiline_interleaved_comment.sse` (offsets written out by hand, not computed
by the kernel).

Add the `stream_replay_modes` feature, its entry point, and steps.

Add the Python check (D12, D13):

- `tests/sse_clients/openai_python_stream.py`: a `unittest` script in the style
  of `tests/test_make_all.py`, with a PEP 723 header
  (`requires-python = ">=3.13"`, `openai` pinned exactly,
  `[tool.uv] exclude-newer` set to the plan approval date), docstrings on every
  definition, and a command-line interface taking `--base-url` and `--output`.
  It builds
  `OpenAI(base_url=…, api_key="test", timeout=10, max_retries=0,
  http_client=DefaultHttpxClient(trust_env=False))`,
  streams one chat completion, and writes the observed chunks (or the
  exception class) as JSON. On an HTTP error it prints the harness's error body.
- `tests/sse_python_client.rs`:
  `#[ignore = "needs uv and network; run by make test-sse-clients"]` tests
  that, per fixture, start a stub upstream serving the fixture bytes, start a
  record-mode harness pointing at it, run the script through it, start a
  raw-mode replay harness on the new cassette, run the script again, and assert
  the two JSON outputs are equal and match the expectations table. Children are
  spawned with `tokio::process::Command::kill_on_drop(true)` inside a 60-second
  timeout, and their output is printed on failure. Stub upstream helpers are
  shared with `tests/chat_completions_replay/` rather than copied.
- `Makefile`: `test-sse-clients` runs
  `cargo nextest run --run-ignored only --test sse_python_client`.
- `.github/workflows/ci.yml`: enable uv caching with
  `cache-dependency-glob: tests/sse_clients/*.py`, update the comment that says
  there is no manifest, and add a `make test-sse-clients` step with
  `timeout-minutes: 15`.

### `EP-M4` Verus proof of the frame kernel

Requirements: `GAP-NO-PROOF-TOOLING`; discharges `INV-FRAME-*`,
`L-FRAME-COMPLETE`, and `L-FRAME-UNIQUE` for all lengths. Delivery of the
roadmap criteria does not depend on this milestone (D7).

Tooling: add `tools/verus/VERSION`, `tools/verus/SHA256SUMS`, and
`tools/rust-prover-tools/REF` following `leynos/mdtablefix`, choosing the
newest Verus release whose checksum can be verified. Add Makefile targets
`verus-install` (idempotent, one download retry), `verus`, `verus-selftest`, and
`verus-mutation`, delegating to `$(PROVER_TOOLS)` with no shell download
logic. Add `[lints.rust] unexpected_cfgs` `check-cfg` for
`cfg(verus_keep_ghost)` in `Cargo.toml`. Add `.github/workflows/verus.yml`
running `make verus`, `make verus-selftest`, and `make verus-mutation` on every
pull request, with `concurrency` and `cancel-in-progress`,
`permissions: contents: read`, actions pinned by commit, `timeout-minutes: 15`,
and a `.verus/<version>` cache keyed on
`hashFiles('tools/verus/**', 'tools/rust-prover-tools/REF')`. Add `.verus/` and
the mutation scratch directory to `.gitignore`. Add
`tests/verus_tooling_contract.rs` (after `tests/build_standard_contract.rs`)
checking non-empty `VERSION` and `SHA256SUMS`, a checksum row for the
configured archive, a 40-hex `REF`, and Makefile recipes that delegate through
`$(PROVER_TOOLS)` without `curl`, `unzip`, `sha256sum`, or
`rustup toolchain install`.

Red: write `verus/sse_frames_spec.rs` with the `spec fn` definitions (`eol_end`,
`eol_start`, `dispatch_end`, and the four postcondition predicates), attach
the four `ensures` clauses to the included kernel through
`src/sse_frames_macros.rs`, and run `make verus`; it must fail on the unproved
loop. Write `verus/smoke.rs` (must fail) and `verus/sse_frames_witnesses.rs`.
The proof clock in `Tolerances` starts when `make verus-selftest` first passes.

Green: add the loop invariants and the minimum lemmas until `make verus`
passes. Prove `L-FRAME-COMPLETE` and `L-FRAME-UNIQUE` as separate `proof fn`s
with `///` comments. Write `scripts/verus_frame_mutation.py` (a `uv` script
following `docs/scripting-standards.md`, with tests in `scripts/tests/`) that
copies `src/` and `verus/` into a scratch directory mirroring the repository
layout so the `#[path]` include resolves, applies each of the three textual
mutations, asserts each applied, runs Verus, and passes only if every run prints
`verification results::` with errors and names the expected predicate. Add
`scripts` to `INTERROGATE_TARGETS`.

If the kernel must change shape for the proof, change it here and rerun the
`EP-M1` suites; record the change in `Decision log`.

### `EP-M5` documentation, ADRs, and roadmap closure

Requirements: `GAP-DOCS-DEFERRAL`; documentation of all requirements.

- `docs/adrs/adr-0003-select-stream-replay-mode-and-event-aligned-raw-frames.md`:
  decisions D1 to D6, D8, D11, and D16; options considered (the alternatives
  table from the review); risks (lone-CR behaviour, client quirks, cassette
  size with the `EP-M0` measurement and the `format_version` 2 trigger); and
  the proof obligations that shaped the kernel API.
- `docs/adrs/adr-0004-adopt-verus-for-the-sse-frame-kernel.md`: decision D7,
  the `#[path]` and macro bridge, pins, gates, what is and is not proved, the
  deviation from the on-ramp's "prover last" advice, the rule that a failing
  proof blocks merge, and a note that it is unrelated to concordat ADR-0004.
- `docs/spycatcher-harness-design.md`: replace both "remains roadmap task
  `2.1.3`" passages; describe both modes, frame alignment, the 501 rule, the
  start-up consistency warning, and the kernel's abstract model and
  postconditions in "Streaming capture and replay"; add an "Implementation
  decisions" entry for task `2.1.3` referencing both ADRs; update the
  "Streaming fidelity edge cases" mitigation; add a risk tying redaction of
  `raw_transcript` to task `2.2.1`.
- `docs/users-guide.md`: configuration bullet, a "Stream replay modes"
  subsection with file, environment, CLI, and library examples, the 501
  diagnostic, that `raw_transcript` is authoritative in raw mode, the lone-CR
  and client-quirk limitations, chunked framing, and the replay flag list.
- `docs/developers-guide.md`: the `sse_frames` kernel's scope and re-use
  policy (owned by the domain; called by replay adapters and later by the
  timing work in `3.1.1`; no other production code may scan transcripts for
  dispatch boundaries, though test oracles may), the consistency check, the
  `raw_framed` metric label and event-count semantics, the Verus workflow and
  proof inventory (what each file proves and does not), the client
  compatibility tests and expectations table, and the new Makefile targets.
- `docs/testing-guide.md`: `googletest`, `pretty_assertions`,
  `eventsource-stream`, the fixtures directory and `.gitattributes` rule, the
  exhaustive small-scope test, and `make test-sse-clients`.
- `docs/contents.md`: add both ADRs.
- `docs/roadmap.md`: tick the three success criteria and mark `2.1.3` done
  only after every acceptance item below passes. Propose (in the pull request,
  for the user's decision) adding `raw_transcript` redaction to task `2.2.1`'s
  success criteria; do not edit `2.2.1` without approval.

Search for stale claims with
`rg -n "2\.1\.3|byte-faithful|not byte" docs --glob '!docs/execplans/**'`.

## Milestones and plateaus

- `EP-M0`: answers recorded; no repository change. Recovery: delete the
  scratch worktree. Compatibility decision: none.
- `EP-M1`: library users can select raw mode; the kernel is tested
  exhaustively for short inputs and by properties; default behaviour is
  unchanged. Conformance: only the authorized public additions; no schema
  change; two authorized dev-dependencies. Recovery: revert the milestone's
  commits. Remaining: layering, clients, proof, documentation. Compatibility
  decision: none (additive, pre-1.0).
- `EP-M2`: CLI, environment, and file select the mode. Recovery: revert.
  Compatibility decision: none.
- `EP-M3`: representative clients are tested in `make test` and CI.
  Recovery: revert; earlier milestones stand. Compatibility decision: none.
- `EP-M4`: the kernel is proved; tooling, workflow, and contract tests exist.
  Conformance: no runtime behaviour change; the kernel body may have changed
  shape (recorded). Recovery: revert tooling and proof commits; the kernel
  stays tested. Compatibility decision: none.
- `EP-M5`: documentation reconciled and the roadmap item marked done.
  Compatibility decision: none.

At every boundary, check: the assigned requirements pass; the design (domain
chooses, adapter frames, kernel pure) still holds; no unapproved public
interface, dependency, trust boundary, or persisted-format change; trace links
current.

## Concrete steps

Working directory: the repository root. Use `set -o pipefail` in the shell
before any `tee` pipeline. Log names follow
`/tmp/<action>-spycatcher-harness-2-1-3-byte-faithful-sse-replay-mode.out`.

Focused commands:

```plaintext
EP-M1 red:   cargo nextest run sse_frames stream_modes replay_transcript replay_handler replay_consistency
             -> new cases fail (kernel stub returns [], Transcript body not produced)
EP-M1 green: same command -> all pass; cargo nextest run replay_stream -> unchanged tests pass
EP-M2 red:   cargo nextest run --test cli_layering_unit --test harness_cli_layering_bdd
               --test binary_stream_mode_e2e -> new cases fail
EP-M2 green: same command -> all pass
EP-M3 red:   cargo nextest run --test sse_client_compat --test raw_transcript_wire_framing
               --test stream_replay_modes_bdd -> fail before fixtures, steps, and the event-field contrast exist
EP-M3 green: same command -> all pass; make test-sse-clients -> passes
EP-M4 red:   make verus -> Verus reports an unproved invariant or postcondition in event_frame_ends
EP-M4 green: make verus -> "verification results:: N verified, 0 errors", N including event_frame_ends
             make verus-selftest -> passes because smoke.rs fails verification
             make verus-mutation -> passes because each mutant fails, naming its predicate
```

Gate sequence after each milestone, run sequentially (delegated to the
`scrutineer` agent where available):

```sh
set -o pipefail
make fmt 2>&1 | tee /tmp/fmt-spycatcher-harness-2-1-3-byte-faithful-sse-replay-mode.out
make check-fmt 2>&1 | tee /tmp/check-fmt-spycatcher-harness-2-1-3-byte-faithful-sse-replay-mode.out
make lint 2>&1 | tee /tmp/lint-spycatcher-harness-2-1-3-byte-faithful-sse-replay-mode.out
make test 2>&1 | tee /tmp/test-spycatcher-harness-2-1-3-byte-faithful-sse-replay-mode.out
make msrv-check 2>&1 | tee /tmp/msrv-check-spycatcher-harness-2-1-3-byte-faithful-sse-replay-mode.out
make markdownlint 2>&1 | tee /tmp/markdownlint-spycatcher-harness-2-1-3-byte-faithful-sse-replay-mode.out
make spelling 2>&1 | tee /tmp/spelling-spycatcher-harness-2-1-3-byte-faithful-sse-replay-mode.out
```

From `EP-M3` onwards also run `make test-sse-clients` and
`make test-workflow-contracts` (CI workflows change). From `EP-M4` onwards also
run `make verus`, `make verus-selftest`, and `make verus-mutation`.
`make nixie` runs only if a Mermaid diagram changes. Record any baseline or
host-specific failure in `Surprises & discoveries` with its log path.

## Validation and acceptance

Acceptance, phrased as behaviour:

- With `[cmds.replay] stream_mode = "raw_transcript"` (or the library field)
  and a cassette whose `raw_transcript` is the CRLF example in
  `Purpose / big picture`, a streaming `POST /v1/chat/completions` returns
  status 200, `content-type: text/event-stream`, chunked transfer encoding, and
  exactly those bytes, written as three chunks, each ending after one of the
  three blank lines.
- With no `stream_mode` set, the same request returns the canonical
  `data: {"choices":[]}\n\n: OPENROUTER PROCESSING\n\ndata: [DONE]\n\n`.
- `SPYCATCHER_HARNESS_CMDS_REPLAY_STREAM_MODE=raw_transcript` and
  `--stream-mode raw_transcript` are accepted, with CLI over environment over
  file, and `verbatim` is rejected with an error naming `stream_mode`.
- In raw mode, an interaction with events but an empty transcript returns 501
  with `{"error": {"kind": "unsupported_stream", ...}}` and the new message,
  and an identical second request gets the same answer.
- In raw mode, start-up logs a warning naming the interaction index and
  `events_differ` for a cassette whose events were edited.
- `eventsource-stream` and the OpenAI Python SDK see every fixture through raw
  replay exactly as they see the recorded upstream, and the expectations table
  holds.

Quality criteria:

- Tests: `make test` passes, including every new suite and every existing
  stream suite with unchanged expectations; `make test-sse-clients` passes.
- Verification: the exhaustive, property, and negative-control tests pass;
  `make verus`, `make verus-selftest`, and `make verus-mutation` pass.
- Lint and format: `make check-fmt`, `make lint`, `make msrv-check`,
  `make markdownlint`, `make spelling`, and `make test-workflow-contracts` pass.
- Performance: no benchmark is required. Raw replay clones the transcript once
  per response under the engine lock (as parsed mode clones events today) and
  slices it without further copying outside the lock.
- Security: no new trust boundary; recorded bytes were already persisted and
  non-stream bodies were already replayed verbatim; no transcript bytes are
  logged.

Red-Green-Refactor evidence (red command and failure, green command and pass,
refactor gates) is recorded per milestone in `Artefacts and notes`.

## Idempotence and recovery

All steps are re-runnable. Tests write cassettes to temporary directories and
bind ephemeral ports. `make verus-install` is idempotent and caches under
`.verus/`. The mutation script works in a scratch directory under the
repository's ignored mutation path and removes it on exit; if interrupted,
delete that directory. Each milestone ends in one or more commits, so reverting
a milestone's commits restores the previous plateau.

## Artefacts and notes

Reconnaissance evidence (2026-11-15, revision `c3e6d76`):

```plaintext
src/replay.rs response_from_recorded: RecordedResponse::Stream -> ReplayBody::Events(events.clone()); raw_transcript ignored
src/replay_types.rs: UnsupportedStream #[expect(dead_code, reason = "reserved for task 2.1.3 ...")]
src/server/record_stream/state.rs observe_chunk: raw_transcript.extend_from_slice(chunk)
src/bin/spycatcher_harness.rs run_harness: start_harness(...) then harness.shutdown()
Cargo.lock: no eventsource, googletest, or pretty_assertions packages
Makefile msrv-check: cargo +1.89.0 check --locked --workspace --all-targets --all-features
```

Design review summary (2026-11-15): four reviewers returned "proceed with
conditions" or "revise". Blocking findings applied: kernel shape versus the
repository's lints (D15); unsound alignment generator and false `finish`
premise (generation rules in `Verification plan`); client read boundaries
cannot show frame alignment (D11, wire-level test); the binary cannot serve
requests (D12); wrong error key and stale message (D16); `replay_tests/mod.rs`
cannot absorb new code (preparatory refactor); kernel `dead_code` at an
intermediate milestone (D10); and an unsafe client oracle (D11, D13). The
strongest alternative offered was to drop Verus in favour of the exhaustive
check alone; it is kept for the user's decision (D7).

Probe, red, green, and negative-control transcripts are added here during
implementation.

## Interfaces and dependencies

Public (library) additions in `src/config.rs`:

```rust
/// Selects how replay emits recorded stream responses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum StreamReplayMode {
    /// Re-serialize the recorded parsed events as canonical SSE frames.
    #[default]
    ParsedEvents,
    /// Replay the recorded upstream bytes verbatim in event-aligned frames.
    RawTranscript,
}

pub struct ReplayConfig {
    // existing fields unchanged
    /// Stream replay output mode.
    pub stream_mode: StreamReplayMode,
}
```

Crate-internal additions:

```rust
// src/sse_frames.rs (pure domain kernel, verified by Verus)
#[must_use]
pub(crate) fn event_frame_ends(transcript: &[u8]) -> Vec<usize>;

// src/replay_types.rs
pub(crate) struct RawTranscriptBody { pub(crate) bytes: Vec<u8>, pub(crate) event_count: usize }
pub(crate) enum ReplayBody { OneShot(Vec<u8>), Events(Vec<StreamEvent>), Transcript(RawTranscriptBody) }

// src/replay_consistency.rs (pure check plus a logging function)
pub(crate) fn check_raw_transcripts(cassette: &Cassette) -> ConsistencyReport;

// src/server/replay_transcript.rs (Axum adapter)
pub(crate) fn build_transcript_body(body: RawTranscriptBody, labels: &ReplayMetricLabels) -> axum::body::Body;

// src/replay_observability.rs
pub(crate) enum StreamDeliveryMode { Eager, Streamed, RawFramed }
```

Re-use policy: `sse_frames` is the only production code that decides SSE
dispatch boundaries over raw bytes. Replay adapters and the future timing work
(`3.1.1`) call it; no other production module may scan transcripts for blank
lines (test oracles may). It must stay free of I/O and of types Verus cannot
include. `replay_consistency` is the only place replay compares transcripts
with events; the `verify` task (`2.2.1`) may reuse it.

Dependencies (all dev-only, caret requirements): `googletest = "0.14"`,
`pretty_assertions = "1.4"`, `eventsource-stream = "0.2.3"`. Python, through
`uv` only: `openai` pinned to an exact version in the script header with
`exclude-newer`. Tools: Verus through `rust-prover-tools` at a pinned commit.
No runtime dependency changes.

## Revision note

- 2026-11-15: initial draft from Wyvern reconnaissance and external research.
- 2026-11-15: applied the community-of-experts design review. Milestones
  reordered (library replay with kernel first, Verus proof fourth and
  separable); postconditions restated in gap form with uniqueness and
  sequential-model lemmas; exhaustive small-scope evidence added; alignment
  generator corrected; client oracle split into transparency, content, and
  contrast with a wire-level framing test; Python check moved to a Rust-driven
  record-then-replay round trip because the binary exits immediately; error key
  and message corrected; start-up consistency warnings, observability fields,
  and framing-header exclusions added; MSRV claim corrected; budgets raised to
  cover the added evidence. Remaining work is unchanged in scope except for the
  start-up consistency check (new) and the binary byte test (replaced by wiring
  tests).
