# 2.2.1 Implement the `verify` subcommand for cassette integrity and redaction

This ExecPlan (execution plan) is a living document. The sections `Constraints`,
`Tolerances`, `Risks`, `Progress`, `Surprises & discoveries`, `Decision log`,
`Outcomes & retrospective`, `Conformance basis`, and `Verification plan` must
be kept up to date as work proceeds.

Status: DRAFT (expert review applied; awaiting user approval)

Branch: `2-2-1-verify-subcommand-for-cassette-integrity`

Roadmap item: `2.2.1` in `docs/roadmap.md`.

Implementation must not begin until the user explicitly approves this plan.
After approval, work proceeds milestone by milestone. It stops only when a
tolerance threshold is reached, a constraint would be violated, or a decision
gate below says to stop.

## Purpose / big picture

Spycatcher harness records Large Language Model (LLM) API sessions to a
"cassette": one JSON file holding an ordered list of request/response
"interactions". Regression suites then replay them. Cassettes are committed to
repositories, edited by hand, merged, and re-recorded. Nothing checks that a
committed cassette is still internally consistent or free of credentials. The
command line already accepts `verify`, but running it fails:

```plaintext
$ spycatcher-harness verify --cassette-dir fixtures/llm --cassette-name smoke.json
Error: failed to start harness: mode not yet implemented: Verify
```

After this task, the same command checks five classes and reports every finding
in one run:

- `io`: the cassette exists, is a regular file, can be read, and is within the
  size ceiling;
- `schema`: the file is JSON, has a supported `format_version`, and
  deserializes into the cassette schema;
- `ordering`: interactions and recorded stream events are in recorded order
  (defined precisely in `D3`);
- `hash`: every request's stored `stable_hash`, `canonical_request`, and
  `parsed_json` equal the values recomputed from its method, path, query, and
  body bytes;
- `redaction`: no request or response header named by the required redaction
  policy survives (default: `authorization`).

A passing run prints a summary and exits `0`:

```plaintext
$ spycatcher-harness verify --cassette-dir fixtures/llm --cassette-name smoke.json
verified fixtures/llm/smoke.json: 3 interactions, 0 findings, 0 checks skipped
```

A failing run prints one line per finding and exits `1`:

```plaintext
fixtures/llm/smoke.json: verification failed with 2 findings
error[hash.stable_hash_mismatch] /interactions/1/request/stable_hash: stored hash differs from recomputed hash
error[redaction.required_header_present] /interactions/2/request/headers/0/0: header must be redacted
```

`--format json` writes a versioned JSON report for tools. `--format github`
writes GitHub Actions workflow commands, so findings appear as pull-request
annotations. All formats use the same qualified codes, for example
`hash.stable_hash_mismatch`. Library callers get the same checks through a new
`spycatcher_harness::verify_cassette` function, which returns a
`VerificationReport` instead of starting a server.

`record` and `replay` keep serving requests exactly as before. The cassette
on-disk schema (`format_version` 1) is unchanged.

Deliberately changed behaviour:

- `start_harness` with `Mode::Verify` returns `HarnessError::InvalidConfig`
  before it touches the cassette.
- `HarnessError::ModeNotYetImplemented` is removed, because verify was its
  only producer.
- Configuration-loading failures, such as a bad config file or an invalid
  locale, exit `2` for every subcommand rather than `1`. This matches clap's
  usage-error code and keeps `1` meaning "the cassette has findings" (`D2`).

The plan also closes a documentation gap. The roadmap links to
`spycatcher-harness-design.md#recommended-additions-for-regression-suite-integration`,
but that text is a plain paragraph rather than a heading, so the anchor does
not exist.

## Constraints

- Implementation begins only after explicit user approval of this plan.
- Do not change the cassette on-disk schema or
  `CassetteFormatVersion::SUPPORTED`.
- Do not change how `record` or `replay` serve requests, record cassettes, or
  compute canonical hashes.
  - Verify reuses `canonicalize` and `stable_hash`
    (`src/cassette/canonical/mod.rs`), the SSE parser (`src/sse.rs`), and the
    redaction predicate. It does not reimplement them.
  - Moving the JSON-body parser into the cassette domain is a pure relocation
    (`EP-M2`).
  - The only other observable change outside verify is the
    configuration-error exit code in `D2`.
- Verify never writes, creates, or locks files. It opens files read-only
  through `cap_std::fs_utf8`.
- Verify output never contains header values, request or response bodies,
  stream payloads, or `serde_json` error text. That error text can quote
  document content, for example `invalid type: string "sk-…"`. A finding
  carries only its code, its JSON Pointer location, and typed parameters from
  this allowlist:
  - a header name that matched the policy;
  - numeric indices;
  - hashes;
  - a parse line and column.
- Hexagonal boundaries:
  - The `cassette::verify` domain module performs no I/O. It does not import
    `http_exchange`, `axum`, `cli`, Fluent, or `std::fs`.
  - The application service `src/verify.rs` is the only filesystem caller.
  - The CLI adapter `src/cli/verify_output/` is the only renderer.
  - The binary is the only owner of process exit and stdout.
- `AGENTS.md` rules:
  - No file exceeds 400 lines.
  - Every module, including test modules, starts with a `//!` comment.
  - Non-test code uses neither `unwrap` nor `expect`.
  - Clippy's `print_stdout` and `print_stderr` stay denied.
  - `too-many-arguments-threshold = 4` applies (`clippy.toml`).
  - Lints are silenced only with item-scoped `#[expect(..., reason = "...")]`,
    and only as a last resort.
  - Every assertion carries a diagnostic message.
  - Comments and documentation use en-GB-oxendict spelling.
- The verify path adds no environment reads; configuration layering uses the
  existing OrthoConfig sources.
- Authorized dependency changes:
  - dev-dependencies `googletest` and `pretty_assertions`, which the user
    authorized, using caret requirements;
  - the `parsing` feature on the existing `time` dependency, needed to parse
    `recorded_at` as RFC 3339 (`D3`).

  No new runtime crate is authorized.
- Verus is not adopted in this task (`D7`). No prover tooling, pins, or
  Makefile targets are added. `make all` keeps exactly the prerequisites that
  `tests/test_make_all.py` pins.
- The library is pre-release: no tag in this repository's history is an
  ancestor of `HEAD`. The `v0.5.0` to `v0.9.1` tags come from an unrelated
  history. Interfaces may therefore change without compatibility shims, but
  every caller must update in the same commit. This covers the config loader
  return type, the removal of `ModeNotYetImplemented`, and the new
  `cassette::verify` types.
- Completed ExecPlans under `docs/execplans/` are historical and must not be
  edited.

## Tolerances (exception triggers)

The comparison base is `origin/main` at `c3e6d76`. Count with:

```bash
git diff --numstat origin/main...HEAD -- . \
  ':(exclude)docs/execplans/2-2-1-verify-subcommand-for-cassette-integrity.md' \
  ':(exclude)Cargo.lock' ':(exclude)typos.toml'
```

Limits are net lines (additions minus deletions) per category. Each file counts
in exactly one category, by whole file:

- Code: `src/**`, and `tests/**` including `.feature` and `insta` `.snap`
  files.
- Configuration: `Cargo.toml` and `i18n/**/*.ftl`.
- Documentation: `docs/**`, except this ExecPlan.
- Excluded: this ExecPlan, the generated `Cargo.lock`, and the generated
  `typos.toml`. If `make spelling` regenerates `typos.toml`, that change goes
  in a separate commit, per `AGENTS.md`.

Fixture cassettes are built in-test from typed builders and written compactly
to temporary directories. They are never committed as pretty-printed JSON,
because `Vec<u8>` fields pretty-print one byte per line and would swamp the
code budget.

Budget survey:

- Plans 1-3-2, 2-1-1, and 2-1-2 declared limits of 22, 24, and 22 files. The
  panel recount shows each changed 31 non-plan files, about 40% over, and none
  recorded a revision.
- Earlier plans 1-2-1 (16 files, 1,200 lines) and 1-2-2 (14 files, 1,000
  lines) were smaller.
- Task 2.2.1, recounted from the module list in `Plan of work`:
  - about 22 new `src` files;
  - about 14 edited `src` files;
  - about 15 test, feature, and snapshot files;
  - 2 configuration files;
  - 6 documentation files.

  That is about 59 files in total.
- Estimated lines: about 1,500 production, 2,000 test, 60 configuration, and
  600 documentation.

The limits below add about 10% contingency, informed by that history:

- Files: more than 64 non-ExecPlan files changed or added.
- Code: more than 3,900 net lines in total, or more than 1,700 net lines of
  production code. Production code is `src/**` excluding `*_tests.rs` and
  `tests.rs` files.
- Configuration: more than 80 net lines.
- Documentation: more than 700 net lines.
- Interface: any public API change beyond the following:
  - `verify_cassette`;
  - the `cassette::verify` types;
  - `cli::LoadedCommand` and the new loader return types;
  - `cli::verify_output` rendering;
  - `RedactionConfig::requires_redaction`;
  - removal of `ModeNotYetImplemented`.
- Dependencies: anything beyond the authorized changes in `Constraints`.
- Architecture: verify needing to change cassette persistence, canonical
  hashing, or SSE parser behaviour.
- Iterations: a focused test that still fails after three fix attempts.
- Time: any milestone that exceeds one working day of active effort.
- Ambiguity: a check whose pass/fail rule admits two readings that differ on
  which record-produced cassettes fail. Stop and present both readings.

Reaching any of these thresholds means stop and escalate.

## Risks

- Risk: false ordering failures on valid cassettes.
  - Cause: `relative_offset_ms` restarts at 0 in every `record` session that
    appends to an existing cassette (`src/cassette/filesystem.rs:71-88`,
    `src/server/record_metadata.rs:110-114`). It is also measured at handler
    entry, while concurrent appends land in completion order.
  - Severity: high. Likelihood without mitigation: high.
  - Mitigation:
    - `D3` flags a pair only when both `relative_offset_ms` and `recorded_at`
      decrease.
    - Setting `MatchMode::Keyed` (`cmds.verify.match_mode`) skips the rule.
    - A record-twice regression test proves that multi-session cassettes
      pass.
