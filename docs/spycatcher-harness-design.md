# Spycatcher harness design

## Context and problem statement

End-to-end (E2E) tests that exercise agent flows against real Large Language
Model (LLM) APIs routinely fail repeatability requirements. Even with
“deterministic” settings, upstream providers can vary outputs due to routing,
capacity, and implementation detail. OpenRouter explicitly supports
model/provider normalization and fallbacks, and supports streaming over
Server-Sent Events (SSE), including comment payloads and mid-stream error
signalling. [^1][^2][^3]

The required capability is a harness that captures a complete agent↔LLM session
once (recording against OpenRouter, free or paid models), then replays it as
part of a regression suite with strong repeatability guarantees and failure
diagnostics. The harness must:

- Expose an OpenAI Chat Completions-compatible HTTP endpoint first
  (`/v1/chat/completions` semantics). [^4]
- Extend to OpenAI Responses, Anthropic-compatible Messages streaming, and
  DeepSeek-compatible APIs over time. [^5][^6][^7]
- Provide a Rust library API and a CLI.
- Load configuration via OrthoConfig’s layered precedence model (CLI > env >
  config files > defaults), including subcommand configuration merging.
  [^8][^9][^10]
- Integrate with VidaiMock for replay realism where it improves test coverage
  (latency/time-to-first-token (TTFT)/jitter, chaos primitives), while keeping
  an early shippable vertical slice that does not depend on undocumented
  VidaiMock fixture formats. VidaiMock advertises OpenAI- and
  Anthropic-compatible endpoints, streaming simulation, and chaos injection via
  headers, and that it runs fully offline/stateless. [^11]

WireMock (or equivalent) can record/play back HTTP interactions by proxying and
producing stub mappings, but the harness should only rely on this class of tool
if VidaiMock or the harness itself cannot provide recording that is fit for LLM
SSE and multi-protocol evolution. [^12][^13]

## Goals and non-goals

### Goals

- Capture and replay LLM API interactions with deterministic matching and
  diagnostics.
- Support streaming capture and replay for OpenRouter/OpenAI-style SSE
  (including comment payloads), because agents frequently stream partial
  output. OpenRouter documents SSE streaming and its comment payloads. [^2][^1]
- Provide two operational modes:
  - **Record**: proxy upstream (OpenRouter) and persist a “cassette”.
  - **Replay**: serve deterministic responses from the cassette, without
    external network.
- Make configuration auditable and reproducible using OrthoConfig:
  - Use `OrthoConfig::load()` precedence rules.
  - Use subcommand config merges for `record`, `replay`, `export`, `verify`.
    [^8][^9]
- Enable progressive enhancement:
  - Initial endpoint: OpenAI Chat Completions-compatible.
  - Later: OpenAI Responses streaming events, Anthropic Messages
    (`/v1/messages`) streaming, and DeepSeek (OpenAI-compatible).
    [^5][^6][^7]
- Provide a stable on-disk format for recorded sessions with explicit
  versioning and forward-compatibility.

### Non-goals

- Attempt to “make the model deterministic” during recording (temperature
  forcing, seed injection) across all vendors. The harness captures reality and
  replays it.
- Mock external tool/service calls beyond the LLM API boundary. Those belong to
  separate fixtures/mocks.
- Require VidaiMock internals/fixture schemas in the first deployable slice;
  VidaiMock integration should be additive.

## Architecture overview

The harness is an HTTP server plus a cassette store, with a protocol adapter
layer. In record mode it proxies to OpenRouter’s OpenAI-compatible API base
(`/api/v1/chat/completions`) and records the full request/response exchange.
OpenRouter documents its OpenAI-like request/response schema and that streaming
is SSE with occasional comment payloads. [^1][^2]

The same server runs in replay mode and answers from the cassette. VidaiMock
can be used as an optional replay backend to simulate time-to-first-token
(TTFT), jitter, and chaos failure modes that it explicitly advertises. [^11]

A short diagram description follows. The diagram shows the record/replay data
flow and the adapter boundary.

