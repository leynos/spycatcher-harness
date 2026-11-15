# 2.1.3 Implement byte-faithful SSE replay mode

This ExecPlan (execution plan) is a living document. The sections `Constraints`,
`Tolerances`, `Risks`, `Progress`, `Surprises & discoveries`, `Decision log`,
`Outcomes & retrospective`, `Conformance basis`, and `Verification plan` must
be kept up to date as work proceeds.

Status: DRAFT

Branch: `2-1-3-byte-faithful-sse-replay-mode`

Roadmap item: `2.1.3` in `docs/roadmap.md`.

Implementation must not begin until the user explicitly approves this plan.
After approval, work proceeds milestone by milestone and stops only when a
tolerance threshold is reached, a constraint would be violated, or an
architecture deviation needs acceptance.

## Purpose / big picture

The Spycatcher harness records a conversation between an agent and a Large
Language Model (LLM) provider into a JSON file called a cassette, then replays
that cassette in continuous integration without network access. Providers such
as OpenRouter stream chat completions as Server-Sent Events (SSE): a
`text/event-stream` HTTP body made of text lines such as `data: {...}` and
`: OPENROUTER PROCESSING`, where a blank line ends ("dispatches") one event.

Roadmap tasks `2.1.1` and `2.1.2` made the harness record every streamed
interaction twice: as a list of parsed `StreamEvent` values and as the exact
upstream bytes in `raw_transcript`. Replay today uses only the parsed events.
It writes them back out in one canonical form: every line ends in `\n`, every
data line starts with `data:` and one space, every comment line starts with `:`
and one space, and fields the parser ignores (`event:`, `id:`, `retry:`) are
lost. That is deliberate and documented as "not byte-faithful".

For example, an upstream that sent this transcript (shown with escapes):

```plaintext
event: chunk\r\nid: 7\r\ndata:{"choices":[]}\r\n\r\n: OPENROUTER PROCESSING\r\n\r\ndata: [DONE]\r\n\r\n
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
  carriage returns, the `event:` and `id:` fields, and the missing space after
  `data:`. The bytes are delivered as a sequence of HTTP body chunks, each of
  which ends exactly where an SSE event is dispatched ("event-aligned frames"),
  so a streaming client never receives half an event in one chunk and the rest
  in the next because of the harness.

The mode is selected in `[cmds.replay]` of the configuration file as
`stream_mode = "raw_transcript"`, by the environment variable
`SPYCATCHER_HARNESS_CMDS_REPLAY_STREAM_MODE=raw_transcript`, by the flag
`spycatcher-harness replay --stream-mode raw_transcript`, or by library callers
through `HarnessConfig.replay.stream_mode`.

Preserved behaviour: matching, cursor commitment, status and header replay, the
`text/event-stream` default content type, non-stream replay, record mode, the
cassette schema (`format_version` 1), and every parsed-events byte. The one
deliberately new diagnostic is that `raw_transcript` mode answers HTTP
`501 Not Implemented` with error code `unsupported_stream` when the matched
stream interaction has recorded events but an empty `raw_transcript` (only
possible for a hand-authored cassette); the interaction is not consumed.

This closes the deferral recorded in `docs/spycatcher-harness-design.md`
("Byte-faithful replay of exact upstream framing remains roadmap task `2.1.3`")
and the mitigation promised under "Known risks and limitations" ("record raw
byte transcript and replay it verbatim as an option; test against
representative SSE clients"). It also gives task `3.1.1` (deterministic timing
controls) the event-aligned frame boundaries at which timing delays can later
be inserted.

## Relevant documentation and skills

Read these before starting. Each is cited where it matters below.

- `docs/roadmap.md`, item `2.1.3`, for the success criteria.
- `docs/spycatcher-harness-design.md`, sections "Cassette definition",
  "Streaming capture and replay", "Known risks and limitations", and
  "Implementation decisions".
- `docs/developers-guide.md`, sections "Rust lint baseline", "SSE parser and
  stream recorder", "Shared test infrastructure", and "Module layout".
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
- `docs/ortho-config-users-guide.md` for subcommand layering
  (`load_and_merge_subcommand`) and enum deserialization.
- `docs/scripting-standards.md` for the Python `uv` script conventions used by
  the OpenAI Python client check and the Verus mutation gate.
- `docs/documentation-style-guide.md` for ADR format, Markdown wrapping, and
  en-GB-oxendict spelling.
- Skills: `execplans` (this format), `rust-router` then `rust-unit-testing`,
  `proptest`, `verus`, and `rust-verification`; `hexagonal-architecture` for
  the domain/adapter split; `domain-web-services` for the Axum body; and
  `codegraph-mcp` for navigation (`codegraph_get_callers` on
  `response_from_recorded` and `build_replay_response`).

## Constraints

- The cassette schema must not change. `CassetteFormatVersion::SUPPORTED`
  stays `1`, `RecordedResponse::Stream` keeps its fields and serde shape, and
  `raw_transcript` stays a required `Vec<u8>` serialized as a JSON integer
  array. Any change requires escalation.
- `parsed_events` must remain the default and must stay byte-identical to the
  current output. The existing tests in `src/server/replay_stream_tests.rs`,
  `src/server/replay_stream_properties.rs`, `src/replay_tests/`, and
  `tests/chat_completions_replay_bdd.rs` must pass without edits to their
  assertions.
- Record mode, request matching, canonicalization, and cursor semantics must
  not change. In particular the replay engine must still peek, build the
  response, and only then commit (`match_recorded_response` in
  `src/replay.rs`), so a rejected response never consumes an interaction.
- `raw_transcript` mode must emit exactly the recorded bytes: no line-ending
  normalization, no comment stripping (`StreamCanonicalPolicy` is a matching
  concern and must not be applied to emitted bytes), no synthetic terminator,
  and no byte-order-mark handling.
- Hexagonal boundaries: the frame-boundary kernel (`src/sse_frames.rs`) is pure
  domain code with no I/O, Axum, `bytes`, or `tokio` imports. The domain
  (`src/replay.rs`, `src/replay_types.rs`) decides the body shape; the Axum
  adapter (`src/server/`) only turns that shape into an HTTP body.
- Configuration layering must follow the existing ortho_config pattern
  (`ReplayArgs` in `src/cli/mod.rs`, mapping in `src/cli/config_build.rs`).
  Tests that need configuration must inject sources (`load_with_sources`,
  `MapEnv`, or subprocess `Command::env`); no in-process environment mutation
  and no `serial_test` or global locks.
- Repository rules from `AGENTS.md`: no file over 400 lines; every module
  starts with `//!`; every assertion has a message; no `.expect()` or
  `.unwrap()` outside tests; lint suppressions only as item-scoped
  `#[expect(..., reason = "...")]`; en-GB-oxendict spelling.
  `src/replay_tests/mod.rs` (399 lines) and `src/cassette/filesystem.rs` (399
  lines) must not grow; new tests go into sibling modules.
- The `#[expect(dead_code, ...)]` on `ReplayError::UnsupportedStream` in
  `src/replay_types.rs` must be removed in the same commit that first
  constructs the variant, because an unfulfilled expectation fails the lint.
- Verus proofs must be about the production kernel body included by
  `#[path]`, not a parallel reimplementation. No `assume`, `admit`,
  `#[verifier::external_body]`, or trusted axioms may be added to make the
  kernel proof pass.
- Authorized new dependencies, all dev-only: `googletest` (`"0.14"`),
  `pretty_assertions` (`"1.4"`), and `eventsource-stream` (`"0.2.3"`), each
  with a caret requirement. No new runtime dependency is authorized. The
  `reqwest` feature set must not change (the client tests use
  `Response::chunk()`, which needs no `stream` feature).
- Authorized new tooling: Verus pinned through `rust-prover-tools`
  (`tools/verus/VERSION`, `tools/verus/SHA256SUMS`,
  `tools/rust-prover-tools/REF`), Makefile targets `verus-install`, `verus`,
  `verus-selftest`, `verus-mutation`, and `test-sse-clients`, and a new
  `.github/workflows/verus.yml`. `make verus` stays out of `make all`,
  `make test`, and `make lint` (per the `verus` skill guidance), but runs on
  every pull request in its own workflow.
