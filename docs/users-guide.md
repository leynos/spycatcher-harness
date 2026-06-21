# Spycatcher harness user's guide

This guide documents the public API surface and usage patterns for the
Spycatcher harness. The harness records LLM API interactions for deterministic
regression testing. In record mode, both non-streaming and streaming
(`"stream": true`) Chat Completions requests are proxied upstream and persisted
to cassette. Replay mode serves matching non-stream responses and matching
recorded Chat Completions SSE streams, including OpenRouter comment frames.
Verify is currently a CLI-only command for cassette and configuration checks.

> **Breaking changes:** record-mode proxying changed raw header handling and
> redaction defaults before the 0.1.0 release. See
> [MIGRATION-0.1.0.md](../MIGRATION-0.1.0.md) before updating cassette
> assertions or proxy-path tests.

## Library API

The `spycatcher_harness` crate exposes two primary entry points for harness
lifecycle management.

### Starting the harness

Call `start_harness` with a `HarnessConfig` to validate configuration and
prepare the harness for operation. In record mode, startup now binds a real
local HTTP listener and returns the actual bound socket address in
`RunningHarness.addr`. In replay mode, startup opens the configured cassette
file read-only, validates its `format_version`, and returns a running harness
that serves the recorded interactions described later in this guide. In verify
mode, library startup returns `HarnessError::ModeNotYetImplemented`; verify
mode is available through the CLI for cassette and configuration checks.

```rust,no_run
use spycatcher_harness::{start_harness, HarnessConfig};

# async fn example() -> spycatcher_harness::HarnessResult<()> {
# Ok(())
# }
```

### Configuration

`HarnessConfig` controls all aspects of harness behaviour. Use struct update
syntax to override specific fields:

```rust
use spycatcher_harness::HarnessConfig;
use spycatcher_harness::config::ListenAddr;

let cfg = HarnessConfig {
    listen: ListenAddr::from(
        "127.0.0.1:9090".parse::<std::net::SocketAddr>().unwrap()
    ),
    ..HarnessConfig::default()
};
```

Configuration fields:

- `listen` — address and port the harness listens on (default:
  `127.0.0.1:8787`).
- `mode` — `Mode::Record` or `Mode::Replay` (default: `Replay`).
- `protocol` — protocol to expose (default: `OpenAiChatCompletions`).
- `match_mode` — replay matching strategy: `SequentialStrict` (default)
  or `Keyed`.
- `cassette_dir` — directory containing cassette files (default:
  `fixtures/llm`).
- `cassette_name` — name of the cassette file (default: `default`).
- `upstream` — upstream provider config (required for record mode).
- `redaction` — header redaction rules (default: drops `authorization`).
  Supply `RedactionConfig { drop_headers: vec![] }` to disable all redaction,
  or extend the default with:

  ```rust
  RedactionConfig {
      drop_headers: vec!["authorization".to_owned(), "x-my-secret".to_owned()],
  }
  ```

- `replay` — timing controls for replay mode.
- `localization` — locale settings.

### Localizing library messages

The library embeds its own Fluent Translation List resources under `i18n/`, but
does not create a process-wide language loader or detect the process locale.
Applications own language negotiation and inject their configured loader when
they need localized library text:

```rust
use i18n_embed::fluent::FluentLanguageLoader;
use spycatcher_harness::HarnessError;
use spycatcher_harness::i18n::{HarnessLocalizations, localize_harness_error};

let fallback = "en-US"
    .parse::<i18n_embed::unic_langid::LanguageIdentifier>()
    .unwrap();
let loader = FluentLanguageLoader::new("spycatcher-harness", fallback.clone());
i18n_embed::select(&loader, &HarnessLocalizations, &[fallback]).unwrap();

let error = HarnessError::InvalidConfig {
    message: "missing upstream".to_owned(),
};
let rendered = localize_harness_error(&loader, &error);

assert_eq!(
    rendered,
    "invalid configuration: \u{2068}missing upstream\u{2069}"
);
```

Successful rendering preserves Fluent's bidirectional isolation marks around
dynamic values. If the supplied loader has not loaded the library resources,
rendering falls back to the existing non-localized `HarnessError` display text.
CLI locale selection and localized `clap` help remain separate
application-level responsibilities. The `spycatcher-harness` binary builds one
language loader at startup from its layered localization configuration and uses
the embedded English catalogue as the default fallback.