```mermaid
flowchart LR
  Agent[Agent / system under test] -->|HTTP: OpenAI-compatible| Harness

  subgraph Harness[Spycatcher harness]
    Router[Protocol router]
    Adapter["Protocol adapter<br/>OpenAI chat, later Responses Anthropic DeepSeek"]
    Store["Cassette store<br/>versioned, immutable"]
    Upstream["Upstream client<br/>OpenRouter in record mode"]
    Router --> Adapter
    Adapter --> Store
    Adapter --> Upstream
  end

  Harness -->|HTTP record mode only| OpenRouter[OpenRouter upstream]

  subgraph ReplayBackends[Replay backends]
    Native["Native replay engine<br/>bytes and SSE"]
    VidaiMockBackend["Optional VidaiMock backend<br/>physics and chaos"]
  end

  Adapter --> Native
  Adapter --> VidaiMockBackend
```

_Figure 1: Architecture flow showing request routing, protocol adaptation, and
record/replay backend selection in the Spycatcher harness._

Key architectural points:

- **Protocol router** mounts HTTP routes for supported APIs:
  - Initial: `POST /v1/chat/completions` (OpenAI Chat Completions-compatible).
    [^4]
  - Later: `POST /v1/responses` (OpenAI Responses API). [^5]
  - Later: `POST /v1/messages` (Anthropic Messages) with SSE event types.
    [^6]
  - Later: DeepSeek Chat Completions (OpenAI-compatible). [^7]
- **Protocol adapter** provides:
  - Request canonicalization and matching keys.
  - Streaming parsers/emitters appropriate to each protocol.
- **Cassette store**:
  - Append-only in record mode.
  - Read-only in replay mode.
  - Supports strict sequential replay and keyed replay modes.
- **Upstream client**:
  - Targets OpenRouter’s base URL and endpoints.
  - Adds OpenRouter optional attribution headers if configured (`HTTP-Referer`,
    `X-Title`). [^14][^15]
- **Replay backend selection**:
  - Native replay: earliest slice, no external dependency.
  - VidaiMock replay: optional enhancement to simulate streaming physics and
    chaos (because VidaiMock advertises both). [^11]

### Record and replay interaction sequence

For screen readers: The following sequence diagram shows end-to-end request
handling in both record mode and replay mode for `POST /v1/chat/completions`.

```mermaid
sequenceDiagram
  actor Agent
  participant Harness as Harness_server
  participant Router as Protocol_router
  participant Adapter as Protocol_adapter
  participant Store as Cassette_store
  participant Upstream as OpenRouter_upstream

  rect rgb(230,230,255)
    note over Agent,Upstream: Record_mode
    Agent->>Harness: POST /v1/chat/completions
    Harness->>Router: route_request
    Router->>Adapter: handle_request_record
    Adapter->>Store: canonicalize_and_hash_request
    Adapter->>Upstream: forward_http_request
    Upstream-->>Adapter: stream_or_nonstream_response
    Adapter->>Store: append_interaction_to_cassette
    Adapter-->>Agent: proxy_response_stream_or_body
  end

  rect rgb(230,255,230)
    note over Agent,Store: Replay_mode
    Agent->>Harness: POST /v1/chat/completions
    Harness->>Router: route_request
    Router->>Adapter: handle_request_replay
    Adapter->>Store: lookup_interaction_by_mode
    Store-->>Adapter: matched_interaction
    Adapter-->>Agent: serve_recorded_response
  end
```

_Figure 2: Record and replay sequence for chat completions requests through the
Spycatcher harness._

## Recording and replay semantics

### Cassette definition

A cassette is a single recorded agent session, consisting of an ordered list of
interactions. Each interaction contains:

- Request:
  - Method, path, query.
  - Selected headers (excluding secrets).
  - Body bytes and parsed JSON (when applicable).
  - Canonical request representation and a stable hash.
- Response:
  - Status, selected headers.
  - Either:
    - Non-stream response body bytes, or
    - A stream transcript (SSE events as parsed units + optional timing).
- Metadata:
  - Protocol identifier (e.g., `openai.chat_completions.v1`).
  - Upstream identifier (`openrouter`).
  - Timestamps (recorded and relative offsets).

### Matching modes

Two matching modes enable early delivery while covering common regression-suite
needs:

- **Sequential strict mode (default)**:
  - Replay expects the next incoming request to match the next recorded
    interaction.
  - Any mismatch fails fast with a diagnostic response (409) containing:
    - Expected interaction ID.
    - Observed request hash.
    - A diff summary of canonical request JSON (field-level).
  - This mode maximizes repeatability and debugging speed for deterministic,
    single-threaded agent loops.