- Risk: false hash or transcript failures caused by values changing on save
  and reload.
  - Cause: `Option<Value>` reloads JSON `null` as `None`. Without
    `float_roundtrip`, `serde_json` may not reproduce a long-mantissa `f64`
    exactly.
  - Severity: high. Likelihood: medium.
  - Mitigation:
    - `D4` recomputes from the body bytes, as `record` did.
    - Stored values are compared only against a persistence round trip of the
      fresh value.
    - `EP-M0` probes `null` and arbitrary `f64` payloads before any rule is
      built.
- Risk: secrets reach CI logs through `serde_json` error text.
  - Severity: high. Likelihood: medium.
  - Mitigation:
    - A schema finding carries only `serde_json::Error::classify()`, the line,
      and the column (`Constraints`).
    - A dedicated sentinel test covers malformed documents
      (`INV-no-secret-echo`).
- Risk: workflow-command injection in any output format. The GitHub runner
  parses every stdout line.
  - Severity: high. Likelihood: low.
  - Mitigation:
    - All renderers escape interpolated values.
    - Every line starts with a prefix the renderer owns.
    - The GitHub escaper follows the toolkit rules.
    - Header names appear only when they match a validated policy token, and
      a valid token cannot contain control characters.
- Risk: verify hangs on a FIFO or reads a device at the cassette path.
  - Severity: medium. Likelihood: low.
  - Mitigation:
    - `Dir::metadata` is checked before opening.
    - A path that is not a regular file is `io.not_a_regular_file`.
    - A file over the size ceiling is `io.cassette_too_large`.
    - The read is bounded with `take`.
    - The residual race is recorded in `D8`.
- Risk: high memory use on large cassettes.
  - Severity: medium. Likelihood: low.
  - Mitigation:
    - Never build a whole-document `serde_json::Value`.
    - The version peek deserializes a one-field struct, and serde skips the
      remaining fields.
    - The typed parse runs once.
    - Peak memory is roughly the bytes plus one typed cassette, about 330 MiB
      at the ceiling.
- Risk: a configured policy silently drops the default `authorization` check.
  - Severity: medium. Likelihood: medium.
  - Mitigation:
    - `D6` adds configured headers to the defaults.
    - Removing the defaults requires `include_default_headers = false`.
    - Configuration rejects names that are not valid HTTP tokens.
- Risk: ortho_config 0.10 may not layer a `Vec<String>` from an environment
  variable.
  - Severity: low. Likelihood: medium.
  - Mitigation: `EP-M0` probes it. If it fails, environment layering for the
    list is documented as unsupported, and only the file and CLI layers are
    tested.
- Risk: `make fmt` reports pre-existing Markdown findings (`1-3-2` lines
  379-382).
  - Severity: low. Likelihood: medium.
  - Mitigation: keep only the hunks in files this plan touches.

## Progress

- [x] (2026-11-15) Branch renamed. Wyvern reconnaissance of the cassette
  domain, CLI, tests, tooling, and documentation completed.
- [x] (2026-11-15) Initial draft written and committed.
- [x] (2026-11-15) Community-of-experts review applied. The four panels were
  structure and contracts, failure modes and security, verification and
  viability, and alternatives and cost. See the revision note.
- [ ] Plan approved by the user.
- [ ] `EP-M0` feasibility probes: persistence round trip, ortho_config list
  layering, googletest and MSRV, timestamp parsing.
- [ ] `EP-M1` finding model, report, and I/O and schema checks.
- [ ] `EP-M2` hash and request-integrity checks.
- [ ] `EP-M3` ordering checks.
- [ ] `EP-M4` redaction checks and layered verify configuration.
- [ ] `EP-M5` application service, renderers, CLI dispatch, and exit codes.
- [ ] `EP-M6` documentation, ADR, roadmap closure, and full gates.

## Surprises & discoveries

- Observation: `verify` is already a parsed subcommand with layered
  `cmds.verify` configuration. It routes through `start_harness`, which opens
  the cassette and then returns `ModeNotYetImplemented`.
  - Evidence: `src/cli/mod.rs:205-225` and `:320-343`; `src/lib.rs:112-127`
    and `:165-168`.
  - Impact: dispatch must branch before `start_harness`, and `start_harness`
    must reject `Verify` before `prepare_cassette` runs.
- Observation: the roadmap's second design reference is a dead anchor.
  - Evidence: `docs/spycatcher-harness-design.md:928` is a plain paragraph.
  - Impact: `EP-M6` turns it into a heading.
- Observation: canonical hashing reads `parsed_json`, not `body`. The record
  path derives `parsed_json` from `body` with `parse_json_bytes`.
  - Evidence: `canonicalize` at `src/cassette/canonical/mod.rs:162-177`;
    `src/http_exchange.rs:118`.
  - Impact: verify recomputes from `body`, as record did (`D4`).
- Observation: `relative_offset_ms` is session-relative. It restarts on every
  `record` run that appends to an existing cassette.
  - Evidence: `SessionMetadata::create_at` and `open_or_create_for_record`.
  - Impact: `D3` adds a `recorded_at` co-condition.
- Observation: the stream recorder establishes three timing invariants:
  - `chunk_offsets_ms.len() == events.len()`;
  - the offsets are non-decreasing;
  - `ttft_ms` is no greater than the first offset.

  Evidence: `observe_chunk` and `finish_recording` at
  `src/server/record_stream/state.rs:120-168`. Impact: these become ordering
  checks, and recorded streams satisfy them by construction.
- Observation: two helpers have narrower visibility than the plan first
  assumed, and one file is at its size limit.
  - `canonical_request_value` lives in `src/cassette/canonical/json/mod.rs:57`
    as `pub(super)`.
  - `open_rooted_parent` is private at `src/cassette/filesystem.rs:224`, and
    that file is 399 lines long.
  - Impact: the hash rule compares against the output of `canonicalize` and
    `stable_hash`, re-deriving the envelope through a new `pub(crate)`
    accessor on `CanonicalRequest`. The bounded read lives in
    `src/verify.rs`.
- Observation: `HarnessError` localization goes through Fluent, which wraps
  arguments in U+2068/U+2069 isolation marks.
  - Evidence: `src/i18n.rs:96`; the snapshot at
    `src/bin/spycatcher_harness.rs:231`.
  - Impact: the JSON report carries no localized prose, and the GitHub
    renderer strips the isolation marks (`D5`).
- Observation: past plans overran their file limits by about 40% without
  recording it.
  - Evidence: the panel's recount of plans 1-3-2, 2-1-1, and 2-1-2.
  - Impact: `Tolerances` was recounted from the module list.

## Decision log

- D1. Decision: report every finding rather than failing fast.
  - Exception: an `io` finding, or a `schema` finding that prevents typed
    deserialization, ends the run. Later rules have no typed cassette to
    inspect.
  - Rationale: CI users fix cassettes in batches.
  - Rejected: fail-fast `HarnessError` variants on `Cassette`. They force a
    fix-and-rerun loop. They also misreport future-version files, because
    `from_reader` deserializes before it checks the version.
  - Date/Author: 2026-11-15, planning agent; confirmed by the panel.
- D2. Decision: define five finding classes and three exit codes.
  - Classes: `io`, `schema`, `ordering`, `hash`, and `redaction`. Every
    finding is an error.
  - Exit codes:
    - `0`: verified with no findings.
    - `1`: a report was produced and has at least one finding.
    - `2`: no report could be produced. This covers clap usage errors
      (unchanged), every configuration-loading error for every subcommand
      (changed from `1`), `verify_cassette` configuration errors, and failure
      to write the report to stdout.
  - Skipped checks (`D4`) are reported but never affect the exit code.
  - Rationale: the roadmap requires a non-zero exit for any failure class. CI
    must also tell "this cassette is bad" apart from "verify could not run". A
    JSON consumer sees exit `2` with empty stdout only when no report exists.
  - Rejected: per-class exit codes, because one run can contain several
    classes.
  - Date/Author: 2026-11-15; revised after panel review (exit `2`).
- D3. Decision: define "ordering" as three checks.
  - Interaction order:
    - A pair of adjacent interactions `(i-1, i)` is reported as
      `ordering.interaction_start_order` when both
      `relative_offset_ms[i] < relative_offset_ms[i-1]` and
      `recorded_at[i] < recorded_at[i-1]` hold. `recorded_at` is parsed as an
      RFC 3339 instant.
    - If either `recorded_at` fails to parse, the pair is reported as skipped
      with `skipped.recorded_at_unparsable`.
    - Under `MatchMode::Keyed` the rule is skipped entirely. Verify takes the
      mode from `HarnessConfig.match_mode`, set through the config-only
      `cmds.verify.match_mode`.
  - Stream timing, when `timing` is `Some`:
    - `chunk_offsets_ms` has one entry per event;
    - the offsets are non-decreasing, and every adjacent descent is reported;
    - `ttft_ms` is no greater than the first offset.
  - Stream transcript: re-parsing a non-empty `raw_transcript` with
    `SseParser` reproduces `events` exactly, after the persistence round trip
    of `D4`.
  - Rationale:
    - A hand reorder or a bad merge moves both timestamps together.
    - A new record session restarts the offset, but `recorded_at` keeps
      rising.
    - A concurrent recording's array order follows completion time, which
      `recorded_at` approximates.
    - The co-condition therefore flags real reordering without failing
      record-produced cassettes.
  - Residual risk: if two concurrent completions race between building their
    metadata and taking the store lock, both signals can invert. The users'
    guide documents this and recommends `match_mode = "keyed"` for
    concurrently recorded cassettes.
  - Rejected: enforcing the offset alone, which fails multi-session
    cassettes; and dropping interaction ordering, which leaves the design's
    "sequential integrity" unchecked.
  - Date/Author: 2026-11-15; revised after panel review.