- Historical documents must remain untouched: every completed plan under
  `docs/execplans/` and the accepted text of ADR 0001 and ADR 0002.
- The public library change is additive: a new
  `pub enum config::StreamReplayMode` and a new `pub stream_mode` field on
  `config::ReplayConfig`. The crate is pre-1.0 and every in-repository
  `HarnessConfig` literal uses `..HarnessConfig::default()`, so no
  compatibility shim is needed or permitted.

## Tolerances (exception triggers)

Counting base: `git diff --numstat c3e6d76...HEAD`, excluding
`docs/execplans/2-1-3-byte-faithful-sse-replay-mode.md`. Each limit measures
net lines (additions minus deletions) and is enforced independently; spare
capacity in one category never offsets another.

Classification is by whole file:

- Code: `src/**`, `tests/**` (including `.feature` files, `.sse` fixtures,
  insta `.snap` files, and the Python client script), `verus/**`, `scripts/**`,
  and `Makefile`.
- Configuration: `Cargo.toml`, `.github/workflows/**`, `tools/**`, and
  `typos.local.toml`. `Cargo.lock` and the generated `typos.toml` are excluded
  from line counts; instead, the lockfile may add at most 12 new `[[package]]`
  entries (count with `git diff c3e6d76 -- Cargo.lock | grep -c '^+name = '`).
- Documentation: `docs/**` except this ExecPlan, plus `README.md`.

Budget survey: the comparable plans `2-1-1` (24 files or about 1,900 net lines),
`2-1-2` (22 files or about 1,700), `1-3-2` (22 files or about 1,700), and
`1-2-3` (16 files or 1,200) used one combined budget, recorded no revisions,
and grew only through repository constraints (file splits for the 400-line
limit and avoiding the `reqwest` `stream` feature). This task adds two new
kinds of work they did not have: a Verus proof with its tooling, and
third-party client compatibility tests. The estimates below are built from the
file-level plan in `Plan of work`.

- Files: stop and escalate if more than 50 non-ExecPlan files change.
- Code: stop and escalate above 2,800 net lines. Estimate: about 1,050 lines
  of production and test Rust for the kernel, domain, adapter, and
  configuration; about 650 lines of behaviour-driven and end-to-end tests and
  fixtures; about 450 lines of Verus proof and gate scripts; about 200 lines
  for the Python client check and Makefile. Contingency is roughly 15 per cent,
  because the proof size is the least certain estimate.
- Configuration: stop and escalate above 220 net lines, or above 12 new
  lockfile packages. Estimate: about 10 lines in `Cargo.toml`, 70 in
  `verus.yml`, 20 in `ci.yml`, and 10 in `tools/`.
- Documentation: stop and escalate above 750 net lines. Estimate: two ADRs
  (about 300), the design document (about 90), the developers' guide (about
  140), the users' guide (about 80), the testing guide (about 40), and roadmap
  and contents updates (about 15).
- Interface: escalate if any public item beyond `config::StreamReplayMode`
  and `ReplayConfig::stream_mode` must change, or if `ReplayBody`,
  `ReplayError`, or `StreamDeliveryMode` need more than one new variant each.
- Dependencies: escalate if a dependency beyond the three authorized dev
  dependencies is needed, including `async-openai`, `reqwest-eventsource`, or a
  `reqwest` feature change.
- Architecture: escalate if raw replay appears to need a cassette schema
  change, recorded upstream chunk boundaries, or changes to record mode.
- Proof: escalate if the Verus bridge (`#[path]` inclusion of
  `src/sse_frames.rs`) cannot be made to verify within 8 working hours of
  `EP-M2`, or if any proof would need `assume`, `admit`, or `external_body`.
  Options to present: a bounded Kani proof of the same postconditions, or a
  Verus proof over a spec mirror plus a differential property test.
- Iterations: escalate if a focused test still fails after 5 fix attempts.
- Time: escalate if any milestone exceeds 8 working hours.
- Ambiguity: escalate if a representative client disagrees with the WHATWG
  rules in a way that changes what "event boundary" means for this plan.

## Risks

- Risk: the Verus toolchain or `#[path]` bridge does not accept the kernel as
  written (unsupported standard-library calls, `Vec` specifications, or a
  toolchain mismatch with `nightly-2026-11-13`). Severity: medium. Likelihood:
  medium. Mitigation: `EP-M0` probes the bridge on a scratch kernel before any
  production code depends on it; the kernel uses only indexing, `while` loops,
  comparisons, and `Vec::push`; Verus runs its own pinned toolchain via
  `prover-tools`, independent of the repository pin.
- Risk: real clients accept or reject streams differently from the WHATWG
  algorithm, for example `eventsource-stream` dispatching an unterminated final
  event, or the OpenAI Python SDK matching `[DONE]` by prefix. Severity:
  medium. Likelihood: high. Mitigation: compatibility tests compare each
  client's view of the replayed stream with the same client's view of the
  recorded bytes ("replay is transparent to the client"), never with an
  idealized event list; observed divergences are recorded as axioms and
  documented.
- Risk: line-ending ambiguity. A frame that ends with a lone carriage return
  (`\r`) cannot be recognized as a complete line by a streaming parser until
  the next byte arrives, because the next byte might be `\n`. Severity: low.
  Likelihood: medium (only CR-only upstreams). Mitigation: the kernel never
  splits `\r\n`, the behaviour is faithful to the upstream, and the users'
  guide documents the limitation.
- Risk: the Python client check needs network access to fetch `openai` from
  the Python Package Index on first run. Severity: low. Likelihood: high for
  offline agents. Mitigation: it is a dedicated `make test-sse-clients` target
  run in CI, not part of `make test`; the in-process Rust client test covers
  the same contract offline.
- Risk: dev-dependency growth slows builds or breaks `make msrv-check`.
  Severity: low. Likelihood: low. Mitigation: `EP-M0` measures the lockfile
  delta; `msrv-check` does not compile dev-dependencies.
- Risk: a hand-authored cassette with an empty `raw_transcript` silently
  replays an empty body in `raw_transcript` mode. Severity: medium. Likelihood:
  low. Mitigation: the `unsupported_stream` 501 rule, tested at domain,
  handler, and behaviour level.
- Risk: the 400-line limit forces splits of `src/replay.rs` (338 lines) or
  `src/cli/mod.rs` (343 lines). Severity: low. Likelihood: medium. Mitigation:
  new logic goes into new focused modules (`src/sse_frames.rs`,
  `src/server/replay_transcript.rs`, `src/cli/replay_args.rs` if needed).

## Progress

- [x] (2026-11-15 15:30Z) Reconnaissance with a Wyvern team: replay stream
  path, configuration surface, test and proof infrastructure, and prior
  ExecPlan budgets.
- [x] (2026-11-15 15:40Z) External research: WHATWG SSE parsing rules, OpenAI
  Python SDK `SSEDecoder`, `eventsource-stream` 0.2.3 as used by `async-openai`
  0.43, and the estate Verus pattern in `mdtablefix`.
- [x] (2026-11-15 16:30Z) Initial draft written.
- [ ] Expert design review applied.
- [ ] Plan approved by the user.
- [ ] `EP-M0` feasibility probe.
- [ ] `EP-M1` event-aligned frame kernel.
- [ ] `EP-M2` Verus proof of the frame kernel.
- [ ] `EP-M3` raw-transcript replay in the library.
- [ ] `EP-M4` configuration layering and binary surface.
- [ ] `EP-M5` representative streaming client compatibility.
- [ ] `EP-M6` documentation, ADRs, and roadmap closure.

## Surprises & discoveries

- Observation: the existing byte-equality stream test cannot distinguish the
  two modes. Evidence: `tests/chat_completions_replay/stream_steps.rs` compares
  the replayed body with `sample_stream_transcript()`, which is already in
  canonical form. Impact: raw-mode tests must use non-canonical transcripts
  (CRLF, CR, `data:` without a space, `event:` and `id:` fields, a comment
  between two data lines).