- **Keyed mode (optional)**:
  - Replay matches by request hash and consumes the next unused interaction
    with that hash.
  - Supports limited reordering and concurrent requests, at the cost of less
    precise failure locations.

### Canonicalization and hashing

Canonicalization normalizes inputs so that stable matching does not depend on:

- JSON key ordering.
- Insignificant whitespace.
- Runtime-generated metadata fields, when configured to ignore them.

The canonicalization pipeline should be explicit and configurable per protocol
adapter. For OpenAI Chat Completions, the request body schema includes fields
like `stream` and `stream_options`, which materially affect response shape, so
those fields must participate in the canonical form. [^4]

Recommended approach:

- Canonicalize query parameters into a stable representation:
  - Parse query pairs from the URL.
  - Sort by key, then value, preserving repeated keys.
  - Re-encode in a consistent form for hashing.
- Parse JSON into a `serde_json::Value`.
- Apply a protocol-specific “normalization pass”:
  - Drop configured paths (example: `metadata.run_id`).
  - Optionally coerce numeric types (avoid `1` vs `1.0` drift).
- Serialize with a canonical serializer (sorted object keys, stable float
  formatting).
- Hash (SHA-256) over:
  `method + path + canonical_query + canonical_json`.

### Streaming capture and replay

#### OpenRouter / OpenAI Chat Completions streaming

OpenRouter supports SSE streaming for Chat Completions; its documentation notes
that comment payloads may be sent (e.g., `: OPENROUTER PROCESSING`) and should
be ignored per SSE rules. [^2]

OpenAI’s Chat Completions API streams “chat completion chunk” objects via SSE
when `stream: true`, and supports `stream_options.include_usage` producing an
additional final chunk with usage. [^4]

Recording strategy for OpenAI-style SSE:

- Implement a streaming proxy:
  - Read upstream bytes incrementally.
  - Parse SSE frames into event records:
    - `comment` frames (leading `:`) recorded as `comment` type.
    - `data:` frames captured as raw string and, when JSON, parsed into `Value`.
  - Forward frames to the downstream client as bytes _without re-chunking_
    where possible (preserves client edge cases).
- Store both:
  - The parsed event list (for deterministic replay), and
  - The raw byte transcript (for fidelity/debugging).

Replay strategy:

- Emit SSE frames from the recorded transcript.
- Provide a configuration flag to apply “physics” timing:
  - TTFT delay before first token.
  - Inter-chunk spacing (fixed or recorded).
- When VidaiMock is used as a replay backend, prefer VidaiMock’s existing
  streaming physics/chaos semantics where feasible, because those behaviours
  are explicitly a VidaiMock feature. [^11]

#### Anthropic Messages streaming

Anthropic streaming uses SSE with explicit `event:` names and a defined event
flow (`message_start`, content block events, `message_delta`, `message_stop`),
and may include `ping` and `error` events. [^6]

Adapter implications:

- SSE parser must support `event:` and `data:` framing.
- Recorder must store event names and JSON payload.
- Replayer must preserve event ordering and allow unknown event types to pass
  through unchanged, because Anthropic may add new event types.

#### OpenAI Responses streaming

OpenAI Responses streaming emits a set of typed events such as
`response.created`, `response.output_text.delta`, `response.completed`, and
`error`. [^5][^16]

Adapter implications:

- Recorder should store the event `type` field and sequence ordering.
- Replayer should preserve:
  - Event ordering,
  - Event payload content exactly,
  - The final “completed/failed/incomplete” event semantics.

#### DeepSeek compatibility

DeepSeek documents that its API format is compatible with OpenAI, and that
clients can use `https://api.deepseek.com/v1` as an OpenAI-compatible base URL.
[^7]

Compatibility implications:

- The “DeepSeek adapter” may be a thin configuration preset over the OpenAI
  Chat Completions adapter.
- Recording against DeepSeek in future should only require:
  - Upstream base URL and API key,
  - Potentially model name mapping.

### VidaiMock and recording tooling

VidaiMock’s public product description emphasizes offline mocking,
provider-compatible endpoints, streaming physics, and chaos injection via
headers, but does not describe an HTTP recording feature. [^11]

Therefore, the early design assumes recording must be implemented by the
harness itself. WireMock is a proven recording/proxy tool (record/snapshot via
proxying), but it introduces a JVM runtime and produces generic HTTP stub
mappings, which are typically insufficient for protocol-aware SSE replay and
multi-protocol evolution without additional transformation. [^12][^13]