- D4. Decision: recomputation and absence rules.
  - Hash recomputation:
    1. Start from `parse_json_body(&request.body)`, exactly as `record`
       derived `parsed_json`.
    2. Build a `RecordedRequest` copy with that value.
    3. Call `canonicalize(&copy, &IgnorePathConfig::default())` once.
    4. Derive both `stable_hash` and the canonical envelope from that result.
  - Comparisons: stored `parsed_json`, `canonical_request`, and stream
    `events` are compared against `persisted(fresh)`. `persisted` serializes
    with `serde_json::to_vec`, then deserializes back through the same field
    types. `Some(Null)` therefore becomes `None`, and floats round-trip
    exactly as persistence made them.
  - A missing `stable_hash` or `canonical_request` is a `hash` finding.
    `record` always writes both, and replay requires `stable_hash`.
  - Skips, which appear in the report's `skipped` list:
    - a stream with an empty `raw_transcript` and non-empty `events` skips the
      transcript check (`skipped.transcript_absent`);
    - a stream with `timing: None` skips the timing checks
      (`skipped.timing_absent`).
  - Out of scope, and documented in the users' guide: cassettes written
    through the library with a non-default `IgnorePathConfig`.
  - Rationale: verification must agree with what `record` produced.
    Persistence normalization would otherwise cause false positives. Visible
    skips mean a recorder regression cannot hide behind a skip.
  - Date/Author: 2026-11-15; revised after panel review.
- D5. Decision: report formats and wire contract.
  - Selection: `--format text|json|github`, default `text`, layered as
    `cmds.verify.format`.
    - The format lives only in the CLI layer, as `VerifyOutputFormat` in
      `src/cli_args.rs`. It reaches the binary through `cli::LoadedCommand`;
      `HarnessConfig` does not carry it.
    - All formats write to stdout and emit qualified codes
      (`<class>.<code>`).
  - `json`: a versioned envelope.
    - Fields: `report_version: 1`, `kind: "cassette_verification"`,
      `cassette`, `outcome`, `interactions_checked`, `findings`, and
      `skipped`.
    - Each finding has `code`, `class`, `interaction`, `pointer`, and typed
      `params`. The report carries no localized prose.
    - Task 2.2.2 can add `kind: "replay_mismatch"` under the same rules.
    - Evolution rule, recorded in ADR 0003: new fields and codes are
      additive, and consumers must ignore unknown fields. Renaming or removing
      a field or code, or changing its meaning, bumps `report_version`.
  - `text`: Fluent-localized, one line per finding. Every interpolated value
    is control-escaped.
  - `github`: one workflow command per finding, of the form
    `::error file=…,title=spycatcher-harness verify: <code>::<message> (<pointer>)`.
    - Fluent isolation marks are stripped.
    - `file` and `title` are escaped as properties.
    - At most nine finding annotations are emitted. When more exist, a tenth
      summary annotation states how many were omitted and points at
      `--format json`.
    - Line numbers are omitted except for schema parse errors.
    - `file=` is the cassette path as configured. The users' guide tells users
      to run from the repository root with a relative `--cassette-dir`, so
      annotations attach to the diff.
  - Rejected:
    - Static Analysis Results Interchange Format (SARIF). It needs the
      `upload-sarif` action and extra permissions, and GitHub Advanced
      Security on private repositories. It remains a candidate later renderer
      over the same envelope.
    - A problem matcher. It moves the regex contract onto users and still
      needs injection-safe text.
    - Span-aware parsing for line numbers. It needs a new dependency.
  - Date/Author: 2026-11-15; revised after panel review.
- D6. Decision: the required redaction policy.
  - Verify requires the union of `RedactionConfig::default().drop_headers`
    (`authorization`) and `cmds.verify.redaction.require_headers`.
  - Setting `cmds.verify.redaction.include_default_headers = false` removes
    the defaults; the setting defaults to `true`.
  - The configuration layer validates names as HTTP header tokens. An invalid
    name is a configuration error and exits `2`. The effective list becomes
    `HarnessConfig.redaction`.
  - Matching uses a new case-insensitive
    `RedactionConfig::requires_redaction(&self, name) -> bool`. The record
    path's private `should_persist_header` delegates to it, so record and
    verify share one predicate and the domain never imports `http_exchange`.
  - Only header names are checked. Values, query parameters, and bodies are
    documented non-goals.
  - Rationale:
    - Because the policy is a union, configuring an extra header cannot
      silently stop the `authorization` check.
    - Token validation turns typos into errors.
    - A shared predicate means a cassette recorded under policy `P` passes
      verify under `P`.
  - Rejected:
    - A wider hard-coded deny list, which fails cassettes recorded under the
      documented default.
    - Value heuristics, which risk false positives and secret echo.
  - Date/Author: 2026-11-15; revised after panel review.
- D7. Decision: do not adopt Verus in this task.
  - What replaces it: exhaustive bounded enumeration plus property tests
    discharge the ordering kernel's invariant.
  - The kernel: `adjacent_violations` returns every index `k` whose adjacent
    pair `(k-1, k)` violates a predicate. Its only unbounded content is that
    adjacent non-descent implies global non-decreasing order, which follows
    from transitivity in one induction step.
  - Why enumeration is exhaustive: the kernel only compares values, so every
    sequence of length `n` is order-isomorphic to a sequence over `{0..n-1}`.
    Enumerating all 55,987 sequences over `{0..5}` of length 0 to 6 is
    therefore exhaustive for every `u64` sequence up to length 6. A property
    test against a quadratic oracle covers longer sequences.
  - Rationale:
    - `AGENTS.md` and the user brief warn against artificial proof
      obligations.
    - Three of four panel experts judged a Verus proof of this kernel
      disproportionate. One called it "rigorous, but thin in substance".
    - It would put the repository's first prover wiring (pins, Makefile
      targets, contract tests, an ADR) on the critical path.
    - It would require a non-idiomatic production loop.
    - The textual-identity bridge cannot work, because Verus `invariant` and
      `decreases` clauses sit inside the loop syntax.
    - No other obligation in this feature has unbounded content over a small,
      stable, pure kernel. Hashing rests on SHA-256 and `serde_json` axioms.
      Escaping is a character map, and a `Seq<char>` proof would not bridge to
      the shipped `String` code.
  - Recommended follow-up: a separate roadmap task to adopt Verus via
    `prover-tools`, aimed at the replay sequential-cursor invariants in
    `src/cassette/matching/`.
  - Rejected:
    - A Verus proof of `first_descent`; see the panel findings in the revision
      note.
    - Kani, which adds bounded checking that the order-isomorphic enumeration
      already provides without tool wiring.
  - Date/Author: 2026-11-15, planning agent after panel review. This is a
    user-visible choice, flagged for approval.
- D8. Decision: limit reads to 256 MiB (`MAX_VERIFY_DOCUMENT_BYTES`).
  - `Dir::metadata` must report a regular file with
    `len() <= MAX_VERIFY_DOCUMENT_BYTES`.
  - The read uses `take(MAX + 1)`, which catches a file that grows after the
    check.
  - Residual: a FIFO swapped in between the check and the open could still
    block. This is recorded.
  - Replay has no ceiling. That is noted for follow-up and not changed here.
  - Date/Author: 2026-11-15; revised after panel review.
- D9. Decision: verify detects inconsistency and accidental leakage, not
  adversarial tampering. The hashes are unkeyed SHA-256, so anyone who edits a
  cassette can recompute them.
  - Date/Author: 2026-11-15.
- D10. Decision: sort findings deterministically.
  - Sort key: interaction index, then class in the order `io`, `schema`,
    `ordering`, `hash`, `redaction`, then typed pointer segments.
  - Index segments compare numerically, so `/headers/2` sorts before
    `/headers/10`.
  - Date/Author: 2026-11-15; revised after panel review.
- D11. Decision: model each finding as a single typed kind.
  - `FindingKind` is a single `#[non_exhaustive]` enum with per-variant data.
    `class()` and `code()` are derived from the variant, so a finding cannot
    pair a class with an unrelated code or data.
  - Domain types do not derive `Serialize`. The JSON envelope is a separate
    DTO in the CLI adapter, so there is exactly one wire schema.
  - The public structs and enums are `#[non_exhaustive]`.
  - Date/Author: 2026-11-15, panel review.

## Outcomes & retrospective

Not started. Complete this at each milestone boundary and at completion,
including reconciling discoveries with the upstream documents in
`Conformance basis`.

## Context and orientation

The inspected base revision is `c3e6d76d9ca300d17c6139e6ec8a344f3e971e2a`. Line
numbers refer to that revision; relocate symbols with
`rg -n '<symbol>' src tests`.

Crate: a single Rust crate, `spycatcher-harness`, with the library at
`src/lib.rs` and the binary at `src/bin/spycatcher_harness.rs`. It uses edition
2024 and `rust-version = "1.89.0"`. The pinned toolchain is
`nightly-2026-11-13`.

Cassette domain (`src/cassette/`):

- `src/cassette/mod.rs` (305 lines) defines:
  - `CassetteFormatVersion(u32)`, with `SUPPORTED = 1`;
  - `Cassette { format_version, interactions }`, with `validate`,
    `from_reader`, and `write_to`;
  - `Interaction { request, response, metadata }`;
  - `RecordedRequest`, with fields `method`, `path`, `query`, `headers:
    Vec<(String, String)>`, `body: Vec<u8>`, `parsed_json: Option<Value>`,
    `canonical_request: Option<Value>`, and `stable_hash: Option<String>`;
  - `RecordedResponse::{NonStream, Stream}`, where `Stream` carries `events`,
    `raw_transcript`, and `timing: Option<StreamTiming>`;
  - `StreamEvent::{Comment, Data}`;
  - `StreamTiming { ttft_ms, chunk_offsets_ms }`;
  - `InteractionMetadata { protocol_id, upstream_id, recorded_at: String,
    relative_offset_ms }`.

  Headers serialize as two-element arrays, so the JSON Pointer to a header name
  is `/interactions/<i>/request/headers/<j>/0`.
- `src/cassette/canonical/mod.rs` provides
  `canonicalize(&RecordedRequest, &IgnorePathConfig)`,
  `stable_hash(&CanonicalRequest)`, and
  `RecordedRequest::populate_canonical_fields`.
  - The stored envelope comes from `canonical_request_value` in
    `canonical/json/mod.rs`, which is `pub(super)`.
  - `serde_json` uses `preserve_order`, and `Value` object equality ignores
    key order.
- `src/cassette/filesystem.rs` (399 lines) holds the `pub(crate)`
  `FilesystemCassetteStore` and the private `open_rooted_parent`.
- Whitaker's `self_named_module_files` rule forbids `foo.rs` beside `foo/`,
  so a new cassette child module is declared from `src/cassette/mod.rs`.

SSE: `src/sse.rs` holds the `pub(crate)` `SseParser`, with `feed` and `finish`.
Its output does not depend on chunk boundaries. The module imports
`cassette::StreamEvent`, so `cassette::verify` using `sse` forms an intra-crate
module cycle. That is acceptable and is noted in the developers' guide.

