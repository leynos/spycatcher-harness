# Developer's guide

This guide documents build configuration choices, dependency rationale,
internal module layout, and test structure patterns for contributors working on
the Spycatcher harness.

## Spelling policy

Run the spelling gate with:

```bash
make spelling
```

`TYPOS_CONFIG_BUILDER_VERSION` in the `Makefile` pins the
`typos-config-builder` release the gate runs (currently `v0.1.3`). Raise it
together with the regenerated `typos.toml`, never on its own.

The tracked `typos.toml` is regenerated on every run from the live shared
dictionary and the repository-specific `typos.local.toml` overlay. The gate
then runs the dictionary-based Typos scan over tracked Markdown and the
phrase-correction check over the whole tracked repository, including Rust
files. Because the dictionary is live, `typos.toml` must never be drift checked
in continuous integration.

The shared `typos-config-builder` CLI refreshes the estate dictionary into an
untracked local cache only when the authoritative copy is newer, so a valid
cache remains usable when the network is unavailable. Keep repository-specific
identifiers and deliberate quotations in `typos.local.toml`. Never edit
generated entries by hand.

## Build configuration

### Toolchain requirements

The crate declares `rust-version = "1.89.0"` and edition 2024. The floor is not
arbitrary: `ortho_config` 0.10.0, which this project uses for layered
subcommand configuration, requires a compiler at or above 1.89.0, so adopting
that release also sets the application MSRV. CI enforces the floor by installing
`1.89.0` and running
`cargo +1.89.0 check --locked --workspace --all-targets --all-features`, which
also proves the lockfile resolves on the oldest supported compiler.

Development uses the nightly pinned in `rust-toolchain.toml`
(`nightly-2026-10-30`) with the `rustfmt` and `clippy` components, so every
contributor formats and lints with the same compiler. Keep that pin, the
`rust-version` field, and this section in step whenever either moves.

`cap-std` is built with its `fs_utf8` feature because path handling uses
`cap_std::fs_utf8`, keeping filesystem access capability-oriented while
treating paths as UTF-8.

### `serde_json` `preserve_order` feature

The `serde_json` dependency is compiled with the `preserve_order` feature
enabled:

```toml
serde_json = { version = "1.0.143", features = ["preserve_order"] }
```

This feature causes `serde_json::Map` to use an insertion-ordered map (
`IndexMap`) instead of a `BTreeMap`. It is **required** for two reasons:

1. **Deterministic canonical hashing.** The canonical request module
   normalizes JSON objects before hashing. Without `preserve_order`,
   round-tripping through `serde_json` could silently reorder keys, producing
   different hashes for semantically identical requests.
2. **Stable diff output.** The field-level diff utility iterates over
   JSON objects in key order. Deterministic iteration order ensures that diff
   summaries are reproducible across runs, which the property-based tests in
   `diff_tests` explicitly verify (see
   `diff_key_ordering_does_not_affect_output`).

Removing this feature flag would break both the hashing invariant and the diff
determinism property tests.

### `tokio` runtime features

The `tokio` dependency enables `rt`, `rt-multi-thread`, `macros`, `net`,
`sync`, and `time`:

```toml
tokio = { version = "1.52.1", features = ["rt", "rt-multi-thread", "macros", "net", "sync", "time"] }
```

The server uses Tokio tasks and networking for the Axum runtime, graceful
shutdown, and blocking cassette writes (`src/server/record.rs`). The
`rt-multi-thread` feature is also required by the concurrency regression test
`concurrent_requests_are_recorded_without_data_loss`, which runs
`RecordService::handle_chat_completions` on a multi-threaded test runtime to
verify record-mode persistence under concurrent requests.

Do not remove these runtime features without checking the server startup path,
record-mode task spawning, and the concurrent record-mode tests.

### `tracing` request events

The `tracing` dependency is used at the HTTP adapter boundary for structured
record-mode request events. `record_chat_completions_handler` calls
`log_chat_request`, which records the HTTP method and `uri.path()` only. Query
strings are intentionally excluded, so credentials passed in query parameters
do not enter request logs.

### `futures-util` — streaming response adapters

`futures-util` is a direct dependency because record mode turns upstream
`reqwest::Response::chunk()` polling into an Axum-compatible `TryStream`. The
crate was already present transitively; depending on it directly avoids
enabling reqwest's broader `stream` feature while keeping the response body
adapter explicit in `src/server/record_stream/mod.rs`.

## Coverage publication

