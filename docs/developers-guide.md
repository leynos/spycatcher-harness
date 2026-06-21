# Developer's guide

This guide documents build configuration choices, dependency rationale,
internal module layout, and test structure patterns for contributors working on
the Spycatcher harness.

## Build configuration

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
adapter explicit in `src/server/record_stream.rs`.

## Dev-dependencies

### `insta` — snapshot testing

[Insta](https://insta.rs/) captures the string output of a function and
compares it against a stored reference value (a "snapshot"). It is used in
`src/cassette/diff_tests.rs` to pin the exact format of diff summaries via
inline snapshots:

```rust
insta::assert_snapshot!(diff, @r#"changed: method: "POST" -> "GET""#);
```

Snapshots are reviewed and updated with:

```sh
cargo insta review
```

This prevents accidental regressions in human-readable diagnostic output
without requiring handwritten expected strings for every format variation.

The `json` feature is enabled because the record-mode BDD suite also snapshots
cassette JSON with `insta::assert_json_snapshot!`, notably the
`cassette_successful_proxying` snapshot in
`tests/record_mode_proxying/helpers.rs`. Without this feature, only string
snapshot assertions are available. Do not remove it without replacing or
reworking the JSON cassette snapshot tests.

### `tracing-test` — captured tracing assertions

`tracing-test` is used for unit tests that assert emitted tracing events. The
`log_chat_request_uses_path_not_full_uri` test uses
`#[tracing_test::traced_test]` and `logs_contain` to verify that request
logging contains `/v1/chat/completions` but not sensitive query strings such as
`api_key=secret`.

### `proptest` — property-based testing

[Proptest](https://proptest-rs.github.io/proptest/) generates random inputs to
verify that properties hold across a broad input space. It is used in
`src/cassette/diff_tests.rs` alongside a custom `arb_json_object()` strategy
that produces small JSON objects with random keys and scalar values. Four
property tests validate diff invariants:

- `identical_values_always_produce_empty_diff` — diffing a value
  against itself always yields an empty string.
- `diff_is_deterministic_across_invocations` — repeated calls with the
  same inputs produce identical output.
- `every_differing_key_is_mentioned` — every key that differs between
  expected and observed appears in the summary.
- `diff_key_ordering_does_not_affect_output` — reversing key insertion
  order does not change the diff result.

These properties complement the example-based `#[rstest]` cases and snapshot
tests, catching edge cases that handwritten examples might miss.

### `rstest` and `rstest-bdd` — fixtures and BDD scenarios

[rstest](https://docs.rs/rstest/) provides parameterized test cases (`#[case]`)
and shared fixtures injected by function signature.
[rstest-bdd](https://docs.rs/rstest-bdd/) extends this with Gherkin-style step
definitions (`#[given]`, `#[when]`, `#[then]`) driven by `.feature` files.

### `uuid` — unique cassette names

The `uuid` crate (with the `v4` feature) generates unique cassette filenames in
integration tests, preventing collisions when tests run in parallel.

### `tempfile` — self-cleaning unit-test cassette paths

`tempfile` provides `TempDir` for record-mode unit tests that instantiate
`FilesystemCassetteStore` only because the current `RecordService` store field
is filesystem-backed. Tests that exercise request-shape guards or API-key
resolution use a `TempDir` cassette path so no files are orphaned under
`target/test-record-service/`. The temporary directory is created under the
project root and converted back to a relative path before it reaches
`FilesystemCassetteStore`, matching the adapter's capability-rooted path model.

Do not replace those fixtures with fixed paths. A future in-memory cassette
store can remove this temporary filesystem dependency once `RecordService`
accepts cassette reader/appender traits generically.

## Module layout

The library crate (`src/lib.rs`) contains the following top-level public and
internal modules:

| Module     | Purpose                                                                    |
| ---------- | -------------------------------------------------------------------------- |
| `cassette` | Schema, canonicalization, hashing, matching, diff, filesystem persistence  |
| `cli`      | CLI argument parsing via `clap`                                            |
| `config`   | `HarnessConfig`, `UpstreamConfig`, `RedactionConfig`, and related types    |
| `error`    | `HarnessError` enum and `HarnessResult` alias                              |
| `i18n`     | Internationalization via Fluent                                            |
| `protocol` | Protocol identifiers and request-shape helpers                             |
| `replay`   | Adapter-neutral replay service and replay error types                      |
| `server`   | Axum HTTP servers: routing, handlers, state, graceful shutdown             |
| `sse`      | Incremental Server-Sent Events parser used by stream recording             |
| `upstream` | Outbound HTTP adapter: URL construction, secret resolution, reqwest client |

_Table 1: Top-level library modules._

The crate root re-exports the public entry point `start_harness`, the
`RunningHarness` type, and the types `HarnessConfig`, `HarnessError`, and
`HarnessResult`. Shutdown is exposed as the `RunningHarness::shutdown` method,
not as a standalone crate-root function.

The `cli` module is split into small private support modules to keep
`src/cli.rs` focused on adapter flow: parse the command line, call
`OrthoConfig`, validate localization, and translate the result into
`HarnessConfig`. Keep new CLI-only serialization shapes, help text, and
localization selection policy in these support modules rather than growing
`src/cli.rs`.

| Internal CLI module       | Purpose                                                                |
| ------------------------- | ---------------------------------------------------------------------- |
| `src/cli.rs`              | Public CLI localization parsing entry points and configuration helpers |
| `src/cli_args.rs`         | `LocalizationArgs` and `RecordUpstreamArgs` serializable merge shapes  |
| `src/cli_help.rs`         | Stock long-form merge help used when CLI localization is disabled      |
| `src/cli/localization.rs` | Localization override selection and validation                         |
| `src/cli/localize_cmd.rs` | Project-owned `LocalizeCmd` trait and localized `clap` parsing helper  |
| `src/cli/localizer.rs`    | Early locale selection and Fluent-backed CLI localizer construction    |

_Table 2: Private CLI support modules._

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

`src/server/record_stream.rs` owns record-mode streaming orchestration. It
resolves the API key through the same `EnvProvider` path as non-stream
recording, forwards the request through `ChatCompletionsUpstream`, streams
upstream byte chunks to the downstream client, feeds those bytes to
`SseParser`, and appends a `RecordedResponse::Stream` only after the upstream
stream finishes cleanly. Do not hold the cassette-store mutex across `.await`;
use the existing `append_interaction` boundary so filesystem writes stay inside
`spawn_blocking`.

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

## Test structure

### Unit tests

Unit tests live alongside their production code under `#[cfg(test)]` modules.
Two patterns are used:

- **Inline test module.** A `mod tests { ... }` block at the bottom of
  the source file (e.g. `src/lib.rs`, `src/cassette/filesystem.rs`).
- **Dedicated test file.** A sibling `*_tests.rs` file imported under
  `#[cfg(test)]` (e.g. `src/cassette/diff_tests.rs`,
  `src/cassette/canonical_tests.rs`). When the test module grows large it may
  be split into a directory with submodules (e.g.
  `src/cassette/matching_tests/` with `construction.rs`, `diagnostic.rs`,
  `fixtures.rs`, `keyed.rs`, `sequential.rs`).

### Integration tests (BDD)

Integration tests reside in the `tests/` directory and follow rstest-bdd
conventions:

```plaintext
tests/
  features/                      Gherkin .feature files
    replay_matching_modes.feature
    ...
  replay_matching_modes/         Step definitions and world
    fixtures.rs
    helpers.rs
    steps.rs
    world.rs
  replay_matching_modes_bdd.rs   Entrypoint wiring features to steps
  support/                       Shared BDD fixtures and utilities
    bdd_fixtures.rs
    test_utils.rs
```

Each BDD test suite has:

1. A `.feature` file in `tests/features/` defining scenarios in
   Gherkin syntax.
2. An entrypoint `*_bdd.rs` file in `tests/` that wires the feature
   file to step definitions.
3. Optional subdirectory (e.g. `tests/replay_matching_modes/`) holding
   step definitions (`steps.rs`), a world struct (`world.rs`), fixtures (
   `fixtures.rs`), and helpers (`helpers.rs`).

### Running tests

All tests are run via the Makefile:

```sh
make test       # nextest + doctests
make lint       # cargo doc + clippy + whitaker
make check-fmt  # formatting check (no modification)
make fmt        # apply formatting (Rust + Markdown)
```

See `AGENTS.md` for the full command expansions and commit gating requirements.

## Replay matching architecture

The replay matching subsystem lives in the `cassette::matching` module and
decides which recorded interaction to serve for each incoming request during
replay mode. It is built around four core types:

### `MatchMode` (defined in `config`)

An enum selecting the matching strategy:

| Variant            | Behaviour                                                                                          |
| ------------------ | -------------------------------------------------------------------------------------------------- |
| `SequentialStrict` | Default. Expects requests in recorded order; mismatches fail fast.                                 |
| `Keyed`            | Matches by request hash, consuming the next unused interaction with that hash regardless of order. |

_Table 3: Matching mode variants._

### `ReplayMatchEngine`

The stateful matching engine, constructed from a loaded `Cassette` and a
`MatchMode`. Construction validates that every interaction carries a
`stable_hash`, returning `HarnessError::InvalidCassette` if any is missing.

The single public entry point is
`next_match(observed_hash, observed_canonical)` which returns a `MatchOutcome`.
Internally it dispatches to one of two private strategies:

- **Sequential strict** — maintains a cursor tracking the next expected
  index. The incoming hash must equal the hash at the cursor position; on
  success the cursor advances, on failure a `MismatchDiagnostic` is returned
  and the cursor stays put (allowing retry at the application level).
- **Keyed** — builds a `HashMap<String, Vec<usize>>` at construction
  time mapping each hash to its interaction indices. On each call it finds the
  first unconsumed index for the observed hash, marks it consumed, and returns
  the interaction. When no index exists or all matching indices are consumed, a
  `MismatchDiagnostic` is returned.

### `MatchOutcome`

The return type of `next_match`:

- `Matched(&Interaction)` — the request matched; carries a borrow of the
  recorded interaction for the caller to replay.
- `Mismatch(MismatchDiagnostic)` — no match found; carries structured
  diagnostics.

### `MismatchDiagnostic`

A plain struct carrying everything the adapter layer needs to build an HTTP 409
response without coupling the domain to HTTP types:

| Field           | Type                  | Purpose                                                                 |
| --------------- | --------------------- | ----------------------------------------------------------------------- |
| `position`      | `InteractionPosition` | Identifies which interaction (or bound) the mismatch relates to.        |
| `expected_hash` | `String`              | Stable hash of the expected request (sequential) or empty (keyed miss). |
| `observed_hash` | `String`              | Stable hash of the incoming request.                                    |
| `diff_summary`  | `String`              | Field-level diff produced by the `diff` module.                         |

_Table 4: `MismatchDiagnostic` fields._

### `InteractionPosition`

Disambiguates the positional semantics carried inside a `MismatchDiagnostic`:

| Variant        | Mode(s)    | Payload meaning                                                        |
| -------------- | ---------- | ---------------------------------------------------------------------- |
| `Expected(n)`  | Sequential | Zero-based index of the next expected interaction.                     |
| `Exhausted(n)` | Sequential | Cassette is exhausted; `n` is the total interaction count.             |
| `KeyedMiss(n)` | Keyed      | No unconsumed interaction matched; `n` is the total interaction count. |

_Table 5: `InteractionPosition` variants._

### Diagnostic constants

Three sentinel strings identify the mismatch category in the `diff_summary`
field:

- `DIAGNOSTIC_EXHAUSTED` (`"cassette-exhausted"`) — no more
  interactions available.
- `DIAGNOSTIC_NO_MATCH` (`"no-matching-interaction"`) — keyed mode
  found no interaction with the observed hash.
- `DIAGNOSTIC_CONSUMED` (`"interaction-already-consumed"`) — keyed
  mode found interactions but all are already consumed.

### Relationship to `HarnessError::RequestMismatch`

`MismatchDiagnostic` is a domain-internal type that does not leave the
`cassette` module boundary as an error. The adapter layer (HTTP server) maps a
`MatchOutcome::Mismatch` into a `HarnessError::RequestMismatch` when surfacing
the failure to callers. `RequestMismatch` mirrors the diagnostic fields (
`interaction_id`, `expected_hash`, `observed_hash`, `diff_summary`) so the
error can be formatted for logging and HTTP responses without re-importing
matching internals.

### Supporting module: `diff`

The `diff` module (`cassette::diff`) provides `canonical_diff_summary`, which
compares two `serde_json::Value` trees and produces a human-readable summary of
field-level differences. The matching engine calls this function when building a
`MismatchDiagnostic` to populate the `diff_summary` field. Output format uses
newline-separated change lines:

- `added: <path>: <value>` — field present in observed but not expected.
- `removed: <path>` — field present in expected but not observed.
- `changed: <path>: <expected> -> <observed>` — differing values.

Determinism of this output depends on the `serde_json` `preserve_order` feature
documented in the build configuration section above.

## Extension guidelines

When adding new modules or test files:

- Place unit tests in a `#[cfg(test)]` module within the source file
  or in a sibling `*_tests.rs` file.
- For BDD scenarios, add a `.feature` file in `tests/features/` and
  wire it through a `*_bdd.rs` entrypoint.
- Dependency versions in `Cargo.toml` must use implicit semver caret
  versioning (e.g. `"1.2.3"`); an explicit `'^'` must not appear (e.g. not
  `"^1.2.3"`). New dev-dependencies must be documented in this guide with their
  rationale.
- New `serde_json` feature flags or build configuration changes must
  be documented here with the invariant they support.