A practical compromise:

- Implement native recording/replay in Rust as the primary path.
- Provide an optional “WireMock export” later if compatibility with existing
  teams/tooling is needed.

## Rust API and module boundaries

### Crate layout

- `spycatcher_harness` (library)
  - `config`: configuration structs and OrthoConfig integration
  - `protocol`: protocol adapter traits and implementations
  - `cassette`: cassette schema, canonicalization, hashing, store trait
  - `server`: HTTP server wiring (Axum/Hyper)
  - `upstream`: OpenRouter/OpenAI/Anthropic/DeepSeek clients
  - `replay`: native replay engine; optional VidaiMock backend driver
- `spycatcher-harness` (binary)
  - CLI definitions (Clap)
  - Delegates to library

### Configuration via OrthoConfig

OrthoConfig provides a `load()` method that loads configuration using
precedence rules where command-line arguments have the highest precedence,
environment variables next, then configuration files, with default attribute
values at the lowest. [^8]

OrthoConfig also supports subcommand configuration merging
(`load_and_merge_subcommand_for` / `SubcmdConfigMerge`) that reads per-command
defaults from configuration under a `cmds` namespace and merges them beneath
CLI args. [^9]

File format support notes:

- TOML parsing is enabled by default, and JSON5/YAML can be enabled by feature
  flags. [^10]

### Core traits and types

```rust,no_run
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct HarnessConfig {
    pub listen: ListenAddr,
    pub mode: Mode,

    pub protocol: Protocol,         // openai_chat_completions initially
    pub match_mode: MatchMode,      // sequential_strict default
    pub cassette_dir: PathBuf,
    pub cassette_name: String,

    pub upstream: Option<UpstreamConfig>, // required for record mode
    pub redaction: RedactionConfig,

    pub replay: ReplayConfig,       // timing, strictness, vv
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpstreamConfig {
    pub kind: UpstreamKind,         // openrouter initially
    pub base_url: String,           // e.g. https://openrouter.ai/api/v1
    pub api_key_env: String,        // env var name, not the key itself
    pub extra_headers: BTreeMap<String, String>, // header name -> value
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum Mode { Record, Replay }

pub trait ProtocolAdapter: Send + Sync {
    fn protocol_id(&self) -> &'static str;

    fn canonical_request(&self, req: &HttpRequest) -> CanonicalRequest;
    fn parse_stream(&self, bytes: &[u8]) -> Vec<StreamEvent>; // protocol-specific

    fn build_response(&self, interaction: &Interaction) -> HttpResponse;
    fn build_stream(&self, interaction: &Interaction) -> StreamEmitter;
}
```

### Core type and module relationships

For screen readers: The following class diagram summarizes the main
configuration types, protocol interfaces, runtime modules, and their
dependencies.

```mermaid
classDiagram
  class HarnessConfig {
    +ListenAddr listen
    +Mode mode
    +Protocol protocol
    +MatchMode match_mode
    +PathBuf cassette_dir
    +String cassette_name
    +Option~UpstreamConfig~ upstream
    +RedactionConfig redaction
    +ReplayConfig replay
  }

  class UpstreamConfig {
    +UpstreamKind kind
    +String base_url
    +String api_key_env
    +BTreeMap~String,String~ extra_headers
  }

  class Mode {
    <<enum>>
    Record
    Replay
  }

  class ProtocolAdapter {
    <<interface>>
    +protocol_id() str
    +canonical_request(req HttpRequest) CanonicalRequest
    +parse_stream(bytes u8[]) Vec~StreamEvent~
    +build_response(interaction Interaction) HttpResponse
    +build_stream(interaction Interaction) StreamEmitter
  }

  class CassetteStore {
    <<interface>>
    +append(interaction Interaction) Result
    +load(path PathBuf) Result~Cassette~
    +lookup_sequential(hash String) Option~Interaction~
    +lookup_keyed(hash String) Option~Interaction~
  }

  class RunningHarness {
    +SocketAddr addr
    +PathBuf cassette_path
    +shutdown() Result
  }

  class ServerModule {
    <<module>>
    +start_harness(cfg HarnessConfig) Result~RunningHarness~
  }

  class ConfigModule {
    <<module>>
    +load() Result~HarnessConfig~
    +load_and_merge_subcommand_for(name String) Result~HarnessConfig~
  }

  class ReplayEngine {
    <<component>>
    +replay_interaction(interaction Interaction, adapter ProtocolAdapter) HttpResponse
    +replay_stream(interaction Interaction, adapter ProtocolAdapter, timing ReplayTiming) StreamEmitter
  }

  class UpstreamClient {
    <<component>>
    +send_request(req HttpRequest, cfg UpstreamConfig) Result~HttpResponse~
  }

  class CliBinary {
    <<binary>>
    +main(args String[]) void
  }

  HarnessConfig --> UpstreamConfig : uses
  HarnessConfig --> Mode : uses

  ServerModule ..> HarnessConfig : config_input
  ServerModule ..> RunningHarness : returns

  ConfigModule ..> HarnessConfig : constructs

  RunningHarness ..> CassetteStore : owns

  ReplayEngine ..> CassetteStore : reads
  ReplayEngine ..> ProtocolAdapter : calls

  UpstreamClient ..> UpstreamConfig : uses
  UpstreamClient ..> ProtocolAdapter : cooperates

  CliBinary ..> ConfigModule : loads_config
  CliBinary ..> ServerModule : starts_server
```

