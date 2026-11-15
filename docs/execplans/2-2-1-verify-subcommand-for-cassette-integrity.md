# 2.2.1 Implement the `verify` subcommand for cassette integrity and redaction

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & discoveries`,
`Decision log`, `Outcomes & retrospective`, `Conformance basis`, and
`Verification plan` must be kept up to date as work proceeds.

Status: DRAFT (awaiting expert review and user approval)

Branch: `2-2-1-verify-subcommand-for-cassette-integrity`

Roadmap item: `2.2.1` in `docs/roadmap.md`.

Implementation must not begin until the user explicitly approves this plan.
After approval, work proceeds milestone by milestone and stops only when a
tolerance threshold is reached, a constraint would be violated, or a decision
gate below says to stop.

## Purpose / big picture

Spycatcher harness records Large Language Model (LLM) API sessions to a
"cassette" (one JSON file holding an ordered list of request/response
"interactions") and replays them in regression suites. Cassettes are committed
to repositories, edited by hand, merged, and regenerated. Today nothing checks
that a committed cassette is still internally consistent or that it does not
carry credentials. The command line already accepts `verify`, but running it
fails:

```plaintext
$ spycatcher-harness verify --cassette-dir fixtures/llm --cassette-name smoke.json
Error: failed to start harness: mode not yet implemented: Verify
```

After this task, the same command checks four failure classes, plus whether
the file can be read at all, and reports every finding:

- schema: the file is JSON, carries a supported `format_version`, and
  deserializes into the cassette schema;
- ordering: interactions and recorded stream events appear in their recorded
  order, as described in the "ordering" definition below;
- hash: every request's stored `stable_hash` and `canonical_request` equal the
  values recomputed from the stored method, path, query, and body;
- redaction: no request or response header named by the required redaction
  policy, by default `authorization`, survives in the cassette.

A passing run prints a summary and exits `0`:

```plaintext
$ spycatcher-harness verify --cassette-dir fixtures/llm --cassette-name smoke.json
verified fixtures/llm/smoke.json: 3 interactions, 0 findings
```

A failing run prints one line per finding and exits `1`:

```plaintext
$ spycatcher-harness verify --cassette-dir fixtures/llm --cassette-name smoke.json
fixtures/llm/smoke.json: verification failed with 2 findings
error[hash.stable_hash_mismatch] /interactions/1/request/stable_hash: stored hash differs from recomputed hash
error[redaction.required_header_present] /interactions/2/request/headers/0/0: header `authorization` must be redacted
```

`--format json` writes a versioned JSON report for tools. `--format github`
writes GitHub Actions workflow commands, so findings appear as annotations on
the pull request. The JSON and GitHub formats use the same finding codes as
the text format.

Library callers get the same behaviour through a new
`spycatcher_harness::verify_cassette` function that returns a
`VerificationReport` value rather than starting a server.

Preserved behaviour: `record` and `replay` are unchanged, and the cassette
on-disk schema (`format_version` 1) is unchanged. Deliberately changed
behaviour: `start_harness` with `Mode::Verify` now returns
`HarnessError::InvalidConfig`, which names `verify_cassette`, instead of the
placeholder `HarnessError::ModeNotYetImplemented`. That variant is removed,
because verify is its only producer.

This plan also closes a documentation gap. The roadmap links to
`spycatcher-harness-design.md#recommended-additions-for-regression-suite-integration`,
but that text is a plain paragraph rather than a heading, so the link is dead.

## Constraints

- Implementation begins only after explicit user approval of this plan.
- Do not change the cassette on-disk schema or `CassetteFormatVersion::SUPPORTED`.
  Verify reads cassettes written by format version 1 exactly as `record`
  writes them today.
- Do not change `record` or `replay` runtime behaviour, HTTP responses, or
  canonical hashing (`canonicalize`, `stable_hash`,
  `RecordedRequest::populate_canonical_fields` in
  `src/cassette/canonical/mod.rs`). Verify reuses them; it does not
  reimplement them.
- Verify must never write to the cassette or create files. Opening is
  read-only through `cap_std::fs_utf8`, as in
  `src/cassette/filesystem.rs`.
- Verify output must never contain header values, request or response bodies,
  or stream payloads. A finding names the header and its JSON Pointer
  location only. This keeps a leaked credential out of CI logs.
- Keep domain checks free of I/O. The `cassette::verify` domain module takes
  bytes and a policy and returns a report. Only the application service
  `src/verify.rs` touches the filesystem, and only the CLI adapter writes to
  stdout. See `Interfaces and dependencies`.
- Follow `AGENTS.md`: no file over 400 lines; `//!` module docs on every
  module, including tests and Verus files; no `unwrap` or `expect` outside
  tests; clippy `print_stdout` and `print_stderr` stay denied; lints are not
  silenced except by item-scoped `#[expect(..., reason = "...")]` as a last
  resort; en-GB-oxendict spelling in comments and docs.
- Environment access follows the `EnvProvider` and `read_ambient` rules in
  `AGENTS.md`. The verify path adds no new environment reads.
- Authorized dependency changes are limited to dev-dependencies `googletest`
  and `pretty_assertions` (user-authorized), using caret requirements.
  No new runtime dependency is authorized.
- Verus is not a Cargo dependency. It is installed and run only through
  `prover-tools` from `leynos/rust-prover-tools`, as the `verus` skill
  prescribes, and `make verus` stays out of `make all`, `make lint`,
  `make test`, and the CI pull-request gate. `tests/test_make_all.py` pins
  the `all` prerequisites to `check-fmt lint test spelling
  test-workflow-contracts`; that set must not change.
- The library is pre-release. No tag in this repository's history is an
  ancestor of `HEAD`; the `v0.5.0` to `v0.9.1` tags come from an unrelated
  history. So the public API may change without compatibility shims. Removing
  `HarnessError::ModeNotYetImplemented` and its Fluent key is in scope, with
  every caller updated in the same commit.
- Completed ExecPlans under `docs/execplans/` are historical and must not be
  edited.

## Tolerances (exception triggers)

The comparison base is `origin/main` at `c3e6d76`. Count with:

```bash
git diff --numstat origin/main...HEAD -- . \
  ':(exclude)docs/execplans/2-2-1-verify-subcommand-for-cassette-integrity.md' \
  ':(exclude)Cargo.lock'
```

Each limit measures net lines (additions minus deletions) per category.
Classification is by whole file:

- Code: `src/**`, `tests/**` (including `.feature` files and `insta` `.snap`
  files), `verus/**`, and `Makefile`.
- Configuration: `Cargo.toml`, `tools/**` pin files (`VERSION`, `SHA256SUMS`,
  `REF`), and `i18n/**/*.ftl` Fluent resources.
- Documentation: `docs/**` except this ExecPlan, plus `tools/verus/README.md`.
- Excluded: this ExecPlan file and the generated `Cargo.lock`.

Budget survey. The comparable plans 1-2-1 (16 files, 1,200 lines),
1-2-2 (14 files, 1,000 lines), 1-3-2 (22 files, about 1,700 lines),
2-1-1 (24 files, about 1,900 lines), and 2-1-2 (22 files, about 1,700 lines)
used one combined code-and-docs budget. None recorded a budget increase in its
decision log or revision notes. Task 2.2.1 is wider: four check families,
three output formats, a CLI exit-code path, the repository's first Verus
proof wiring, and documentation including two ADRs. The estimate is about
1,350 production lines, 1,500 test lines (unit, property, BDD, end-to-end,
snapshots), 250 Verus and wiring lines, 90 configuration lines, and 700
documentation lines. Budgets add roughly 20% contingency:

- Files: more than 48 non-ExecPlan files changed or added: stop and escalate.
- Code: more than 3,400 net lines: stop and escalate. Within that, production
  code under `src/**` must not exceed 1,600 net lines.
- Configuration: more than 120 net lines: stop and escalate.
- Documentation: more than 850 net lines: stop and escalate.
- Interface: any public API change beyond the following stop the work for
  escalation: `verify_cassette`, `cassette::verify` types,
  `config::VerifyFormat`, the new `RedactionConfig` usage by verify, and
  removal of `ModeNotYetImplemented`.
- Dependencies: any dependency other than `googletest` and
  `pretty_assertions` (dev): stop and escalate.
- Architecture: if verify needs to change cassette persistence, canonical
  hashing, or the SSE parser's behaviour, stop and escalate.
- Verus: if `EP-M0` cannot install and run the pinned Verus through
  `prover-tools` on this host, or the kernel cannot be proved within two
  working sessions, stop and present the fallback in `D7`.
- Iterations: if a focused test still fails after three fix attempts, stop and
  escalate.
- Time: if any single milestone exceeds one working day of active effort,
  stop and escalate.
- Ambiguity: if a check's pass/fail rule admits two readings that change
  which existing recorded cassettes fail, stop and present both.

## Risks

- Risk: interaction-level ordering is not monotonic in valid concurrent
  recordings. `relative_offset_ms` is measured at handler entry, while
  appends happen at completion (`src/server/record.rs`), so concurrent record
  sessions append in completion order.
  Severity: high. Likelihood: medium. Mitigation: enforce interaction start
  order only when the effective match mode is `MatchMode::SequentialStrict`,
  which is the only mode for which array order is the replay contract. See
  `D3`.
