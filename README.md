# Spycatcher Harness

[![Ask DeepWiki](https://deepwiki.com/badge.svg)](
https://deepwiki.com/leynos/spycatcher-harness)

Spycatcher Harness is a record/replay HTTP proxy for OpenAI-compatible chat
completions. It records real upstream exchanges into cassettes for later
deterministic replay, so agent and LLM-facing tests can run without repeatedly
calling live providers.

## Quick start

For a published crate, add the dependency with Cargo:

```sh
cargo add spycatcher-harness
```

For workspace development, use a path dependency:

```toml
[dependencies]
spycatcher-harness = { path = "../spycatcher-harness" }
```

Start record mode with an upstream base URL and an environment variable name
for the provider credential:

```rust,no_run
use spycatcher_harness::{HarnessConfig, start_harness};
use spycatcher_harness::config::{Mode, UpstreamConfig, UpstreamKind};

# async fn example() -> spycatcher_harness::HarnessResult<()> {
let cfg = HarnessConfig {
    mode: Mode::Record,
    upstream: Some(UpstreamConfig {
        kind: UpstreamKind::OpenRouter,
        base_url: "https://openrouter.ai/api/v1".to_owned(),
        api_key_env: "OPENROUTER_API_KEY".to_owned(),
        ..UpstreamConfig::default()
    }),
    ..HarnessConfig::default()
};

let harness = start_harness(cfg).await?;
// Point OpenAI-compatible clients at `harness.addr`.
harness.shutdown().await?;
# Ok(())
# }
```

See [docs/users-guide.md](docs/users-guide.md) for user-facing configuration
and [docs/developers-guide.md](docs/developers-guide.md) for implementation
notes.

For localized error messages, see the
[Localizing library messages](docs/users-guide.md#localizing-library-messages)
section of the users' guide.

## Security defaults

`RedactionConfig::default()` drops `authorization` before persistence. Supply
`RedactionConfig { drop_headers: vec![] }` only when cassette files are allowed
to retain credentials.

## Compatibility

The proxy path preserves raw header bytes. Percent-encoding of non-UTF-8 header
values happens only at the cassette persistence boundary, so replay fixtures can
store a stable string representation without weakening the transport contract.

## Breaking changes

Record-mode proxying introduced pre-1.0 cassette and header-contract changes.
See [MIGRATION-0.1.0.md](MIGRATION-0.1.0.md) before updating tests or fixtures.