_Figure 3: Core type and module relationships for configuration, request
handling, replay execution, and CLI orchestration._

### Public library API surface

The library API should allow both:

- CLI-driven execution, and
- Test harness embedding (spawn server on ephemeral port, run scenario, extract
  diagnostics).

```rust,no_run
use std::net::SocketAddr;
use thiserror::Error;

pub type HarnessResult<T> = std::result::Result<T, HarnessError>;

#[derive(Debug, Error)]
pub enum HarnessError {
    #[error("invalid configuration: {message}")]
    InvalidConfig { message: String },
    #[error("cassette not found: {cassette_name}")]
    CassetteNotFound { cassette_name: String },
    #[error("request mismatch at interaction {interaction_id}")]
    RequestMismatch { interaction_id: usize },
    #[error("upstream request failed")]
    UpstreamRequestFailed,
    #[error("io failure")]
    Io,
}

pub struct RunningHarness {
    pub addr: SocketAddr,
    pub cassette_path: std::path::PathBuf,
}

impl RunningHarness {
    pub async fn shutdown(self) -> HarnessResult<()>;
}

pub async fn start_harness(cfg: HarnessConfig) -> HarnessResult<RunningHarness>;
```

Library-facing APIs should return typed error enums, so callers can branch on
retryability, map failures to HTTP status codes, and preserve semantic handling
without string matching. Opaque reports (`eyre::Report` or `anyhow::Error`)
should be reserved for the CLI binary entrypoint and other app-layer boundaries.

Recommended additions for regression suite integration:

- `CassetteVerifier`:
  - Verifies cassette format version.
  - Verifies no secrets leaked (headers scrubbed).
  - Verifies sequential integrity.
- `MismatchReport`:
  - Structured diff output for CI annotations.

## CLI integration and configuration

### CLI shape

Subcommands map directly to vertical-slice deliverables:

- `record`: run proxy server, write cassette
- `replay`: run replay server from cassette
- `verify`: validate cassette integrity and redaction
- `export vidaimock`: generate VidaiMock-compatible fixtures (introduced once
  VidaiMock schema is confirmed)
- `export wiremock`: optional later

Subcommand-specific config merging should be enabled via OrthoConfig to support
per-command defaults in config files. [^9]

### Example CLI usage

```bash
# Record a session by running a local OpenAI-compatible endpoint.
spycatcher-harness record \
  --listen 127.0.0.1:8787 \
  --cassette-name podbot_smoke_001 \
  --upstream.kind openrouter \
  --upstream.base-url https://openrouter.ai/api/v1 \
  --upstream.api-key-env OPENROUTER_API_KEY
```

```bash
# Replay the recorded session without network access.
spycatcher-harness replay \
  --listen 127.0.0.1:8787 \
  --cassette-name podbot_smoke_001
```

### Example configuration file

TOML is the default supported file format for OrthoConfig. [^10][^8]

In replay configuration, `ttft_ms` is the time-to-first-token (TTFT) delay in
milliseconds, and `tps` is tokens per second (TPS). In upstream configuration,
`extra_headers` is a map of HTTP header names to header values. This uses TOML
table syntax so it deserializes directly into `BTreeMap<String, String>` in
`UpstreamConfig`.