Redaction:

- `RedactionConfig { drop_headers }` is at `src/config.rs:185-198`. Its
  default is `["authorization"]`.
- `redact_headers` and the private `should_persist_header` are at
  `src/http_exchange.rs:229-250`. That file imports `axum::http::HeaderMap`.
- `parse_json_bytes` is at `src/http_exchange.rs:118`.

Recording metadata: `recorded_at` comes from
`OffsetDateTime::now_utc().format(&Rfc3339)`, and `relative_offset_ms` is
measured from the session start (`src/server/record_metadata.rs`).

CLI:

- `src/cli/mod.rs` (343 lines) declares `Commands::{Record, Replay, Verify}`
  and `VerifyArgs`.
- The public loaders `load_subcommand_config*` (`:85-143`) and
  `load_subcommand_config_from_iter_with_sources`
  (`src/cli/source_loader.rs:65`) return `HarnessConfig`. About 20 call sites
  exist across `src/cli`, `src/bin`, `tests/cli_localization_unit.rs`, and
  `tests/cli_layering/source_loader.rs`.
- `RecordArgs.upstream` shows the config-only pattern (`#[serde(default)]`
  with `#[arg(skip)]`), with its types in `src/cli_args.rs`.

Binary:

- `main() -> eyre::Result<()>` builds the localizer and runtime, then calls
  `start_harness`.
- `cli_startup::handle_cli_config_result` exits `0` for help and `2` for
  parse errors. It wraps other configuration errors into `eyre`, which gives
  exit `1`.
- `process_output.rs` owns the stdout and stderr writers.

Errors and localization:

- `HarnessError` is in `src/error.rs`, with `ModeNotYetImplemented` at `:85`.
- `src/i18n.rs` maps errors to Fluent IDs. The catalogue is
  `i18n/en-US/spycatcher-harness.ftl`.
- `ModeNotYetImplemented` is referenced from `src/error.rs:145`,
  `src/i18n.rs:30` and `:156`, `src/i18n_tests.rs:52` and `:109`,
  `tests/lifecycle_tests.rs:171-188`, and `docs/users-guide.md:29` and `:334`.

Tests and gates:

- BDD uses `rstest-bdd` 0.6; see `tests/cassette_schema_versioning_bdd.rs`.
- End-to-end binary tests run
  `Command::new(env!("CARGO_BIN_EXE_spycatcher-harness"))` with `env_clear()`
  and `tests/common/coverage_profile.rs`; see
  `tests/binary_localization_e2e.rs`.
- Snapshots live in `tests/snapshots/`.
- The one property block, at `src/server/replay_stream_properties.rs:56`, uses
  64 cases.
- `googletest` and `pretty_assertions` are not used yet.
- Gates:
  - `make check-fmt`;
  - `make lint`, which runs interrogate over the Python in `tests/`,
    `cargo doc`, clippy, and Whitaker;
  - `make test`, which runs nextest plus the doc tests;
  - `make all` adds `spelling` and `test-workflow-contracts`;
  - `make markdownlint` and `make nixie` check the documentation.

Signposted documentation:

- `docs/spycatcher-harness-design.md`: sections "Cassette definition",
  "Canonicalization and hashing", "Public library API surface", "CLI shape",
  and "Implementation decisions".
- `docs/users-guide.md`, `docs/developers-guide.md`, and
  `docs/testing-guide.md`.
- `docs/rust-testing-with-rstest-fixtures.md`,
  `docs/rstest-bdd-users-guide.md`,
  `docs/reliable-testing-in-rust-via-dependency-injection.md`,
  `docs/rust-doctest-dry-guide.md`, and
  `docs/complexity-antipatterns-and-refactoring-strategies.md`.
- `docs/ortho-config-users-guide.md`, for `cmds.verify` layering and list
  fields.
- `docs/documentation-style-guide.md`, for the ADR template at `:521-588`.

Signposted skills: `execplans`, `rust-router`, `hexagonal-architecture`,
`rust-unit-testing`, `proptest`, `rust-verification` (the reasoning behind
`D7`), `rust-errors`, `domain-cli-and-daemons`, `codegraph-mcp`, and
`en-gb-oxendict-style`.

## Conformance basis

Upstream artefacts, inspected at `c3e6d76`:

- `docs/roadmap.md`: task 2.2.1 (`:192-202`); its dependencies 1.2.1 and
  1.2.2, both done; and its consumer, task 2.2.2 (`:204-213`).
- `docs/spycatcher-harness-design.md`:
  - "CLI shape" (`:939-963`);
  - the `CassetteVerifier` and `MismatchReport` paragraph (`:928-935`);
  - "Cassette definition" (`:229-287`);
  - "Canonicalization and hashing" (`:376-422`).
- `docs/adrs/adr-0001-*` and `adr-0002-*`, which this plan respects and does
  not change.
- No terms of reference exist for this project.

Requirements:

- `REQ-schema`: check the format version. Sources: roadmap criterion 1; the
  design's "Verifies cassette format version".
- `REQ-ordering`: check ordering. Sources: roadmap criterion 1; the design's
  "Verifies sequential integrity".
- `REQ-hash`: recompute hashes. Source: roadmap criterion 1.
- `REQ-redaction`: check the required redaction policies. Sources: roadmap
  criterion 1; the design's "Verifies no secrets leaked (headers scrubbed)".
- `REQ-output`: provide machine-readable and human-readable output for CI
  annotations. Source: roadmap criterion 2.
- `REQ-exit`: exit non-zero on any failure class. Source: roadmap
  criterion 3.
- `REQ-cli`: provide a `verify` subcommand with `cmds.verify` layering.
  Source: the design's "CLI shape".

Gaps:

- `GAP-stub`: verify returns `ModeNotYetImplemented`.
- `GAP-anchor`: the roadmap link
  `#recommended-additions-for-regression-suite-integration` is dead.
- `GAP-redaction-layering`: no subcommand layers `RedactionConfig`.

Trace links:

```plaintext
REQ-schema    -> EP-M1 -> verify::schema tests; BDD "unsupported format version"; e2e exit 1
REQ-hash      -> EP-M2 -> verify::hash tests + mutation property; BDD "hand-edited body"
REQ-ordering  -> EP-M3 -> ordering enumeration + properties; BDD "stream timing", "record twice"
REQ-redaction -> EP-M4 -> redaction tests + agreement property; BDD "authorization header"
REQ-output    -> EP-M5 -> insta snapshots (text/json/github); escaping + secret-echo properties
REQ-exit      -> EP-M1, EP-M5 -> outcome table; e2e table over every class and exit 2 cases
REQ-cli       -> EP-M4, EP-M5 -> cli layering tests; e2e cmds.verify config file
GAP-stub      -> EP-M5 -> tests/lifecycle_tests.rs; e2e pass case
GAP-anchor    -> EP-M6 -> make markdownlint; anchor check
GAP-redaction-layering -> EP-M4 -> tests/cli_layering verify redaction cases
```

## Verification plan

### Abstract model

A cassette document is a byte string `D`. A policy is `P = (R, m)`: `R` is the
set of required header names, ASCII lower-cased, and `m` is the match mode.
Verification is a total function `verify(D, P) = (findings, skipped)`. Each
finding is a typed `FindingKind` with an interaction index (or document scope)
and a JSON Pointer. The stages run in order:

1. I/O, at the application boundary. A missing path, a non-regular file, a
   size above `MAX`, or a read error yields one `io` finding, and the run stops.
2. Schema. One `schema` finding is produced, and the run stops, if any of the
   following holds:
   - `D` is not JSON;
   - `format_version` is missing or is not an unsigned 32-bit integer;
   - the version is unsupported;
   - `D` does not deserialize into `Cassette`.
3. Rules. For the typed cassette `C`, the findings are the union of the hash,
   redaction, interaction-ordering, and stream rules. Each rule is a pure
   function of `P` and either one interaction or one adjacent pair of
   interactions.
4. Outcome. The outcome is `Passed` if and only if there are no findings.
   `Passed` exits `0` and `Failed` exits `1`. An error before any report exists
   exits `2`.

The notation `persisted(v)` means `deserialize(serialize(v))` through the
cassette field types.

### Invariants and lemmas

- `INV-exit` (`REQ-exit`): the exit code is `0` if and only if a report
  exists with no findings, `1` if and only if a report exists with findings, and
  `2` if and only if no report exists.
- `INV-adjacent-complete` (`REQ-ordering`): for any `s: &[u64]` and the
  descent predicate, `adjacent_violations(s)` equals
  `{ k | 1 <= k < |s|, s[k-1] > s[k] }` in ascending order. It is empty if and
  only if `s` is globally non-decreasing.
- `L-adjacent-global`: if no adjacent pair descends, then `s[i] <= s[j]` for
  every `i <= j`. The proof is an induction on `j - i` using transitivity. The
  design document states the argument, and the enumeration checks it up to the
  bound.
- `INV-order-pair` (`REQ-ordering`, `D3`): a pair is reported if and only if
  its offset and its `recorded_at` both decrease and the mode is
  `SequentialStrict`. An unparsable `recorded_at` produces a skip, not a
  finding.
- `INV-record-clean`, the no-false-positives invariant: a cassette produced
  by the record path has no findings under its own recording policy. This
  includes:
  - multi-session appends;
  - streams with comments;
  - `null` and float payloads;
  - empty and non-JSON bodies;
  - percent-encoded header values.
- `INV-hash-exact` (`REQ-hash`): each mutation class yields exactly this
  finding set:

  | Mutation                            | Findings                                                                     |
  | ----------------------------------- | ---------------------------------------------------------------------------- |
  | `method`, `path`, or `query` edited | `stable_hash_mismatch`, `canonical_request_mismatch`                         |
  | `body` edited to different JSON     | `stable_hash_mismatch`, `canonical_request_mismatch`, `parsed_json_mismatch` |
  | only `parsed_json` edited           | `parsed_json_mismatch`                                                       |
  | only `stable_hash` edited           | `stable_hash_mismatch`                                                       |
  | `stable_hash` deleted               | `stable_hash_missing`                                                        |
  | JSON keys reordered anywhere        | none                                                                         |