- Risk: hand-authored cassettes, such as fixtures in other repositories, lack
  `canonical_request`, `raw_transcript`, or `timing`, and would fail verify
  for reasons unrelated to integrity.
  Severity: medium. Likelihood: medium. Mitigation: define precisely which
  absences are findings (`D4`) and document the rules in the users' guide.
- Risk: Verus release and toolchain churn. Verus pins its own Rust toolchain,
  separate from the repository's `nightly-2026-11-13`.
  Severity: medium. Likelihood: medium. Mitigation: `EP-M0` probes
  installation and a trivial proof before any proof work. Verus stays out of
  the default gates.
- Risk: the Verus proof covers a mirror rather than shipped code, the
  "idealized structure" finding that the `verus` skill warns about.
  Severity: medium. Likelihood: medium. Mitigation: `EP-M0` decides whether
  the production kernel can be written in the Verus-verifiable subset and
  checked textually (`D7`). Otherwise the residual gap is recorded and a
  property test closes it against an independent oracle.
- Risk: GitHub Actions shows at most 10 error annotations per step and 50
  per job (community discussion 68471), so a cassette with many findings
  loses annotations silently.
  Severity: low. Likelihood: medium. Mitigation: the GitHub format emits at
  most nine finding annotations, then one summary annotation stating the
  number omitted and pointing at `--format json` (`D5`).
- Risk: workflow-command injection. A hostile cassette could carry a header
  name containing a newline followed by `::add-mask::` or similar.
  Severity: high. Likelihood: low. Mitigation: escape every interpolated value
  per the GitHub toolkit rules, and test the escaping with an independent
  property oracle and named injection cases.
- Risk: large cassettes and memory use. Verify reads the whole document.
  Severity: low. Likelihood: low. Mitigation: a bounded read with a 256 MiB
  ceiling that reports `io.cassette_too_large`. The ceiling is a named
  constant (`D8`).
- Risk: `make fmt` reports pre-existing Markdown findings, as recorded in
  `1-3-2` lines 379-382.
  Severity: low. Likelihood: medium. Mitigation: run `make fmt` and then
  inspect the diff, keeping only hunks in files this plan touches.

## Progress

- [x] (2026-11-15 00:00Z) Branch renamed to
  `2-2-1-verify-subcommand-for-cassette-integrity`; Wyvern reconnaissance of
  the cassette domain, CLI, tests, tooling, and documentation completed.
- [x] (2026-11-15 00:00Z) Initial draft written.
- [ ] Community-of-experts review applied.
- [ ] Plan approved by the user.
- [ ] `EP-M0` Verus and dependency feasibility probe.
- [ ] `EP-M1` verification report model, schema checks, and outcome mapping.
- [ ] `EP-M2` hash recomputation and request-body integrity checks.
- [ ] `EP-M3` ordering checks over interactions and streams.
- [ ] `EP-M4` Verus proof of the ordering kernel and `make verus`.
- [ ] `EP-M5` redaction policy checks and layered verify configuration.
- [ ] `EP-M6` application service, output formats, CLI exit codes, BDD and
  end-to-end coverage.
- [ ] `EP-M7` documentation, ADRs, roadmap closure, and full gates.

## Surprises & discoveries

- Observation: `verify` is already a parsed subcommand with layered
  `cmds.verify` configuration, but it routes through `start_harness`, which
  opens the cassette and then returns `ModeNotYetImplemented`.
  Evidence: `src/cli/mod.rs:205-225` and `:320-343`, `src/lib.rs:123-127`.
  Impact: no new subcommand plumbing is needed. Dispatch must branch before
  `start_harness`.
- Observation: the roadmap's second design reference is a dead anchor.
  Evidence: `docs/spycatcher-harness-design.md:928` is a plain paragraph
  ("Recommended additions for regression suite integration:"), not a heading.
  Impact: `EP-M7` turns it into a heading so the anchor resolves.
- Observation: canonical hashing reads `parsed_json`, not `body`.
  Evidence: `canonicalize` in `src/cassette/canonical/mod.rs:162-177`.
  Impact: recomputing the hash alone cannot detect an edit to `body` that
  leaves `parsed_json` unchanged. Verify adds a body-to-`parsed_json`
  consistency check (`EP-M2`).
- Observation: the record path always uses `IgnorePathConfig::default()` and
  never persists an ignore configuration.
  Evidence: `src/server/record.rs:305`, `src/server/record_stream/mod.rs:155`.
  Impact: verify can recompute with the default configuration and match
  `record` exactly. Cassettes produced through the library with non-default
  ignore paths cannot be verified faithfully (`D4`).
- Observation: the stream recorder establishes three timing invariants:
  `chunk_offsets_ms.len() == events.len()`, offsets are non-decreasing, and
  `ttft_ms` is no greater than the first offset.
  Evidence: `observe_chunk` and `finish_recording` in
  `src/server/record_stream/state.rs:120-168`.
  Impact: these become ordering checks.
- Observation: there is no Verus, Kani, or prover wiring in the repository,
  although `prover-tools`, `kani`, and `cargo-kani` are installed on this
  host and `verus` is not.
  Evidence: `command -v` probes, and no prover matches in `Makefile` or
  `Cargo.toml`.
  Impact: `EP-M0` and `EP-M4` add the first pins and Makefile targets.

## Decision log

- D1. Decision: report every finding rather than failing fast, except that a
  schema-class finding which prevents deserialization stops further checks.
  Rationale: CI users fix cassettes in batches, so one run should show every
  problem. Checks after a failed deserialization have nothing typed to
  inspect. Rejected: fail-fast, which forces a fix-rerun loop.
  Date/Author: 2026-11-15, planning agent.
- D2. Decision: classify findings into five classes, `io`, `schema`,
  `ordering`, `hash`, and `redaction`, each with stable snake_case codes, and
  treat every finding as an error. There is no warning severity in this slice.
  The exit code is `0` if and only if there are no findings, and `1`
  otherwise. Clap usage errors keep exit `2`, and configuration errors raised
  before verification keep the existing `main` error path (exit `1`).
  Rationale: the roadmap requires a non-zero exit on any failure class. A
  single severity keeps that rule total and easy to check. Missing or
  unreadable cassettes are `io` findings rather than process errors, so CI
  still receives a structured report and an annotation. Rejected: per-class
  exit codes, which are fragile because one run can contain several classes.
  Date/Author: 2026-11-15, planning agent.
- D3. Decision: "ordering" means four things. First, with
  `MatchMode::SequentialStrict` (the default and the only CLI replay mode),
  `metadata.relative_offset_ms` is non-decreasing across `interactions`.
  Second, each stream's `timing.chunk_offsets_ms` has one entry per event.
  Third, those offsets are non-decreasing and `ttft_ms` is no greater than the
  first offset. Fourth, re-parsing `raw_transcript` with the production
  `SseParser` yields exactly `events`, in order. Interaction start order is
  not enforced under `MatchMode::Keyed`.
  Rationale: sequential strict replay consumes interactions in array order, so
  a start-order regression means the array order differs from the client's
  issue order. Keyed replay deliberately tolerates reordering, and concurrent
  recordings append in completion order. Rejected: always enforcing start
  order, which fails valid concurrent recordings. Also rejected: never
  enforcing it, which leaves "sequential integrity" from the design document
  unchecked.
  Date/Author: 2026-11-15, planning agent.
- D4. Decision: rules for absent optional fields.
  - A missing `stable_hash` or `canonical_request` is a `hash` finding,
    because replay requires `stable_hash` and record always writes both.
  - A stream with an empty `raw_transcript` and non-empty `events` skips the
    transcript re-parse check, because it is hand-authored.
  - A stream with `timing: None` skips the timing checks.
  - A request whose `body` is not valid JSON must have
    `parsed_json: None`, otherwise it is a `hash.parsed_json_mismatch`
    finding.

  Hash recomputation always uses `IgnorePathConfig::default()`, matching the
  record path. Cassettes written through the library with custom ignore paths
  are out of scope and documented as such.
  Rationale: this aligns verify with what `record` produces while tolerating
  the documented hand-authoring workflow. Rejected: requiring every optional
  field, which breaks hand-authored stream fixtures.
  Date/Author: 2026-11-15, planning agent.
- D5. Decision: three output formats are selected by
  `--format text|json|github` (default `text`), layered as
  `cmds.verify.format`.
  - `json` is a versioned document (`report_version: 1`) that is written to
    stdout.
  - `github` emits one `::error` workflow command per finding, capped at nine,
    plus one summary annotation when findings are omitted. All formats write
    to stdout.
  - Annotations use `file=` with the cassette path as configured, a
    `title=` of `spycatcher-harness verify: <class>.<code>`, and the message
    and JSON Pointer as data. Line numbers are omitted, because
    `serde_json::Value` carries no spans. The exception is a schema parse
    error, which reports the `serde_json` line and column.

  Rationale: JSON serves tools and the later 2.2.2 mismatch reports, and
  workflow commands give native pull-request annotations with no extra
  action. Rejected: Static Analysis Results Interchange Format (SARIF), which
  needs the code-scanning upload action and permissions and is heavier than
  the requirement. Also rejected: computing line numbers with a span-aware
  parser, which needs a new dependency.
  Date/Author: 2026-11-15, planning agent.