```toml
listen = "127.0.0.1:8787"
mode = "replay"
protocol = "openai_chat_completions"
match_mode = "sequential_strict"

cassette_dir = "fixtures/llm"
cassette_name = "podbot_smoke_001"

[redaction]
drop_headers = ["authorization", "x-api-key"]

[replay]
simulate_timing = true
ttft_ms = 20
tps = 200

[cmds.record.upstream]
kind = "openrouter"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "OPENROUTER_API_KEY"

# Optional OpenRouter attribution headers.
# OpenRouter documents HTTP-Referer and X-Title as optional. [^14][^15]
[cmds.record.upstream.extra_headers]
"HTTP-Referer" = "https://example.invalid"
"X-Title" = "CI Regression Harness"
```

## Testing, observability, and rollout roadmap

### Test strategy

- Unit tests:
  - Canonicalization stability (JSON key order, whitespace).
  - Hash stability for known request fixtures.
  - SSE parser correctness for:
    - OpenAI-style `data:` frames and end markers.
    - OpenRouter comment frames (leading `:`). [^2][^4]
    - Anthropic `event:` + `data:` frames and event flow ordering.
      [^6]
- Integration tests:
  - Full record→replay cycle with a stub upstream server (no real OpenRouter
    calls).
  - Mismatch diagnostics content and exit codes.
- Contract tests:
  - Ensure response shapes remain OpenAI-compatible for Chat Completions.
    [^4]

### Observability

VidaiMock advertises built-in Prometheus metrics and request tracing for
simulation runs. [^11]

The harness should provide, at minimum:

- Structured logs:
  - Interaction index/ID.
  - Record/replay mode.
  - Upstream latency and stream duration (record mode).
  - Mismatches (expected vs observed hashes).
- Metrics (Prometheus optional):
  - `harness_requests_total{mode,protocol}`
  - `harness_mismatches_total{protocol}`
  - `harness_recorded_interactions_total{cassette}`
  - `harness_replayed_interactions_total{cassette}`

### Roadmap tasks

The following tasks focus on early, deployable slices and measurable outcomes,
avoiding time commitments.

#### 1.1. OpenAI Chat Completions record and replay

- [ ] 1.1.1. Implement HTTP server for `POST /v1/chat/completions` (non-stream).
  - [ ] Return recorded JSON response bytes verbatim during replay.
  - [ ] Store cassette as `cassette.json` with `format_version` and ordered
        interactions.
  - [ ] Add strict sequential matching with request hash and diff summary on
        mismatch.
- [ ] 1.1.2. Add streaming proxy/recorder for OpenAI-style SSE.
  - [ ] Parse and record `data:` frames and preserve raw transcript.
  - [ ] Replay recorded SSE frames deterministically.
- [ ] 1.1.3. Add OpenRouter streaming comment handling.
  - [ ] Ignore comment frames for canonical replay matching.
  - [ ] Optionally emit recorded comment frames during replay to preserve
        realism. [^2]

#### 1.2. Configuration and ergonomics

- [ ] 1.2.1. Integrate OrthoConfig `load()` into both library and CLI with
      documented precedence.
  - [ ] CLI overrides env and config file values by construction.
        [^8]
- [ ] 1.2.2. Implement subcommand config merging (`cmds` namespace).
  - [ ] `cmds.record.*` defaults merged beneath CLI flags for record mode.
        [^9]
- [ ] 1.2.3. Add `verify` subcommand.
  - [ ] Validate cassette version, ordering, and that redaction rules removed
        secrets.

#### 1.3. VidaiMock integration for replay realism

- [ ] 1.3.1. Implement “native physics” replay controls (TTFT/TPS/jitter).
  - [ ] Provide deterministic timing presets for CI and “realistic” presets for
        resilience tests.
- [ ] 1.3.2. Add optional VidaiMock backend driver.
  - [ ] Start VidaiMock as a subprocess and configure chaos/physics via
        supported mechanisms (headers/env/config), using VidaiMock’s advertised
        primitives. [^11]
  - [ ] Export deterministic fixtures into the VidaiMock format once the schema
        is confirmed.

#### 1.4. Multi-protocol support