- `INV-transcript` (`REQ-ordering`): for recorder-produced streams, re-parsing
  the transcript gives `persisted(events)`. Swapping two distinct events, or
  editing one payload, yields exactly one `stream_transcript_mismatch`, at the
  first differing index.
- `INV-redaction` (`REQ-redaction`):
  - Exactness: the reported pointers are exactly the persisted header-name
    pointers whose name equals a member of `R` under ASCII case folding.
  - Uniqueness: each such header is reported once, even when the policy lists
    duplicate or differently cased entries.
  - Agreement: verifying `redact_headers(H, P)` against `P` reports nothing.
- `INV-locality` (stage 3 independence): appending a clean interaction never
  changes the findings for earlier interactions.
- `INV-sorted` (`D10`): findings are always sorted by the `D10` key, and
  rendering is byte-identical across runs.
- `INV-no-secret-echo`: no output, in any format, contains a header value,
  body byte, stream payload, or document substring quoted by `serde_json`.
- `INV-annotation-safe`:
  - Every line of every format begins with a prefix the renderer owns.
  - No interpolated value introduces CR or LF.
  - GitHub data escapes `%`, CR, and LF; GitHub properties also escape `:`
    and `,`.
  - `%` is escaped first, and decoding inverts encoding.
- `INV-cap`: GitHub output has `min(n, 9)` finding annotations, plus one
  summary annotation when `n > 9`. Shown plus omitted equals `n`.

### Axioms (trusted, not verified here)

- `AXIOM-serde`: `serde_json` parsing and serialization are deterministic.
  `Value` object equality ignores key order. `Error::classify`, `line`, and
  `column` behave as documented.
- `AXIOM-sha2`: SHA-256 is deterministic.
- `AXIOM-canonical`: `canonicalize` and `stable_hash` are deterministic, as
  task 1.2.2 established.
- `AXIOM-sse`: `SseParser` output is independent of chunk boundaries, as the
  tests in `src/sse.rs` show.
- `AXIOM-time`: `time`'s RFC 3339 parser accepts every string that `record`
  formats with `Rfc3339`.
- `AXIOM-gha`: GitHub Actions parses `::error <props>::<data>` as follows:
  - data escapes `%` as `%25`, CR as `%0D`, and LF as `%0A`;
  - properties additionally escape `:` as `%3A` and `,` as `%2C`;
  - at most 10 error annotations display per step, and 50 per job;
  - lines split on LF and bare CR, but not on U+2028 or U+0085.

  Sources: GitHub Docs, "Workflow commands for GitHub Actions"; `@actions/core`
  `command.ts`; community discussion 68471.
- `AXIOM-ortho`: OrthoConfig merges the `cmds.verify` file, environment, and
  CLI layers as documented. `EP-M0` confirms the behaviour for list fields.

### Methods per obligation

Property tests use an explicit budget of
`ProptestConfig { cases: 256, max_global_rejects: 25, .. }` unless stated
otherwise. Generators reach the material classes by construction, using
`prop_oneof!` over named sub-strategies rather than filtering.

Proptest cannot classify cases, so each property has a companion deterministic
test of its class mix. The test draws 512 values from the strategy with
`TestRunner::deterministic()`, counts them per class, and asserts that every
class appears in at least 10% of draws. Proptest regression files are committed
only for real failures.

- `INV-exit`:
  - Method: an `rstest` table over every class, mapping report to outcome to
    exit code. An end-to-end `rstest` table then drives the real binary with
    one in-test fixture per class and asserts exit `1`.
  - Exit `2` rows: an invalid redaction token in the config, an empty
    cassette name, and an unreadable config file.
  - Red: the pass row fails with today's `mode not yet implemented`.
  - Non-vacuity: before asserting the exit code, each fixture is confirmed
    in-process to produce exactly its intended class. A seeded fault that
    maps `Failed` to `0` must turn the rows red, and is then reverted.
- `INV-adjacent-complete` and `L-adjacent-global`:
  - Exhaustive enumeration: every sequence over `{0..5}` of length 0 to 6
    (55,987 cases), compared against an independent quadratic oracle. The
    oracle checks every pair `i < j` for global order and lists every adjacent
    descent.
  - Longer sequences: a property over lengths 7 to 64, with the strategies
    `sorted` (prefix sums of non-negative deltas), `descent_at_1`,
    `descent_interior`, `descent_last`, and `many_descents`.
  - Named cases: empty, single, an equal run, and `[5,1,6,2]`, which must
    report `[1,3]`.
  - Rationale: the order-isomorphism argument in `D7` makes the enumeration
    exhaustive up to length 6.
  - Non-vacuity: the enumeration asserts that it visits both sorted and
    unsorted sequences and every descent position. Two seeded faults must
    each fail the enumeration: `>` changed to `>=`, and reporting `k-1`.
- `INV-order-pair`:
  - Method: an `rstest` table crossing the offset direction (up or down), the
    `recorded_at` direction (up or down), the mode (`SequentialStrict` or
    `Keyed`), and an unparsable timestamp.
  - BDD: "a cassette recorded in two sessions verifies cleanly". It uses the
    real record service with an injected clock and session start from
    `record_tests_helpers`.
  - Non-vacuity: the down-and-down row must produce a finding under
    `SequentialStrict`, which proves the precondition is reachable.
- `INV-record-clean`:
  - Method: a property that builds interactions with the record path's own
    builders, `build_interaction` and `build_stream_interaction`, via the
    `record_tests_helpers` fake upstream. It persists them with `write_to`,
    reloads, verifies, and asserts zero findings.
  - Generated classes: JSON bodies including `null`, arbitrary `f64`, and
    nested objects; non-JSON and empty bodies; streams with comments and
    `data: null`.
  - Budget: 128 cases, because record-path setup is expensive.
  - Ordering: `EP-M0` runs the `null` and `f64` round-trip probe first, so any
    persistence asymmetry is found before the rules exist.
  - Non-vacuity: the class-mix test asserts that the `null`, float, stream,
    and comment classes all appear.
- `INV-hash-exact`:
  - Method: one property that, per case, generates a request, populates it,
    and asserts no findings. It then applies one mutation from a `prop_oneof!`
    over the mutation classes and asserts the exact finding set from the
    table.
  - Each mutation changes the canonical form by construction:
    - a verb drawn from a set that excludes the current one;
    - a numeric leaf incremented by one;
    - an appended query pair.
  - Key reordering is a separate negative-control row.
  - Known-answer cases reuse the hash vectors from
    `tests/features/canonical_request_hashing.feature` as an independent
    oracle.
- `INV-transcript`:
  - Method: a property in which a test-side SSE writer serializes generated
    events, the transcript is split at random boundaries, and `SseParser`
    builds `events`. Verify must report nothing.
  - Mutation branch: either swap two events that differ by construction (a
    comment with a data event), or edit one payload. Assert exactly one
    finding at the expected index.
- `INV-redaction`:
  - Method: one property over generated header lists and policies.
    - Names are drawn partly from the policy, in random ASCII case.
    - Policies include duplicate entries in different case, such as
      `["Authorization", "authorization"]`.
    - An independent oracle, `eq_ignore_ascii_case` over every pair, gives the
      exact expected pointer set.
  - A second property asserts that verifying `redact_headers(H, P)` finds
    nothing.
  - Non-vacuity: the class-mix test asserts that at least 30% of lists contain
    a policy header. A seeded case-sensitive fault must fail.
- `INV-locality`:
  - Method: a metamorphic property. Take any generated cassette with
    findings, append one clean recorded interaction, and assert that the
    findings for earlier indices are unchanged.
- `INV-sorted`:
  - Method: a property over generated multi-finding cassettes, asserting that
    the findings are sorted by the `D10` key.
  - Named case: `/headers/2` sorts before `/headers/10`.
- `INV-no-secret-echo`:
  - Method: two properties that render all three formats and assert that no
    sentinel appears in any output.
    1. Planted sentinels: high-entropy values in header values (including one
       on a header whose name the policy requires), bodies, and stream
       payloads, in a cassette with hash, ordering, and redaction findings.
    2. Malformed documents: one cassette whose `body` is a JSON string
       containing a sentinel, and one whose `kind` tag is an unknown sentinel
       value. Both are schema failures where `serde_json` would quote the
       sentinel.
  - Non-vacuity: a seeded fault that renders `serde_json`'s `to_string()`
    must fail the malformed-document property.
- `INV-annotation-safe`:
  - Method: a property over arbitrary Unicode strings, including CR, LF, `%`,
    `:`, `,`, U+2028, and U+0085. It asserts that escaped output contains no
    forbidden characters and that a test-side decoder round-trips every value.
  - Prefix check: every line of every rendered format starts with a known
    prefix: the word `verified`, `error[`, `::error`, a cassette-path
    summary, or `{` for JSON.
  - Named injection cases:
    - cassette names containing `,`, `:`, and `\n::add-mask::x`;
    - already-escaped `%0A` sequences;
    - a cassette path containing `::stop-commands::`.
  - Non-vacuity: two seeded faults must fail: dropping the `,` escape, and
    escaping `%` last.
- `INV-cap`:
  - Method: an `rstest` table over 0, 1, 9, 10, and 25 findings, asserting
    the annotation counts and the summary arithmetic.
- Output contract:
  - Snapshots: `insta` snapshots cover the text pass and fail cases, the
    JSON, and the GitHub output for a cassette with hash, ordering, and
    redaction findings.
  - `io` and `schema` failures get separate snapshots, because `D1` stops the
    run early for them.
  - Paths are made deterministic by running in a temporary directory with a
    relative `--cassette-dir`.
  - JSON: `googletest` `assert_that!` matchers check the parsed report's
    `report_version`, `kind`, `outcome`, and code set.
  - A test-side mirror struct with `deny_unknown_fields` acts as an
    in-repository change detector. Consumer tolerance of unknown fields is
    stated separately, in ADR 0003.
  - Size boundary: a file of exactly `MAX` bytes passes the size check, and
    `MAX + 1` gives `io.cassette_too_large`. A lower test-only ceiling is
    injected through `pub(crate)` service options.
  - FIFO and directory: end-to-end rows each yield `io.not_a_regular_file`
    without hanging. The FIFO row is Unix-only and creates the FIFO with
    `mkfifo` through `std::process::Command`. A 10-second test timeout guards
    against a hang.

Assertion style:

- `pretty_assertions::assert_eq!` is for whole-struct comparisons in unit
  tests. It always carries a message, because clippy's `missing_assert_message`
  lint does not see third-party macros.
- `googletest::assert_that!` is for JSON report matching. `expect_that!` is
  used only inside `#[gtest]` tests.
- Neither crate is used inside proptest bodies, which use `prop_assert*`, or
  rstest-bdd steps, which use `eyre::ensure!`. Shrinking and step errors must
  keep working there.

### Formal verification decision

No formal prover is used (`D7`).

The one unbounded obligation, `L-adjacent-global`, is a single transitivity
induction. The design document states and argues it. Exhaustive enumeration by
order-isomorphism checks it for every `u64` sequence up to length 6, and
property tests cover longer inputs.

A Verus proof would add unbounded certainty over a mirror of a six-line
function. It would cost the repository's first prover wiring, and its bridge to
the shipped code would be weaker than the exhaustive check of the shipped code
itself.

All other obligations either depend on external behaviour treated as axioms
(SHA-256, `serde_json`, the GitHub runner) or are finite maps and tables that
the tests cover exhaustively.

Residual gaps:

- the kernel above length 6 is covered by the transitivity argument and
  sampling, not by proof;
- the time-of-check-to-time-of-use (TOCTOU) race in `D8`;
- the concurrent-completion race in `D3`.

A follow-up Verus adoption task is recommended for the replay cursor
invariants, where the content is unbounded and non-trivial.

### Suggested BDD features

Path: `tests/features/cassette_verify.feature`.

- Bound by: `tests/cassette_verify_bdd.rs`, with world and steps in
  `tests/cassette_verify/`.
- Scope: steps drive `verify_cassette` and the renderers in-process. The
  binary is covered by `tests/verify_cli_e2e.rs`.
- Timing: each scenario is written red in the milestone that introduces its
  rule.

```gherkin
Feature: Cassette verification

  Scenario: A recorded cassette verifies cleanly
    Given a cassette recorded with two non-stream and one stream interaction
    When the cassette is verified
    Then verification passes with 3 interactions checked

  Scenario: A cassette recorded in two sessions verifies cleanly
    Given a cassette recorded in two separate record sessions
    When the cassette is verified
    Then verification passes with 4 interactions checked

  Scenario: An unsupported format version is a schema failure
    Given a cassette with format version 9
    When the cassette is verified
    Then verification fails with exactly the finding "schema.unsupported_format_version"

  Scenario: A hand-edited request body is a hash failure
    Given a recorded cassette whose second request body was edited without rehashing
    When the cassette is verified
    Then interaction 1 has findings "hash.stable_hash_mismatch", "hash.canonical_request_mismatch" and "hash.parsed_json_mismatch"

  Scenario: Reordered interactions are an ordering failure
    Given a recorded cassette whose first two interactions were swapped by hand
    When the cassette is verified
    Then verification fails with finding "ordering.interaction_start_order" for interaction 1

  Scenario: Keyed replay cassettes skip interaction start order
    Given a recorded cassette whose first two interactions were swapped by hand
    When the cassette is verified with keyed matching
    Then verification passes

  Scenario: Out-of-order stream timing is an ordering failure
    Given a recorded stream cassette whose chunk offsets decrease at event 2
    When the cassette is verified
    Then verification fails with finding "ordering.stream_offsets_not_monotonic" for interaction 0

  Scenario: An unredacted authorization header is a redaction failure
    Given a cassette whose first request carries an "Authorization" header with a secret value
    When the cassette is verified
    Then verification fails with finding "redaction.required_header_present" for interaction 0
    And no rendered report contains the secret value

  Scenario: Configured headers extend the default policy
    Given a cassette whose request carries "authorization" and whose response carries "x-api-key"
    And verify also requires redaction of "x-api-key"
    When the cassette is verified
    Then interaction 0 has 2 "redaction.required_header_present" findings
```

## Plan of work

Module map:

```plaintext
src/cassette/mod.rs                          (edit) `pub mod verify;`
src/cassette/canonical/mod.rs                (edit) `parse_json_body` moved here; envelope accessor
src/cassette/verify/mod.rs                   `verify_document`, `VerifyPolicy`, rule orchestration
src/cassette/verify/finding.rs               `FindingKind`, `FindingClass`, `Finding`, `Skip`, sort key
src/cassette/verify/report.rs                `VerificationReport`, `VerificationOutcome`
src/cassette/verify/schema.rs                version peek and typed parse
src/cassette/verify/ordering.rs              `adjacent_violations` kernel, interaction pair rule
src/cassette/verify/stream.rs                stream timing and transcript rules
src/cassette/verify/hash.rs                  hash, envelope, and parsed_json rules
src/cassette/verify/redaction.rs             required-header rule
src/cassette/verify/persisted.rs             `persisted()` round-trip helper
src/cassette/verify/test_support.rs          `#[cfg(test)]` shared strategies and builders
src/cassette/verify/*_tests.rs               unit and property tests per rule
src/verify.rs                                `verify_cassette`, bounded read, service options
src/config.rs                                (edit) `RedactionConfig::requires_redaction`
src/http_exchange.rs                         (edit) delegate predicate and JSON-body parse
src/cli/mod.rs, src/cli/source_loader.rs     (edit) `LoadedCommand` return type
src/cli_args.rs                              (edit) `VerifyOutputFormat`, `VerifyRedactionArgs`
src/cli/config_build.rs                      (edit) verify redaction union, validation, match_mode
src/cli/verify_output/{mod,text,json,github,escape}.rs   renderers and escapers
src/cli/verify_output/tests.rs               renderer, escaping, cap, secret-echo tests
src/bin/spycatcher_harness.rs                (edit) `main -> eyre::Result<ExitCode>`, verify branch
src/bin/spycatcher_harness/verify_run.rs     verify dispatch and stdout rendering
src/bin/spycatcher_harness/cli_startup.rs    (edit) config errors exit 2
src/lib.rs, src/error.rs, src/i18n.rs, src/i18n_tests.rs   (edit) start_harness, variant removal
i18n/en-US/spycatcher-harness.ftl            (edit) `verify-*` messages
```

### `EP-M0` feasibility probes (prototype, no tracked changes)

1. Persistence round trip.
   - Probe: a scratch property test, kept as a `/tmp` copy, writes
     `RecordedRequest` and `StreamEvent` values containing `null`, arbitrary
     `f64`, and nested JSON through `Cassette::write_to` and `from_reader`. It
     compares `persisted(parse_json_body(body))` with the reloaded
     `parsed_json`.
   - Decision: this confirms the `D4` comparison design. Any inequality means
     stop and escalate before `EP-M2`.
2. OrthoConfig list layering.
   - Probe: check whether
     `SPYCATCHER_HARNESS_CMDS_VERIFY_REDACTION__REQUIRE_HEADERS` merges into a
     `Vec<String>`, and whether the CLI and file layers do.
   - Decision: which layers `EP-M4` tests and documents.
3. Test dependencies.
   - Probe: add `googletest` and `pretty_assertions` in a scratch commit. Run
     `make msrv-check`, and one test with `#[rstest]` and `#[gtest]` in each
     attribute order.
   - Decision: exact versions, and the attribute order to record in the
     developers' guide.
4. Timestamp parsing.
   - Probe: confirm that `time` with the `parsing` feature parses every
     output of `SystemClock::now_rfc3339`.

Delete the scratch artefacts afterwards, and record the transcripts in
`Artefacts and notes`.

### `EP-M1` finding model, report, and I/O and schema checks

Assigned: `REQ-schema`, the domain half of `REQ-exit`, `D1`, `D2`, `D8`, `D10`,
and `D11`.

Red:

1. Add the dev-dependencies, at the versions chosen in `EP-M0`.
2. Write `src/cassette/verify/schema_tests.rs`, covering:
   - empty input;
   - non-JSON input, whose finding carries the line and column;
   - JSON that is not an object;
   - a missing `format_version`;
   - versions `-1`, `"1"`, and `4294967296`;
   - version `9`;
   - version `1` with an `interactions` field of the wrong type, whose finding
     carries the category, line, and column and no `serde_json` text;
   - a valid empty cassette.
3. Write `report_tests.rs`, with the class-to-outcome table and the sort-key
   cases.
4. Add the feature file with its schema scenario, and the BDD entrypoint.
5. Run
   `cargo nextest run --all-features -E 'test(/verify/)' --test cassette_verify_bdd`.
   It fails to compile because the module is missing, which is the expected
   red.

Green: implement `finding.rs`, `report.rs`, `schema.rs`, and `verify_document`
with the schema stage only. Declare `pub mod verify;` in `src/cassette/mod.rs`.

Refactor: keep files under 400 lines, then run the gates.

### `EP-M2` hash and request integrity

Assigned: `REQ-hash`, `D4`, `INV-hash-exact`, and the hash part of
`INV-record-clean`.

Red: write `hash_tests.rs`, with these named cases:

- a clean recorded request;
- each mutation row of the `INV-hash-exact` table;
- an empty body;
- a non-JSON body with `parsed_json: None`, which passes;
- a non-JSON body with `parsed_json: Some`, which fails;
- `null` and float bodies.

Then add the mutation property and the hash BDD scenario.

Green:

1. Move `parse_json_bytes` from `src/http_exchange.rs` to `cassette::canonical`
   as `pub(crate) fn parse_json_body`, and import it at the old call sites.
   This is a pure relocation.
2. Add a `pub(crate)` envelope accessor on `CanonicalRequest` that delegates to
   `json::canonical_request_value`.
3. Implement `persisted.rs` and `hash.rs` per `D4`. The pointers are
   `/interactions/<i>/request/{stable_hash,canonical_request,parsed_json}`.

Gates: `make check-fmt`, `make lint`, and `make test`.

### `EP-M3` ordering checks

Assigned: `REQ-ordering`, `D3`, `D7`, `INV-adjacent-complete`,
`L-adjacent-global`, `INV-order-pair`, and `INV-transcript`.

Red:

1. `ordering_tests.rs`: the exhaustive enumeration, the long-sequence
   property, the named kernel cases, and the pair-rule table.
2. `stream_tests.rs`, covering:
   - a length mismatch;
   - each descent being reported;
   - `ttft_ms` greater than the first offset;
   - `timing: None` (skip);
   - an empty transcript (skip);
   - an unparsable transcript;
   - the transcript property;
   - a `data: null` event.
3. The ordering BDD scenarios, including "recorded in two sessions".

Green:

1. Add the `time` crate's `parsing` feature.
2. Implement
   `adjacent_violations<T>(items: &[T], violates: impl Fn(&T, &T) -> bool) -> Vec<usize>`
   with iterator adapters. Clippy's `indexing_slicing` is denied, so it must
   not index.
3. Implement the pair rule, the stream rules, and the skip records.

The codes are `ordering.interaction_start_order`,
`ordering.stream_timing_length_mismatch`,
`ordering.stream_offsets_not_monotonic`,
`ordering.stream_ttft_after_first_event`,
`ordering.stream_transcript_unparsable`, and
`ordering.stream_transcript_mismatch`.

Gates: as for `EP-M2`.

### `EP-M4` redaction checks and layered verify configuration

Assigned: `REQ-redaction`, the configuration half of `REQ-cli`, `D6`,
`GAP-redaction-layering`, and `INV-redaction`.

Red:

1. `redaction_tests.rs`, covering:
   - request and response headers;
   - mixed case;
   - duplicate headers, each reported;
   - duplicate policy entries;
   - an empty effective policy;
   - the two properties.
2. `tests/cli_layering/` cases:
   - `[cmds.verify.redaction] require_headers = ["x-api-key"]` gives the
     effective list `["authorization", "x-api-key"]`;
   - `include_default_headers = false` removes `authorization`;
   - an invalid token such as `"bad header"` is a configuration error;
   - `[cmds.verify] match_mode = "keyed"` reaches `HarnessConfig.match_mode`.
3. The redaction BDD scenarios.

Green:

1. Add `RedactionConfig::requires_redaction`, and make `should_persist_header`
   delegate to it.
2. Implement `redaction.rs`.
3. Add `VerifyRedactionArgs`, with fields
   `require_headers: Option<Vec<String>>` and
   `include_default_headers: Option<bool>`, and a config-only `match_mode` to
   `src/cli_args.rs` and `VerifyArgs`.
4. In `to_verify_config`, compute the union of default and configured
   headers. Validate each token with `axum::http::HeaderName::from_bytes`; this
   belongs in the configuration layer, not the domain. Then set `redaction` and
   `match_mode`.

Gates: as for `EP-M2`.

### `EP-M5` application service, renderers, CLI, and exit codes

Assigned: `REQ-output`, `REQ-exit`, `REQ-cli`, `GAP-stub`, `D2`, `D5`,
`INV-exit`, `INV-no-secret-echo`, `INV-annotation-safe`, `INV-cap`,
`INV-sorted`, and `INV-locality`.

Red:

1. `tests/verify_cli_e2e.rs`, covering:
   - the pass case;
   - the per-class exit table, including the exit-`2` rows;
   - `--format json`, checked with `googletest`;
   - `cmds.verify.format = "json"` set in a config file;
   - the FIFO and directory rows;
   - the snapshots.
2. `src/cli/verify_output/tests.rs`, covering escaping, injection, the
   annotation cap, secret echo, and sorted output.
3. Update `tests/lifecycle_tests.rs:171-188` to expect `InvalidConfig`
   without creating a cassette file.
4. Update the loader call sites to `LoadedCommand`. They fail to compile
   until the green step lands.

Green:

1. In `src/verify.rs`, re-exported from `src/lib.rs`, implement
   `verify_cassette(config: &HarnessConfig) ->
   HarnessResult<VerificationReport>`,
   plus a `pub(crate)` `verify_cassette_with(config, &VerifyServiceOptions)`
   for the test-only ceiling. In order, it:
   1. validates the cassette name with an extracted `validate_cassette_name`,
      ignoring `mode` and `upstream`;
   2. opens the parent `Dir` read-only;
   3. checks the metadata;
   4. reads with the bound;
   5. maps I/O failures to findings;
   6. calls `verify_document`.
2. Make `cli::LoadedCommand`, with fields `config: HarnessConfig` and
   `verify_output: Option<VerifyOutputFormat>`, the return type of every
   `load_subcommand_config*` function. Update every caller in the same commit.
3. Add the renderers in `src/cli/verify_output/`.
   - Entry point: `render_report(report, &RenderContext { format,
     cassette_path, loader }, writer)`.
   - `escape.rs` holds `escape_workflow_data`, `escape_workflow_property`,
     and `escape_control`.
   - Add the `verify-*` Fluent keys to the catalogue.
4. Update the binary.
   - `main` returns `eyre::Result<ExitCode>`.
   - `cli_startup` exits `2` for every configuration error other than a
     display request.
   - `verify_run.rs` branches on `Mode::Verify` before the runtime is built
     and calls `verify_cassette`.
   - On `Err`, it writes the localized error to stderr and exits `2`.
   - Otherwise it renders to locked stdout. A write failure exits `2`;
     success exits `0` or `1` according to the outcome.
5. Make `start_harness` reject `Mode::Verify` with `InvalidConfig` before
   `prepare_cassette` runs.
   - Remove `Verify` from `prepare_cassette`.
   - Update the rustdoc at `src/lib.rs:90-95`.
   - Remove `ModeNotYetImplemented` and every reference to it.

Refactor: if `src/cli/mod.rs` would exceed 400 lines, move `VerifyArgs` to
`src/cli/verify_args.rs`.

Gates: as for `EP-M2`.

### `EP-M6` documentation, ADR, roadmap closure, and gates

Assigned: `GAP-anchor`, the documentation duties in `AGENTS.md`, and the
documented-schema half of `REQ-output`.

1. Update `docs/users-guide.md`:
   - Replace the text at `:9`, `:28-30`, and `:334`.
   - Add a "Verifying cassettes" section after the CLI exit-status text near
     `:446`. Cover:
     - the classes and codes;
     - the exit codes `0`, `1`, and `2`, including the configuration-error
       change;
     - the three formats, with examples;
     - the JSON envelope and its evolution rule;
     - a GitHub Actions workflow snippet that runs from the repository root;
     - `cmds.verify.redaction.*` and `cmds.verify.match_mode`;
     - the skip rules and the concurrent-recording residual;
     - the non-goals: header values, query parameters, bodies, and
       tampering.
   - Reconcile the example at `:497-503`.
2. Update `docs/developers-guide.md`:
   - Add the new modules to the module tables at `:281-293`, `:309-318`, and
     `:371-379`.
   - Add a "Cassette verification" subsection covering:
     - the domain, service, and adapter split;
     - how to add a rule and its code;
     - the `sse` module-cycle note;
     - the assertion-style rules.
   - State the reuse policy:
     - `adjacent_violations` is the only adjacent-order scan;
     - `escape_workflow_*` are the only workflow-command escapers, and task
       2.2.2 reuses them;
     - `RedactionConfig::requires_redaction` is the only redaction predicate;
     - `parse_json_body` is the only body parser.
3. Update `docs/spycatcher-harness-design.md`:
   - Turn the regression-suite paragraph into the heading
     `### Recommended additions for regression suite integration`, so the
     roadmap anchor resolves, and link it to the implementation.
   - Update "CLI shape" and the `HarnessError` example.
   - Add `### Cassette verification (task 2.2.1)` under "Implementation
     decisions". Summarize `D1` to `D11`, include the `L-adjacent-global`
     argument and the order-isomorphism justification from `D7`, and link ADR
     0003.
4. Add
   `docs/adrs/adr-0003-define-the-cassette-verification-report-and-exit-contract.md`.
   It records the JSON envelope, the `kind` discriminator shared with task
   2.2.2, the evolution rule, the qualified codes, the exit codes, and the
   GitHub annotation format and cap.
5. List ADR 0003 and this ExecPlan in `docs/contents.md`.
6. Tick task 2.2.1 and its three success criteria in `docs/roadmap.md` once
   `EP-M5` acceptance passes.
7. Run `make fmt`, keeping only the hunks in files this plan touched. Then
   run `make check-fmt`, `make lint`, `make test`, `make markdownlint`,
   `make nixie`, and `make spelling`.

## Milestones and plateaus

No milestone needs a compatibility layer: the library is pre-release, and
callers update in the same commit.

- `EP-M0`:
  - End state: probe results recorded; no tracked changes.
  - Recovery: delete the scratch artefacts.
- `EP-M1`:
  - End state: `verify_document` reports schema findings and outcomes. The CLI
    is still a stub.
  - Acceptance: the schema and report tests, and the BDD schema scenario.
  - Conformance: `REQ-schema` is met in the domain; there is no
    persisted-format change and no new runtime dependency.
  - Recovery: revert the milestone commit.
  - Remaining gaps: the other rules and the CLI.
- `EP-M2`:
  - End state: hash findings.
  - Acceptance: the hash tests, properties, and BDD scenario.
  - Conformance: `git diff origin/main -- src/cassette/canonical
    src/http_exchange.rs` shows only the relocation and the accessor.
  - Recovery: revert.
- `EP-M3`:
  - End state: ordering findings.
  - Acceptance: the enumeration, the properties, and the BDD scenarios,
    including the two-session case.
  - Conformance: `SseParser` is unchanged.
  - Recovery: revert.
- `EP-M4`:
  - End state: redaction findings and `cmds.verify` layering.
  - Acceptance: the redaction, layering, and BDD tests.
  - Conformance: record behaviour is unchanged, and the existing record tests
    pass.
  - Recovery: revert.
- `EP-M5`:
  - End state: `spycatcher-harness verify` works end to end.
  - Acceptance: the e2e tests, snapshots, renderer properties, and lifecycle
    test.
  - Conformance: public API changes match `Tolerances` exactly, and exit codes
    match `D2`.
  - Recovery: revert.
- `EP-M6`:
  - End state: documentation accurate, ADR accepted, roadmap ticked.
  - Acceptance: all gates pass, and the anchor resolves.
  - Conformance: discoveries are reconciled with the design and the roadmap.
  - Recovery: revert the documentation commit.

## Concrete steps

Run everything from the repository root. Run the gates sequentially, each
through `tee` with `pipefail`:

```bash
set -o pipefail
BRANCH=$(git branch --show-current)
make check-fmt 2>&1 | tee /tmp/check-fmt-spycatcher-harness-$BRANCH.out
make lint      2>&1 | tee /tmp/lint-spycatcher-harness-$BRANCH.out
make test      2>&1 | tee /tmp/test-spycatcher-harness-$BRANCH.out
```

Focused loops:

```plaintext
Red:   cargo nextest run --all-features -E 'test(/verify/)'   -> fails for the named missing rule
Green: same command                                            -> selected tests pass
BDD:   cargo nextest run --all-features --test cassette_verify_bdd
E2E:   cargo nextest run --all-features --test verify_cli_e2e
Snap:  cargo insta review   (accept only renderings confirmed as the intended contract)
```

Commit after each milestone's gates pass, using an imperative subject such as
`Add hash recomputation checks to cassette verify`.

## Validation and acceptance

Make a fresh recording against a stub upstream, as the record e2e tests do, and
run these checks from a temporary directory holding it:

1. Run
   `spycatcher-harness verify --cassette-dir fixtures/llm --cassette-name ok.json`.
   Expect
   `verified fixtures/llm/ok.json: N interactions, 0 findings, 0 checks skipped`
   and exit `0`.
2. Replace one `stable_hash` with 64 zeros and run the same command:
   - Text output prints a `hash.stable_hash_mismatch` line and exits `1`.
   - With `--format json`, `jq -r '.outcome, .findings[0].code'` prints
     `failed` and `hash.stable_hash_mismatch`.
   - With `--format github`, the first line starts with
     `::error file=fixtures/llm/ok.json,title=spycatcher-harness verify%3A
     hash.stable_hash_mismatch::`.
3. Run `spycatcher-harness verify --cassette-name ''`. Expect exit `2`, an
   error on stderr, and nothing on stdout.

Quality criteria:

- Tests: `make test` passes, and each new test was red before its rule
  landed.
- Verification: every invariant in `Methods per obligation` is discharged,
  with its non-vacuity evidence and seeded-fault results recorded in
  `Artefacts and notes`.
- Lint: `make check-fmt`, `make lint`, `make markdownlint`, `make nixie`, and
  `make spelling` pass.
- Security: `INV-no-secret-echo` and `INV-annotation-safe` pass, and the
  seeded faults for both are rejected.

## Idempotence and recovery

Every step can be re-run safely, and verify itself is read-only. Apply each
seeded fault in the working tree, observe the failure, and revert it before
committing; confirm with `git diff --stat`. Commit proptest regression files
only when they capture a real failure that has been fixed. Gate Unix-only
tests, such as the FIFO case, with `#[cfg(unix)]`.

## Artefacts and notes

Reconnaissance evidence at `c3e6d76`:

```plaintext
$ git log --oneline -S "Recommended additions for regression suite" -- docs
5430ebc Initial design (#4)        # introduced as a paragraph, never a heading
$ rg -n "relative_offset_ms" docs/developers-guide.md
671: `relative_offset_ms` measures from the session start to the interaction start
```

The GitHub workflow-command reference, consulted on 2026-11-15, gives the form
`::error file={name},line={line},endLine={endLine},title={title}::{message}`.

Append probe, red/green, seeded-fault, and snapshot-review transcripts here
during implementation.

## Interfaces and dependencies

Domain (`spycatcher_harness::cassette::verify`; no I/O, no adapter imports):

```rust
#[non_exhaustive]
pub struct VerifyPolicy {
    pub required_redactions: RedactionConfig,
    pub match_mode: MatchMode,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FindingClass { Io, Schema, Ordering, Hash, Redaction }

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingKind {
    CassetteNotFound,
    NotARegularFile,
    CassetteTooLarge { limit_bytes: u64 },
    CassetteUnreadable { io_kind: std::io::ErrorKind },
    MalformedDocument { category: JsonErrorCategory, line: usize, column: usize },
    FormatVersionMissing,
    FormatVersionInvalid,
    UnsupportedFormatVersion { found: u32, supported: u32 },
    InteractionStartOrder,
    StreamTimingLengthMismatch { events: usize, offsets: usize },
    StreamOffsetsNotMonotonic,
    StreamTtftAfterFirstEvent,
    StreamTranscriptUnparsable,
    StreamTranscriptMismatch,
    StableHashMissing,
    StableHashMismatch { stored: String, recomputed: String },
    CanonicalRequestMissing,
    CanonicalRequestMismatch,
    ParsedJsonMismatch,
    RequiredHeaderPresent { header: String },  // lower-cased policy token, never the value
}

impl FindingKind {
    #[must_use] pub const fn class(&self) -> FindingClass;
    #[must_use] pub const fn code(&self) -> &'static str;      // qualified, e.g. "hash.stable_hash_mismatch"
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding { pub kind: FindingKind, pub interaction: Option<usize>, pub pointer: JsonPointer }

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub interactions_checked: usize,
    pub findings: Vec<Finding>,   // sorted per D10
    pub skipped: Vec<Skip>,       // code + interaction + pointer
}

impl VerificationReport { #[must_use] pub fn outcome(&self) -> VerificationOutcome; }

#[must_use]
pub fn verify_document(document: &[u8], policy: &VerifyPolicy) -> VerificationReport;

pub(crate) fn adjacent_violations<T>(items: &[T], violates: impl Fn(&T, &T) -> bool) -> Vec<usize>;
```

`JsonPointer` is a small newtype over typed segments, each either
`Key(&'static str)` or `Index(usize)`. Its `Display` produces RFC 6901 text,
and its `Ord` follows `D10`. `JsonErrorCategory` mirrors
`serde_json::error::Category` without re-exporting it.

`VerifyPolicy` is built from `HarnessConfig` in `src/verify.rs`, so the domain
does not depend on the session configuration. The application service:

```rust
pub const MAX_VERIFY_DOCUMENT_BYTES: u64 = 256 * 1024 * 1024;

/// # Errors
/// Returns `HarnessError::InvalidConfig` for an invalid cassette name; cassette
/// problems are findings, not errors.
pub fn verify_cassette(config: &HarnessConfig) -> HarnessResult<VerificationReport>;
```

Configuration (`src/config.rs`):

```rust
impl RedactionConfig {
    /// Returns whether `name` must not be persisted, comparing ASCII case-insensitively.
    #[must_use] pub fn requires_redaction(&self, name: &str) -> bool;
}
```

CLI (`spycatcher_harness::cli`):

```rust
#[non_exhaustive]
pub struct LoadedCommand { pub config: HarnessConfig, pub verify_output: Option<VerifyOutputFormat> }

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyOutputFormat { #[default] Text, Json, Github }

pub struct RenderContext<'a> {
    pub format: VerifyOutputFormat,
    pub cassette_path: &'a Utf8Path,
    pub loader: &'a FluentLanguageLoader,
}

/// # Errors
/// Returns write failures from `writer`.
pub fn render_report(
    report: &VerificationReport,
    context: &RenderContext<'_>,
    writer: &mut impl std::io::Write,
) -> std::io::Result<()>;
```

The escapers live in `cli::verify_output::escape`: `escape_workflow_data`,
`escape_workflow_property`, and `escape_control`, all `pub(crate)`. No other
code escapes workflow commands. The JSON DTOs (`JsonReportV1`, `JsonFinding`,
and `JsonSkip`) are private to `cli::verify_output::json`. The envelope looks
like this:

```json
{
  "report_version": 1,
  "kind": "cassette_verification",
  "cassette": "fixtures/llm/smoke.json",
  "outcome": "failed",
  "interactions_checked": 3,
  "findings": [
    {
      "code": "hash.stable_hash_mismatch",
      "class": "hash",
      "interaction": 1,
      "pointer": "/interactions/1/request/stable_hash",
      "params": { "stored": "00…00", "recomputed": "9f…c1" }
    }
  ],
  "skipped": []
}
```

Binary:
`verify_run::run_verify(command: LoadedCommand, loader:
&FluentLanguageLoader) -> ExitCode`,
and `main` becomes `fn main() -> eyre::Result<ExitCode>`.

Reused rather than duplicated:

- `canonicalize` and `stable_hash`;
- `parse_json_body`, after the relocation;
- the redaction predicate, via `requires_redaction`;
- `SseParser`;
- the Fluent loader;
- the `process_output` writer conventions;
- in tests, the `record_tests_helpers` record builders.

Dependencies:

- dev-only `googletest` (about `0.14`) and `pretty_assertions` (about `1.4`),
  with exact versions settled in `EP-M0`;
- the `time` crate's `parsing` feature, on the existing dependency.

There is no new runtime crate and no prover tooling.

## Revision note

- 2026-11-15: initial draft from Wyvern reconnaissance.
- 2026-11-15: applied the community-of-experts review. The four panels were
  structure and contracts, failure modes and security, verification and
  viability, and alternatives and cost.
  - Verification approach:
    - Removed Verus in favour of exhaustive order-isomorphic enumeration plus
      property tests (`D7`). The panel found the textual bridge infeasible,
      the proof thin, and the tooling cost disproportionate.
    - Made the non-vacuity mechanisms implementable: generators that reach
      each class by construction, deterministic class-mix tests, and
      rejection budgets.
    - Added the exact-set, locality, and sortedness invariants.
  - False-positive and safety fixes:
    - Added the `recorded_at` co-condition to the ordering rule, after the
      panel found that multi-session cassettes would fail.
    - Hash recomputation now starts from body bytes and compares values
      through a persistence round trip, after `null` and float false
      positives were found.
    - Schema findings no longer carry `serde_json` text, which can echo
      secrets.
    - Specified FIFO and size handling.
    - Applied injection escaping to every output format.
    - Made the redaction policy a union over validated tokens.
    - Made skipped checks visible in the report.
  - Contract and boundary changes:
    - Configuration errors now exit `2` (`D2`).
    - Moved the output format out of `HarnessConfig` into
      `cli::LoadedCommand`.
    - Decoupled the domain from `http_exchange` via `requires_redaction` and
      the relocated `parse_json_body`.
    - `FindingKind` now derives class and code, and the JSON DTO is separate
      from the domain types (`D11`).
    - Added the JSON `kind` discriminator and an evolution rule shared with
      task 2.2.2.
  - Recounted the tolerances against historical overruns.
  - Corrected facts: where `canonical_request_value` lives, that
    `open_rooted_parent` is private, and an additional
    `ModeNotYetImplemented` reference at `docs/users-guide.md:334`.
  - Remaining work: user approval, then `EP-M0`.