#### Security considerations

Fluent argument substitution is named and does not re-parse argument values as
Fluent Translation List syntax, so user-supplied strings cannot escape the
template or invoke arbitrary selectors.

`HarnessError::Io` includes the underlying [`std::io::Error`] text, which may
carry sensitive path information. Callers should treat that text accordingly
and avoid surfacing it in user-visible output without sanitization.

Replay startup expectations:

- The cassette file must already exist at `cassette_dir/cassette_name`.
- The file name is exactly `cassette_name`; the harness does not append an
  implicit `.json` suffix.
- The stored cassette must use the currently supported `format_version`.

Record-mode startup expectations:

- `upstream` must be configured.
- The listener is bound during `start_harness`, so `RunningHarness.addr` may
  differ from the requested address when the configured port is `0`.
- `shutdown()` gracefully stops the bound record-mode server.

### Record mode proxying

Record mode supports OpenAI-compatible chat completions requests:

- The harness accepts `POST /v1/chat/completions`.
- Requests with `stream` unset or `false` are proxied upstream, returned to the
  client, and appended to the configured cassette.
- Requests with `stream: true` are proxied as Server-Sent Events (SSE),
  returned to the client as upstream bytes arrive, and appended to the
  configured cassette after the stream completes successfully.

Upstream authentication and enrichment:

- The bearer token is sourced from `upstream.api_key_env` at request time.
- `upstream.extra_headers` are added only to the outbound upstream request.
- The recorded request in the cassette reflects the client-visible inbound
  request after header selection and redaction, not the enriched outbound proxy
  request.

Header capture and redaction:

- Request capture drops hop-by-hop and framing headers.
- Persisted request headers additionally exclude `host`, `content-length`, and
  `accept-encoding`.
- Persisted response headers exclude hop-by-hop headers and `content-length`.
- `redaction.drop_headers` removes matching header names
  case-insensitively immediately before persistence, preserving the observed
  order and duplicates of the retained headers.

`RedactionConfig` is secure by default: `authorization` is in `drop_headers`
unless an explicit `RedactionConfig` is provided. To retain `authorization` in
the cassette, supply `RedactionConfig { drop_headers: vec![] }`.

Persisted response contract:

- Persisted response headers exclude hop-by-hop headers and `content-length`.
  Redaction via `redaction.drop_headers` removes matching header names
  case-insensitively immediately before persistence, preserving the observed
  order and duplicates of the retained headers.
- Non-stream upstream response bodies are stored byte-for-byte in the
  cassette.
- `parsed_json` is populated only when the response body decodes and parses as
  valid JSON; otherwise `parsed_json` is left empty so consumers know what
  replay can reconstruct.
- Streamed upstream responses are stored as `kind: "stream"` responses with
  selected response headers, parsed stream events, raw transcript bytes, and
  timing metadata.
- SSE comment lines such as `: OPENROUTER PROCESSING` are recorded as comment
  events. `data:` frames are recorded with their raw payload text and
  `parsed_json` when the payload is valid JSON. Terminal `data: [DONE]` markers
  are retained as data events with no parsed JSON.
- If a streamed upstream response contains invalid UTF-8 or ends with an
  incomplete SSE event, the harness still returns any bytes already received
  from upstream to the client, but it does not append a successful cassette
  entry for that malformed stream.

Replay behaviour for chat completions:

- Replay mode accepts `POST /v1/chat/completions` against an existing
  cassette.
- A matching request returns the recorded status, persisted selected response
  headers, and response body from the cassette.
- Non-stream responses replay the recorded body bytes.
- Stream responses replay the recorded parsed SSE events as canonical SSE
  frames. Recorded comment events are emitted as `: ...` frames, recorded data
  events are emitted as `data: ...` frames, and event order is preserved.
- If a stream cassette omits `content-type`, replay sets
  `text/event-stream`.
- Replay mode does not require upstream configuration or an upstream API key,
  and it constructs no outbound upstream client. If `upstream` is present in a
  replay configuration, it is ignored by the replay request path.
- Mismatched requests return HTTP `409 Conflict` with a JSON
  `request_mismatch` diagnostic containing the position, expected hash,
  observed hash, and diff summary.
- Replay rejects malformed or non-JSON chat completions request bodies with
  HTTP `400 Bad Request` and a JSON `malformed_json` error before matching.
  This prevents different malformed byte sequences from sharing the same
  body-less replay hash.