- [ ] 1.4.1. Add OpenAI Responses endpoint support (`POST /v1/responses`).
  - [ ] Record and replay typed streaming events (`response.created`,
        `response.output_text.delta`, `response.completed`, `error`).
        [^5][^16]
- [ ] 1.4.2. Add Anthropic Messages endpoint support (`POST /v1/messages`).
  - [ ] Record and replay SSE with `event:` names and content block events.
        [^6]
- [ ] 1.4.3. Add DeepSeek compatibility presets.
  - [ ] Support base URL presets (`https://api.deepseek.com/v1`) and model IDs
        as configuration. [^7]

## Known risks and limitations

- **Private VidaiMock fixture schema uncertainty**: VidaiMock advertises
  powerful simulation features and provider compatibility but its public
  description does not specify a recording feature or an on-disk fixture
  schema. [^11] Mitigation: ship with native recording/replay first; add
  VidaiMock export/backend once the schema is confirmed from authoritative
  documentation.
- **Streaming fidelity edge cases**: SSE proxying can break clients if frame
  boundaries or headers differ. OpenRouter’s comment frames and mid-stream
  error reporting increase edge cases. [^2][^3] Mitigation: record raw byte
  transcript and replay it verbatim as an option; test against representative
  SSE clients.
- **Request drift due to metadata**: Agents may include run IDs or timestamps
  in `metadata`, breaking strict matching. Mitigation: configurable
  normalization rules with explicit ignored JSON paths and clear mismatch
  diagnostics showing ignored vs compared fields.
- **Concurrency in agents**: Parallel LLM calls can defeat sequential strict
  mode. Mitigation: keyed matching mode and per-request “interaction group”
  tags; document limitations and recommend deterministic single-threaded test
  profiles for core regressions.
- **WireMock dependency pressure**: WireMock record/playback works well for
  generic HTTP but is not tailored for LLM streaming protocols, and adds
  operational complexity. [^12][^13] Mitigation: keep WireMock integration
  optional and export-only; avoid requiring it for baseline harness operation.

## Source references

[^1]: `turn2search0` OpenRouter API behaviour and model/provider compatibility
      overview: <https://openrouter.ai/docs/api-reference/overview>.
[^2]: `turn2search1` OpenRouter Server-Sent Events (SSE) streaming behaviour
      and framing guidance:
      <https://openrouter.ai/docs/api-reference/streaming>.
[^3]: `turn2search10` OpenRouter error signalling and error handling guidance:
      <https://openrouter.ai/docs/api-reference/errors>.
[^4]: `turn7search1` OpenAI Chat Completions API reference:
      <https://platform.openai.com/docs/api-reference/chat/create>.
[^5]: `turn7search0` OpenAI Responses API reference:
      <https://platform.openai.com/docs/api-reference/responses>.
[^6]: `turn3search10` Anthropic Messages API streaming reference:
      <https://docs.anthropic.com/en/api/messages-streaming>.
[^7]: `turn3search0` DeepSeek OpenAI-compatible API guidance:
      <https://api-docs.deepseek.com/guides/openai_sdk>.
[^8]: `turn4search1` OrthoConfig loading and precedence documentation:
      <https://docs.rs/ortho_config/latest/ortho_config/>.
[^9]: `turn4search9` OrthoConfig subcommand merge documentation
      (`SubcmdConfigMerge`):
      <https://docs.rs/ortho_config/latest/ortho_config/trait.SubcmdConfigMerge.html>.
[^10]: `turn4search3` OrthoConfig feature flags and file-format support:
       <https://docs.rs/crate/ortho_config/latest/features>.
[^11]: `turn2search7` VidaiMock capability documentation for offline
       provider-compatible mocks, streaming simulation, and chaos primitives
       (source URL not retained in this repository).
[^12]: `turn0search0` WireMock record and playback documentation:
       <https://wiremock.org/docs/record-playback/>.
[^13]: `turn0search1` WireMock proxying documentation:
       <https://wiremock.org/docs/proxying/>.
[^14]: `turn2search2` OpenRouter optional attribution headers (`HTTP-Referer`,
       `X-Title`) documentation (source URL not retained in this repository).
[^15]: `turn0search3` OpenRouter optional attribution headers (`HTTP-Referer`,
       `X-Title`) documentation (source URL not retained in this repository).
[^16]: `turn7search4` OpenAI Responses streaming event model documentation:
       <https://platform.openai.com/docs/api-reference/responses-streaming>.
