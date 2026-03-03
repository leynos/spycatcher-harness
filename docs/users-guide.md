# Spycatcher harness user's guide

This guide documents the public API surface and usage patterns for the
Spycatcher harness. The harness records and replays LLM API interactions for
deterministic regression testing.

## Library API

The `spycatcher_harness` crate exposes two primary entry points for harness
lifecycle management.

### Starting the harness

Call `start_harness` with a `HarnessConfig` to validate configuration and
prepare the harness for operation:

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

### Error handling

All public API functions return `HarnessResult<T>`, which is an alias for
`Result<T, HarnessError>`. The `HarnessError` enum provides typed variants for
each failure mode:

- `InvalidConfig` — configuration validation failed.
- `CassetteNotFound` — the named cassette does not exist.
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

The `spycatcher-harness` binary delegates all behaviour to the library. CLI
argument parsing and subcommand support will be added in a future release (task
1.1.2).

Currently, the binary starts the harness with a default configuration and shuts
it down immediately:

```sh
cargo run --bin spycatcher-harness
```