- A request with `stream: true` must still match a recorded request whose
  canonical body includes the same streaming shape. If no interaction matches,
  replay returns the normal HTTP `409 Conflict` request-mismatch diagnostic.
- If a `stream: true` request matches a manually authored non-stream response,
  replay returns HTTP `501 Not Implemented` with `stream_cassette_required`.
- Replay currently serializes parsed stream events rather than the raw
  `raw_transcript` bytes. Byte-faithful SSE replay remains deferred to roadmap
  task `2.1.3`.

### Replay matching modes

The harness supports two matching modes for replay:

- **Sequential strict mode (default)**: requests must arrive in the exact
  recorded order. Each incoming request is expected to match the next
  interaction in the cassette. Mismatches fail fast with an HTTP 409 response
  containing:
  - The expected interaction ID (zero-based index).
  - The expected and observed request hashes.
  - A field-level diff summary comparing the canonical request JSON values.

  This mode maximizes repeatability and debugging speed for deterministic,
  single-threaded agent loops.

- **Keyed mode**: requests are matched by their canonical request hash. The
  engine consumes the next unused interaction with the matching hash, allowing
  requests to arrive out of order. When multiple interactions share the same
  hash, they are consumed in recorded order.

  This mode supports limited reordering and concurrent requests, at the cost of
  less precise failure locations.

Configure the matching mode using the `match_mode` field:

```rust
use spycatcher_harness::HarnessConfig;
use spycatcher_harness::config::MatchMode;

let cfg = HarnessConfig {
    match_mode: MatchMode::Keyed,
    ..HarnessConfig::default()
};
```

The cassette module also exposes
`canonicalize_events(events, StreamCanonicalPolicy::ignore_comments())` for
tools that compare recorded stream-event sequences while ignoring comment-only
drift. This is a library helper for cassette consumers and future verification
work; it is not a replay CLI configuration option.

When a mismatch occurs in sequential strict mode, the diagnostic response
includes a field-level diff showing the differences between the expected and
observed canonical requests. The diff format uses:

- `added: <path>: <value>` — field present in observed but not expected.
- `removed: <path>` — field present in expected but not observed.
- `changed: <path>: <expected_value> -> <observed_value>` — differing values.

Paths use dotted notation for nested objects (e.g.,
`canonical_body.metadata.run_id`) and bracket notation for array elements (e.g.,
`messages[0].role`).

### Canonical request hashing

The cassette module exposes deterministic canonicalization helpers for replay
matching and diagnostics:

```rust
use serde_json::json;
use spycatcher_harness::cassette::{
    IgnorePathConfig, RecordedRequest, canonicalize, stable_hash,
};

# fn example() -> Result<(), spycatcher_harness::cassette::CanonicalError> {
let request = RecordedRequest {
    method: "post".to_owned(),
    path: "/v1/chat/completions".to_owned(),
    query: "b=2&a=1".to_owned(),
    headers: Vec::new(),
    body: br#"{"metadata":{"run_id":"42"},"model":"gpt-test"}"#.to_vec(),
    parsed_json: Some(json!({
        "metadata": {"run_id": "42"},
        "model": "gpt-test"
    })),
    canonical_request: None,
    stable_hash: None,
};

let ignore_paths = IgnorePathConfig {
    ignored_body_paths: vec!["/metadata/run_id".to_owned()],
};
let canonical = canonicalize(&request, &ignore_paths)?;
let hash = stable_hash(&canonical);

assert_eq!(canonical.method, "POST");
assert_eq!(canonical.canonical_query, "a=1&b=2");
assert_eq!(hash.len(), 64);
# Ok(())
# }
```

### Error handling

All public API functions return `HarnessResult<T>`, which is an alias for
`Result<T, HarnessError>`. The `HarnessError` enum provides typed variants for
each failure mode:

- `InvalidConfig` — configuration validation failed.
- `CassetteNotFound` — the named cassette does not exist.
- `InvalidCassette` — the cassette JSON is malformed or missing required
  fields.
- `UnsupportedCassetteFormatVersion` — cassette schema version is not supported
  during cassette loading or startup (includes replay and verify modes).
- `ModeNotYetImplemented` — the selected operating mode is configured but does
  not yet start a running harness.
- `RequestMismatch` — a replayed request did not match the expected
  interaction.