- Observation: parsed events are lossy in order as well as framing. Evidence:
  `src/sse.rs` emits a comment as soon as its line is read, so
  `data: a\n: c\ndata: b\n\n` yields `[Comment c, Data "a\nb"]`. Impact: raw
  mode must take bytes from `raw_transcript`, never from events.
- Observation: no replay option is configurable from the CLI, environment, or
  file today; `match_mode` and `replay.*` are library-only. Evidence:
  `ReplayArgs` in `src/cli/mod.rs` and `to_replay_config` in
  `src/cli/config_build.rs`. Impact: `stream_mode` is the first layered replay
  option and needs a full set of layering tests.
- Observation: `src/server/snapshots/*unsupported_stream*.snap` exists but no
  test case references it. Impact: `EP-M3` re-attaches it.
- Observation: the upstream request strips `accept-encoding`
  (`src/upstream.rs`, `src/http_exchange.rs`), so recorded transcripts are
  identity-encoded SSE text, not compressed bytes.

## Decision log

- D1. Decision: two modes, `parsed_events` (default) and `raw_transcript`,
  spelt in `snake_case` in files, environment, and CLI. Rationale: keeps the
  default byte-identical for existing users and matches the repository's only
  enum precedent (`RecordUpstreamKind`, `rename_all = "snake_case"`) and the
  cassette field name `raw_transcript`. Rejected: making `raw_transcript` the
  default, which would silently change output for existing suites. Date/Author:
  2026-11-15, planning agent.
- D2. Decision: the field lives on `ReplayConfig` as `stream_mode`, and on
  `ReplayArgs` as `stream_mode: Option<StreamModeArg>`. Rationale:
  `ReplayConfig` already owns replay output shaping; a `ReplayArgs` field is
  the established ortho_config route to file, environment, and CLI layering.
  Date/Author: 2026-11-15, planning agent.
- D3. Decision: raw mode delivers event-aligned frames: one HTTP body chunk per
  SSE dispatch boundary, computed by a pure kernel over the recorded bytes.
  Rationale: upstream chunk boundaries were never persisted (`2.1.1` appends
  chunks to one buffer), and TCP does not preserve chunk boundaries anyway; the
  strongest boundary guarantee available is that every chunk the harness writes
  ends where a conformant client dispatches an event, and that no chunk splits
  `\r\n`. This also gives task `3.1.1` its timing insertion points. Rejected:
  one eager body (correct bytes, but no boundary guarantee and no hook for
  timing); re-deriving chunks from parsed events (lossy, see Surprises).
- D4. Decision: raw mode always streams frames and never uses the 64 KiB eager
  path; delivery is recorded with a new `StreamDeliveryMode::RawFramed`
  (`"raw_framed"`). Rationale: frame alignment is the point of the mode, and a
  distinct bounded label keeps the existing `eager` and `streamed` metric
  semantics unchanged.
- D5. Decision: an empty `raw_transcript` with non-empty `events` returns
  `ReplayError::UnsupportedStream` (HTTP 501, `unsupported_stream`) during the
  peek, before commit; an empty transcript with no events replays an empty
  body. Rationale: explicit failure instead of silent fallback to parsed events
  (`AGENTS.md`: do not convert errors into absence); reuses the variant `2.1.2`
  reserved for this task and its existing snapshot.
- D6. Decision: raw mode does not validate that `raw_transcript` parses to
  the recorded `events`. Rationale: cassette integrity checking is roadmap task
  `2.2.1` (`verify`); duplicating it in the request path would add cost and a
  second policy.
- D7. Decision: Verus proves the frame kernel; property tests and named
  examples cover its refinement to real parsers; integration tests cover
  clients. Rationale: the kernel's postconditions are an unbounded property
  over arbitrary byte sequences of a small, pure, stable function, which is the
  case the `rust-router` testing hierarchy reserves for Verus. Everything else
  (mode selection, headers, layering, cursor behaviour) is finite control flow
  and gets example tests. Rejected: Kani alone (bounded, would leave lengths
  beyond the bound unverified); property tests alone (sampled).
- D8. Decision: representative clients are `eventsource-stream` 0.2.3
  (in-process, part of `make test`) and the official OpenAI Python SDK (via a
  `uv` script, `make test-sse-clients`, run in CI). Rationale:
  `eventsource-stream` is the SSE decoder used by `async-openai` 0.43 and
  `reqwest-eventsource`; the OpenAI Python SDK is the most common client in
  agent frameworks and has its own decoder (`openai._streaming.SSEDecoder`).
  Together they cover a Rust nom-based parser and a Python line decoder.
  Rejected: `async-openai` as a dev-dependency, because it adds a large type
  tree while exercising the same `eventsource-stream` framing code; payload
  deserialization is unchanged by this task.
- D9. Decision: record the mode and framing decision in ADR 0003 and the Verus
  adoption in ADR 0004. Rationale: both are hard to reverse and outlive this
  task. ADR 0002's "Amended by ADR-0004" banner refers to concordat's ADR-0004,
  an external document, so the local number is free; ADR 0004's title names
  Verus explicitly to avoid confusion.

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
(`src/server/replay.rs`), the only place configuration reaches the replay
service: it creates a `ReplayMatchEngine` with `cfg.match_mode` and a
`ReplayService` with a `ReplayContext` (`src/replay_types.rs`, fields
`cassette` and `protocol`).

The Axum handler `replay_chat_completions_handler`
(`src/server/replay_handler.rs`) calls `ReplayService::handle_chat_completions`
(`src/replay.rs`). Matching peeks the next interaction, calls
`response_from_recorded`, and commits only if that succeeds. For
`RecordedResponse::Stream` it returns `ReplayBody::Events(events.clone())` and
ignores `raw_transcript` and `timing`. `build_replay_response` turns
`ReplayBody::OneShot` into a plain body and `ReplayBody::Events` into
`build_stream_body` (`src/server/replay_stream.rs`), which serializes each
event with `serialize_event` and either concatenates all chunks (total up to
`EAGER_STREAM_LIMIT_BYTES`, 64 KiB) or streams one chunk per event. When the
body is a stream and no `content-type` was recorded it inserts
`text/event-stream`. `ReplayError` maps to HTTP in
`build_replay_error_response`; `UnsupportedStream` exists but is unused and
carries `#[expect(dead_code, reason = "reserved for task 2.1.3 ...")]`.

The cassette model is in `src/cassette/mod.rs`:
`RecordedResponse::Stream {
status, headers, events, raw_transcript: Vec<u8>, timing: Option<StreamTiming>
}`,
with `StreamEvent::{Comment { text }, Data { raw, parsed_json }}`. Record mode
(`src/server/record_stream/state.rs`, `observe_chunk`) appends every upstream
chunk to `raw_transcript` and persists only after `SseParser::finish` succeeds,
so recorded transcripts are syntactically complete apart from an optional
trailing comment.

The SSE parser `SseParser` (`src/sse.rs`, 375 lines, `pub(crate)`) handles LF,
CR, and CRLF, defers a trailing CR until the next chunk, emits comments
immediately, joins multiple `data` lines with `\n`, ignores `event`, `id`,
`retry`, and unknown fields, and does not strip a byte-order mark.

Configuration: `HarnessConfig` and `ReplayConfig` live in `src/config.rs` (290
lines). `ReplayConfig` holds `simulate_timing`, `ttft_ms`, and `tps`, none of
which is read by replay yet. The `replay` subcommand's ortho_config-derived
`ReplayArgs` (`src/cli/mod.rs`, 343 lines) has `listen`, `cassette_dir`,
`cassette_name`, `locale`, `fallback_locale`, and `localization`;
`to_replay_config` (`src/cli/config_build.rs`) maps it onto
`HarnessConfig::default()`. Files use `[cmds.replay]`; environment variables use
`SPYCATCHER_HARNESS_CMDS_REPLAY_<FIELD>`. Option help is plain clap doc
comments, not Fluent messages, so no `.ftl` change is required.

