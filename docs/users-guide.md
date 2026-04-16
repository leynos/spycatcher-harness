# Spycatcher harness user's guide

This guide documents the public API surface and usage patterns for the
Spycatcher harness. The harness records and replays LLM API interactions for
deterministic regression testing.

## Library API

The `spycatcher_harness` crate exposes two primary entry points for harness
lifecycle management.

### Starting the harness

Call `start_harness` with a `HarnessConfig` to validate configuration and
prepare the harness for operation. In replay mode, startup now opens the
configured cassette file read-only and validates its `format_version` before
returning a `RunningHarness`:

```rust,no_run
use spycatcher_harness::{start_harness, HarnessConfig};

# async fn example() -> spycatcher_harness::HarnessResult<()> {
let cfg = HarnessConfig::default();
let harness = start_harness(cfg).await?;
// The harness is now running.
// harness.addr contains the listen address.
// harness.cassette_path contains the cassette file path.
harness.shutdown().await?;
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
- `redaction` — header redaction rules.
- `replay` — timing controls for replay mode.
- `localization` — locale settings.

Replay startup expectations:

- The cassette file must already exist at `cassette_dir/cassette_name`.
- The file name is exactly `cassette_name`; the harness does not append an
  implicit `.json` suffix.
- The stored cassette must use the currently supported `format_version`.

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

When a mismatch occurs in sequential strict mode, the diagnostic response
includes a field-level diff showing the differences between the expected and
observed canonical requests. The diff format uses:

- `added: <path>: <value>` — field present in observed but not expected.
- `removed: <path>` — field present in expected but not observed.
- `changed: <path>: <expected_value> -> <observed_value>` — differing values.

Paths use dotted notation for nested objects (e.g.,
`canonical_body.metadata.run_id`) and bracket notation for array elements
(e.g., `messages[0].role`).

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

Canonicalization rules:

- Methods are uppercased before hashing.
- Query parameters decode percent triplets, preserve literal `+`, are sorted
  by key then value, and are re-encoded with uppercase hex escapes.
- JSON bodies are compacted with object keys sorted recursively.
- Ignore paths use JSON Pointer syntax (RFC 6901), for example
  `/metadata/run_id`.
- `canonicalize` returns `Result<CanonicalRequest, CanonicalError>` and
  `RecordedRequest::populate_canonical_fields` returns
  `Result<(), CanonicalError>`.
- Canonicalization returns `CanonicalError::InvalidPointerPath` when any
  configured ignore path is empty or is not a valid RFC 6901 JSON Pointer.
- Non-JSON bodies keep `canonical_body` as `None`; the stable hash is then
  derived from the method, path, and canonical query only.

`RecordedRequest::populate_canonical_fields(&IgnorePathConfig)` fills the
reserved `canonical_request` and `stable_hash` fields in-place after the ignore
paths are successfully validated. This is the current public configuration
surface for ignore paths; `HarnessConfig` remains source-compatible in this
release, so canonicalization configuration is not yet threaded through harness
startup.

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

`record` additionally supports nested upstream config through file and env
layering under `cmds.record.upstream`.

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

[cmds.verify]
cassette_name = "verify_smoke"
```

### Environment variable shape

Environment variables use the prefix `SPYCATCHER_HARNESS_CMDS_<SUBCOMMAND>_...`.

Examples:

```sh
SPYCATCHER_HARNESS_CMDS_REPLAY_CASSETTE_NAME=env_replay
SPYCATCHER_HARNESS_CMDS_RECORD_UPSTREAM__BASE_URL=https://example.invalid/api
```

Nested keys use double underscores (`__`).

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