- `UpstreamRequestFailed` — a request to the upstream provider failed.
- `Io` — an I/O operation failed.

### Shutdown

Call `shutdown()` on a `RunningHarness` to tear down the harness gracefully:

```rust,no_run
# use spycatcher_harness::{start_harness, HarnessConfig};
# async fn example() -> spycatcher_harness::HarnessResult<()> {
# let cfg = HarnessConfig::default();
# let harness = start_harness(cfg).await?;
harness.shutdown().await?;
# Ok(())
# }
```

## CLI binary

The `spycatcher-harness` binary now supports three subcommands:

- `record`
- `replay`
- `verify`

Each subcommand loads configuration using layered precedence:

`CLI > env > config files > defaults`

Per-subcommand defaults are loaded from the `cmds` namespace in config files:

- `cmds.record`
- `cmds.replay`
- `cmds.verify`

CLI help documents this merged shape:

```sh
cargo run --bin spycatcher-harness -- --help
```

### Common CLI flags

The current subcommands support the same top-level override flags:

- `--listen <SOCKET_ADDR>`
- `--cassette-dir <PATH>`
- `--cassette-name <NAME>`
- `--locale <LANGID>`
- `--fallback-locale <LANGID>`

`record` additionally supports nested upstream config through file and env
layering under `cmds.record.upstream`.

`--locale` selects the preferred BCP 47 language identifier for localized
application messages. `--fallback-locale` selects the deterministic fallback
locale and defaults to `en-US`. Invalid language identifiers fail startup
before the harness begins serving.

### Localized CLI help, version, and parse errors

The binary renders `clap` help, version, and parse-error text through
OrthoConfig's `Localizer` abstraction. The bundled en-US Fluent catalogue in
`i18n/en-US/spycatcher-harness.ftl` contains the `cli-*` help strings,
`cli-version`, and the `clap-error-*` parse-error strings used by the
command-line interface.

CLI parsing happens before subcommand configuration has been fully merged, so
help, version, and parse errors use a best-effort early locale. The binary
checks `SPYCATCHER_HARNESS_LOCALE`, then `SPYCATCHER_HARNESS_FALLBACK_LOCALE`,
then falls back to `en-US`. After parsing, harness library errors still use the
authoritative `--locale` and `--fallback-locale` values from the merged
configuration.

Set `SPYCATCHER_HARNESS_DISABLE_LOCALIZATION` to a truthy value (`1`, `true`,
`yes`, or `on`) to force stock `clap` help, version, and parse-error output.
This is intended as a diagnostic escape hatch if localized CLI assets need to
be ruled out while investigating startup behaviour. The binary uses the same
`NoOpLocalizer` fallback automatically when localized CLI resources cannot be
loaded.

### Configuration file shape

Create `.spycatcher_harness.toml` in the working directory:

```toml
[cmds.record]
cassette_name = "record_smoke"

[cmds.record.upstream]
kind = "openrouter"
base_url = "https://openrouter.ai/api/v1"
api_key_env = "OPENROUTER_API_KEY"

[cmds.replay]
cassette_name = "replay_smoke"

[cmds.replay.localization]
locale = "en-GB"
fallback_locale = "en-US"

[cmds.verify]
cassette_name = "verify_smoke"
```

### Environment variable shape

Environment variables use the prefix `SPYCATCHER_HARNESS_CMDS_<SUBCOMMAND>_...`.

Examples:

```sh
SPYCATCHER_HARNESS_CMDS_REPLAY_CASSETTE_NAME=env_replay
SPYCATCHER_HARNESS_CMDS_RECORD_UPSTREAM__BASE_URL=https://example.invalid/api
SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__LOCALE=en-GB
SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__FALLBACK_LOCALE=en-US
```

Nested environment keys use a double underscore between path segments. For
example, `LOCALIZATION__LOCALE` maps to `cmds.<subcommand>.localization.locale`.

### CLI usage examples

```sh
# Record using layered defaults and an explicit cassette name override.
cargo run --bin spycatcher-harness -- record --cassette-name cli_record

# Replay with layered configuration.
cargo run --bin spycatcher-harness -- replay

# Verify with layered configuration.
cargo run --bin spycatcher-harness -- verify
```

For replay and verify mode, ensure the cassette file already exists at the
configured `cassette_dir/cassette_name` path and was created by a compatible
`format_version`.