- D6. Decision: the required redaction policy reuses `RedactionConfig`. Verify
  gains a config-only `cmds.verify.redaction.drop_headers` layer, mirroring
  how `cmds.record.upstream` is config-only. The default is
  `RedactionConfig::default()` (`["authorization"]`). Header matching reuses
  the record path's case-insensitive predicate (`should_persist_header` in
  `src/http_exchange.rs`, widened to `pub(crate)`). Only names are checked,
  never values.
  Rationale: one predicate and one default guarantee that a cassette recorded
  under policy `P` passes verify under `P`. Rejected: a wider hard-coded deny
  list (`cookie`, `x-api-key`), which would fail cassettes recorded under the
  documented default. Users widen the policy in configuration instead. Also
  rejected: value heuristics such as `Bearer` prefixes, which are deferred
  because they produce false positives and risk echoing secrets.
  Date/Author: 2026-11-15, planning agent.
- D7. Decision: Verus is warranted only for the ordering kernel
  `first_descent`, which returns the first index whose value is smaller than
  its predecessor. The proof shows that the adjacent-pair scan is complete
  for global non-decreasing order over sequences of any length. All other
  obligations use `rstest`, `proptest`, `insta`, BDD, and end-to-end tests.
  The bridge to production is decided in `EP-M0`. Preferred: the production
  kernel is written in the Verus-verifiable subset, and a contract test
  asserts that its body is textually identical to the verified mirror.
  Fallback: Verus proves a loop mirror, and a `proptest` property checks the
  production kernel against an independent quadratic oracle, with the
  residual gap recorded.
  Rationale: the kernel carries the one unbounded, contractual invariant in
  this feature (`INV-order-complete`). Formatting, escaping, and plumbing are
  covered better by tests. Rejected: Kani, which is bounded and adds nothing
  here that the unbounded proof does not.
  Date/Author: 2026-11-15, planning agent.
- D8. Decision: verify reads at most 256 MiB, as `MAX_VERIFY_DOCUMENT_BYTES`,
  and reports `io.cassette_too_large` beyond that.
  Rationale: `AGENTS.md` requires byte bounds on input. 256 MiB comfortably
  exceeds realistic cassettes while bounding memory. Replay has no such
  ceiling today; that gap is recorded for roadmap follow-up and not changed
  here.
  Date/Author: 2026-11-15, planning agent.
- D9. Decision: verify detects inconsistency and accidental leakage, not
  adversarial tampering. Hashes are unkeyed SHA-256, so anyone can recompute
  them after an edit.
  Rationale: this states the guarantee honestly. Authenticity would need
  signing, which is out of scope.
  Date/Author: 2026-11-15, planning agent.
- D10. Decision: findings are ordered deterministically by interaction index
  (document-level findings first), then class in the order `io`, `schema`,
  `ordering`, `hash`, `redaction`, then JSON Pointer.
  Rationale: stable output makes snapshots, CI diffs, and the nine-annotation
  cap reproducible.
  Date/Author: 2026-11-15, planning agent.

## Outcomes & retrospective

Not started. Complete at each milestone boundary and at completion, including
the reconciliation of discoveries with the upstream documents named in
`Conformance basis`.

## Context and orientation

Base revision inspected: `c3e6d76d9ca300d17c6139e6ec8a344f3e971e2a`. Line
numbers below refer to that revision; relocate symbols with `rg -n '<symbol>'
src tests`.

Repository shape. This is a single Rust crate, `spycatcher-harness`, with
library `src/lib.rs` and binary `src/bin/spycatcher_harness.rs`. It uses
edition 2024, `rust-version = "1.89.0"`, and the toolchain
`nightly-2026-11-13`.

Cassette domain (`src/cassette/`):

- `src/cassette/mod.rs` (305 lines) defines:
  - `CassetteFormatVersion(u32)`, with `SUPPORTED = 1`;
  - `Cassette { format_version, interactions: Vec<Interaction> }`, with
    `validate`, `from_reader`, and `write_to`;
  - `Interaction { request, response, metadata }`;
  - `RecordedRequest`, with fields `method`, `path`, `query`, `headers:
    Vec<(String, String)>`, `body: Vec<u8>`, `parsed_json: Option<Value>`,
    `canonical_request: Option<Value>`, and `stable_hash: Option<String>`;
  - `RecordedResponse::{NonStream, Stream}`, where the `Stream` variant has
    fields `status`, `headers`, `events`, `raw_transcript`, and
    `timing: Option<StreamTiming>`;
  - `StreamEvent::{Comment, Data}`;
  - `StreamTiming { ttft_ms, chunk_offsets_ms }`;
  - `InteractionMetadata`, with fields `protocol_id`, `upstream_id`,
    `recorded_at`, and `relative_offset_ms`.

  Headers serialize as two-element arrays, so a header name's JSON Pointer is
  `/interactions/<i>/request/headers/<j>/0`.
- `Cassette::from_reader` deserializes before checking the version, so a
  future-version file with a different shape surfaces as `InvalidCassette`
  rather than `UnsupportedCassetteFormatVersion`. Verify peeks at
  `format_version` first.
- `src/cassette/canonical/mod.rs` provides `canonicalize(&RecordedRequest,
  &IgnorePathConfig) -> Result<CanonicalRequest, CanonicalError>`,
  `stable_hash(&CanonicalRequest) -> String`, and
  `RecordedRequest::populate_canonical_fields`. The stored
  `canonical_request` is the envelope produced by the private
  `canonical_request_value`. `serde_json` is built with `preserve_order`, and
  `Value` equality on objects is order-insensitive, so comparing the stored
  `canonical_request` with the recomputed one does not depend on key order.
- `src/cassette/filesystem.rs` holds `FilesystemCassetteStore`
  (`pub(crate)`), with `open_for_replay`, `open_existing`, and the
  `open_rooted_parent` cap-std helper.
- Rule from the 1-2-1 plan: a new cassette child module is declared in
  `src/cassette/mod.rs`, because Whitaker `self_named_module_files` forbids a
  `verify.rs` next to a `verify/` directory.

SSE parsing: `src/sse.rs` holds `SseParser` (`pub(crate)`), with `feed(&[u8])`
and `finish()` returning `Result<Vec<StreamEvent>, SseParseError>`. Its tests
already show that results are independent of chunk boundaries
(`parser_handles_frames_split_across_every_boundary`).

Redaction:

- `src/config.rs:185-198` defines `RedactionConfig { drop_headers }` with the
  default `["authorization"]`.
- `src/http_exchange.rs:229-250` defines `redact_headers` (`pub(crate)`) and
  the private `should_persist_header`. Matching is case-insensitive.
- `src/protocol.rs:65-135` has property tests over `redact_headers`.
- `src/http_exchange.rs:118` defines `parse_json_bytes(&[u8]) ->
  Option<Value>`, the parser the record path uses for `parsed_json`.

CLI and binary:

- `src/cli/mod.rs` (343 lines) declares `Commands::{Record, Replay, Verify}`
  and `VerifyArgs { listen, cassette_dir, cassette_name, locale,
  fallback_locale, localization }`. `load_verify_config` merges
  `cmds.verify` and then calls `config_build::to_verify_config`
  (`src/cli/config_build.rs:54-59`).
- `RecordArgs.upstream` shows the config-only pattern: `#[serde(default)]` and
  `#[arg(skip)]` on an `Option<RecordUpstreamArgs>` declared in
  `src/cli_args.rs`.
- `src/bin/spycatcher_harness.rs` defines `main() -> eyre::Result<()>`,
  which loads configuration, builds the Fluent loader, and calls `run_harness`
  and then `start_harness`.
- `src/bin/spycatcher_harness/process_output.rs` owns the stdout and stderr
  writers (`write_display_output` and `write_error_output`), and
  `src/bin/spycatcher_harness/cli_startup.rs` exits `0` for help and version
  and `2` for parse errors.

Errors and localization:

- `src/error.rs` defines `HarnessError`; the `ModeNotYetImplemented` variant
  is at `:85`.
- `src/i18n.rs` maps errors to Fluent IDs; `localize_harness_error` is at
  `:101`.
- The only catalogue is `i18n/en-US/spycatcher-harness.ftl`, which contains
  `cli-verify-about` at `:34`.
- `src/i18n_tests.rs` and `tests/lifecycle_tests.rs:171-188` refer to
  `ModeNotYetImplemented`.

Tests and gates:

- BDD suites use `rstest-bdd` 0.6. Examples are
  `tests/cassette_schema_versioning_bdd.rs` and
  `tests/features/cassette_schema_versioning.feature`, with a
  `#[derive(Default, ScenarioState)]` world, `Slot<T>`, and
  `#[scenario(path = ..., name = ...)]`.
- End-to-end tests run
  `std::process::Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"))`
  with `env_clear()` and `tests/common/coverage_profile.rs`. An example is
  `tests/binary_localization_e2e.rs`.
- `insta` snapshots live in `tests/snapshots/`. `proptest` regressions are
  committed beside their suites.
- `googletest`, `pretty_assertions`, and `assert_cmd` are not used yet.
- The gates are `make check-fmt`, `make lint` (`interrogate` over `tests/`,
  `cargo doc`, clippy, and Whitaker), and `make test` (nextest plus doc
  tests). `make all` adds `spelling` and `test-workflow-contracts`.
  `make markdownlint` and `make nixie` are separate.