Main owns both persistent coverage outputs, following concordat's CV-005 rule.
Pull requests measure coverage in `ci.yml` with `with-ratchet: 'true'` and
`publish-artefact: 'false'`, so they check the ratchet against the stored
baseline and do nothing else: no pull request uploads a report, runs
`cs-coverage`, receives `CS_ACCESS_TOKEN`, or contacts `codescene.io`.
CodeScene accepts an upload only for an analysed branch, which a pull request
head is not, and its check mode fails on every project whose coverage gates are
off. What the split takes off the pull request is the call to the service; the
CLI archive is already pinned by digest.

Both coverage callers (`ci.yml` and `coverage-main.yml`) set
`baseline-rust-file: .coverage-baseline-v2.rust`. The ratchet restores its
baseline from a cache keyed by that path, so a new path starts from the
committed figure instead of the cached one. The file was introduced when the
bump to `nightly-2026-10-30` lowered measured coverage from 90.1% to 87.4% with
every test unchanged, and holds the new measurement rounded down to one decimal
place (87.4). To reset the baseline after a similar toolchain change, commit
the new figure under a new suffix (`.coverage-baseline-v3.rust`) and point both
callers at it.

`.github/workflows/coverage-main.yml` is the one publisher. It runs on a push to
`main`, writes the ratchet baseline, and uploads to CodeScene only when both
hold:

- a `Check CodeScene token` step, whose sole command is
  `echo "available=${{ secrets.CS_ACCESS_TOKEN != '' }}" >> "$GITHUB_OUTPUT"`,
  reports the token as set; the expression is evaluated before the shell runs,
  so the step binds nothing;
- `github.ref == 'refs/heads/main'`, which keeps the upload on `main` if a
  dispatch trigger, which may name any branch, is ever added.