Tests: `rstest` 0.28, `rstest-bdd` 0.6, `proptest` 1.11, and `insta` 1.47 (with
`json`) are dev-dependencies; `googletest`, `pretty_assertions`, and any SSE
client are not. `make test` runs
`cargo nextest run --all-targets --all-features` and then doctests. Relevant
suites: `src/sse.rs` tests, `src/server/replay_stream_tests.rs` (337 lines) with
`replay_stream_properties.rs`, `src/server/replay_handler_tests.rs` with
snapshots in `src/server/snapshots/`, `src/replay_tests/` (`mod.rs` is 399
lines), `tests/chat_completions_replay_bdd.rs` with
`tests/features/chat_completions_replay.feature`, the layering suites
`tests/harness_cli_layering_bdd.rs`, `tests/cli_layering/`, and the subprocess
suite `tests/binary_cli_layering_e2e.rs`. Integration tests read streams with
`reqwest` `.bytes()`, so they never see chunk boundaries.

There is no Verus or Kani set-up in this repository. Kani 0.x and `cargo-kani`
are installed on the development host, Verus is not; Verus will be installed
per repository by `prover-tools`. The estate precedent is `leynos/mdtablefix`:
`verus/lib.rs` includes production kernels with `#[path = "../src/..."]`,
kernels gate ghost code on `cfg(verus_keep_ghost)`, `verus/smoke.rs` must fail
(`make verus-selftest`), mutation targets must fail with the expected
postcondition named, and `.github/workflows/verus.yml` runs them on pull
requests.

CI (`.github/workflows/ci.yml`) installs `uv`, runs `make check-fmt`,
`make lint`, `make msrv-check` (Rust 1.89.0), and the tests through the shared
coverage action. Python is used only through `uv` scripts.

External facts gathered for this plan (see `Verification plan`, axioms):

- WHATWG HTML "Server-sent events", section "Parsing an event stream": lines
  end with CRLF, lone CR, or LF; a blank line dispatches; a line starting with
  `:` is ignored; `data` appends the value plus LF; at end of file an
  incomplete event is discarded; streams are UTF-8 decoded and one leading BOM
  is stripped.
- OpenAI Python SDK (`src/openai/_streaming.py`): `SSEDecoder` splits on any
  of CR, LF, or CRLF, ignores comment lines, dispatches on a blank line, and
  `Stream.__stream__` stops when `data` starts with `[DONE]`.
- `async-openai` 0.43 depends on `eventsource-stream` 0.2 (latest 0.2.3,
  2022); its nom parser accepts CRLF, CR, and LF and returns
  `EventStreamError::{Utf8, Parser, Transport}`.

## Conformance basis

Upstream artefacts, all at revision `c3e6d76`:

- `docs/roadmap.md`, item `2.1.3` and its three success criteria.
- `docs/spycatcher-harness-design.md`, "Cassette definition", "Streaming
  capture and replay", "Known risks and limitations" ("Streaming fidelity edge
  cases"), and "Implementation decisions".
- ADR 0001 and ADR 0002 (not affected).
- `AGENTS.md` and `docs/developers-guide.md` "Rust lint baseline" as governing
  standards.
- No Terms of Reference document exists for this project.

Requirements:

- `REQ-MODE-CONFIG`: replay supports parsed-event and raw-transcript stream
  modes selected by configuration (roadmap `2.1.3` criterion 1).
- `REQ-RAW-BOUNDARIES`: raw-transcript mode preserves the event boundaries SSE
  clients need (criterion 2).
- `REQ-CLIENT-COMPAT`: integration tests validate compatibility with
  representative streaming clients (criterion 3; design "Known risks").