- CI is `.github/workflows/ci.yml`.

Budget survey: see `Tolerances`. Production files most likely to approach the
400-line limit are `src/cli/mod.rs` (343) and `src/cassette/mod.rs` (305).
New verify code therefore lives in new modules.

Signposted documentation:

- `docs/spycatcher-harness-design.md`: sections "Cassette definition",
  "Canonicalization and hashing", "Public library API surface" (including the
  `CassetteVerifier` paragraph at `:928`), "CLI shape", and "Implementation
  decisions".
- `docs/users-guide.md`, `docs/developers-guide.md`,
  `docs/testing-guide.md`, and `docs/replay-matching-guide.md`.
- `docs/rust-testing-with-rstest-fixtures.md`,
  `docs/rstest-bdd-users-guide.md`,
  `docs/reliable-testing-in-rust-via-dependency-injection.md`,
  `docs/rust-doctest-dry-guide.md`,
  `docs/complexity-antipatterns-and-refactoring-strategies.md`, and
  `docs/ortho-config-users-guide.md`, the last for the `cmds.verify`
  layering.
- `docs/documentation-style-guide.md`, for ADR structure and numbering.

Signposted skills: `execplans`, `rust-router`, `hexagonal-architecture`,
`rust-unit-testing`, `proptest`, `verus`, `rust-verification`,
`rust-errors`, `domain-cli-and-daemons`, `codegraph-mcp`, and
`en-gb-oxendict-style`.

## Conformance basis

Upstream artefacts inspected at `c3e6d76`:

- `docs/roadmap.md` task 2.2.1 (lines 192-202) and its dependencies 1.2.1
  and 1.2.2, both complete.
- `docs/spycatcher-harness-design.md`: "CLI shape" (`:939-963`), the
  "Public library API surface" paragraph recommending `CassetteVerifier`
  (`:928-933`), "Cassette definition" (`:229-287`), and "Canonicalization and
  hashing" (`:376-422`).
- `docs/adrs/adr-0001-*` (localization diagnostic switch) and
  `docs/adrs/adr-0002-*` (build standard). Both constrain, and neither is
  changed.
- No terms-of-reference document exists for this project.

Requirements:

- `REQ-schema`: verify checks the cassette format version (roadmap 2.2.1
  success criterion 1; design `CassetteVerifier`: "Verifies cassette format
  version").
- `REQ-ordering`: verify checks ordering (roadmap criterion 1; design:
  "Verifies sequential integrity").
- `REQ-hash`: verify recomputes hashes (roadmap criterion 1).
- `REQ-redaction`: verify checks the required redaction policies (roadmap
  criterion 1; design: "Verifies no secrets leaked (headers scrubbed)").
- `REQ-output`: failures are machine-readable and human-readable, suitable
  for CI annotations (roadmap criterion 2).
- `REQ-exit`: verify exits non-zero on any failure class (roadmap
  criterion 3).
- `REQ-cli`: `verify` validates cassette integrity and redaction as a
  subcommand with `cmds.verify` layering (design "CLI shape").

Gaps:

- `GAP-stub`: verify currently returns `ModeNotYetImplemented`.
- `GAP-anchor`: the roadmap link to
  `#recommended-additions-for-regression-suite-integration` is dead.
- `GAP-redaction-layering`: `RedactionConfig` is not layered from
  configuration for any subcommand.

Trace links:

```plaintext
REQ-schema    -> EP-M1 -> cassette::verify::schema tests; BDD "unsupported format version"; e2e exit 1
REQ-hash      -> EP-M2 -> cassette::verify::hash tests + proptest; BDD "tampered body"
REQ-ordering  -> EP-M3, EP-M4 -> ordering tests + proptest + verus/cassette_verify_ordering.rs; BDD "stream timing"
REQ-redaction -> EP-M5 -> redaction tests + agreement proptest; BDD "authorization header"
REQ-output    -> EP-M6 -> insta snapshots (text, json, github); escaping proptest; e2e --format
REQ-exit      -> EP-M1, EP-M6 -> outcome tests; e2e rstest table over every class
REQ-cli       -> EP-M5, EP-M6 -> cli layering tests; e2e cmds.verify config file
GAP-stub      -> EP-M6 -> tests/lifecycle_tests.rs updated; e2e pass case
GAP-anchor    -> EP-M7 -> make markdownlint; manual anchor check
GAP-redaction-layering -> EP-M5 -> tests/cli_layering cases for cmds.verify.redaction
```

## Verification plan

### Abstract model

A cassette document is a byte string `D`. Verification is a total function
`verify(D, P) -> R`, where:

- `P = (required_headers: set of lower-cased names, match_mode)`;
- `R = (outcome, findings: Seq<Finding>)`;
- `Finding = (class, code, interaction: Option<nat>, pointer: JSON Pointer)`.

The pipeline runs in four stages:

1. If `|D| > MAX`, then `R` has the single finding
   `io.cassette_too_large`.
2. If `D` is not JSON, has no unsigned 32-bit integer `format_version`, has
   an unsupported version, or fails to deserialize into `Cassette`, then `R`
   has exactly one `schema` finding and no other checks run.
3. Otherwise, with `C` the typed cassette, the findings are the union of four
   independent per-interaction rule sets: ordering, hash, redaction, and the
   stream rules. Each rule set is a pure function of `C` and `P`.
4. `outcome = Passed` if and only if `findings` is empty. `exit_code` is `0`
   for `Passed` and `1` for `Failed`.

Missing or unreadable files are modelled at the application boundary as `R`
with one `io` finding.

### Invariants and lemmas

- `INV-exit` (`REQ-exit`): `exit_code(R) == 0` if and only if `R.findings` is
  empty. Equivalently, every finding of every class yields a non-zero exit.
- `INV-order-complete` (`REQ-ordering`): for any sequence `s` of `u64`,
  `first_descent(s) == None` if and only if `s` is globally non-decreasing
  (`forall i <= j < |s|: s[i] <= s[j]`). Also, `first_descent(s) == Some(k)`
  implies `1 <= k < |s|`, `s[k-1] > s[k]`, and that `s[..k]` is
  non-decreasing, so `k` is the least violating index.
- `INV-order-scope` (`REQ-ordering`, `D3`): interaction start order is checked
  if and only if `P.match_mode == SequentialStrict`. Stream rules are checked
  regardless of mode.
- `INV-transcript` (`REQ-ordering`): for any recorder-produced stream,
  `SseParser` applied to the whole of `raw_transcript` equals `events`, so a
  correct recording never yields `ordering.stream_transcript_mismatch`.
- `INV-hash-sound` (`REQ-hash`): for any request on which
  `populate_canonical_fields(&IgnorePathConfig::default())` has run, the hash
  rules produce no finding. This rules out false positives.
- `INV-hash-complete` (`REQ-hash`): any edit to `method`, `path`, `query`, or
  `parsed_json` that changes the canonical envelope, without recomputing
  `stable_hash`, yields `hash.stable_hash_mismatch`. Any edit to `body` that
  changes its parsed JSON, without updating `parsed_json`, yields
  `hash.parsed_json_mismatch`.
- `INV-redaction-agree` (`REQ-redaction`, `D6`): for any header list `H` and
  policy `P`, the redaction findings for `redact_headers(H, P)` under `P`
  are empty. This ties record and verify together.
- `INV-redaction-complete` (`REQ-redaction`): for any persisted header whose
  name equals some `p` in `P.required_headers` under ASCII case folding,
  exactly one `redaction.required_header_present` finding names its pointer.
- `INV-no-secret-echo` (constraint): no rendered output, in any format,
  contains any header value, body byte, or stream payload from the cassette.
- `INV-annotation-safe` (`REQ-output`): every GitHub-format line is a single
  line. Escaped message data contains no carriage return (CR) or line feed
  (LF). Escaped property values contain no CR, LF, `,`, or `:`. Unescaping
  inverts escaping.
- `INV-deterministic` (`D10`): `verify` is a function of `(D, P)`. Repeated
  runs render byte-identical output in every format.
- `L-adjacent-global`, the lemma under `INV-order-complete`: if
  `s[i-1] <= s[i]` for every `0 < i < |s|`, then `s[i] <= s[j]` for every
  `i <= j < |s|`. Proof by induction on `j - i`, using transitivity of `<=`.

### Axioms (trusted, not verified here)

- `AXIOM-serde`: `serde_json` parsing, `Value` equality (order-insensitive for
  objects under `preserve_order`), and error line and column reporting behave
  as documented. Relied on by the schema and hash rules.
- `AXIOM-sha2`: `sha2::Sha256` is a deterministic function of its input.
  Relied on by `INV-hash-sound`.
- `AXIOM-canonical`: `canonicalize` and `stable_hash` are deterministic. This
  is established by task 1.2.2 tests and is not re-proved here.
- `AXIOM-sse-chunking`: `SseParser` output is independent of chunk boundaries.
  This is established by `src/sse.rs` tests and exercised again by the
  `INV-transcript` property.
- `AXIOM-gha`: GitHub Actions parses `::error <props>::<data>` workflow
  commands as documented. Data escapes `%` as `%25`, CR as `%0D`, and LF as
  `%0A`. Properties additionally escape `:` as `%3A` and `,` as `%2C`. The UI
  shows at most 10 error annotations per step. Source: GitHub Docs, "Workflow
  commands for GitHub Actions"; `@actions/core` `command.ts`; community
  discussion 68471.
- `AXIOM-verus`: the pinned Verus release and Z3 are sound for the fragment
  used. Only linear arithmetic over `int` and `Seq` is needed.

### Methods per obligation

- Obligation `INV-exit`.
  - Method: an exhaustive `rstest` table over every `FindingClass` variant
    (finite). A `proptest` property maps any non-empty generated finding
    multiset to `Failed` and exit `1`. End-to-end, an `rstest` table drives
    the real binary with one fixture cassette per class.
  - Rationale: the class set is finite and small. The externally owned promise
    is the process exit status, so it must be tested through the binary.
  - Artefacts: `src/cassette/verify/report_tests.rs` and
    `tests/verify_cli_e2e.rs`.
  - Evidence: Red is the e2e table failing first, because verify exits `1`
    with `mode not yet implemented` for the pass case and the stderr text is
    wrong for the failing cases. Green is all rows passing.
  - Non-vacuity: each fixture is first checked to produce exactly its intended
    class through `verify_document`. A seeded fault, mapping `Failed` to `0`,
    must turn the e2e rows red; revert it and record the result.
- Obligation `INV-order-complete` and `L-adjacent-global`.
  - Method: a Verus proof (unbounded), plus a `proptest` differential check of
    the production kernel against an independent quadratic oracle, plus named
    `rstest` cases (empty, single, equal run, descent at start, middle, and
    end).
  - Rationale: the lemma quantifies over sequences of every length. This is
    the one place the `verus` skill's "unbounded, small, pure kernel" test is
    met.
  - Artefacts: `verus/cassette_verify_ordering.rs`,
    `src/cassette/verify/ordering.rs`, and
    `src/cassette/verify/ordering_tests.rs`.
  - Evidence: `make verus` reports `verification results:: N verified,
    0 errors`. The proptest passes with 1,024 cases.
  - Non-vacuity: the proof file includes `proof fn` witnesses that build a
    sorted 3-element `Seq` and an unsorted one, and asserts the contract's
    antecedents for both. A seeded fault, changing the exec comparison from
    `>` to `>=`, must make Verus fail. Record the failing output, then revert.
    The proptest generator classifies cases as sorted or unsorted and as
    descent-at-0, interior, or last. Each class must reach at least 5% of
    cases, asserted with `prop_assume`-free classification counters.
- Obligation `INV-order-scope`, plus the stream timing rules.
  - Method: an `rstest` table over `MatchMode` crossed with ordered and
    reordered offsets. Named cases cover length mismatch, `ttft_ms` above the
    first offset, `timing: None`, and empty streams.
  - Artefact: `src/cassette/verify/stream_tests.rs`.
  - Non-vacuity: the reordered fixture must produce a finding under
    `SequentialStrict`, which proves the precondition is reachable, and none
    under `Keyed`.
- Obligation `INV-transcript`.
  - Method: `proptest`. Generate event sequences (comments and multi-line
    data) and serialize them into a transcript using an independent test-side
    SSE writer. Split the transcript at random chunk boundaries, feed the
    chunks through `SseParser` as the recorder does, and assert that the
    verify rule reports nothing. Then mutate one event by swapping two
    adjacent distinct events or editing one payload, and assert that the rule
    reports `ordering.stream_transcript_mismatch` with the first differing
    index.
  - Artefact: `src/cassette/verify/stream_tests.rs`.
  - Non-vacuity: the mutation branch filters out swaps of equal events,
    because those leave the sequence unchanged, and records the rejection
    rate. Fewer than 10% of cases may be rejected.
- Obligations `INV-hash-sound` and `INV-hash-complete`.
  - Method: `proptest` over generated requests (method case, query order,
    nested JSON bodies), run through `populate_canonical_fields`, asserting no
    findings. Mutation strategies (method, path, query pair, JSON leaf, body
    only) assert the specific finding code. Known-answer `rstest` vectors
    reuse the hash fixtures from `tests/features/canonical_request_hashing.feature`
    as an independent oracle.
  - Artefact: `src/cassette/verify/hash_tests.rs`.
  - Non-vacuity: the mutation strategy discards mutations that leave the
    canonical form unchanged, for example reordering JSON keys. Assert the
    discard rate is below 25% and log the class counts. Reordering keys must
    produce no finding, a negative control for false positives.
- Obligations `INV-redaction-agree` and `INV-redaction-complete`.
  - Method: `proptest` that generates header lists and policies, with names
    drawn partly from the policy in random ASCII case. One property asserts
    that verify over `redact_headers(H, P)` finds nothing. The other uses an
    independent oracle (`eq_ignore_ascii_case` over every pair) to compute the
    expected pointer set and asserts equality with the reported pointers.
  - Artefact: `src/cassette/verify/redaction_tests.rs`.
  - Non-vacuity: at least 30% of generated lists contain a policy header;
    classify and assert this. A seeded fault, making the comparison
    case-sensitive, must fail the completeness property.
- Obligation `INV-no-secret-echo`.
  - Method: `proptest` that plants random sentinel values (high-entropy
    strings) in header values, bodies, and stream payloads of a cassette that
    has findings of every class. Render all three formats and assert that no
    sentinel occurs.
  - Artefact: `src/cli/verify_output_tests.rs`.
  - Non-vacuity: the cassette must yield at least one redaction finding whose
    header carries a sentinel value, so a renderer that echoed values would
    fail.
- Obligation `INV-annotation-safe`.
  - Method: `proptest` over arbitrary Unicode strings, including CR, LF, `%`,
    `:`, and `,`. Assert that the escaped data and properties contain no
    forbidden characters and that a test-side decoder round-trips them. Named
    `rstest` injection cases cover header names such as
    `"x\n::add-mask::secret"`, `"a,b:c"`, and `"%0A"`.
  - Artefacts: `src/cli/verify_output_tests.rs`, plus `insta` snapshots of the
    GitHub rendering.
  - Non-vacuity: the generator includes forbidden characters in at least 50%
    of cases (classified). A seeded fault, dropping the `,` escape for
    properties, must fail.
- Obligation `INV-deterministic` and the output contract.
  - Method: `insta` snapshots of the text, JSON, and GitHub renderings of one
    multi-finding cassette that covers all five classes, plus the passing
    summary. Paths are normalized by running in a temporary directory with a
    relative `--cassette-dir`. Semantic `googletest` assertions on the parsed
    JSON report check `report_version`, outcome, and the class set. The JSON
    schema contract is tested by deserializing into a test-side mirror struct
    with `deny_unknown_fields`.
  - Artefacts: `tests/verify_cli_e2e.rs` and `tests/snapshots/verify_cli_e2e__*.snap`.

### Formal verification decision

Verus is used for `INV-order-complete` and `L-adjacent-global` only (`D7`).
The kernel is pure, total, and small. The property quantifies over every
sequence length, and the result is reused by both interaction and stream
checks. Kani would explore only bounded lengths. A property test samples,
which is why it accompanies the proof as the bridge to production rather than
replacing it.

Everything else stays with tests:

- Hash recomputation depends on SHA-256 and `serde_json`, which are axioms
  here.
- Escaping is a character map, and property tests with seeded faults give
  proportionate assurance.
- Exit mapping is a finite two-way match.

Stronger methods would add little. A Verus proof of escaping would cover a
`Seq<char>` mirror, not the shipped `String` code, and would leave the
GitHub parser unverified anyway. The proof leaves the production iterator or
loop form unverified unless the `EP-M0` textual-identity bridge succeeds;
that residual gap is stated in `D7`.

### Suggested BDD features

Proposed path: `tests/features/cassette_verify.feature`, bound by
`tests/cassette_verify_bdd.rs`, with steps in `tests/cassette_verify/`. The
steps drive `verify_cassette` and the renderers in-process. The binary is
covered by `tests/verify_cli_e2e.rs`.

```gherkin
Feature: Cassette verification

  Scenario: A freshly recorded cassette verifies cleanly
    Given a cassette recorded with two non-stream and one stream interaction
    When the cassette is verified
    Then verification passes with 3 interactions checked
    And the report has no findings

  Scenario: An unsupported format version is a schema failure
    Given a cassette with format version 9
    When the cassette is verified
    Then verification fails with finding "schema.unsupported_format_version"
    And no other checks are reported

  Scenario: A hand-edited request body is a hash failure
    Given a recorded cassette whose second request body was edited without rehashing
    When the cassette is verified
    Then verification fails with finding "hash.stable_hash_mismatch" for interaction 1

  Scenario: Out-of-order stream timing is an ordering failure
    Given a recorded stream cassette whose chunk offsets decrease at event 2
    When the cassette is verified
    Then verification fails with finding "ordering.stream_offsets_not_monotonic" for interaction 0

  Scenario: Interaction start order is only enforced for sequential replay
    Given a cassette whose interaction start offsets are 10, 5
    When the cassette is verified with keyed matching
    Then verification passes

  Scenario: An unredacted authorization header is a redaction failure
    Given a cassette whose first request carries an "Authorization" header
    When the cassette is verified
    Then verification fails with finding "redaction.required_header_present"
    And no report output contains the header value

  Scenario: A configured required header is enforced
    Given a cassette whose response carries an "x-api-key" header
    And verify requires redaction of "x-api-key"
    When the cassette is verified
    Then verification fails with finding "redaction.required_header_present" for interaction 0

  Scenario: A missing cassette is an io failure
    Given no cassette file exists
    When the cassette is verified
    Then verification fails with finding "io.cassette_not_found"

  Scenario: GitHub output escapes hostile header names
    Given a cassette whose request carries a header named "x\n::add-mask::oops"
    And verify requires redaction of that header
    When the report is rendered for GitHub Actions
    Then every output line starts with "::error " or is the summary line
    And no output line starts with "::add-mask::"
```

Each scenario maps to the requirement and milestone that its step names cite
in `Conformance basis`. The scenarios are written red in the milestone that
introduces their rule. Each scenario exists in the feature file from that
milestone, failing until the rule lands.

## Plan of work

Orientation for the new modules (all new unless marked):

```plaintext
src/cassette/mod.rs                 (edit) declare `pub mod verify;`
src/cassette/verify/mod.rs          domain entry `verify_document`, policy, orchestration
src/cassette/verify/report.rs       Finding, FindingClass, FindingCode, VerificationReport, outcome
src/cassette/verify/schema.rs       size, JSON, version peek, typed deserialization
src/cassette/verify/ordering.rs     `first_descent` kernel + interaction start-order rule
src/cassette/verify/stream.rs       stream timing + transcript re-parse rules
src/cassette/verify/hash.rs         hash, canonical envelope, and parsed_json rules
src/cassette/verify/redaction.rs    required-header rule
src/cassette/verify/*_tests.rs      unit and property tests per rule
src/verify.rs                       application service `verify_cassette` (bounded cap-std read)
src/cli/verify_output.rs            text/json/github renderers + workflow-command escaping
src/cli/verify_output_tests.rs      renderer, escaping, and secret-echo tests
src/bin/spycatcher_harness/verify_run.rs  binary dispatch, stdout writer, ExitCode
verus/cassette_verify_ordering.rs   Verus proof of the ordering kernel
tools/verus/{VERSION,SHA256SUMS,README.md}, tools/rust-prover-tools/REF
```

### `EP-M0` feasibility probe (prototype)

Questions:

1. Can `prover-tools verus install --repo-root .` install a pinned Verus
   release on this host?
2. Can `prover-tools verus run` verify a scratch file containing the
   candidate kernel?
3. Can the production kernel be written in a form that is both clippy-clean
   under this repository's lints (`indexing_slicing` denied) and accepted by
   Verus, so that its body can be textually identical?
4. Do `googletest` and `pretty_assertions` build under the minimum supported
   Rust version (MSRV) 1.89 and with `rstest` 0.28?

Steps:

1. Resolve the latest Verus release tag and its x86-linux archive checksum
   with `gh release view --repo verus-lang/verus`.
2. Resolve the `leynos/rust-prover-tools` `main` commit with
   `git ls-remote https://github.com/leynos/rust-prover-tools.git HEAD`.
3. Write the pins into `tools/verus/VERSION`, `tools/verus/SHA256SUMS`, and
   `tools/rust-prover-tools/REF` (`ref: <40-hex>`).
4. Write `/tmp/verus-probe/kernel.rs` with the two candidate kernel bodies:
   - a `while` loop using `values.get(i)` pattern matching;
   - a `while` loop using `values[i]` with the guard `i < values.len()`.
5. Run `prover-tools verus run --repo-root . --proof-file
   /tmp/verus-probe/kernel.rs`.
6. In a scratch branch, run `cargo +1.89.0 check --tests` with the two
   dev-dependencies added.

Go/no-go:

- If Verus cannot be installed or run, stop and escalate (`D7` tolerance).
- If one candidate body passes both Verus and clippy unmodified, adopt the
  textual-identity bridge.
- Otherwise adopt the fallback bridge: a verified loop mirror plus the
  `proptest` oracle. Record the residual gap in `D7`.

The probe files in `/tmp` are deleted. The pins are retained and committed in
`EP-M4`. Record the transcripts in `Artefacts and notes`.

### `EP-M1` report model, schema checks, and outcome mapping

Assigned: `REQ-schema`, `REQ-exit` (domain half), `D1`, `D2`, `D8`, `D10`.

Red:

1. Add the dev-dependencies `googletest` and `pretty_assertions` to
   `Cargo.toml`.
2. Write `src/cassette/verify/report_tests.rs` and `schema_tests.rs`
   covering:
   - the exhaustive class-to-outcome table;
   - an empty document;
   - non-JSON input;
   - a missing `format_version`;
   - a `format_version` of `-1`, `"1"`, and `4294967296`;
   - version `9`;
   - version `1` with an `interactions` field of the wrong type, which must
     report `schema.malformed_document` with a line and column;
   - a document over the size ceiling, which uses a test-only lowered ceiling
     through a `pub(crate)` constructor parameter rather than a 256 MiB
     fixture;
   - a valid empty cassette.
3. Add the BDD feature with the schema scenario.
4. Run
   `cargo nextest run --all-features -E 'test(verify)'`. It fails to
   compile because the module is missing, which is the expected red.

Green: implement `report.rs`, `schema.rs`, and `verify_document` with the
schema stage only. Declare `pub mod verify` in `src/cassette/mod.rs`.

Refactor: keep each file under 400 lines, then run `make check-fmt`,
`make lint`, and `make test`.

### `EP-M2` hash recomputation and request integrity

Assigned: `REQ-hash`, `D4`, `INV-hash-sound`, `INV-hash-complete`.

Red: write `hash_tests.rs` with the following cases and the two properties
from the verification plan, and add the BDD hash scenario. All of them fail
until the rule is wired in.

- a recorded request with no findings;
- `stable_hash` missing;
- `canonical_request` missing;
- an edited `stable_hash`;
- an edited `parsed_json` leaf;
- a body edit with stale `parsed_json`;
- a non-JSON body whose `parsed_json` is `Some`;
- reordered JSON keys, which must produce no finding.

Green: implement `hash.rs`. It recomputes with `canonicalize(request,
&IgnorePathConfig::default())`, compares `stable_hash` with `stable_hash(&c)`,
and compares the stored `canonical_request` `Value` with the recomputed
envelope. Expose the private `canonical_request_value` as `pub(crate)` from
`src/cassette/canonical/mod.rs` rather than duplicating it. It also compares
`parsed_json` with `http_exchange::parse_json_bytes(&body)`. Pointers are
`/interactions/<i>/request/stable_hash`, `.../canonical_request`, and
`.../parsed_json`.

Gates: `make check-fmt`, `make lint`, `make test`.

### `EP-M3` ordering checks

Assigned: `REQ-ordering`, `D3`, `INV-order-complete` (tests),
`INV-order-scope`, `INV-transcript`.

Red: write `ordering_tests.rs` (named kernel cases, the oracle proptest, and
the interaction start-order table over `MatchMode`) and `stream_tests.rs`
(length mismatch, non-monotonic offsets, `ttft_ms` above the first offset,
`timing: None`, empty transcript skip, the transcript mismatch, and the
`INV-transcript` property). Add the BDD ordering scenarios.

Green:

1. Implement `first_descent(values: &[u64]) -> Option<usize>` in the form
   chosen by `EP-M0`.
2. Implement the interaction rule using `relative_offset_ms`, emitting
   `ordering.interaction_start_order` at `/interactions/<k>/metadata/relative_offset_ms`.
3. Implement the stream rules, emitting:
   - `ordering.stream_timing_length_mismatch`;
   - `ordering.stream_offsets_not_monotonic` at
     `/interactions/<i>/response/timing/chunk_offsets_ms/<k>`;
   - `ordering.stream_ttft_after_first_event`;
   - `ordering.stream_transcript_unparseable`;
   - `ordering.stream_transcript_mismatch` at
     `/interactions/<i>/response/events/<k>`.

   The transcript re-parse uses `SseParser::feed` over the whole transcript,
   then `finish`.
4. `SseParser` is `pub(crate)` already; no visibility change is expected.

Gates: as before.

### `EP-M4` Verus proof of the ordering kernel

Assigned: `INV-order-complete` (proof), `L-adjacent-global`, `D7`.

Steps:

1. Commit the pins from `EP-M0`, and add `tools/verus/README.md` stating what
   is pinned and how to bump it.
2. Add Makefile targets `install-verus` and `verus` that delegate through
   `$(PROVER_TOOLS)` exactly as in the `verus` skill's on-ramp. They include
   the `override` REF guard and the command-line-only contract-test seam, set
   `VERUS_PROOF_FILE ?= verus/cassette_verify_ordering.rs`, and stay out of
   `all`.
3. Write `verus/cassette_verify_ordering.rs`. It starts with `//!` docs and
   contains:
   - `spec fn non_decreasing(s: Seq<int>) -> bool`, defined globally as
     `forall i, j: 0 <= i <= j < |s| ==> s[i] <= s[j]`;
   - `spec fn adjacent_non_decreasing(s)`;
   - `proof fn lemma_adjacent_implies_global(s)`, by induction;
   - `proof fn lemma_global_implies_adjacent(s)`;
   - the exec `first_descent` with an `ensures` stating the `INV-order-complete`
     contract and a loop invariant that `s[..i]` is adjacent non-decreasing;
   - the non-vacuity witness lemmas.

   Each `proof fn` has `///` docs.
4. If `EP-M0` selected the textual bridge, add
   `tests/verus_kernel_contract.rs`. It extracts the bodies between
   `// verus-mirror:begin first_descent` and `// verus-mirror:end` markers in
   both files, normalizes whitespace, and asserts equality with a diagnostic
   diff.
5. Add `tests/verus_tooling_contract.rs`, with:
   - non-empty `VERSION` and `SHA256SUMS`;
   - a checksum row naming the configured archive;
   - a REF line matching `^ref: [0-9a-f]{40}$`;
   - a `make -n verus PROVER_TOOLS_CONTRACT_TEST=1 PROVER_TOOLS=<fake>` dry
     run asserting the exact `verus run --repo-root . --proof-file
     verus/cassette_verify_ordering.rs` invocation;
   - a check that the recipes contain no `curl`, `unzip`, `sha256sum`, or
     `rustup toolchain install`.
6. Run the seeded fault (`>` to `>=` in the exec comparison), record that
   Verus fails, and revert.

Acceptance:

- `make verus` reports `0 errors`, and the seeded fault reports at least one
  error.
- `make check-fmt`, `make lint`, and `make test` pass.
- `make test-make-all` still passes, which shows that `all` is unchanged.

### `EP-M5` redaction checks and layered verify configuration

Assigned: `REQ-redaction`, `REQ-cli` (configuration half), `D6`,
`GAP-redaction-layering`, `INV-redaction-agree`, and `INV-redaction-complete`.

Red: write `redaction_tests.rs` (named cases for request and response
headers, mixed case, duplicates each reported, an empty policy, plus the two
properties). Add a `tests/cli_layering/` case showing that
`[cmds.verify.redaction] drop_headers = ["authorization", "x-api-key"]` and
the environment variable
`SPYCATCHER_HARNESS_CMDS_VERIFY_REDACTION__DROP_HEADERS` reach
`HarnessConfig.redaction`. Add the BDD redaction scenarios.

Green:

1. Widen `should_persist_header` to `pub(crate)`. Its name stays the same, so
   the record path is unchanged.
2. Implement `redaction.rs`, which emits
   `redaction.required_header_present` at
   `/interactions/<i>/{request,response}/headers/<j>/0` with the lower-cased
   header name as a finding parameter.
3. Add `VerifyRedactionArgs { drop_headers: Option<Vec<String>> }` to
   `src/cli_args.rs`, and a config-only `redaction` field on `VerifyArgs`.
4. In `to_verify_config`, set `config.redaction` when it is supplied. An
   explicit list replaces the default, so an empty list disables the check,
   as documented.

Gates: as before.

### `EP-M6` application service, output formats, and CLI

Assigned: `REQ-output`, `REQ-exit`, `REQ-cli`, `GAP-stub`, `D5`,
`INV-exit` (e2e), `INV-no-secret-echo`, `INV-annotation-safe`, and
`INV-deterministic`.

Red:

1. Write `tests/verify_cli_e2e.rs` with:
   - the pass case (exit `0`);
   - an `rstest` table with one fixture per class (exit `1`, with the code
     present in each format);
   - `--format json` parsed and checked with `googletest` matchers;
   - `--format github` line-shape checks;
   - `cmds.verify.format = "json"` from a configuration file;
   - `insta` snapshots for the three renderings of the all-classes fixture.
2. Write `src/cli/verify_output_tests.rs` with the escaping, injection, and
   secret-echo properties.
3. Update `tests/lifecycle_tests.rs:171-188` to expect `InvalidConfig` from
   `start_harness` with `Mode::Verify`.

Green:

1. Add `pub fn verify_cassette(config: &HarnessConfig) ->
   HarnessResult<VerificationReport>` in `src/verify.rs`, re-exported from
   `src/lib.rs`. It runs `validate_config`, opens the cassette read-only
   through `cap_std::fs_utf8`, reusing `open_rooted_parent`, and reads at
   most `MAX_VERIFY_DOCUMENT_BYTES + 1` bytes. It maps not-found and other
   I/O failures to `io` findings, then calls
   `cassette::verify::verify_document(bytes, &VerifyPolicy::from(config))`.
   It returns `Err` only for configuration errors.
2. Add `config::VerifyFormat { Text, Json, Github }` (serde snake_case and
   clap `ValueEnum`) and a `format` field on `VerifyArgs` (`--format`).
   Carry it in `HarnessConfig` as `verify_format: VerifyFormat`; see
   `Interfaces and dependencies`.
3. Add `src/cli/verify_output.rs` with
   `render_report(report, format, cassette_path, loader, writer: &mut impl Write)`
   and the two escaping functions. Human messages come from new Fluent keys
   `verify-*` in `i18n/en-US/spycatcher-harness.ftl`. JSON carries both the
   stable `code` and the localized `message`.
4. Add `src/bin/spycatcher_harness/verify_run.rs`. Change `main` to return
   `eyre::Result<ExitCode>`. Branch on `Mode::Verify` before building the
   Tokio runtime: call `verify_cassette`, render the report to a locked
   stdout, and return `ExitCode::SUCCESS` or `ExitCode::from(1)`.
5. Change `start_harness`'s `Mode::Verify` arm to return
   `HarnessError::InvalidConfig` with the message
   `verify mode does not start a server; call verify_cassette`.
6. Remove `HarnessError::ModeNotYetImplemented`, its Fluent key, its
   `i18n.rs` constant and arm, and its test fixtures in `src/error.rs` and
   `src/i18n_tests.rs`.

Refactor: confirm that `src/cli/mod.rs` stays under 400 lines, and move
`VerifyArgs` to `src/cli/verify_args.rs` if it would not.

Gates: `make check-fmt`, `make lint`, `make test`, then `make verus`.

### `EP-M7` documentation, ADRs, roadmap closure, and gates

Assigned: `GAP-anchor`, the documentation duties in `AGENTS.md`, and
`REQ-output` (documented schema).

1. `docs/users-guide.md`:
   - Replace the "Verify is currently a CLI-only command" and
     `ModeNotYetImplemented` text at `:9` and `:28-30`.
   - Add a "Verifying cassettes" section after the CLI output and exit-status
     text near `:446`, covering checks, failure classes and codes, exit
     codes, the three formats with examples, the JSON report fields, a GitHub
     Actions workflow snippet, `cmds.verify.redaction.drop_headers`, the
     hand-authoring rules from `D4`, and the tampering limitation from `D9`.
   - Reconcile the example at `:497-503`.
2. `docs/developers-guide.md`:
   - Update the module tables at `:281-293`, `:309-318`, and `:371-379` with
     `cassette::verify`, `verify`, and `cli::verify_output`.
   - Add a "Cassette verification" subsection describing the domain, service,
     and adapter split, how to add a rule, and the reuse policy for
     `first_descent`, workflow-command escaping, and `should_persist_header`.
   - Add a "Formal verification" subsection describing `make install-verus`,
     `make verus`, the pins, the proof inventory, and what the proof does not
     cover.
3. `docs/spycatcher-harness-design.md`:
   - Turn the "Recommended additions for regression suite integration"
     paragraph into a `###` heading, so the roadmap anchor resolves, and
     point it at the implementation.
   - Update "CLI shape" with `--format`.
   - Add an "Implementation decisions" entry
     `### Cassette verification (task 2.2.1)` that summarizes `D1` to `D10`
     and links the ADRs.
   - Update the `start_harness` and `HarnessError` example to drop
     `ModeNotYetImplemented`.
4. Add `docs/adrs/adr-0003-define-the-cassette-verification-report-and-exit-contract.md`
   (the `D2`, `D5`, and `D10` contract) and
   `docs/adrs/adr-0004-adopt-verus-for-selected-unbounded-invariants.md`
   (`D7` and the tooling route). Follow the documentation style guide
   (`:521-588`).
5. `docs/contents.md`: list both ADRs and this ExecPlan.
6. `docs/roadmap.md`: tick 2.2.1 and its three success criteria only after
   `EP-M6` acceptance passes.
7. Run `make fmt` and keep only hunks in files this plan touched. Then run
   `make check-fmt`, `make lint`, `make test`, `make markdownlint`,
   `make nixie`, `make spelling`, and `make verus`.

## Milestones and plateaus

- `EP-M0`: probe only, with no tracked changes except pins held for `EP-M4`.
  Recovery: delete `/tmp/verus-probe`. Compatibility: none.
- `EP-M1`:
  - Outcome: `cassette::verify::verify_document` reports schema findings and
    computes the outcome. The CLI is still the stub.
  - Acceptance: the schema unit tests and BDD schema scenario pass.
  - Conformance: `REQ-schema` is satisfied in the domain; no persisted-format
    change; no new runtime dependency.
  - Recovery: revert the milestone commit.
  - Remaining: other classes and the CLI. Compatibility: none.
- `EP-M2`:
  - Outcome: hash findings.
  - Acceptance: hash tests, properties, and the BDD scenario pass.
  - Conformance: canonical hashing is reused, not altered (verify with
    `git diff origin/main -- src/cassette/canonical`, which may show only the
    `pub(crate)` visibility change).
  - Recovery: revert. Compatibility: none.
- `EP-M3`:
  - Outcome: ordering findings.
  - Acceptance: ordering and stream tests, properties, and BDD scenarios
    pass.
  - Conformance: `D3` is honoured, and `SseParser` is unchanged.
  - Recovery: revert. Compatibility: none.
- `EP-M4`:
  - Outcome: proved kernel and `make verus`.
  - Acceptance: `make verus` reports 0 errors, the seeded fault is rejected,
    and the contract tests pass.
  - Conformance: Verus is outside `all`, and the trusted assumptions are
    inspected.
  - Recovery: revert. The kernel tests from `EP-M3` still stand.
    Compatibility: none.
- `EP-M5`:
  - Outcome: redaction findings and `cmds.verify.redaction` layering.
  - Acceptance: redaction tests, properties, layering tests, and BDD pass.
  - Conformance: the record path's predicate is reused and record behaviour
    is unchanged.
  - Recovery: revert. Compatibility: none.
- `EP-M6`:
  - Outcome: `spycatcher-harness verify` works end to end in three formats
    with the specified exit codes. `start_harness(Verify)` returns
    `InvalidConfig`.
  - Acceptance: e2e tests, snapshots, renderer properties, and the lifecycle
    test pass.
  - Conformance: `REQ-output`, `REQ-exit`, and `REQ-cli` are satisfied. The
    public API changes are exactly those listed in `Tolerances`.
  - Recovery: revert.
  - Compatibility: none, because the library is pre-release.
- `EP-M7`:
  - Outcome: the documentation is accurate, the ADRs are accepted, and the
    roadmap is ticked.
  - Acceptance: all gates pass, and the anchor resolves.
  - Conformance: discoveries are reconciled with the design and roadmap.
  - Recovery: revert the documentation commit.

## Concrete steps

The working directory is the repository root. Gates run sequentially, and
each is piped through `tee` with `set -o pipefail`:

```bash
set -o pipefail
BRANCH=$(git branch --show-current)
make check-fmt 2>&1 | tee /tmp/check-fmt-spycatcher-harness-$BRANCH.out
make lint      2>&1 | tee /tmp/lint-spycatcher-harness-$BRANCH.out
make test      2>&1 | tee /tmp/test-spycatcher-harness-$BRANCH.out
make verus     2>&1 | tee /tmp/verus-spycatcher-harness-$BRANCH.out   # from EP-M4 onwards
```

Focused red and green loops per milestone:

```plaintext
Red:   cargo nextest run --all-features -E 'test(/verify/)'  -> failing or not compiling for the named missing rule
Green: same command -> all selected tests pass
BDD:   cargo nextest run --all-features --test cassette_verify_bdd
E2E:   cargo nextest run --all-features --test verify_cli_e2e
Snap:  cargo insta review (accept only after confirming the rendering is the intended contract)
Proof: make verus -> "verification results:: N verified, 0 errors"
```

Commit after each milestone's gates pass, using an imperative subject, for
example `Add hash recomputation checks to cassette verify`.

## Validation and acceptance

Behavioural acceptance, run from a temporary directory holding a fresh
recording:

- `spycatcher-harness verify --cassette-dir fixtures/llm --cassette-name ok.json`
  prints `verified fixtures/llm/ok.json: N interactions, 0 findings` and
  exits `0`.
- After replacing one request's `stable_hash` with 64 zeros, the same command
  prints a `hash.stable_hash_mismatch` line and exits `1`. With
  `--format json`, `jq '.outcome, .findings[0].code'` yields `"failed"` and
  `"stable_hash_mismatch"`. With `--format github`, the first line starts with
  `::error file=fixtures/llm/ok.json,title=spycatcher-harness verify%3A hash.stable_hash_mismatch::`.

Quality criteria:

- Tests: `make test` passes, including the new unit, property, BDD,
  end-to-end, and snapshot tests. Each new test is shown red before its rule
  lands.
- Verification: `make verus` passes. `INV-order-complete` and
  `L-adjacent-global` are discharged by Verus. Every other invariant is
  discharged by the tests named in `Methods per obligation`, with the
  non-vacuity evidence recorded.
- Lint and typecheck: `make check-fmt`, `make lint`, `make markdownlint`,
  `make nixie`, and `make spelling` pass.
- Security: `INV-no-secret-echo` and `INV-annotation-safe` properties pass,
  and their seeded faults are rejected.

## Idempotence and recovery

Every step is re-runnable. Verify is read-only. `prover-tools verus install`
is idempotent and installs into `.verus/`, which must be added to
`.gitignore` in `EP-M4` if `prover-tools` does not already ignore it.
Seeded-fault experiments are made in the working tree, recorded, and reverted
before committing. Confirm with `git diff --stat`. Generated `proptest`
regression files are committed only when they capture a genuine fixed
failure.

## Artefacts and notes

Reconnaissance evidence (obtained at `c3e6d76`):

```plaintext
$ command -v verus cargo-kani prover-tools
(verus missing) /home/leynos/.cargo/bin/cargo-kani /home/leynos/.local/bin/prover-tools
$ git log --oneline -S "Recommended additions for regression suite" -- docs
5430ebc Initial design (#4)      # paragraph, never a heading
```

GitHub workflow-command reference (2026-11-15):
`::error file={name},line={line},endLine={endLine},title={title}::{message}`.

Probe, red/green, seeded-fault, and proof transcripts are appended here during
implementation.

## Interfaces and dependencies

Domain (`src/cassette/verify/`, public under `spycatcher_harness::cassette::verify`):

```rust
pub struct VerifyPolicy {
    pub required_redactions: RedactionConfig,
    pub match_mode: MatchMode,
}

impl From<&HarnessConfig> for VerifyPolicy { /* copies redaction and match_mode */ }

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingClass { Io, Schema, Ordering, Hash, Redaction }

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub class: FindingClass,
    pub code: FindingCode,              // `#[non_exhaustive]` enum, snake_case serde
    pub interaction: Option<usize>,
    pub pointer: String,                // RFC 6901 JSON Pointer into the document
    pub detail: FindingDetail,          // typed parameters, never header values or bodies
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerificationReport {
    pub interactions_checked: usize,
    pub findings: Vec<Finding>,         // sorted per D10
}

impl VerificationReport {
    #[must_use] pub fn outcome(&self) -> VerificationOutcome;  // Passed iff findings empty
}

#[must_use]
pub fn verify_document(document: &[u8], policy: &VerifyPolicy) -> VerificationReport;
```

`pub(crate) fn first_descent(values: &[u64]) -> Option<usize>` in
`ordering.rs` is the only ordering kernel. Any future monotonicity check, for
example in task 3.1.1 timing, must reuse it rather than re-implementing a
scan.

The application service in `src/verify.rs` is re-exported at the crate root:

```rust
pub const MAX_VERIFY_DOCUMENT_BYTES: u64 = 256 * 1024 * 1024;
pub fn verify_cassette(config: &HarnessConfig) -> HarnessResult<VerificationReport>;
```

Configuration (`src/config.rs`): add `VerifyFormat { Text, Json, Github }` and
`HarnessConfig.verify_format: VerifyFormat` (default `Text`). The existing
`HarnessConfig.redaction` and `match_mode` fields are reused. This touches
`HarnessConfig` struct literals in tests. `..HarnessConfig::default()` is
already the dominant pattern; update the remaining literals in the same
commit.

The CLI adapter (`src/cli/verify_output.rs`, `pub` within `spycatcher_harness::cli`)
provides:

```rust
pub fn render_report(
    report: &VerificationReport,
    format: VerifyFormat,
    cassette_path: &Utf8Path,
    loader: &FluentLanguageLoader,
    writer: &mut impl std::io::Write,
) -> std::io::Result<()>;
```

It also has two `pub(crate)` escaping helpers, `escape_workflow_data` and
`escape_workflow_property`. These are the only workflow-command escapers;
2.2.2 mismatch annotations must reuse them.

The JSON report shape (`report_version` 1) is documented in ADR 0003 and the
users' guide:

```json
{
  "report_version": 1,
  "cassette": "fixtures/llm/smoke.json",
  "outcome": "failed",
  "interactions_checked": 3,
  "findings": [
    {
      "class": "hash",
      "code": "stable_hash_mismatch",
      "interaction": 1,
      "pointer": "/interactions/1/request/stable_hash",
      "message": "stored hash does not match the hash recomputed from the request"
    }
  ]
}
```

The binary (`src/bin/spycatcher_harness/verify_run.rs`) provides
`fn run_verify(config: &HarnessConfig, loader: &FluentLanguageLoader) ->
eyre::Result<ExitCode>`. `main` becomes `fn main() -> eyre::Result<ExitCode>`.

Reused, not duplicated:

- `canonicalize` and `stable_hash`, with `canonical_request_value` widened to
  `pub(crate)`;
- `http_exchange::parse_json_bytes`;
- `http_exchange::should_persist_header`, widened to `pub(crate)`;
- `sse::SseParser`;
- `cassette::filesystem::open_rooted_parent`;
- `i18n` Fluent loading;
- `process_output` writer locking conventions.

Dependencies: dev-only `googletest = "0.14"` and `pretty_assertions = "1.4"`,
with the exact patch versions confirmed in `EP-M0`. There is no new runtime
dependency. Verus is an external tool pinned in `tools/verus/`, run through
`prover-tools`, which is pinned by `tools/rust-prover-tools/REF`.

## Revision note

- 2026-11-15: initial draft from Wyvern reconnaissance; awaiting
  community-of-experts review.
