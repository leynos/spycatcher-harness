# Spycatcher harness roadmap

This roadmap defines implementation work for the Spycatcher harness based on
`docs/spycatcher-harness-design.md`. Tasks are written as measurable delivery
units with explicit dependencies and completion criteria.

## 1. Deterministic record and replay foundation

### 1.1. Configuration and executable skeleton

- [x] 1.1.1. Implement the library and binary crate skeleton for harness
      startup.
  - Depends on: none.
  - Success criteria:
    - [x] `spycatcher_harness` exposes `start_harness(cfg)` and
          `RunningHarness::shutdown()` as compile-checked public APIs.
    - [x] Public library entry points return typed error enums and do not
          export opaque error types.
    - [x] `spycatcher-harness` CLI binary delegates all startup and shutdown
          behaviour to library entry points.
    - [x] `cargo test --workspace` passes with baseline smoke tests for startup
          and shutdown.
  - Design references:
    [Rust API and module boundaries](spycatcher-harness-design.md#rust-api-and-module-boundaries),
    [Public library API surface](spycatcher-harness-design.md#public-library-api-surface).

- [x] 1.1.2. Integrate layered configuration loading for all subcommands.
  - Depends on: 1.1.1.
  - Success criteria:
    - [x] Configuration precedence is proven by tests:
          `CLI > env > config files > defaults`.
    - [x] `record`, `replay`, and `verify` support `cmds.<subcommand>` merge
          values with test coverage for overrides.
    - [x] CLI help and docs describe the merged configuration shape.
  - Design references:
    [Configuration via OrthoConfig](spycatcher-harness-design.md#configuration-via-orthoconfig),
    [CLI integration and configuration](spycatcher-harness-design.md#cli-integration-and-configuration).

### 1.2. Cassette model and matching engine

- [ ] 1.2.1. Implement cassette schema versioning and append-only persistence.
  - Depends on: 1.1.1.
  - Success criteria:
    - [ ] Stored cassette includes `format_version`, ordered interactions,
          protocol identifiers, and metadata fields from the design.
    - [ ] Replay mode opens cassettes read-only and rejects unsupported
          `format_version` values with actionable errors.
    - [ ] Schema round-trip tests verify lossless serialization for non-stream
          and stream interactions.
  - Design references:
    [Cassette definition](spycatcher-harness-design.md#cassette-definition),
    [Architecture overview](spycatcher-harness-design.md#architecture-overview).

- [ ] 1.2.2. Implement canonical request generation and stable hashing.
  - Depends on: 1.2.1.
  - Success criteria:
    - [ ] Canonicalization includes query parameter normalization in addition
          to request body normalization.
    - [ ] Canonicalization normalizes JSON key ordering and insignificant
          whitespace.
    - [ ] Ignore-path configuration supports metadata drift without affecting
          hash stability.
    - [ ] Fixture tests confirm that equivalent requests produce identical
          hashes and materially different requests do not.
  - Design references:
    [Canonicalization and hashing](spycatcher-harness-design.md#canonicalization-and-hashing).

- [ ] 1.2.3. Deliver strict sequential and keyed replay matching modes.
  - Depends on: 1.2.2.
  - Success criteria:
    - [ ] Sequential strict mode fails mismatches with HTTP 409 and includes
          expected interaction ID, observed hash, and field-level diff summary.
    - [ ] Keyed mode consumes the next unused interaction for a matching hash.
    - [ ] Integration tests cover mismatch diagnostics and concurrent replay
          order handling.
  - Design references:
    [Matching modes](spycatcher-harness-design.md#matching-modes).

### 1.3. OpenAI chat completions non-stream path

- [ ] 1.3.1. Implement `POST /v1/chat/completions` record mode proxying.
  - Depends on: 1.1.2, 1.2.2.
  - Success criteria:
    - [ ] Requests are proxied to configured upstream with selected headers and
          body capture.
    - [ ] Non-stream responses are stored as exact bytes plus parsed JSON when
          valid.
    - [ ] Redaction rules remove configured secret headers before persistence.
  - Design references:
    [Architecture overview](spycatcher-harness-design.md#architecture-overview),
    [Streaming capture and replay](spycatcher-harness-design.md#streaming-capture-and-replay).

- [ ] 1.3.2. Implement non-stream replay for `POST /v1/chat/completions`.
  - Depends on: 1.2.3, 1.3.1.
  - Success criteria:
    - [ ] Replay returns recorded status, headers, and body bytes verbatim for
          non-stream interactions.
    - [ ] Replay requires no outbound network access and fails fast if network
          calls are attempted.
    - [ ] End-to-end record to replay integration tests pass using a stub
          upstream service.
  - Design references:
    [Goals and non-goals](spycatcher-harness-design.md#goals-and-non-goals),
    [Recording and replay semantics](spycatcher-harness-design.md#recording-and-replay-semantics).

### 1.4. Localization foundation

- [ ] 1.4.1. Embed library Fluent resources and expose loader-injected message
      rendering.
  - Depends on: 1.1.1.
  - Success criteria:
    - [ ] Library-owned FTL assets are embedded in the library crate and
          versioned with the API.
    - [ ] Localized message rendering APIs accept an application-provided
          `FluentLanguageLoader`.
    - [ ] Library components do not create process-global language loaders.
  - Design references:
    [Localization architecture](spycatcher-harness-design.md#localization-architecture),
    [Core traits and types](spycatcher-harness-design.md#core-traits-and-types).

- [ ] 1.4.2. Add localization configuration layering for the binary application.
  - Depends on: 1.1.2.
  - Success criteria:
    - [ ] `locale` and `fallback_locale` are loadable through
          `CLI > env > config files > defaults`.
    - [ ] Startup locale negotiation is deterministic and tested for fallback
          behaviour.
    - [ ] One authoritative language loader is created at startup and reused.
  - Design references:
    [Localization architecture](spycatcher-harness-design.md#localization-architecture),
    [Configuration via OrthoConfig](spycatcher-harness-design.md#configuration-via-orthoconfig).

- [ ] 1.4.3. Localize CLI help and parse errors via OrthoConfig localizer hooks.
  - Depends on: 1.4.2.
  - Success criteria:
    - [ ] CLI help output uses `Command::localize(&localizer)` with a Fluent
          localizer implementation.
    - [ ] `clap` parsing failures are rendered via
          `localize_clap_error_with_command(..)`.
    - [ ] Binary falls back to `NoOpLocalizer` when localization assets fail to
          load.
  - Design references:
    [Localization architecture](spycatcher-harness-design.md#localization-architecture),
    [CLI shape](spycatcher-harness-design.md#cli-shape).

## 2. Streaming fidelity and cassette verification

### 2.1. OpenAI and OpenRouter streaming support

- [ ] 2.1.1. Add SSE parser and recorder for OpenAI-style `data:` streams.
  - Depends on: 1.3.1.
  - Success criteria:
    - [ ] Recorder captures parsed stream events and raw transcript bytes for
          every streamed interaction.
    - [ ] Parser handles completion chunks, usage-including final chunks, and
          end markers.
    - [ ] Unit tests cover fragmented frame boundaries and malformed event
          handling.
  - Design references:
    [Streaming capture and replay](spycatcher-harness-design.md#streaming-capture-and-replay).

- [ ] 2.1.2. Preserve OpenRouter comment frames and replay deterministically.
  - Depends on: 2.1.1.
  - Success criteria:
    - [ ] Comment frames are recorded as typed stream events.
    - [ ] Matching logic ignores comment frames when configured for canonical
          matching.
    - [ ] Replay can emit recorded comment frames without changing non-comment
          event ordering.
  - Design references:
    [Streaming capture and replay](spycatcher-harness-design.md#streaming-capture-and-replay),
    [Known risks and limitations](spycatcher-harness-design.md#known-risks-and-limitations).

- [ ] 2.1.3. Implement byte-faithful SSE replay mode.
  - Depends on: 2.1.1.
  - Success criteria:
    - [ ] Replay supports parsed-event mode and raw-transcript mode selected by
          configuration.
    - [ ] Raw-transcript mode preserves event boundaries needed by SSE clients.
    - [ ] Integration tests validate compatibility with representative streaming
          clients.
  - Design references:
    [Streaming capture and replay](spycatcher-harness-design.md#streaming-capture-and-replay),
    [Known risks and limitations](spycatcher-harness-design.md#known-risks-and-limitations).

### 2.2. Verification and diagnostics workflow

- [ ] 2.2.1. Implement `verify` subcommand for cassette integrity and redaction.
  - Depends on: 1.2.1, 1.2.2.
  - Success criteria:
    - [ ] `verify` checks schema version, ordering, hash recomputation, and
          required redaction policies.
    - [ ] Verification failures provide machine-readable and human-readable
          output suitable for CI annotations.
    - [ ] Verification exits non-zero on any failure class.
  - Design references:
    [CLI shape](spycatcher-harness-design.md#cli-shape),
    [Recommended additions for regression suite integration](spycatcher-harness-design.md#recommended-additions-for-regression-suite-integration).

- [ ] 2.2.2. Add structured mismatch reports for CI consumption.
  - Depends on: 1.2.3, 2.2.1.
  - Success criteria:
    - [ ] Mismatch reports include interaction index, canonical diff summary,
          and ignored-field indicators.
    - [ ] JSON report schema is documented and validated in tests.
    - [ ] Replay command can output mismatch reports to file without changing
          HTTP response semantics.
  - Design references:
    [Matching modes](spycatcher-harness-design.md#matching-modes),
    [Public library API surface](spycatcher-harness-design.md#public-library-api-surface).

## 3. Replay realism and operational visibility

### 3.1. Native replay physics

- [ ] 3.1.1. Implement deterministic timing controls for replay.
  - Depends on: 2.1.3.
  - Success criteria:
    - [ ] Configuration supports time-to-first-token (TTFT) delay and
          inter-chunk timing controls.
    - [ ] CI preset produces repeatable timings with bounded variance proven by
          test assertions.
    - [ ] Realistic preset supports jitter ranges without violating event order.
  - Design references:
    [Streaming capture and replay](spycatcher-harness-design.md#streaming-capture-and-replay),
    [Roadmap tasks](spycatcher-harness-design.md#roadmap-tasks).

### 3.2. Observability and run diagnostics

- [ ] 3.2.1. Add structured logs for record and replay interactions.
  - Depends on: 1.3.2, 2.2.2.
  - Success criteria:
    - [ ] Logs include interaction ID, mode, protocol, upstream latency, and
          mismatch outcomes.
    - [ ] Log schema is documented with sample lines for successful and failed
          replays.
    - [ ] Integration tests assert key log fields are present for failure paths.
  - Design references:
    [Observability](spycatcher-harness-design.md#observability).

- [ ] 3.2.2. Expose replay and mismatch metrics.
  - Depends on: 3.2.1.
  - Success criteria:
    - [ ] Metrics include request totals, mismatch totals, and recorded/replayed
          interaction counters by protocol and cassette.
    - [ ] Metrics endpoint behaviour is tested in both record and replay modes.
    - [ ] Metric names and labels match the documented contract.
  - Design references:
    [Observability](spycatcher-harness-design.md#observability).

### 3.3. Optional VidaiMock realism backend

- [ ] 3.3.1. Add optional VidaiMock subprocess backend driver.
  - Depends on: 3.1.1, 3.2.1.
  - Success criteria:
    - [ ] Harness can start and stop VidaiMock as a managed subprocess during
          replay.
    - [ ] Harness maps replay physics and chaos settings to supported VidaiMock
          controls.
    - [ ] Replay falls back cleanly to native mode when VidaiMock is
          unavailable.
  - Design references:
    [VidaiMock and recording tooling](spycatcher-harness-design.md#vidaimock-and-recording-tooling),
    [Roadmap tasks](spycatcher-harness-design.md#roadmap-tasks).

- [ ] 3.3.2. Implement VidaiMock export once fixture schema is authoritative.
  - Depends on: 3.3.1.
  - Success criteria:
    - [ ] Export command is feature-gated behind schema version support.
    - [ ] Exported fixtures validate against the confirmed schema.
    - [ ] Export command emits clear unsupported-schema errors when mapping is
          incomplete.
  - Design references:
    [VidaiMock and recording tooling](spycatcher-harness-design.md#vidaimock-and-recording-tooling),
    [Known risks and limitations](spycatcher-harness-design.md#known-risks-and-limitations).

## 4. Multi-protocol expansion

### 4.1. OpenAI responses support

- [ ] 4.1.1. Add `POST /v1/responses` routing and adapter implementation.
  - Depends on: 2.1.3, 2.2.1.
  - Success criteria:
    - [ ] Recorder persists typed response events including
          `response.created`, text delta events, completion events, and errors.
    - [ ] Replayer preserves event ordering and final completion state.
    - [ ] Contract tests verify endpoint compatibility for non-stream and stream
          response forms.
  - Design references:
    [OpenAI Responses streaming](spycatcher-harness-design.md#openai-responses-streaming),
    [Architecture overview](spycatcher-harness-design.md#architecture-overview).

### 4.2. Anthropic messages support

- [ ] 4.2.1. Add `POST /v1/messages` routing and Anthropic SSE adapter.
  - Depends on: 4.1.1.
  - Success criteria:
    - [ ] Recorder stores `event:` name plus payload for Anthropic stream
          events.
    - [ ] Replayer preserves canonical Anthropic event flow and allows unknown
          event types to pass through unchanged.
    - [ ] Adapter tests cover `message_start`, content block events,
          `message_delta`, `message_stop`, `ping`, and `error` handling.
  - Design references:
    [Anthropic Messages streaming](spycatcher-harness-design.md#anthropic-messages-streaming),
    [Architecture overview](spycatcher-harness-design.md#architecture-overview).

### 4.3. DeepSeek compatibility preset

- [ ] 4.3.1. Add DeepSeek OpenAI-compatible upstream preset.
  - Depends on: 1.1.2, 1.3.2.
  - Success criteria:
    - [ ] Configuration preset resolves DeepSeek base URL and model mapping
          defaults without custom adapter code.
    - [ ] Record and replay flows pass contract tests against
          DeepSeek-compatible
          fixtures.
    - [ ] Documentation explains preset limitations and override points.
  - Design references:
    [DeepSeek compatibility](spycatcher-harness-design.md#deepseek-compatibility).

### 4.4. Optional WireMock export path

- [ ] 4.4.1. Add optional WireMock export for teams requiring existing tooling.
  - Depends on: 2.2.1.
  - Success criteria:
    - [ ] Export command converts cassette interactions into WireMock mappings
          without changing native cassette format.
    - [ ] Export includes metadata that links each mapping to interaction IDs.
    - [ ] Documentation states limitations for protocol-aware streaming replay.
  - Design references:
    [VidaiMock and recording tooling](spycatcher-harness-design.md#vidaimock-and-recording-tooling),
    [Known risks and limitations](spycatcher-harness-design.md#known-risks-and-limitations).

## Dependency checkpoints

- Tasks 1.4.1 to 1.4.3 should complete before shipping user-facing CLI workflows
  that surface localized messages.
- Completion of phase 2 is required before phase 4 protocol expansion tasks are
  started in parallel.
- Task 3.3.2 is blocked until authoritative VidaiMock fixture schema
  documentation is available.
- Task 4.4.1 remains optional and should not block baseline harness delivery.