- `REQ-RAW-VERBATIM`: raw mode replays recorded bytes verbatim (design "Known
  risks": "replay it verbatim as an option").
- `REQ-PARSED-PRESERVED`: parsed-event replay semantics in the design
  ("Cassette definition", "Replay strategy") are unchanged and remain the
  default.
- `REQ-NO-SCHEMA-CHANGE`: cassette `format_version` 1 and the stream variant
  are unchanged (design "Cassette file naming and schema versioning").
- `REQ-EXPLICIT-FAILURE`: unusable raw data is reported, not silently
  replaced (`AGENTS.md`, "Architecture and operational boundaries").

Gaps observed at the base revision:

- `GAP-RAW-UNREAD`: nothing on the replay side reads `raw_transcript`.
- `GAP-NO-LAYERED-REPLAY-OPTION`: no replay option is reachable from CLI,
  environment, or file.
- `GAP-NO-CLIENT-TESTS`: no test consumes a replayed stream with an SSE
  client or observes chunk boundaries.
- `GAP-NO-PROOF-TOOLING`: no Verus pins, targets, or workflow.
- `GAP-RESERVED-ERROR`: `ReplayError::UnsupportedStream` is reserved and its
  snapshot is orphaned.
- `GAP-DOCS-DEFERRAL`: the design document and users' guide say byte-faithful
  replay "remains roadmap task `2.1.3`".

Trace links:

```plaintext
ROADMAP-2.1.3-SC1 -> REQ-MODE-CONFIG -> EP-M3, EP-M4
  -> tests::stream_replay_modes_bdd, tests::cli_layering::stream_mode, tests::binary_stream_replay_e2e
ROADMAP-2.1.3-SC2 -> REQ-RAW-BOUNDARIES -> EP-M1, EP-M2, EP-M3 -> INV-FRAME-*, verus/lib.rs, server::replay_transcript tests
ROADMAP-2.1.3-SC2 -> REQ-RAW-VERBATIM -> EP-M3 -> INV-RAW-VERBATIM tests
ROADMAP-2.1.3-SC3 -> REQ-CLIENT-COMPAT -> EP-M5 -> tests::sse_client_compat, make test-sse-clients
DESIGN-STREAMING -> REQ-PARSED-PRESERVED -> EP-M3 -> existing replay_stream tests unchanged
DESIGN-SCHEMA -> REQ-NO-SCHEMA-CHANGE -> all milestones -> cassette tests unchanged
AGENTS-ERRORS -> REQ-EXPLICIT-FAILURE -> EP-M3 -> INV-EMPTY-TRANSCRIPT tests
GAP-NO-PROOF-TOOLING -> EP-M2 -> make verus, make verus-selftest, make verus-mutation
GAP-DOCS-DEFERRAL -> EP-M6 -> design, users' guide, ADR 0003, ADR 0004
```

## Verification plan

### Abstract model

Let `s` be a finite byte sequence of length `n` (the recorded
`raw_transcript`). Bytes are indexed `s[0]` to `s[n-1]`. Write `LF` for 0x0A and
`CR` for 0x0D.

A position `p` with `1 <= p <= n` is an end-of-line end (`eol_end(s, p)`) when
either `s[p-1] == LF`, or `s[p-1] == CR` and (`p == n` or `s[p] != LF`). This
is the WHATWG `end-of-line = cr lf / cr / lf` rule with the usual greedy
reading: a CR immediately followed by LF belongs to one CRLF terminator.

For an end-of-line end `p`, its start `eol_start(s, p)` is `p - 2` when
`s[p-1] == LF`, `p >= 2`, and `s[p-2] == CR`; otherwise it is `p - 1`.

A dispatch end (`dispatch_end(s, p)`) is an end-of-line end whose line is empty:
`eol_start(s, p) == 0`, or `eol_end(s, eol_start(s, p))`. That is the position
just after the blank line that makes a WHATWG parser dispatch.

The kernel returns a sequence `ends` of positions. The frames are the slices
`s[0..ends[0]]`, `s[ends[0]..ends[1]]`, and so on.

Preconditions: none. Every byte sequence, including the empty sequence, invalid
UTF-8, and transcripts with an incomplete final event, is admissible. Malformed
input is not rejected by the kernel; byte faithfulness requires replaying it as
recorded, and its final frame is simply the remainder.

Postconditions, for all `s`:

1. Ordering: `ends` is strictly increasing and every element is in `1..=n`.
2. Termination: if `n > 0` then the last element of `ends` is `n`; if `n == 0`
   then `ends` is empty.
3. Soundness: every element except possibly the last is a dispatch end, and
   the last is a dispatch end or `n`.
4. Completeness: every dispatch end in `1..=n` is an element of `ends`.

Representation relationship: the production kernel
`crate::sse_frames::event_frame_ends(&[u8]) -> Vec<usize>` is the verified
function itself, included into `verus/lib.rs` by `#[path]`; its slice and `Vec`
are viewed through `vstd` as `Seq<u8>` and `Seq<usize>`. The adapter
(`src/server/replay_transcript.rs`) maps `ends` to `Bytes::slice` ranges; that
step is tested, not proved.

### Invariants and lemmas

- `INV-FRAME-ORDER`, `INV-FRAME-TERMINATION`, `INV-FRAME-SOUND`,
  `INV-FRAME-COMPLETE`: postconditions 1 to 4 above (`REQ-RAW-BOUNDARIES`).
- `L-FRAME-LOSSLESS`: concatenating the frames defined by any `ends` that
  satisfies `INV-FRAME-ORDER` and `INV-FRAME-TERMINATION` yields exactly `s`
  (`REQ-RAW-VERBATIM`).
- `L-NO-CRLF-SPLIT`: for every element `e < n` of `ends`, it is not the case
  that `s[e-1] == CR` and `s[e] == LF`. This follows from `INV-FRAME-SOUND` and
  the definition of `eol_end`, and is stated separately because it is the
  property clients rely on.
- `L-FRAME-ALIGNMENT` (refinement to the repository parser): for every frame
  except the last, parsing the frame alone with a fresh `SseParser` and calling
  `finish` succeeds; and the concatenation of the per-frame events equals the
  events from parsing `s` whole. This is where "event boundaries needed by SSE
  clients" meets an executable parser.
- `L-CLIENT-TRANSPARENT`: for each representative client, the events the
  client yields when reading the replayed HTTP response equal the events the
  same client yields when given `s` directly (`REQ-CLIENT-COMPAT`).
- `INV-RAW-VERBATIM`: in `raw_transcript` mode the HTTP body bytes equal
  `raw_transcript` exactly, for empty, small, and larger-than-64-KiB transcripts
  (`REQ-RAW-VERBATIM`).
- `INV-PARSED-UNCHANGED`: in `parsed_events` mode the body bytes equal the
  pre-change serialization (`REQ-PARSED-PRESERVED`).
- `INV-MODE-SELECT`: the configured mode, from library, file, environment, or
  CLI with precedence CLI > environment > file > default, determines the body
  source; the default is `parsed_events` (`REQ-MODE-CONFIG`).
- `INV-SHARED-ENVELOPE`: status, recorded headers, and the
  `text/event-stream` default are identical in both modes.
- `INV-EMPTY-TRANSCRIPT`: in raw mode, empty transcript with non-empty events
  yields 501 `unsupported_stream` and leaves the cursor unchanged; empty
  transcript with no events yields 200 with an empty body
  (`REQ-EXPLICIT-FAILURE`).

### Axioms (trusted, not verified here)

- `AXIOM-WHATWG-EOL`: conformant SSE parsers split lines on CRLF, lone CR, and
  LF, with CR followed by LF read as one terminator, and dispatch on a blank
  line (WHATWG HTML, "Parsing an event stream"). The kernel's `eol_end`
  definition models this rule; the model is checked against the repository
  parser by `L-FRAME-ALIGNMENT`.
- `AXIOM-CLIENT-DECODERS`: `eventsource-stream` 0.2.3 and the pinned OpenAI
  Python SDK behave as their source shows. Their internals are not verified;
  `L-CLIENT-TRANSPARENT` only compares each client with itself.
- `AXIOM-HYPER-BODY`: Axum and hyper write the bytes of each body frame in
  order without alteration; HTTP chunked framing and TCP may merge frames but
  not split or reorder bytes within the response. The harness guarantees frame
  alignment of what it writes, not what the client's socket reads.
- `AXIOM-RECORDED-IDENTITY`: recorded transcripts are identity-encoded because
  the recorder strips `accept-encoding` from upstream requests.
- `AXIOM-VERUS`: the pinned Verus release and its `vstd` specifications of
  slices and `Vec` are sound.

### Methods per obligation

- Obligation: `INV-FRAME-ORDER`, `INV-FRAME-TERMINATION`, `INV-FRAME-SOUND`,
  `INV-FRAME-COMPLETE`, `L-FRAME-LOSSLESS`, `L-NO-CRLF-SPLIT`.
  - Method: Verus deductive proof of the production kernel, plus a
    `proptest` runtime check of postconditions 1 to 4 and losslessness against
    an independent oracle, plus named `rstest` examples.
  - Rationale: the property must hold for every length and every byte value;
    the kernel is small, pure, and loop-based.
  - Domain: all `Seq<u8>` (proof); generated transcripts up to 4 KiB from a
    structured SSE generator and from `proptest::collection::vec(any::<u8>(),
    0..512)` biased towards CR, LF, `:`, and ASCII letters (tests).
  - Artefacts: `verus/lib.rs`, `verus/sse_frames_spec.rs`, `verus/smoke.rs`,
    `scripts/verus_frame_mutation.py`; `src/sse_frames_tests.rs`,
    `src/sse_frames_properties.rs`.
  - Evidence: `make verus` reports all functions verified with zero errors;
    `make verus-selftest` reports the smoke proof failing; `make
    verus-mutation` reports a failure naming the completeness or soundness
    `ensures` clause; `cargo nextest run sse_frames` passes.
  - Non-vacuity: the proof has no `requires`, so its antecedent is every
    input; the witness file `verus/sse_frames_witnesses.rs` proves concrete
    facts such as "for `data: a\r\n\r\n` the ends are `[11]`" and "for
    `\r\r\n` the ends are `[1, 3]`", which would be unprovable if the
    postconditions were vacuous or contradictory. The mutation gate replaces
    the CRLF look-ahead so a CR before LF counts as a line end; Verus must
    reject it. The property-test oracle is a separate, deliberately naive
    implementation that classifies every position with `dispatch_end` directly
    from the definitions (quadratic, test-only), so it shares no code with
    the kernel. `proptest` classifications must record at least one case each
    of LF, CRLF, lone-CR, and mixed terminators, an incomplete final event,
    and an empty input.
- Obligation: `L-FRAME-ALIGNMENT`.
  - Method: `proptest` over structured transcripts, plus named examples.
  - Rationale: it relates the proved kernel to the repository's executable
    parser, which Verus cannot include (it uses `serde_json` and `String`).
  - Domain: transcripts generated as sequences of events, each a sequence of
    lines (`data`, comment, `event`, `id`, `retry`, unknown field) with
    per-line terminators drawn from LF, CRLF, and CR, ended by a blank line,
    optionally followed by one trailing comment line.
  - Artefact: `src/sse_frames_properties.rs`.
  - Evidence: `cargo nextest run sse_frames` passes; seeded failures are kept
    in `src/sse_frames_properties.proptest-regressions`.
  - Non-vacuity: the generator produces only complete transcripts, so
    `finish` on the whole input must succeed; a classification counter
    asserts that at least 20 per cent of cases contain a CR terminator and that
    multi-frame cases exist. Negative control: a temporary kernel variant that
    puts a boundary after every line end makes the property fail on a
    multi-line event; it is reverted before commit and the failure transcript
    is recorded in `Artefacts and notes`.
- Obligation: `INV-RAW-VERBATIM`, `INV-PARSED-UNCHANGED`,
  `INV-SHARED-ENVELOPE`, `INV-EMPTY-TRANSCRIPT`.
  - Method: `rstest` tables with `googletest` matchers at domain and adapter
    level, a `proptest` that the concatenated adapter frames equal the input
    and every frame is non-empty, and an `insta` snapshot contrasting both
    modes for one non-canonical cassette.
  - Artefacts: `src/replay_tests/stream_modes.rs`,
    `src/server/replay_transcript_tests.rs`,
    `src/server/replay_handler_tests.rs`, and
    `src/server/snapshots/*stream_mode_contrast*.snap`.
  - Non-vacuity: transcripts are non-canonical, so a mode switch that
    accidentally serializes events fails the byte comparison; the
    larger-than-64-KiB case exercises the path the eager limit would
    otherwise take; the cursor test sends a second request and asserts which
    interaction answers it.
- Obligation: `INV-MODE-SELECT`.
  - Method: `rstest` precedence tables through `load_with_sources`, a
    behaviour scenario in `tests/features/harness_cli_layering.feature`, and a
    subprocess test of the binary with `Command::env`.
  - Non-vacuity: each row sets a different value at each layer so a wrong
    precedence order selects a different mode; an invalid value
    (`stream_mode = "verbatim"`) must produce a merge error naming the field.
- Obligation: `L-CLIENT-TRANSPARENT`.
  - Method: integration tests. In-process: start the harness with
    `start_harness`, read the response with `reqwest` `Response::chunk()`,
    feed the chunks to `eventsource_stream::Eventsource::eventsource()`, and
    compare with the same decoder over the fixture bytes delivered as one
    chunk and as single-byte chunks. Out of process: `make test-sse-clients`
    runs `tests/sse_clients/openai_python_stream.py`, which starts the built
    binary on a fixture cassette and consumes it with
    `openai.OpenAI(...).chat.completions.create(stream=True)`.
  - Domain: fixtures in `tests/fixtures/sse_transcripts/`: OpenAI-style LF,
    OpenRouter-style with `: OPENROUTER PROCESSING` and a mid-stream error
    chunk, CRLF with `event:` and `id:`, lone CR, `data:` without a space,
    multi-line data with an interleaved comment, usage-including final chunk,
    and a larger-than-64-KiB transcript.
  - Non-vacuity: every fixture is also replayed in `parsed_events` mode; at
    least the `event:`/`id:` fixture must show a client-visible difference
    between modes (the `event` field), proving the test can see what raw mode
    adds.

### Formal verification decision

Verus is warranted for the frame kernel only (D7). It is the single new piece
of contractual logic: its four postconditions and two lemmas must hold for
transcripts of any length, and a wrong boundary silently corrupts every
client's view of a stream. The kernel is small, pure, and stable, so the proof
should stay cheap to maintain.

Verus is deliberately not used for mode selection, configuration precedence,
header handling, the 501 rule, or the Axum body adapter. These are finite
control flow or third-party plumbing, where named tests and tables give
complete coverage of the cases and a proof would only restate the code. Kani is
not used: it would bound the transcript length and add nothing the unbounded
proof lacks. `L-FRAME-ALIGNMENT` stays a property test because `SseParser` uses
`String`, `serde_json`, and `Result` plumbing outside the Verus subset, and
modelling it in Verus would be a second parser to keep in step.

The proof plan is a sketch. The likely shape is a loop invariant relating the
scan position, the start of the current line, and the ends pushed so far
("every dispatch end at or before the scan position has been pushed, and every
pushed value is a dispatch end"), plus a small lemma that a position strictly
inside a non-empty line is not a dispatch end. Implementation may refine or
split these. Auxiliary sequence lemmas, triggers, and `broadcast use`
directives are deferred to `EP-M2`.

### Suggested BDD features

Proposed path: `tests/features/stream_replay_modes.feature`, run by
`tests/stream_replay_modes_bdd.rs` with steps in `tests/stream_replay_modes/`.

```gherkin
Feature: Stream replay modes
  Replay can re-emit parsed stream events or the recorded upstream bytes.

  Background:
    Given a stream cassette whose raw transcript uses CRLF line endings and an event field

  Scenario: Raw-transcript mode replays the recorded bytes exactly
    Given the replay harness uses the "raw_transcript" stream mode
    When a client requests the streamed chat completion
    Then the response status is 200
    And the response content type is "text/event-stream"
    And the response body equals the recorded raw transcript

  Scenario: Parsed-events mode remains the default
    Given the replay harness uses the default stream mode
    When a client requests the streamed chat completion
    Then the response body equals the canonical serialization of the recorded events

  Scenario: Raw-transcript mode delivers event-aligned chunks
    Given the replay harness uses the "raw_transcript" stream mode
    When a client reads the streamed chat completion chunk by chunk
    Then every received chunk boundary falls on an SSE event boundary

  Scenario: Raw-transcript mode rejects a stream recording without a transcript
    Given a stream cassette whose raw transcript is empty but whose events are not
    And the replay harness uses the "raw_transcript" stream mode
    When a client requests the streamed chat completion
    Then the response status is 501
    And the error code is "unsupported_stream"
    And the stream interaction has not been consumed
```

The third scenario's "every received chunk boundary" assertion holds on
loopback in practice but is not guaranteed by TCP (`AXIOM-HYPER-BODY`);
implement it as "every received chunk boundary is one of the kernel's frame
ends", which is true however frames are merged.

Addition to `tests/features/harness_cli_layering.feature`:

```gherkin
  Scenario: Stream replay mode follows layered precedence
    Given config file sets replay stream_mode to "raw_transcript"
    And environment sets replay stream_mode to "parsed_events"
    And the command line sets replay stream_mode to "raw_transcript"
    When the layered command configuration is loaded
    Then replay stream_mode is "raw_transcript"
```

Runner: `cargo nextest run --test stream_replay_modes_bdd` and
`cargo nextest run --test harness_cli_layering_bdd`.

## Plan of work

### `EP-M0` feasibility probe (prototype, deleted)

Questions:

1. Can the pinned Verus release, installed by `prover-tools verus install`,
   verify a scratch kernel included through `#[path]` with ghost code gated on
   `cfg(verus_keep_ghost)`, using `while` loops, slice indexing, and
   `Vec::push`, while the same file compiles under Cargo with the repository's
   lints?
2. Does `reqwest` 0.13 `Response::chunk()` work without the `stream` feature,
   and does `eventsource-stream` 0.2.3 decode a stream built from those chunks?
3. How many lockfile packages do `googletest`, `pretty_assertions`, and
   `eventsource-stream` add?
4. Does `eventsource-stream` dispatch an unterminated final event, and does
   the OpenAI Python SDK accept CR-only streams?

Work in a scratch branch or worktree (`git worktree add ../sh-2-1-3-probe`),
never on the plan branch. Record transcripts in `Artefacts and notes` and
delete the scratch worktree. Go: questions 1 and 2 answered yes and question 3
within the lockfile tolerance. No-go on question 1 triggers the proof
escalation in `Tolerances`; no-go on 2 or 3 triggers the dependency escalation.
Question 4 only feeds the axioms and fixtures.

### `EP-M1` event-aligned frame kernel

Requirements: `REQ-RAW-BOUNDARIES`, `REQ-RAW-VERBATIM` (kernel part).

Red: add `src/sse_frames_tests.rs` with an `rstest` table of named cases (empty
input; one LF event; one CRLF event; CR-only events; `\r\r\n`; a leading blank
line; two events; a trailing comment without a blank line; an incomplete final
event; CR at the very end; invalid UTF-8 bytes) asserting the exact `ends`
vector, and `src/sse_frames_properties.rs` with the oracle properties and
`L-FRAME-ALIGNMENT`. Declare `mod sse_frames;` in `src/lib.rs` with a stub
returning an empty vector so the tests compile and fail.

Green: implement `event_frame_ends` as a single left-to-right scan that tracks
the start of the current line, recognizes CRLF before lone CR, pushes the end
of every empty line's terminator, and finally pushes `n` if it is not already
last. Keep the function free of iterators and closures so Verus can include it
unchanged in `EP-M2`. Add a test-only accessor only if `L-FRAME-ALIGNMENT`
needs one; prefer parsing each frame with a fresh `SseParser`, which needs no
new accessor.

Refactor: keep the file below 120 lines and document the abstract model in the
module comment.

### `EP-M2` Verus proof of the frame kernel

Requirements: `GAP-NO-PROOF-TOOLING`; discharges `INV-FRAME-*`,
`L-FRAME-LOSSLESS`, `L-NO-CRLF-SPLIT`.

Add `tools/verus/VERSION`, `tools/verus/SHA256SUMS`, and
`tools/rust-prover-tools/REF`, copying the pinning approach of
`leynos/mdtablefix` and choosing the newest Verus release whose checksum can be
verified. Add Makefile targets `verus-install`, `verus`, `verus-selftest`, and
`verus-mutation`, delegating to `$(PROVER_TOOLS)` with no shell download logic.
Add `[lints.rust] unexpected_cfgs` `check-cfg` for `cfg(verus_keep_ghost)` in
`Cargo.toml`. Add `.github/workflows/verus.yml` running `make verus`,
`make verus-selftest`, and `make verus-mutation` on pull requests, with the
`.verus/<version>` cache. Add `.verus/` to `.gitignore`.

Red: write `verus/sse_frames_spec.rs` with the `spec fn` definitions `eol_end`,
`eol_start`, and `dispatch_end`, attach the four `ensures` clauses to the
included kernel, and run `make verus`; it must fail on the unproved loop. Write
`verus/smoke.rs` and `verus/sse_frames_witnesses.rs`.

Green: add loop invariants and the minimum lemmas until `make verus` passes.
Prove `L-FRAME-LOSSLESS` and `L-NO-CRLF-SPLIT` as separate `proof fn`s with
`///` comments stating them in words. Write `scripts/verus_frame_mutation.py` (a
`uv` script per `docs/scripting-standards.md`) that copies the proof tree to a
temporary directory, applies one textual mutation to the copied kernel, asserts
the mutation applied, runs Verus, and passes only if Verus fails and names the
expected `ensures` clause.

If the kernel must change shape for the proof, change it here and rerun the
`EP-M1` tests; record the change in `Decision log`.

### `EP-M3` raw-transcript replay in the library

Requirements: `REQ-MODE-CONFIG` (library), `REQ-RAW-VERBATIM`,
`REQ-RAW-BOUNDARIES`, `REQ-PARSED-PRESERVED`, `REQ-EXPLICIT-FAILURE`,
`GAP-RAW-UNREAD`, `GAP-RESERVED-ERROR`.

Add `googletest` and `pretty_assertions` to `[dev-dependencies]`.

Red, domain: in a new `src/replay_tests/stream_modes.rs` (declared from
`src/replay_tests/mod.rs` with a one-line `mod`, keeping that file at or under
400 lines), add tests that a `ReplayService` built with
`StreamReplayMode::RawTranscript` returns `ReplayBody::Transcript` with the
recorded bytes, that the default still returns `ReplayBody::Events`, and the two
`INV-EMPTY-TRANSCRIPT` cases including the unchanged cursor.

Red, adapter: add `src/server/replay_transcript_tests.rs` asserting, by polling
the body with `http_body_util::BodyExt::frame`, that frames equal the kernel's
slices, that their concatenation equals the input for empty, small, and 70 KiB
inputs, and that delivery is recorded as `raw_framed`. Extend
`src/server/replay_handler_tests.rs` with the shared-envelope table, the
`unsupported_stream` case that re-attaches the orphaned snapshot, and the
`stream_mode_contrast` snapshot.

Green:

1. `src/config.rs`: add
   `pub enum StreamReplayMode { #[default] ParsedEvents, RawTranscript }` with
   `Debug, Clone, Copy, Default, PartialEq, Eq` and a doctest; add
   `pub stream_mode: StreamReplayMode` to `ReplayConfig` and its `Default`.
2. `src/replay_types.rs`: add `stream_mode` to `ReplayContext`; add
   `ReplayBody::Transcript(Vec<u8>)`; remove the `expect` and `FIXME` from
   `UnsupportedStream`.
3. `src/server/replay.rs`: pass `cfg.replay.stream_mode` into the context.
4. `src/replay.rs`: in the `RecordedResponse::Stream` arm, choose the body by
   mode; in raw mode return `UnsupportedStream` (after `log_rejection`) for the
   empty-transcript case, otherwise clone `raw_transcript`. Extract a helper if
   the function would exceed the complexity limits.
5. `src/sse_frames.rs`: already present from `EP-M1`.
6. `src/server/replay_transcript.rs`:
   `build_transcript_body(transcript: Vec<u8>, labels: &ReplayMetricLabels) -> Body`,
   converting to `Bytes` once and streaming `bytes.slice(start..end)` for each
   frame with `futures_util::stream::iter`, inside a `debug_span` like
   `build_stream_body`.
7. `src/replay_observability.rs`: add `StreamDeliveryMode::RawFramed`.
8. `src/server/replay_handler.rs`: route `ReplayBody::Transcript` to
   `build_transcript_body` and treat it as a stream for the content-type
   default.

Refactor: confirm no file exceeds 400 lines, rerun the `EP-M1` and existing
stream suites unchanged.

### `EP-M4` configuration layering and binary surface

Requirements: `REQ-MODE-CONFIG`, `GAP-NO-LAYERED-REPLAY-OPTION`.

Red: add `tests/cli_layering/stream_mode.rs` with an `rstest` precedence table
(default; file only; environment over file; CLI over environment) and an
invalid-value case, using `load_with_sources`; add the layering scenario above
to `tests/features/harness_cli_layering.feature` with its steps; add
`tests/binary_stream_replay_e2e.rs` that writes a non-canonical stream
cassette, starts the binary with `Command::env` setting
`SPYCATCHER_HARNESS_CMDS_REPLAY_STREAM_MODE=raw_transcript`, reads the listen
address, sends one streaming request, and compares bytes; then repeats with
`--stream-mode parsed_events` overriding the environment.

Green: add a private `StreamModeArg` enum (`serde` `rename_all = "snake_case"`,
clap `ValueEnum` with `rename_all = "snake_case"`, `From` into
`config::StreamReplayMode`) beside `RecordUpstreamKind` in `src/cli_args.rs`;
add `#[arg(long, value_enum)] stream_mode: Option<StreamModeArg>` to
`ReplayArgs`; map it in `to_replay_config`. If `src/cli/mod.rs` would pass 400
lines, move `ReplayArgs` to `src/cli/replay_args.rs`. No `.ftl` change is
needed because option help is not localized; confirm the help snapshots in
`tests/snapshots/` are unchanged.

### `EP-M5` representative streaming client compatibility

Requirements: `REQ-CLIENT-COMPAT`, `GAP-NO-CLIENT-TESTS`.

Add `eventsource-stream = "0.2.3"` to `[dev-dependencies]`. Add fixtures under
`tests/fixtures/sse_transcripts/` as `.sse` files with a short `README.md`
describing each one's purpose; build cassettes from them at test time with the
public `cassette` API.

Red then green: add `tests/sse_client_compat.rs` with an `rstest` case per
fixture and per mode, implementing `L-CLIENT-TRANSPARENT` and the
`event:`-field contrast; add the `stream_replay_modes` feature, entry point,
and steps. Add `tests/sse_clients/openai_python_stream.py`, a `uv` script
pinned to an exact `openai` version, with docstrings for `interrogate`, that
starts `target/debug/spycatcher-harness replay` on a fixture cassette in raw
mode, consumes the stream with the OpenAI SDK, and asserts the concatenated
`delta.content` and the number of chunks; add `make test-sse-clients` (depends
on `build`) and a CI step running it after the tests.

### `EP-M6` documentation, ADRs, and roadmap closure

Requirements: `GAP-DOCS-DEFERRAL`; all requirements' documentation.

- `docs/adrs/adr-0003-select-stream-replay-mode-and-event-aligned-raw-frames.md`:
  decisions D1 to D6 and D8, options considered, risks (CR-only latency, TCP
  coalescing), and the proof obligations that shaped the kernel API.
- `docs/adrs/adr-0004-adopt-verus-for-the-sse-frame-kernel.md`: decision D7,
  the `#[path]` bridge, pins, gates, what is and is not proved, and the rule
  that `make verus` stays out of `make all`.
- `docs/spycatcher-harness-design.md`: replace both "remains roadmap task
  `2.1.3`" passages; describe both modes, frame alignment, the 501 rule, and
  the abstract model and postconditions of the kernel in "Streaming capture and
  replay"; add an "Implementation decisions" entry for task `2.1.3` referencing
  both ADRs; update the "Streaming fidelity edge cases" mitigation.
- `docs/users-guide.md`: configuration bullet, a "Stream replay modes"
  subsection with file, environment, CLI, and library examples, the 501
  diagnostic, the CR-only limitation, and the replay-flag list.
- `docs/developers-guide.md`: the `sse_frames` kernel's scope and re-use
  policy (owned by the domain, callable by replay adapters and later by the
  timing work in `3.1.1`; no parallel boundary scanners), the Verus workflow
  and proof inventory, the client-compatibility tests, and the new Makefile
  targets.
- `docs/testing-guide.md`: `googletest`, `pretty_assertions`,
  `eventsource-stream`, the fixtures directory, and `make test-sse-clients`.
- `docs/contents.md`: add both ADRs.
- `docs/roadmap.md`: tick the three success criteria and mark `2.1.3` done
  only after every acceptance item below passes.

Search for stale claims with
`rg -n "2\.1\.3|byte-faithful|not byte" docs --glob '!docs/execplans/**'`.

## Milestones and plateaus

- `EP-M0`: answers recorded; no repository change. Recovery: delete the
  scratch worktree. Compatibility decision: none.
- `EP-M1`: the kernel exists with tests and properties but has no caller
  besides tests. Conformance: no public interface, dependency, or format
  change. Recovery: revert the commit. Remaining: proof, adapter, config.
  Compatibility decision: none.
- `EP-M2`: the kernel is proved; tooling and workflow exist. Conformance: no
  runtime change; the kernel body may have changed shape (record it). Recovery:
  revert tooling and proof commits; the kernel stays tested. Remaining: replay
  integration. Compatibility decision: none.
- `EP-M3`: library users can select raw mode; default behaviour is unchanged.
  Conformance: only the authorized public additions; no schema change.
  Recovery: revert; `EP-M1` and `EP-M2` remain valid. Remaining: CLI,
  environment, and file layering; client tests; documentation. Compatibility
  decision: none (additive, pre-1.0).
- `EP-M4`: CLI, environment, and file select the mode. Recovery: revert.
  Compatibility decision: none.
- `EP-M5`: representative clients are tested in `make test` and CI.
  Recovery: revert; earlier milestones stand. Compatibility decision: none.
- `EP-M6`: documentation reconciled and the roadmap item marked done.
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
EP-M1 red:   cargo nextest run sse_frames -> failures in sse_frames_tests (stub returns [])
EP-M1 green: cargo nextest run sse_frames -> all pass
EP-M2 red:   make verus -> Verus reports an unproved postcondition in event_frame_ends
EP-M2 green: make verus -> "verification results:: N verified, 0 errors"
             make verus-selftest -> passes because smoke.rs fails verification
             make verus-mutation -> passes because the mutated kernel fails, naming the ensures clause
EP-M3 red:   cargo nextest run stream_modes replay_transcript replay_handler -> new cases fail
EP-M3 green: same command -> all pass; cargo nextest run replay_stream -> unchanged tests pass
EP-M4 red:   cargo nextest run --test cli_layering_unit --test harness_cli_layering_bdd \
               --test binary_stream_replay_e2e -> new cases fail
EP-M4 green: same command -> all pass
EP-M5 red:   cargo nextest run --test sse_client_compat --test stream_replay_modes_bdd
               -> fail before the fixtures and steps exist
EP-M5 green: same command -> all pass; make test-sse-clients -> passes
```

Gate sequence after each milestone, run sequentially (delegated to the
`scrutineer` agent where available):

```sh
set -o pipefail
make fmt 2>&1 | tee /tmp/fmt-spycatcher-harness-2-1-3-byte-faithful-sse-replay-mode.out
make check-fmt 2>&1 | tee /tmp/check-fmt-spycatcher-harness-2-1-3-byte-faithful-sse-replay-mode.out
make lint 2>&1 | tee /tmp/lint-spycatcher-harness-2-1-3-byte-faithful-sse-replay-mode.out
make test 2>&1 | tee /tmp/test-spycatcher-harness-2-1-3-byte-faithful-sse-replay-mode.out
make markdownlint 2>&1 | tee /tmp/markdownlint-spycatcher-harness-2-1-3-byte-faithful-sse-replay-mode.out
make spelling 2>&1 | tee /tmp/spelling-spycatcher-harness-2-1-3-byte-faithful-sse-replay-mode.out
```

From `EP-M2` onwards also run `make verus`, `make verus-selftest`, and
`make verus-mutation`; from `EP-M5` onwards also run `make test-sse-clients`.
`make nixie` runs only if a Mermaid diagram changes. Record any baseline or
host-specific failure in `Surprises & discoveries` with its log path.

## Validation and acceptance

Acceptance, phrased as behaviour:

- With `[cmds.replay] stream_mode = "raw_transcript"` and a cassette whose
  `raw_transcript` is the CRLF example in `Purpose / big picture`, a streaming
  `POST /v1/chat/completions` returns status 200,
  `content-type: text/event-stream`, and exactly those bytes.
- With no `stream_mode` set, the same request returns the canonical
  `data: {"choices":[]}\n\n: OPENROUTER PROCESSING\n\ndata: [DONE]\n\n`.
- `SPYCATCHER_HARNESS_CMDS_REPLAY_STREAM_MODE=raw_transcript` and
  `--stream-mode raw_transcript` have the same effect, with CLI over
  environment over file.
- In raw mode, an interaction with events but an empty transcript returns 501
  with `"code": "unsupported_stream"`, and a following request still matches
  that interaction.
- `eventsource-stream` and the OpenAI Python SDK read every fixture through
  the harness exactly as they read the fixture bytes directly.

Quality criteria:

- Tests: `make test` passes, including every new suite and every existing
  stream suite with unchanged assertions; `make test-sse-clients` passes.
- Verification: `make verus`, `make verus-selftest`, and
  `make verus-mutation` pass; the property tests record the required
  classifications.
- Lint and format: `make check-fmt`, `make lint`, `make markdownlint`, and
  `make spelling` pass.
- Performance: no benchmark is required; raw replay clones the transcript
  once per response and slices it without further copying.
- Security: no new trust boundary; recorded bytes are already redacted at the
  header level, and bodies were already replayed in non-stream mode.

Red-Green-Refactor evidence (red command and failure, green command and pass,
refactor gates) is recorded per milestone in `Artefacts and notes`.

## Idempotence and recovery

All steps are re-runnable. Tests write cassettes to temporary directories.
`make verus-install` is idempotent and caches under `.verus/`. The mutation
script works on a temporary copy and removes it on exit; if interrupted, delete
any leftover `verus/.mutation-*` directory. Each milestone ends in a commit, so
`git revert` of that milestone's commits restores the previous plateau.

## Artefacts and notes

Reconnaissance evidence (2026-11-15, revision `c3e6d76`):

```plaintext
src/replay.rs response_from_recorded: RecordedResponse::Stream -> ReplayBody::Events(events.clone()); raw_transcript ignored
src/replay_types.rs: UnsupportedStream #[expect(dead_code, reason = "reserved for task 2.1.3 ...")]
src/server/record_stream/state.rs observe_chunk: raw_transcript.extend_from_slice(chunk)
Cargo.lock: no eventsource, googletest, or pretty_assertions packages
```

Probe, red, green, and negative-control transcripts are added here during
implementation.

## Interfaces and dependencies

Public (library) additions in `src/config.rs`:

```rust
/// Selects how replay emits recorded stream responses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
pub(crate) fn event_frame_ends(transcript: &[u8]) -> Vec<usize>;

// src/replay_types.rs
pub(crate) enum ReplayBody { OneShot(Vec<u8>), Events(Vec<StreamEvent>), Transcript(Vec<u8>) }

// src/server/replay_transcript.rs (Axum adapter)
pub(crate) fn build_transcript_body(transcript: Vec<u8>, labels: &ReplayMetricLabels) -> axum::body::Body;

// src/replay_observability.rs
pub(crate) enum StreamDeliveryMode { Eager, Streamed, RawFramed }
```

Re-use policy: `sse_frames` is the only place that decides SSE event boundaries
over raw bytes. Replay adapters and the future timing work (`3.1.1`) call it;
no other module may scan transcripts for blank lines. It must stay free of I/O
and of types Verus cannot include.

Dependencies (all dev-only, caret requirements): `googletest = "0.14"`,
`pretty_assertions = "1.4"`, `eventsource-stream = "0.2.3"`. Python, through
`uv` only: `openai` pinned to an exact version in the script header. Tools:
Verus through `rust-prover-tools` at a pinned commit. No runtime dependency
changes.

## Revision note

- 2026-11-15: initial draft from Wyvern reconnaissance and external research.