The upload passes the token only as `access-token`, never through an `env`: the
upload action is composite and hands its step's `env` to the nested steps it
runs. The concurrency group is `${{ github.workflow }}-${{ github.ref }}` and
never cancels, so runs never overlap and, for triggered runs (pushes), uploads
land in commit order and the newest baseline wins. A manual "Re-run jobs" on an
older `main` run is an operator action: it keeps its old SHA and republishes
that commit's coverage and baseline until the next push supersedes it. One gap
is known and accepted: a Dependabot pull request merged by the automerge
workflow with `GITHUB_TOKEN` fires no push, so it publishes nothing until the
next push to `main` (shared-actions #518).

`make test-workflow-contracts` holds the rule by running
`cv005-contracts check`, the shared contract library in `leynos/shared-actions`
(`packages/cv005-contracts`), from a full commit named by `CV005_CONTRACTS_REF`
in the Makefile. `make all` includes this check and runs its gates serially,
even when invoked with `-j`. CI also runs the contract check. A fix to the
rules is therefore a pin bump. The target needs `uv`, which fetches the Python
3.13 the library runs under. The repository's only parameter is `repository` in
`.github/cv005.toml`. The library's own suite proves each rule refuses the
shape it exists to refuse, so this repository keeps no copy of the readers or
the refusal cases. The pull-request clauses run over every workflow a pull
request can reach through local `uses:` calls, the host and token clauses read
every scalar in each document, the upload condition is split on `&&` with any
`||` refused, and workflows are read strictly: a duplicate key is refused
rather than silently resolved, and a reading failure exits 2 rather than
passing.

## Testing guidance

Detailed dev-dependency rationale and test layout conventions now live in
[`testing-guide.md`](testing-guide.md). Keep this guide focused on build
configuration, module layout, internal abstractions, and review residuals.

### Shared test infrastructure

`tests/support/test_utils.rs::build_runtime` returns
`std::io::Result<tokio::runtime::Runtime>`. Propagate runtime-construction
failures with `?`; for example, a BDD step can use
`let runtime = build_runtime()?;`.

The replay and record-mode proxy BDD worlds create their runtime lazily through
`ensure_runtime()`. Call it before using the runtime in a step. It does nothing
when the world already has a runtime and propagates construction errors to the
step result.

Record-service unit-test helpers in `src/server/record_tests_helpers.rs` return
`eyre::Result`, including `cassette_fixture`, `service_fixture`,
`service_fixture_ephemeral`, and `load_cassette`. Use `?` so
temporary-directory, path, cassette-open, and cassette-decode failures retain
their context. Keep the `CassetteFixture` alive while the service uses its
path; its `TempDir` owns the fixture directory's lifetime. The replay and proxy
BDD cassette readers return boxed errors instead; propagate those with `?` as
well.

For build-standard reader tests, use `create_fake_program(label, script)?` from
`tests/build_standard/fake_program.rs`; it returns
`std::io::Result<FakeProgram>` and creates an executable in an owned, unique
scratch directory. Pass `program.path()` to `Command::new` and retain
`FakeProgram` until the child finishes, for example through
`wait_with_output()`; dropping it removes the directory. The helper writes the
script in a child and waits for successful writer completion before returning,
so the parent does not hold the writer's writable descriptor while it later
launches the stand-in.

Matching-test helpers such as `nth_response` and `extract_response_id` return
`Option` when the interaction, response kind, or response ID may be absent.
Check the option explicitly. For a non-stream response, for example, assert
`assert_eq!(extract_response_id(&interaction), Some("response-id"))`; assert
`None` when testing a stream response or missing ID. When a sample cassette
guarantees an indexed response, use `expect` with an interaction-specific
message rather than silently substituting a default.

## Module layout

The library crate (`src/lib.rs`) contains the following top-level public and
internal modules:

| Module                 | Purpose                                                                    |
| ---------------------- | -------------------------------------------------------------------------- |
| `cassette`             | Schema, canonicalization, hashing, matching, diff, filesystem persistence  |
| `cli`                  | CLI argument parsing via `clap`                                            |
| `config`               | `HarnessConfig`, `UpstreamConfig`, `RedactionConfig`, and related types    |
| `error`                | `HarnessError` enum and `HarnessResult` alias                              |
| `i18n`                 | Internationalization via Fluent                                            |
| `protocol`             | Protocol identifiers and request-shape helpers                             |
| `replay`               | Adapter-neutral replay service and replay error types                      |
| `replay_observability` | Replay-mode metrics labels and emitters                                    |
| `server`               | Axum HTTP servers: routing, handlers, state, graceful shutdown             |
| `sse`                  | Incremental Server-Sent Events parser used by stream recording             |
| `upstream`             | Outbound HTTP adapter: URL construction, secret resolution, reqwest client |

_Table 1: Top-level library modules._

The crate root re-exports the public entry point `start_harness`, the
`RunningHarness` type, and the types `HarnessConfig`, `HarnessError`, and
`HarnessResult`. Shutdown is exposed as the `RunningHarness::shutdown` method,
not as a standalone crate-root function.

The `cli` module is split into small support modules to keep `src/cli/mod.rs`
focused on parsing commands, merging OrthoConfig layers, and exposing
configuration-loading entry points. Its private `config_build` child translates
merged arguments into `HarnessConfig` and validates locale overrides. Keep new
CLI-only serialization shapes, help text, and localization selection policy in
their support modules rather than growing `src/cli/mod.rs`.

| Internal CLI module        | Purpose                                                                          |
| -------------------------- | -------------------------------------------------------------------------------- |
| `src/cli/mod.rs`           | Public loader entry points and parse/merge orchestration                         |
| `src/cli/config_build.rs`  | Private override application, locale validation and `HarnessConfig` construction |
| `src/cli/source_loader.rs` | Explicit file and environment sources for embedding and isolated tests           |
| `src/cli_args.rs`          | `LocalizationArgs` and `RecordUpstreamArgs` serializable merge shapes            |
| `src/cli_help.rs`          | Stock long-form merge help used when CLI localization is disabled                |
| `src/cli/localization.rs`  | Localization override selection and validation                                   |
| `src/cli/localize_cmd.rs`  | `LocalizeCmd` adapter, per-command `$binary`/`$version`, localized parser        |
| `src/cli/localizer.rs`     | Early locale selection and Fluent-backed CLI localizer construction              |

_Table 2: CLI adapter and support modules._

Early command-line localization uses the private `LocaleEnvSetting` values in
`src/cli/localizer.rs` to pair each environment variable with its invalid-value
warning. `early_locale_plan` tries `SPYCATCHER_HARNESS_LOCALE` first and reads
`SPYCATCHER_HARNESS_FALLBACK_LOCALE` only if the primary is absent,
non-Unicode, or fails to parse as a `LanguageIdentifier`. A valid primary is
selected without consulting the fallback; if neither setting yields a supported
identifier, the default is `en-US`. Parse failures warn: an invalid primary
reports “trying fallback locale”, while an invalid fallback reports “using
en-US”. Absent or non-Unicode values produce no candidate and no parse warning.
The value-selection helper accepts lazy sources, so precedence and lazy
fallback behaviour can be tested without mutating process environment state.

### Injected configuration sources

The normal CLI loader uses the process environment and current discovery
context. Tests and embedding callers can use
`load_subcommand_config_from_iter_with_sources` to pass a
`SubcommandFileContext` and a `SharedScanEnvSource` explicitly. One `MapEnv`
can back both capabilities: file discovery performs named lookups, while the
subcommand merge scans only values under `SPYCATCHER_HARNESS_`. The independent
capabilities keep file-path selection separate from configuration merging.
Layer precedence and the `cmds.record`, `cmds.replay`, and `cmds.verify`
namespaces match the process-backed path.

CLI argument/help modules remain at `src/cli_args.rs` and `src/cli_help.rs`,
reached through `../` paths from `src/cli/mod.rs`.

The `i18n` module owns the library Fluent assets at
`i18n/en-US/spycatcher-harness.ftl`, exposes `HarnessLocalizations` for
application loaders, and renders `HarnessError` values through
`localize_harness_error(&FluentLanguageLoader, &HarnessError)`. Keep loader
construction, locale detection, and process-global localization state out of
library modules; callers inject a configured loader when they need localized
text.

The upstream adapter returns `ObservedResponse` values for non-stream responses
and `StreamingObservedResponse` values for streamed responses. Non-stream
responses carry the HTTP status code, raw header byte pairs for proxying as
`Vec<(String, Vec<u8>)>`, and exact response body bytes. Streaming responses
carry the same status and header selections plus a boxed byte stream. Header
value percent-encoding happens only at the persistence boundary when
cassette-safe string headers are derived.

Header selection and hop-by-hop filtering live in `src/http_exchange.rs`, not
`src/protocol.rs`. Use that module when changing which headers are forwarded,
proxied downstream, or persisted in cassettes.

The `cassette` module contains several submodules:

| Submodule          | Purpose                                             |
| ------------------ | --------------------------------------------------- |
| `canonical`        | Request normalization and `stable_hash`             |
| `diff`             | Field-level JSON diff summaries                     |
| `filesystem`       | `FilesystemCassetteStore` (reader/appender)         |
| `matching`         | `ReplayMatchEngine` and match outcome types         |
| `stream_canonical` | Comment-aware stream-event canonicalization helpers |

_Table 3: Cassette submodules._

### Private implementation boundaries

Directory-module roots keep their existing API boundaries. Their extracted
children are private `mod` declarations; helpers needed by the parent use
`pub(super)`, while child-only helpers remain private. The split keeps each
Rust source file within 400 lines without changing configuration precedence,
matching semantics, stream persistence, or process output.

| Private child module                                 | Responsibility                                                                                          |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `src/cassette/canonical/json/serialize.rs`           | Canonical JSON writing, escaping and ordering for `json/mod.rs`                                         |
| `src/cassette/matching/selection.rs`                 | Candidate selection and sequential/keyed cursor or consumption updates for `matching/mod.rs`            |
| `src/replay_request.rs`                              | Observed-request canonicalization and matching-field access for `src/replay.rs`                         |
| `src/replay_types.rs`                                | Adapter-neutral replay context, response, body and error contracts re-exported by `src/replay.rs`       |
| `src/server/record_state.rs`                         | Record-mode service construction from validated configuration for `record.rs`                           |
| `src/server/record_stream/state.rs`                  | Stream accumulation, chunk parsing, finalization and completed-record append for `record_stream/mod.rs` |
| `src/bin/spycatcher_harness/process_output.rs`       | Locked stdout/stderr writes and flushing for the sole binary entry point                                |
| `src/bin/spycatcher_harness/cli_startup.rs`          | CLI localizer selection, parse-span lifetime, and configuration-result exit orchestration               |
| `src/bin/spycatcher_harness/startup_localization.rs` | Configured locale validation, ordered selection, and embedded Fluent catalogue construction             |

_Table 4: Private implementation children._

The CLI configuration builder described in Table 2 follows the same private
child boundary. Binary output helpers stay below `src/bin/spycatcher_harness/`
so Cargo does not discover a second binary.

Test-module paths follow the source file that declares them:

- `src/http_exchange_tests.rs` and `src/server/replay_stream_tests.rs` contain
  the extracted private `#[cfg(test)]` suites, wired by their parent modules.
- `src/replay_tests/mod.rs` retains replay tests and its `concurrent_stream`
  child.
- `tests/cli_layering_unit.rs` retains the property and parameterized layering
  coverage. Its nested `tests/cli_layering/support.rs` provides a temporary
  configuration directory, a closed `MapEnv`, fixture constants, and locale
  strategies, with only parent-used items exposed through `pub(super)`. The BDD
  suite uses the same source-injected helper. It is not a separate Cargo
  integration-test crate.
- `tests/chat_completions_replay/harness_config.rs` is a private test-support
  module. Its `pub(super)` record/replay configuration builders are re-exported
  by `steps.rs` for chat replay and `stream_steps.rs`. The private
  `split_cassette_path` helper separates a path into its parent and file name.
  A bare name such as `demo.json` is accepted with an empty parent; a path
  without a file name is rejected.
- `tests/replay_matching_modes/helpers.rs` is a private test-support module
  with a shared `pub(super) assert_slot_string_eq` response-ID slot assertion.
  Matching steps use it to report unset or unequal named slots consistently.

The binary installs one fallible tracing subscriber before early CLI parsing.
It writes warnings and errors to stderr without ANSI escape sequences, using a
fixed warning threshold rather than reading `RUST_LOG`. Keep subscriber
initialization in the executable: library callers retain ownership of their
process-wide diagnostics. Help and version text remain on stdout.

The binary crate (`src/bin/spycatcher_harness.rs`) delegates semantic behaviour
to the library entry points. It owns two localization phases. Before CLI
parsing, it builds a best-effort `ortho_config::FluentLocalizer` for help,
version, and parse-error text, falling back to `NoOpLocalizer` if resources
cannot be loaded or `SPYCATCHER_HARNESS_DISABLE_LOCALIZATION` is set. After
layered CLI configuration is loaded, it parses the requested `locale`, parses
`fallback_locale`, constructs one authoritative `FluentLanguageLoader`, and
loads `HarnessLocalizations` into it for library error rendering. Library
modules must continue to accept injected loaders for localized rendering rather
than constructing their own loaders or reading process locale state.

### CLI property-test command trees

`tests/cli_localization_property.rs` keeps raw command and option strings
paired in `CommandIdentity` until `command_spec_node` constructs each node. The
node builder assigns `option-{raw}` to each option and names children
`child-{index}-{raw}`, with each index local to that parent's sibling list.

Keep every generated child in the tree. Normalize at node construction rather
than rejecting or filtering collisions, so Proptest shrinking retains the input
material that exposed them. The independent expected-shape oracle and fixed
cases check raw-derived names. The saved Proptest seed in
`tests/cli_localization_property.proptest-regressions` records a minimized
pre-normalization input with duplicate raw sibling names. Separately, fixed
regression cases pass raw duplicate identities through `command_spec_node` and
assert that both children survive with distinct normalized names.

If the normalization boundary changes, update this subsection and the
raw-derived expected-shape checks. Keep the collision case covered by fixed
tests and retain its saved regression seed. This subsection records naming
conventions for the private property-test fixtures.

## Internal abstractions

The record-mode server is built around several narrow traits and helpers that
allow unit tests to inject fakes without spawning real HTTP servers or reading
process state.

Replay-mode HTTP serving uses the same `server::runtime` lifecycle as record
mode, but with separate state and handlers:

- `src/server/replay.rs` loads the cassette through
  `FilesystemCassetteStore::open_for_replay`, constructs a `ReplayMatchEngine`,
  and wraps it in `ReplayService`.
- `src/server/replay_handler.rs` owns Axum extraction and response mapping for
  `POST /v1/chat/completions`.
- `src/server/replay_stream.rs` owns Axum `Body` construction for replayed
  parsed SSE events. It preserves recorded event order, emits comment events as
  SSE comments, emits data events as `data:` frames, serializes every event to
  bytes first, and only then chooses eager `Body::from` delivery or
  cancellation-safe `Body::from_stream` delivery.
- `src/replay.rs` owns the adapter-neutral request canonicalization, matcher
  advancement, response-shape classification, and typed replay body selection.

Comment-aware stream comparison belongs to the replay matching layer, not to
external consumers or HTTP body construction. Use
`ReplayMatchEngine::with_policy(cassette, mode, policy)` to configure a
`StreamCanonicalPolicy`, and use `ReplayMatchEngine::stream_policy()` when a
verification surface needs to read the configured policy. The Axum replay path
still constructs response bodies from recorded events in
`src/server/replay_stream.rs`; if a stream-shaped request matches a cassette
entry that has no recorded stream response, `src/server/replay_handler.rs` maps
the replay error to HTTP `501` with the `stream_cassette_required` error code.

Replay state must not hold `UpstreamConfig`, `ReqwestUpstreamClient`, an
environment provider, or any outbound transport trait. This is the compile-time
boundary that keeps replay deterministic and no-network. The only mutable
replay state is the `ReplayMatchEngine`, guarded by a narrow `Mutex` in
`ReplayService`; the guard is held only while calling `next_match`, and never
across an `.await`.

Replay observability lives in `src/replay_observability.rs`. That module owns
the bounded metric label type, replay-mode constants, and metric emitters used
by `ReplayService`, `replay_handler`, and `replay_stream`. It emits through the
`metrics` facade only; library code must not install recorders or exporters.
Keep metric labels low-cardinality (`protocol`, `route`, `outcome`, `reason`,
and stream `delivery`). Cassette names, hashes, interaction identifiers, and
user input belong in tracing fields or logs, not metric labels.

### `Clock` and `SystemClock` (`src/server/record_metadata.rs`)

`Clock` is a single-method trait returning the current time as an RFC 3339
string. `SystemClock` is the production implementation backed by
`time::OffsetDateTime::now_utc()`. Tests inject `FixedClock`, which returns a
hard-coded timestamp, to make `recorded_at` deterministic. Use
`SessionMetadata::with_clock_and_start` to inject both the clock and a
pre-captured `Instant` session start.

### `EnvProvider` and `ProcessEnvProvider` (`src/upstream.rs`)

`EnvProvider` provides one method, `read(&self, name: &str) -> Option<String>`,
wrapping environment variable lookup. `ProcessEnvProvider` delegates to
`std::env::var`. Tests inject `FakeEnvProvider` to simulate absent or preset
API keys without mutating process state.

### `ChatCompletionsUpstream` and `ReqwestUpstreamClient` (`src/upstream.rs`)

`ChatCompletionsUpstream` is a two-method async trait that forwards a
`ChatCompletionsRequest` to an upstream provider and returns either an
`ObservedResponse` for non-stream calls or a `StreamingObservedResponse` for
stream calls. `ReqwestUpstreamClient` is the production implementation; use
`ReqwestUpstreamClient::with_client(client)` in tests to inject a client with
custom timeout or intercept behaviour. Tests inject `FakeUpstream`, which
returns hard-coded responses or errors.

### SSE parser and stream recorder

`src/sse.rs` owns adapter-neutral SSE framing policy. It accepts arbitrary byte
fragments, emits completed `StreamEvent` values, parses valid JSON `data:`
payloads, preserves `[DONE]` as a raw data event, and reports invalid UTF-8 or
incomplete final events as typed parser errors. It does not know about Axum,
reqwest, cassette files, or process state.

`src/server/record_stream/mod.rs` owns record-mode streaming orchestration. It
resolves the API key through the same `EnvProvider` path as non-stream
recording, forwards the request through `ChatCompletionsUpstream`, streams
upstream byte chunks to the downstream client, and delegates chunk processing
to its private `src/server/record_stream/state.rs` child. That child owns
stream accumulation, `SseParser` finalization, and completed-record append. It
appends `RecordedResponse::Stream` only after the upstream stream finishes
cleanly. Do not hold the cassette-store mutex across `.await`; use the existing
`append_interaction` boundary so filesystem writes stay inside `spawn_blocking`.

Future streaming protocol work should keep this split: parser state and
protocol event classification stay outside HTTP adapters, while Axum response
body construction and reqwest polling stay in server/upstream adapters.

`src/cassette/stream_canonical.rs` provides
`canonicalize_events(events, policy)` for stream-event comparison policies. The
current policy can drop `StreamEvent::Comment` values before comparison, so
verification-style tooling can ignore OpenRouter keep-alive comment drift
without changing recorded data events. Replay request matching remains based on
canonical request hashes; stream-event canonicalization is a helper for
consumers that explicitly compare stream transcripts.

#### Request timeout

The outbound upstream adapter in [`src/upstream.rs`](../src/upstream.rs) uses a
fixed request timeout:

```rust
pub(crate) const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(30);
```

`ReqwestUpstreamClient::new()` applies that constant via
`reqwest::Client::builder().timeout(UPSTREAM_TIMEOUT)`. It bounds non-stream
chat completion requests sent to the adapter's chat/completions endpoint, which
is built from `config.base_url` plus `chat/completions`, so stalled upstreams
do not block graceful shutdown indefinitely.

### Record metadata plumbing (`src/server/record_metadata.rs`)

#### MetadataFactory and SessionMetadata

[`src/server/record_metadata.rs`](../src/server/record_metadata.rs) contains
the metadata construction seam. `MetadataFactory` is the narrow record-service
port responsible for producing `InteractionMetadata` values used for cassette
persistence:

```rust
pub(crate) trait MetadataFactory: Clone + Send + Sync + 'static {
    fn create(&self) -> HarnessResult<InteractionMetadata>;
    fn create_at(&self, start: Instant) -> HarnessResult<InteractionMetadata>;
}
```

`SessionMetadata` has three constructors:

- `new(kind: UpstreamKind)` is the production default. It uses `SystemClock`
  and captures the session start with `Instant::now()`.
- `with_clock(kind, Arc<dyn Clock>)` injects a clock while still capturing the
  session start with `Instant::now()`.
- `with_clock_and_start(kind, Arc<dyn Clock>, Instant)` injects both the clock
  and a captured session start for deterministic `relative_offset_ms` tests.

`relative_offset_ms` measures from the session start to the interaction start,
captured at record-service handler entry, not the end of the upstream
round-trip.

### Request/response capture types

`ChatCompletionsRequest<'a>` ([`src/upstream.rs`](../src/upstream.rs)) carries:

- `api_key: &'a str`
- `headers: &'a [(String, Vec<u8>)]`, preserving raw header bytes
- `body: &'a [u8]`
- `query: &'a str`
- `config.base_url`, passed to `chat_completions_url()` as-is before appending
  the `chat/completions` path segments to the configured base path. Existing
  trailing slashes are collapsed with `pop_if_empty()`, so a provider base such
  as `/api/v1` or `/api/v1/` becomes `/api/v1/chat/completions`. Existing base
  query parameters are preserved, and inbound query parameters are appended.

`ObservedRequest` ([`src/http_exchange.rs`](../src/http_exchange.rs))
represents what the inbound adapter observed. Its
`forward_headers: Vec<(String, Vec<u8>)>` field holds inbound headers selected
for forwarding in a binary-safe form.

`ObservedResponse` ([`src/http_exchange.rs`](../src/http_exchange.rs)) carries:

- `proxy_headers: Vec<(String, Vec<u8>)>`, raw header bytes forwarded
  downstream
- `headers: Vec<(String, String)>`, cassette-persistence headers with
  non-UTF-8 values percent-encoded losslessly
- `body: Vec<u8>`, exact response bytes

Hop-by-hop headers and `Connection` tokens are removed in all selectors.
Redaction is applied immediately before persistence; the default redaction
configuration drops `authorization`.

### Header selection helpers (`src/http_exchange.rs`)

The header helpers in [`src/http_exchange.rs`](../src/http_exchange.rs) define
the byte/string boundary:

- `selected_request_headers(&HeaderMap) -> Vec<(String, String)>`
  percent-encodes values for cassette persistence.
- `selected_forward_headers(&HeaderMap) -> Vec<(String, Vec<u8>)>` preserves
  raw bytes for upstream forwarding.
- `selected_response_headers(&HeaderMap) -> Vec<(String, String)>`
  percent-encodes values for cassette persistence.
- `selected_response_proxy_headers(&HeaderMap) -> Vec<(String, Vec<u8>)>`
  preserves raw bytes for downstream proxying.

Record request headers are selected in two forms. Forwarded record request
headers drop hop-by-hop and framing headers before proxying to upstream.
Persisted request headers additionally exclude `host`, `content-length`, and
`accept-encoding` before cassette storage.

Persist response headers and downstream response proxy headers both exclude
hop-by-hop headers and `content-length` only. Percent-encoding of non-UTF-8
values occurs only in the string-returning helpers, preserving raw bytes
throughout the proxy path.

`redaction.drop_headers` is applied case-insensitively immediately before
persistence to remove any configured header names from persisted request and
persisted response headers.

### Observability

Record-mode request and decision logs are emitted through `tracing`. Metrics,
alerts, distributed tracing spans, and correlation identifiers remain tracked
in issues `#31` and `#33` and are intentionally out of scope for the public API
at this stage.

## Review residuals

Current record-mode review residuals are tracked as deliberate follow-ups:

- Observability: request logging uses bounded paths only. Metrics, alerting,
  Prometheus export, and distributed tracing remain tracked by issues `#31` and
  `#33`.
- Performance: header-selection duplication has been removed through
  `select_headers_unified` and `build_disallowed_set`.
- Concurrency: functional concurrent recording coverage exists in
  `concurrent_requests_are_recorded_without_data_loss`; remaining resource-use
  concerns are fixture hygiene, not request-state correctness.
- Test storage: record-service unit tests use `TempDir` cassette paths,
  including tests that reload persisted cassette contents. A post-test
  verification checks that `target/test-record-service/` remains empty.

## Replay matching and extension guides

See [`replay-matching-guide.md`](replay-matching-guide.md) for
`ReplayMatchEngine`, `MatchOutcome`, diagnostics, diff support, and extension
guidelines for new modules and test files.

## Build standard

Development builds follow the estate's Rust build standard, which
`.cargo/config.toml` sets and Cargo auto-discovers, so a bare `cargo build`
gets it. Every `rustflags` source enables the parallel `rustc` frontend with
`-Zthreads=8`, and the `cfg(target_os = "linux")` source also links with
`mold`; macOS and Windows keep their platform linker. A Linux host building for
a Linux target links with `mold`, so install it before a `cargo` build or a
`make` development target, build scripts included; `make release` and Make
builds for a non-Linux target add no `mold`, though a caller's own `RUSTFLAGS`
may still name it. Cranelift is not the development-profile backend here; the
exception below records why.

The development profile also follows the dev-build footprint standard:
`.cargo/config.toml` sets `incremental = false` and `debug = 0` there, and the
native Cargo key `[unstable] embed-metadata = false`, which
`nightly-2026-07-31` and later pins use to avoid duplicated metadata in `.rlib`
files; Cargo still produces the corresponding `.rmeta` metadata. Do not replace
it with a raw `rustc` metadata flag; those broke build scripts.
`tests/dev_build_footprint.rs` holds the three settings.

Cargo applies a single `rustflags` source rather than merging them, and an
assigned `RUSTFLAGS` replaces every source. So each source repeats the frontend
flag, and the Makefile restates both flags as `STANDARD_RUSTFLAGS` for the
targets that assign `RUSTFLAGS`, adding them to any `RUSTFLAGS` the recipe
inherits (setup-rust exports one in CI) rather than replacing it; every `lint`
command assigns it too. The Makefile adds `mold` only when both the host and
the compilation target (`CARGO_BUILD_TARGET`, when set) are Linux; an Android
triple contains `-linux-` but is not Linux to Cargo's `target_os`, so it does
not get `mold`. `make release` assigns the inherited `RUSTFLAGS`, which is
empty when the caller exports none, so it adds neither standard flag (a
caller's own value can still contain either). A bare `cargo build --release`
still takes both flags, because Cargo does not select `rustflags` by profile.
CI installs `mold` before the first gate target. Bare Cargo on a non-Linux host
cross-building for a Linux target is outside the standard: the configuration
selects `mold` by the compilation target alone, so that build would be handed a
linker the host lacks. Make avoids it by checking the host as well, so use Make
or a Linux host for it. `tests/build_standard_contract.rs` (readers in
`tests/build_standard/support.rs`) holds the configuration and the Makefile
recipes to this, and `tests/build_standard_ci.rs` holds the CI install order.

### Cranelift exception

The decision is recorded in
[ADR 002](adr/2026-10-03-adopt-the-rust-build-standard.md).

Two observations on the pinned `nightly-2026-02-26`, neither of which
establishes a compiler root cause:

- **2026-09-28, linking.** Under Cranelift every binary and test target that
  links `aws-lc-rs` (through `rustls`) fails at the link step with `mold`
  reporting undefined `aws_lc_0_40_0_*` symbols (for example
  `aws_lc_0_40_0_EVP_DigestInit_ex`), so `lifecycle_tests`,
  `canonical_request_hashing_bdd`, `cli_localization_unit` and the
  `spycatcher-harness` binary never run. The same build on LLVM, with the same
  `-Zthreads=8` and `mold` flags, links and passes.
- **2026-09-30, panic propagation.** On the Cranelift route
  `catch_unwind_reports_err` fails and `thread_join_reports_err` aborts with
  `failed to initiate panic, error 5`; the `should_panic_recognizes_panic`
  control and the LLVM controls pass
  ([issue #146](https://github.com/leynos/spycatcher-harness/issues/146)).

Development builds therefore stay on LLVM. Revisit on or after 2027-04-03, as
issue #146 records, by re-running the workspace suite and the three
panic-semantics tests under both backends on the then-supported nightly; if
Cranelift passes, adopt it and update the routing contracts.

The pinned toolchain is now `nightly-2026-10-30`. No measurement has been taken
on it: both observations above were made on `nightly-2026-02-26`, and the
exception stands on them until the 2027-04-03 revisit, when the suite is
re-measured under both backends.
