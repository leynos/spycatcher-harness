# Adopt OrthoConfig v0.9.0 and consolidate CLI localization

This ExecPlan is a living document. Keep `Constraints`, `Tolerances`, `Risks`,
`Progress`, `Surprises & discoveries`, `Decision log`,
`Outcomes & retrospective`, `Conformance basis`, and `Verification plan`
current.

Status: COMPLETE

## Purpose / big picture

Upgrade Spycatcher Harness to the tagged `ortho_config` v0.9.0 release and
delegate localized command parsing to its public APIs. Users retain the current
record, replay, and verify configuration precedence, Fluent identifiers, help
and error behaviour, and the public localization helpers. A build on the
declared Rust 1.89.0 floor and the repository's normal gates will make success
observable.

## Constraints

- Use only APIs present in the `ortho_config` v0.9.0 tag.
- Keep the `load_subcommand_config*` and
  `cli::localization::{LocalizeCmd, try_parse_localized_from_iter}` public
  signatures usable by downstream callers.
- Preserve defaults < file < environment < CLI precedence in
  `cmds.record`, `cmds.replay`, and `cmds.verify`; retain record-only upstream
  configuration and nested localization values.
- Preserve Fluent IDs, including `cli-*`, `cli-record-*`, and
  `cli-merge-help`, and preserve the `binary` and `version` formatting
  arguments and untranslated fallback.
- Keep early CLI-localizer selection separate from the authoritative
  post-merge harness-localizer construction. Retain the disable switch and
  `NoOpLocalizer` fallback.
- Do not make library code print or terminate the process. Help and version
  remain successful display requests; parse errors remain errors.
- Do not enable YAML or a metrics recorder, add a new CLI command, change the
  translation catalogue, or derive documentation metadata without a consumer.
- Treat the CLI as an adapter: do not move parser or localization concerns into
  the harness domain or expose domain APIs to Clap.
- Keep `docs/execplans/adopt-ortho-config-v0-8-0.md` and the v0.8 localization
  plans as historical records.

## Tolerances (exception triggers)

- Scope: stop if the implementation exceeds 25 changed files or 900 net code
  lines, excluding generated lockfile updates and prose documentation.
- Interface: stop if preserving a released public helper requires changing its
  signature or removing an existing catalogue identifier.
- Dependencies: stop if a new runtime dependency or an optional OrthoConfig
  feature is required. A test-only dependency is acceptable when it provides
  direct evidence for an explicit acceptance criterion and does not change the
  runtime graph.
- Configuration: stop if a required precedence or namespace cannot be
  expressed through the v0.9.0 public API without changing the user contract.
- Ambiguity: stop if the locked graph cannot support the declared Rust floor or
  if upstream parsing cannot preserve error/display semantics.
- Validation: do not commit or request CodeRabbit review while a named
  deterministic gate fails. Fix or independently reproduce and document any
  baseline failure first.

## Risks

- Risk: v0.9.0's parser may use a different Fluent identifier shape or omit
  project-specific formatting arguments. Severity: high Likelihood: medium
  Mitigation: use `LocalizeCmd::with_base` and `parse_localized_command`; keep
  only a narrow adapter for the existing `cli-*`, version, and merge-help
  contract, backed by exact catalogue tests.
- Risk: v0.9.0 raises the compiler floor and updates its macro and transitive
  dependency graph. Severity: medium Likelihood: medium Mitigation: inspect the
  locked graph and compile it with Rust 1.89.0; preserve the current default
  `serde_json` and TOML feature selection.
- Risk: configuration-value environment variables are read by the
  `load_and_merge_subcommand` convenience API and cannot be injected with
  `MapEnv`. Severity: high Likelihood: high Mitigation: test these values in a
  child process with an explicit controlled environment. Use `MapEnv` only for
  discovery APIs that accept an `EnvSource`.
- Risk: downstream users rely on public helper and catalogue semantics that
  cannot be inferred from internal call sites. Severity: high Likelihood:
  medium Mitigation: preserve signatures and IDs, and retain compatibility
  tests.

## Progress

- [x] (2026-09-30 10:02Z) Rename the work branch, establish tracking against
  `origin/issue-123-adopt-ortho-config-v0-9-0-and-consolidate-localized-cli-parsing-without-changing-configuration-precedence`,
  and rename the Lody session.
- [x] (2026-09-30 10:02Z) Load the requested skills; inspect the CLI adapter,
  current test layout, and tagged v0.9.0 migration, manifest, and localization
  APIs.
- [x] (2026-09-30 10:02Z) Draft this migration plan and pass the documentation
  formatting, spelling, Markdown, and diagram gates.
- [x] (2026-09-30 10:47Z) Move configuration and locale environment tests into
  isolated child processes; cover file/environment/CLI precedence for every
  command, missing inheritance, malformed files, and record-only upstream
  settings. Baseline tests pass on OrthoConfig 0.8.0 because this slice
  protects existing behaviour rather than introducing a new one.
- [x] (2026-09-30 11:11Z) Review the complete test-isolation diff. Centralize
  child snapshot loading, add bounded and redacted child-stderr context, and
  assert the secret-value configuration path loads only the environment-key
  name. All six focused CLI test targets pass after these review fixes.
- [x] (2026-09-30 11:14Z) Run the full deterministic gates after those changes.
  Five gates passed; `make lint` found six Clippy issues in checked slicing,
  shadowing, and implicit string cloning. Apply the lint suggestions and rerun
  all gates before requesting CodeRabbit.
- [x] (2026-09-30 11:29Z) Address CodeRabbit's complete-file review findings:
  redact environment values before truncating stderr, preserve UTF-8 boundaries
  within the byte limit, and replace the module-wide dead-code expectation with
  target-specific probe adapters. Remove generated `typos.toml` churn. All six
  focused targets pass after these changes; rerun full gates before review.
- [x] (2026-09-30 11:32Z) Repeat all deterministic gates. Five passed, including
  337 Nextest cases and 27 doctests; Clippy found one `needless_pass_by_value`
  in the localization test shim. Change it to borrow the probe request before
  the next full run.
- [x] (2026-09-30 11:40Z) All six full deterministic gates passed after the
  helper split; CodeRabbit then requested tracing spans on child launch and
  probe dispatch. Add `skip_all` instrumentation so no arguments, config, or
  environment values become span fields, then validate and review again.
- [x] (2026-09-30 11:48Z) The follow-up CodeRabbit pass confirmed all six gates
  passed and requested proof that the `handle_probe` span reaches captured
  output. Enable synthesized span-creation events and assert the span is
  present before checking sentinel non-disclosure.
- [x] (2026-09-30 11:53Z) The next full run passed all six deterministic gates:
  formatting, lint, typecheck, 337 tests plus 27 doctests, Markdown/spelling,
  and Mermaid. CodeRabbit's only finding was the unstaged `typos.toml` churn
  generated by the spelling gate; restore that file before continuing.
- [x] (2026-09-30 12:00Z) Restore generated `typos.toml` churn, rerun the
  review against the 11 intended staged files, and receive a clean result with
  zero findings. Commit the test-isolation milestone as `36072ac` and push it
  to the issue branch.
- [x] (2026-09-30 12:06Z) Upgrade `ortho_config` and its resolved macro crate
  to 0.9.0, declare Rust 1.89.0, add the locked all-targets/all-features CI
  floor check, and document the supported compiler and discovery error
  distinction. The first MSRV compile exposed an implicit `cap-std/fs_utf8`
  feature inherited from OrthoConfig 0.8 through dependency unification. Make
  the project's existing `cap-std` feature use explicit; the retry passed.
  Evidence: both locked dependency-tree queries report runtime and macro at
  0.9.0, and
  `cargo +1.89.0 check --locked --workspace --all-targets --all-features`
  passed after the direct feature declaration.
- [x] (2026-09-30 12:09Z) Run the four requested CLI regression targets after
  the v0.9.0 upgrade: 24 layering unit cases, 9 layering BDD cases, 40
  localization unit cases, and 5 binary localization end-to-end cases passed.
  Logs are `/tmp/test-cli-layering-unit-issue-123-v09.out`,
  `/tmp/test-harness-cli-layering-bdd-issue-123-v09.out`,
  `/tmp/test-cli-localization-unit-issue-123-v09.out`, and
  `/tmp/test-binary-localization-e2e-issue-123-v09.out`.
- [x] (2026-09-30 12:16Z) The first full six-gate run passed formatting,
  typecheck, all 337 tests plus 27 doctests (4 ignored), Markdown, and Mermaid.
  Clippy found a missing-`const` lint on the unchanged `should_forward_header`
  predicate in `src/upstream.rs`; `git blame` and `origin/main` confirm the
  function predates this issue branch. Make the predicate `const`, rerun
  `make lint` successfully, and repeat the Rust 1.89.0 all-targets/all-features
  check successfully. Logs: full pass/failure bundle suffix `-12.out`, lint
  retry suffix `-13.out`, and `/tmp/msrv-check-issue-123-after-lint-fix.out`.
- [x] (2026-09-30 12:20Z) Repeat all six repository gates after the Clippy
  fix and plan formatting. Formatting, typecheck, lint, 337 tests plus 27
  doctests (4 ignored), Markdown, and Mermaid all pass. Logs use suffix
  `-14.out`; restore the generated `typos.toml` refresh before review.
- [x] (2026-09-30 12:31Z) CodeRabbit reviewed 16 changed files and found one
  major issue in the child probe: the response marker could share a line with
  Rust's test-runner progress output, while the parent accepts it only at line
  start. Prefix the response with a newline and rerun the three affected child
  probe suites; all 73 tests passed. Full gates and review remain pending.
- [x] v0.9.0 does not expose an incompatibility in the established CLI
  contracts that needs a failing migration test. Keep the isolated baseline
  contract coverage green and test the upstream parser integration directly
  without manufacturing a failure.
- [x] (2026-10-03) Replace duplicate localized parsing with v0.9.0
  `LocalizeCmd::with_base(...).localize_self` and `parse_localized_command`.
  Retain the public project trait and helper, mapping `long_about` to the
  existing `long-about` key and `after_long_help` to `merge-help`, and restore
  `binary`/`version` arguments at each command boundary. A focused
  compatibility test covers the root/subcommand IDs, interpolation, and
  merge-help text; a second test exercises the upstream `FromArgMatches` error
  path and verifies the conversion error is localized once.
- [x] (2026-10-03) Run the requested CLI suites after parser consolidation:
  layering unit (24), layering BDD (9), localization unit (40), and binary
  localization end-to-end (6) all passed on the final runtime tree. The
  dedicated two-case compatibility target also passed. Logs are
  `/tmp/test-cli-layering-unit-issue-123-final.out`,
  `/tmp/test-harness-cli-layering-bdd-issue-123-final.out`,
  `/tmp/test-cli-localization-unit-issue-123-final.out`,
  `/tmp/test-binary-localization-e2e-issue-123-no-side-effects.out`, and
  `/tmp/test-cli-localize-cmd-compat-issue-123-final.out`.
- [x] (2026-10-03) Confirm all-command file/environment/CLI layering, nested
  locale overrides, and record-only upstream scope with the controlled child
  environment suites. The new binary test also proves an unknown option fails
  before binding the requested listener, contacting a configured local
  upstream, or creating a cassette directory.
- [x] (2026-10-03) Update the user and developer guides, localization
  implementation notes, and this plan; retain the historical v0.8.0 plans.
  `make fmt` formatted the Rust and Markdown sources and Markdown formatting
  reported zero errors. Full Markdown, spelling, and diagram gates remain part
  of the final sequence.
- [x] (2026-10-04) First full parser-milestone gate attempt: formatting,
  typecheck, 340 tests, 27 doctests (4 ignored), Markdown, and Mermaid passed.
  `make lint` found a similar-name binding and two missing `OrthoConfig`
  backticks in the new adapter. Rename the local binding and format the API
  name, then repeat every deterministic gate. The test log ended with
  successful summaries but did not capture the shell exit marker; the next full
  run must provide definitive test-command completion evidence.
- [x] (2026-10-04) Fix the parser-adapter and compatibility-test lint findings:
  backtick `OrthoConfig`, rename the similar local binding, and recover a
  poisoned test mutex without a panic. The fresh sequential run passed all six
  repository gates: formatting, typecheck, Clippy and Whitaker, all 340 tests
  and 27 doctests (4 ignored), Markdown/spelling, and Mermaid. The locked Rust
  1.89.0 all-targets/all-features check also passed.
- [x] (2026-10-04) CodeRabbit reviewed 21 files, including the staged
  compatibility test, and reported zero findings. Review log:
  `/tmp/coderabbit-93806b0d-068e-40a6-9c86-809b8f1dc9c6-issue-123-adopt-ortho-config-v0-9-0-and-consolidate-localized-cli-parsing-without-changing-configuration-precedence-18.out`.
- [x] (2026-10-04) Commit the reviewed parser and documentation milestone as
  `402ce62` and push it over SSH. GitHub authenticated as `leynos`, and the
  local and remote branch heads both resolved to
  `402ce620261a92924228ce87c36b727d6674a640`.
- [x] (2026-10-04) Create draft PR
  [#147](https://github.com/leynos/spycatcher-harness/pull/147) against `main`.
  Its summary says `Closes #123`, and its final References section links the
  renamed Lody session.
- [x] (2026-10-04) Rebase the eight issue commits from `e56704a` onto the
  observed `origin/main` target `58e54ff`. This is historical evidence; the
  branch was rebased again when main advanced. The CI conflict kept main's
  shared-action pins and comprehensive Make/workflow-contract gates, added the
  Rust 1.89.0 check, and retained the exact target-main lockfile during replay.
  Main's `rstest-bdd` 0.6 graph and serial `make all` were preserved.
- [x] (2026-10-10) Rebase the seven surviving issue commits from `58e54ff` onto
  the fetched `origin/main` target `d5fc21f`. `git range-diff` maps seven of
  the original eight commits; the generated-spelling-only commit became
  obsolete under main's new `typos-config-builder` gate. Resolve the CI
  conflict by retaining main's updated action pins, disabled uv cache,
  mdtablefix 0.6.1, and `mold` setup while keeping the Rust 1.89.0 check. Keep
  main's exact `Cargo.lock` blob during replay. Its new build-standard files,
  serial comprehensive Make gate, workflow-contract checks, and `rstest-bdd`
  0.6 graph remain in the rebased tree.
- [x] (2026-10-10) Resolve the generated spelling artefact conflict with the
  project generator. `make spelling` regenerated `typos.toml` byte-for-byte
  equal to the target-main version, and a second run reported it current. The
  skipped issue commit only replaced one ignore regex; main's generator
  selected the target version for the merged source. Logs:
  `/tmp/spelling-issue123-main-rebase.out` and
  `/tmp/spelling-issue123-main-rebase-idempotence.out`.
- [x] (2026-10-10) Regenerate `Cargo.lock` for `ortho_config` 0.9.0 and the
  merged manifests. It resolves both `ortho_config` and `ortho_config_macros`
  to 0.9.0 while preserving the manifest feature set.
- [x] (2026-10-10) Run the focused layering/localization tests, full repository
  tests, format, typecheck, lint, spelling, Markdown, diagram, Make/workflow
  contract gates, and the locked Rust 1.89.0 check. The latter passed with
  `RUSTFLAGS=''`, matching CI's override of local development rustflags while
  retaining normal build admission.
- [x] (2026-10-10) Rerun `make check-fmt`, `make spelling`, and
  `make markdownlint` after correcting the Progress-list indentation and
  wrapping; all passed. The follow-up CodeRabbit review confirmed that fix.
- [x] (2026-10-10) Verify CodeRabbit's concern about the user-guide
  `with_base(...).localize(...)` example against the tagged v0.9.0 source. The
  wrapper returns `Command`, and the shipped rustdoc uses the same call; the
  reported type mismatch is a false positive. Keep the correct example.
- [x] (2026-10-10) Commit the regenerated lockfile and migration evidence as
  `94cb2fc`. Force-push with an explicit lease from observed remote SHA
  `1194b85`; verify remote parity at `94cb2fc`. Update and read back draft PR
  #147 with the required title reference, `Closes #123`, current gate evidence,
  and a terminal References section containing the Lody session link.
- [x] (2026-10-10) Diagnose the first post-rebase hosted CI failure: the
  coverage ratchet measured 87.09% against an 88.00% minimum. The isolated CLI
  probes cleared `LLVM_PROFILE_FILE` along with configuration inputs, so child
  executions did not contribute to coverage. Forward only that instrumentation
  variable; a CI-equivalent local coverage run then measured 88.99%. The hosted
  CodeScene review also flagged string-heavy test helper signatures; group the
  related scenario inputs and verify the files with `cs review`. The final
  local coverage rerun reached 89.04%; see the completed validation entry
  below. Hosted revalidation remains pending.
- [x] (2026-10-10 22:00Z) Complete the post-fix validation sequence. The four
  requested CLI suites passed (24 layering unit, 9 layering BDD, 40
  localization unit, and 6 binary localization cases), followed by
  `make check-fmt`, `make test` (432 tests and 27 doctests; 4 ignored),
  `make typecheck`, `make lint`, spelling, Markdown, Mermaid,
  workflow-contract, and `make test-make-all` gates. The locked Rust 1.89.0
  all-targets, all-features check passed. CI-equivalent coverage measured 3,354
  of 3,767 lines (89.04%), above the 88.00% minimum. CodeRabbit reviewed the
  validated diff and reported zero findings. Hosted revalidation follows.
- [x] (2026-10-10 22:43Z) Validate the published follow-up at
  `0d6da52aae9b04e32c5f7806cd87b98578a23550`. CI attempt 1 encountered
  `ETXTBSY` in `tests/build_standard/reader_tests.rs`, which is unchanged from
  `origin/main` and was introduced by main's `Adopt the Rust build standard`
  commit. The exact test passed in 20 sequential local runs using the CI
  coverage target directory. CI attempt 2 passed, including the coverage
  ratchet at 89.04% against the 89.0% baseline; CodeScene Code Health passed.
  CodeScene Code Coverage remains pending because the pull-request coverage
  action explicitly neither publishes coverage nor contacts CodeScene; `main`
  owns that check. Preserve this as an out-of-scope status, not a failed
  migration gate.

## Surprises & discoveries

- Observation: the repository has no `docs/contents.md` or
  `docs/repository-layout.md`, despite `AGENTS.md` naming them as orientation
  documents. Evidence: neither path exists in this checkout. Impact: use the
  existing design document and guides as the local reference; do not create
  unrelated index documents.
- Observation: the application directly calls
  `load_and_merge_subcommand`; its returned errors are mapped into
  `CliConfigError::Merge`. The v0.9.0 implementation loads candidate files with
  error propagation and layers configuration environment values before CLI
  values. Evidence: the tagged public `subcommand` source calls
  `load_config_file(p)?`, merges the focused `cmds.<name>` table and
  environment provider, then overlays sanitized CLI values. Impact: retain this
  API and test its observed contracts; do not invent an environment injection
  parameter.
- Observation: `LocalizedParse` derives its default identifier root from
  `bin_name` or command name. The tagged `message_id_for` joins command path
  segments with hyphens. `LocalizeCmd::with_base` therefore supports the
  existing `cli-*` keys, while version and merge-help still need their existing
  argument and suffix handling. Evidence: the v0.9.0 rustdoc pages for
  `LocalizedParse`, `LocalizeCmd`, `parse_localized_command`, and
  `message_id_for`. Impact: use the command-based parser with a custom root and
  a narrow adapter; do not directly re-export the upstream trait as the project
  public trait.
- Observation: `figment::Jail` changed the parent test process environment and
  filesystem while the CLI adapter reads process environment directly. The
  shared test probe now runs that same adapter in a child with a private
  working directory and cleared, explicit environment. Evidence: focused
  layering, BDD, localization, malformed-file, and missing-extends tests pass
  against the 0.8.0 baseline. Impact: preserve this isolation for configuration
  value tests; use `MapEnv` only for APIs that accept it as a discovery source.
- Observation: the tagged v0.9.0 runtime and macro packages both resolve at
  0.9.0, and the runtime uses `cap-std` 4 while this project directly uses
  `cap-std` 3. Evidence: `cargo tree -i ortho_config --locked` and the first
  Rust 1.89.0 all-targets/all-features compile. Impact: dependency feature
  unification no longer enables `fs_utf8` for the project's direct v3 use, so
  declare that already-required feature on the direct dependency.
- Review finding: CodeRabbit questioned whether the user-guide example passes
  OrthoConfig's `WithBase<Command>` wrapper to `parse_localized_command`.
  Evidence: the tagged v0.9.0 implementation's inherent
  `WithBase<Command>::localize` returns `Command`, and the crate's own rustdoc
  uses the same `.with_base(...).localize(...)` sequence; the guide imports
  `LocalizeCmd` so the extension method is in scope. The project adapter uses
  `localize_self` because it localizes subcommands separately. Decision: leave
  the valid recursively localized example unchanged. Date/Author: 2026-10-10,
  Codex.
- Observation: the full Clippy run surfaced the existing
  `should_forward_header` `missing_const_for_fn` lint, also present on
  `origin/main`. Evidence: the unchanged source is attributed to commit
  `765ab38d`, the failed lint log ends in `-12.out`, and the successful retry
  ends in `-13.out`. Impact: add `const` without changing the predicate's
  runtime behaviour so the required lint gate can pass.
- Observation: the child probe's stdout marker was not guaranteed to begin a
  line when the libtest runner wrote progress before the `--nocapture` output.
  Evidence: CodeRabbit review log `-15.out` and the parent parser's line-based
  `strip_prefix(RESPONSE_PREFIX)` call. Impact: start the serialized response
  on a fresh line; the configuration/localization results remain unchanged.
- Observation: OrthoConfig 0.9 derives `long_about` and `after_long_help`
  identifiers with underscores, while the existing application catalogue uses
  `long-about` and `merge-help`. Its command localizer also omits `version`
  from metadata lookup arguments and does not provide args for `about`. Impact:
  the compatibility localizer remaps those two identifiers and supplies the
  existing per-command `binary` and optional `version` values while delegating
  metadata traversal and parse-error handling to OrthoConfig.
- Observation: Fluent renders interpolated `binary` and `version` values with
  bidirectional isolation marks. Evidence: the compatibility test's actual
  rendered value. Impact: preserve these marks in regression assertions; the
  adapter keeps the values and Fluent formatting behaviour intact.
- Observation: the binary parses all Clap input before loading and starting the
  harness. Evidence: an end-to-end parse failure with a held listener port, a
  configured local upstream, and an isolated cassette directory leaves all
  three untouched. Impact: parser failures cannot start record-mode effects.
- Observation: the first complete deterministic gate attempt for parser
  consolidation found only `similar_names` and `doc_markdown` Clippy errors in
  the new adapter. Its test log records all 340 tests and 27 doctests passing,
  but lacks a final shell exit status. Impact: fix the lint findings and use a
  fresh sequential full gate run for conclusive test evidence.
- Observation: the second parser-milestone gate run found a missing
  `OrthoConfig` backtick and `expect_used` in the new compatibility test.
  Impact: backtick the crate name and recover a poisoned test mutex without a
  panic; both findings are covered by the passing follow-up gate run.
- Observation: the first full gate pass exposed Clippy findings in the shared
  probe/BDD helper and two Oxford-spelling failures in this plan. `make test`
  passed 336 Nextest cases and 27 doctests; formatting, typecheck, and diagram
  checks also passed. Impact: correct the findings and rerun all gates before
  committing or requesting CodeRabbit. The spelling gate generated unrelated
  entries in `typos.toml`; those entries were restored.
- Observation: after those fixes, the second full gate pass cleared Clippy,
  spelling, formatting, typecheck, workspace tests, and diagram validation.
  Whitaker then rejected the fixture's direct `std::fs::write` call. Evidence:
  the second `make lint` log points to `tests/support/isolated_cli_process.rs`;
  the other second-run gate logs pass. Impact: write the private probe config
  through `cap_std::fs::Dir` and repeat the full sequence.
- Observation: after the review fixes, the next full gate run passed five of
  six gates, including 337 Nextest cases and 27 doctests. Clippy found checked
  slicing, a shadowed binding, and implicit string clones. Impact: apply the
  reported Clippy forms; CodeRabbit was withheld until deterministic gates are
  green again.
- Observation: the complete CodeRabbit pass found stderr could be truncated
  before a secret value was redacted and a module-wide dead-code expectation
  hid target-specific unused probe wrappers. Impact: sanitize the full lossy
  stderr before selecting a UTF-8-safe tail, and split wrappers into modules
  only included by the test targets that use them. The review also surfaced
  unrelated generated `typos.toml` entries; restore those after the spelling
  gate rather than editing the generated file.
- Observation: the next full deterministic run passed five of six gates, with
  Clippy identifying `needless_pass_by_value` in the target-local localization
  shim. Impact: borrow the probe request and rerun every gate; do not request
  CodeRabbit while lint remains red.
- Observation: after the helper split, all six deterministic gates passed and
  CodeRabbit found one observability gap in `run_child` and `handle_probe`.
  Impact: instrument both with `skip_all`, preserving spans without recording
  request, config, or environment values.
- Observation: with instrumentation present, the child subscriber still
  captured no span events, so the secret-tracing assertion did not prove the
  redacted instrumentation path ran. Impact: emit synthesized `NEW` span events
  and assert the `handle_probe` span is captured before checking that the
  sentinel is absent.
- Observation: after the span assertion, all six deterministic gates passed
  and CodeRabbit found no issue in the staged milestone files. Its only comment
  concerned unrelated `typos.toml` output regenerated by the spelling gate.
  Impact: restore that generated churn; retain no spelling-policy changes in
  this migration.
- Observation: the first hosted retry after the coverage fix encountered
  `ETXTBSY` while launching the main-owned `build_standard` test fixture.
  Evidence: `tests/build_standard/reader_tests.rs` has no diff from
  `origin/main`; 20 sequential local runs of the exact test passed; hosted CI
  attempt 2 passed the full coverage job. Impact: record the runner transient
  without broadening this migration into an unrelated fixture rewrite.
- Observation: CodeScene Code Coverage remains pending on the pull request.
  Evidence: `.github/workflows/ci.yml` says the PR coverage action neither
  publishes coverage nor contacts CodeScene; `main` owns that check. Impact:
  report the hosted ratchet result independently and do not describe the
  pending CodeScene status as a pass.

## Decision log

- Decision: use `parse_localized_command` for the project's public iterator
  helper because the project needs a fixed catalogue root and already-built
  localized command metadata. Rationale: v0.9.0 documents this path for an
  explicit identifier base and localizes both Clap and argument-conversion
  errors. It avoids the default `LocalizedParse` root changing from `cli` to
  the binary name. Date/Author: 2026-09-30, Codex.
- Decision: preserve the project-owned `LocalizeCmd` trait and parser helper,
  delegating command metadata and parsing to OrthoConfig while adapting the
  existing version placeholder and `-merge-help` suffix. Rationale: the task
  names these as public helpers and explicitly requires the existing Fluent
  contract. Removing or directly re-exporting them would not preserve those
  semantics. Date/Author: 2026-09-30, Codex.
- Decision: defer `OrthoConfigSubcommandDocs` derivation.
  Rationale: there is no existing complete-command metadata consumer, and the
  payload DTOs do not currently provide the required metadata. Record a
  follow-up rather than adding a consumer or CLI surface to this migration.
  Date/Author: 2026-09-30, Codex.
- Decision: retain the target-main build and CI improvements during rebase,
  including shared-action pins, the comprehensive Make and workflow-contract
  checks, and the `rstest-bdd` 0.6 dependency graph. Add the migration's Rust
  1.89.0 check to that CI workflow. Resolve the lockfile conflict to the exact
  target-main blob, then regenerate it after replay against the combined
  manifests. Rationale: this preserves main's newer shared checks while
  applying the user-requested lockfile merge policy and migration floor.
  Date/Author: 2026-10-04, Codex.
- Decision: rebase onto the newer `d5fc21f` main and keep its Rust build
  standard, CI action updates, serial `make all`, workflow-contract gates, and
  `typos-config-builder v0.1.3`. Retain main's lockfile during replay and
  regenerate it only after the merged manifest history is complete. Resolve the
  generated spelling conflict by running `make spelling`; because its output
  exactly matched main and the repeated run was current, omit the branch's
  generated-only regex replacement. Rationale: the new generator is the
  canonical source for `typos.toml`, and the branch commit introduced no
  independent spelling policy. Date/Author: 2026-10-10, Codex.
- Decision: leave the unchanged `build_standard` test fixture untouched after
  the single hosted `ETXTBSY` failure. Rationale: the failure did not reproduce
  in 20 sequential local runs and the hosted retry passed the full coverage
  job; a fixture rewrite would be unrelated without reproducible evidence.
  Date/Author: 2026-10-10, Codex.

## Outcomes & retrospective

The migration implementation, validation, and draft PR publication are
complete. The branch is rebased onto `origin/main` at `d5fc21f`, with seven
issue commits retained and the generated-only spelling commit superseded by
main's generator. The merged lockfile, deterministic gates, review disposition,
force-push, and PR description are recorded below.

The OrthoConfig 0.9.0 dependency and Rust 1.89.0 floor are in place, and the
localized parser delegates to the tagged `LocalizeCmd` and
`parse_localized_command` APIs behind the stable project adapter. Compatibility
tests preserve Fluent IDs, arguments, bidi formatting, and one-time conversion
error localization. Layering and binary tests cover the unchanged precedence
contract and prove a parse failure causes no record startup effects. On the
rebased head, all four focused tests and the full deterministic gates passed,
including 432 tests, 27 doctests, and the Rust 1.89.0 locked check. CodeRabbit
reviewed that head and found a minor formatting issue in this plan, which was
corrected and verified. Its remaining guide-example finding is a false positive
against the tagged API and is recorded above. Commit `94cb2fc` was force-pushed
with an explicit lease, and local and remote heads matched afterwards. The
follow-up commit `0d6da52` was also pushed with an explicit lease and verified
against the remote head. Draft pull request 147 remains open against `main`,
with `(#123)` in its title, `Closes #123` in its description, and the Lody
session in the terminal References section. Hosted CI attempt 2 passed on
`0d6da52`, including the coverage ratchet at 89.04% against the 89.0% baseline;
CodeScene Code Health passed. CodeScene Code Coverage remains pending by design
because the pull-request workflow does not publish the report or contact
CodeScene; the main-branch workflow owns that check. The first hosted attempt's
`ETXTBSY` in an unchanged main-owned test was not reproduced in 20 local runs,
and the hosted retry passed.

The test-isolation slice is implemented. Initial full gates exposed Clippy,
plan-spelling, and ambient filesystem-write issues; each was corrected. A
subsequent full run passed all six deterministic gates, with 336 Nextest cases
and 27 doctests passing. CodeRabbit reviewed all ten staged files and raised
five trivial comments in three topics; the shared loading adapter, bounded
redacted stderr context, and stronger upstream snapshot assertion now address
them. The requested focused CLI targets passed again after those fixes. The
next full run passed five gates, including 337 Nextest cases and 27 doctests,
but Clippy found six issues in the new edits. These are corrected; a full
deterministic rerun and final CodeRabbit pass remain before this milestone can
be committed. The next CodeRabbit pass found four concerns in stderr redaction,
test helper dead-code suppression, and generated spelling-config churn. All
were addressed; six focused suites pass on the revised helper layout. The
following full run passed five gates and 337 Nextest tests, but Clippy found
one final needless by-value request parameter. That parameter now uses a
reference; the next full run passed all six gates with 337 Nextest tests and 27
doctests. CodeRabbit requested `skip_all` tracing spans for child launch and
probe dispatch. They are added; repeat full gates and CodeRabbit before
committing. CodeRabbit then duplicated a trivial request to prove the captured
span is present. The child subscriber now emits span-creation events, and the
secret regression checks for the `handle_probe` span as well as sentinel
absence. The subsequent full gate run passed formatting, lint, typecheck, all
337 workspace tests and 27 doctests, Markdown/spelling, and diagram validation.
CodeRabbit's only finding concerned generated `typos.toml` churn, which was
restored; the intended test milestone is ready to commit.

## Context and orientation

`src/cli.rs` defines Clap argument DTOs and maps merged values into
`HarnessConfig`; it does not put Clap types into the harness domain. The public
localization adapter lives in `src/cli/localize_cmd.rs` and is re-exported by
`src/cli/localization.rs`. `src/cli/localizer.rs` chooses a best-effort early
CLI locale, while the binary constructs the authoritative library localization
state after configuration loading. Layering examples and tests live in
`tests/cli_layering_unit.rs`, `tests/harness_cli_layering_bdd.rs`, and
`tests/features/harness_cli_layering.feature`.

The v0.9.0 release retains `load_and_merge_subcommand`, adds `LocalizedParse`,
`LocalizeCmd::with_base`, and `parse_localized_command`, and re-exports its
derive macros from the runtime crate. Its runtime manifest declares Rust 1.89.0
and defaults to TOML and JSON support. The current project has only a direct
`ortho_config` dependency; it does not directly depend on the macro crate.

## Conformance basis

The governing request is
[Spycatcher Harness issue #123](https://github.com/leynos/spycatcher-harness/issues/123)
and its acceptance criteria in the task brief. The upstream API and compiler
contract are the immutable `ortho_config` v0.9.0 tag, especially its migration
guide, runtime manifest, public rustdoc, and source.

No separate Terms of Reference or technical design exists for this migration.
The relevant existing architecture contract is
`docs/spycatcher-harness-design.md`; the existing localization disable policy is
`docs/adr/2026-06-04-cli-localization-disable-switch.md`.

- `EP-REQ-DEP`: runtime and macro graph resolves to v0.9.0; Rust 1.89.0 is the
  declared and CI-tested floor; feature selection remains unchanged. Trace:
  issue #123 step 1 -> EP-M1 -> `cargo tree`, manifest assertions, and Rust
  1.89.0 compilation.
- `EP-REQ-LOC`: localized help, version, parse and conversion errors, public
  helpers, existing IDs, and fallback remain compatible. Trace: issue #123 step
  2 -> EP-M2 -> localization unit tests and binary end-to-end tests.
- `EP-REQ-LAYER`: all command namespaces preserve defaults < file < env < CLI,
  explicit locale precedence, and record-only upstream values. Trace: issue
  #123 step 3 -> EP-M3 -> layering unit and BDD tests.
- `EP-REQ-ERR`: missing optional files use defaults; malformed, unreadable,
  unsupported, inheritance, and locale failures remain actionable and prevent
  startup effects. Trace: issue #123 step 3 -> EP-M3 -> focused loader and
  startup tests.
- `EP-REQ-DOC`: user, developer, and implementation documentation describes
  the release floor and preserved architecture; historical v0.8.0 plans remain
  untouched. Trace: issue #123 step 4 -> EP-M4 -> documentation review and
  Markdown gates.

## Verification plan

- Obligation: `EP-INV-DEP` — the direct runtime and resolved macro package are
  both v0.9.0, default feature selection is unchanged, and the locked graph
  compiles on Rust 1.89.0. Method: inspect Cargo metadata/tree and compile the
  locked workspace with the minimum compiler; run normal typecheck and lint on
  the repository-pinned toolchain. Rationale: the dependency graph and compiler
  promise are finite, observable metadata and build properties. Domain: all
  workspace targets and features, with the declared minimum compiler. Artefact:
  `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml`, and
  `docs/developers-guide.md`. Evidence:
  `cargo tree -i ortho_config_macros --locked` shows `0.9.0`; the Rust 1.89.0
  check and named gates pass. A wrong version or feature choice must fail the
  metadata check or compilation. Non-vacuity: inspect the package version and
  enabled features from Cargo metadata; compile actual code rather than a
  manifest-only fixture.
- Obligation: `EP-INV-LOC` — every public parsing helper localizes the command
  once, preserves the legacy ID and argument contract, then localizes parse and
  conversion errors once; missing translations preserve stock Clap text.
  Method: parameterized unit tests and binary end-to-end tests. Rationale: this
  is a finite surface of command fields and Clap outcomes; exact output
  snapshots detect ID, stream, and display-exit regressions. Domain: root and
  record/replay/verify help, version, unknown argument or subcommand, missing
  value, invalid typed value, and conversion failure. Artefact:
  `tests/cli_localization_unit.rs`, `tests/cli_localize_cmd_compat.rs`,
  `tests/binary_localization_e2e.rs`, and the existing snapshots. Evidence: run
  the compatibility, localization, and binary end-to-end targets; help/version
  exit successfully, errors fail on stderr, and stock output works with
  `NoOpLocalizer` and the disable switch. Non-vacuity: use deliberately
  distinct localized and stock strings and test the actual `cli-version` and
  `cli-merge-help` placeholders; a wrong ID or missing argument changes the
  asserted output.
- Obligation: `EP-INV-LAYER` — for each command and each field, absent CLI
  values do not replace file/environment values with `None` or Clap defaults,
  and the effective order is defaults < file < environment < CLI. Method:
  finite BDD scenarios plus unit/property tests for locale syntax. Rationale:
  three precedence sources and three command namespaces are small explicit
  partitions; BDD scenarios make the source witness visible. Domain: record,
  replay, and verify; common scalar fields and nested localization; both locale
  flags; record-only upstream values. Artefact:
  `tests/features/harness_cli_layering.feature`,
  `tests/harness_cli_layering_bdd.rs`, and `tests/cli_layering_unit.rs`.
  Evidence: run `cargo test --test cli_layering_unit` and
  `cargo test --test harness_cli_layering_bdd`; use unique values at every
  layer so a precedence reversal fails the expected-value assertion.
  Non-vacuity: include CLI-over-env-over-file witnesses and no-CLI witnesses;
  retain valid and invalid BCP 47 cases with an exercised accepted witness.
- Obligation: `EP-INV-ERR` — absent configuration is distinct from a candidate
  that exists but is malformed or unreadable; applicable unsupported formats
  and missing `extends` targets remain actionable; no failed parse or locale
  starts a listener, contacts an upstream provider, or writes a cassette.
  Method: loader regression tests and controlled-process behavioural tests.
  Rationale: the outcomes and startup boundary are explicit observable
  contracts and do not require formal proof. Domain: no candidate, valid TOML,
  malformed TOML, unsupported format where discovered, missing inheritance
  target, invalid locale, and record-only upstream setup. Artefact: focused
  tests in `tests/cli_layering_unit.rs`, `tests/harness_cli_layering_bdd.rs`,
  and `tests/binary_localization_e2e.rs`. Evidence: named tests return defaults
  only for absence, retain loader errors in `CliConfigError`, and show no
  cassette or listener-side effect on failure. Non-vacuity: create a real
  config candidate for each failure and assert its distinct outcome; a
  nonexistent path is the control case.
- Obligation: `EP-INV-BOUNDARY` — CLI parsing and localization remain in the
  inbound adapter; only validated values cross into `HarnessConfig`. Method:
  codegraph context and direct architecture review, supported by existing
  public type and startup tests. Rationale: this is a dependency-direction
  constraint, not a new business invariant; the change should not require a new
  port or domain dependency. Domain: imports and calls between `src/cli*`,
  `HarnessConfig`, and startup. Artefact: `src/cli.rs`,
  `src/cli/localize_cmd.rs`, and `docs/spycatcher-harness-design.md`. Evidence:
  codegraph dependency context and `cargo check --workspace` confirm the
  existing adapter direction. Non-vacuity: inspect the actual new call graph
  and confirm domain modules do not import Clap or OrthoConfig localization
  types.

No new mathematical invariant, lemma, concurrency protocol, or persisted format
is introduced, so property model checking and formal proof are not warranted.
The principal external axiom is that the published v0.9.0 public APIs implement
their documented contracts; verify repository integration against the actual
tagged API, not upstream internals.

## Plan of work

Stage A is complete: inspect the current adapter and tests, verify the v0.9.0
manifest and public localization API, and record the migration boundary.

Stage B adds tests before production edits. Extend the localization tests for
legacy IDs, version/merge-help placeholders, conversion errors, and stock
fallback. Extend layering scenarios with controlled precedence cases for all
three commands and malformed/missing configuration outcomes. Environment value
tests must execute in a child process with only the required variables set; do
not use parent-process environment mutation or claim `MapEnv` injects the value
layer of `load_and_merge_subcommand`. The first layering and
environment-isolation tests are preservation checks and pass on the v0.8.0
baseline; record a failing baseline only when a test demonstrates a real v0.9.0
incompatibility.

Stage C updates `Cargo.toml` to `ortho_config = "0.9.0"`, updates `Cargo.lock`,
changes `rust-version` to `1.89.0`, and adds a CI check using the actual locked
graph on Rust 1.89.0. Keep the default feature set and the repository's pinned
developer toolchain. Confirm that the runtime crate alone supplies the matching
derive macros.

Stage D changes `src/cli/localize_cmd.rs` to delegate command metadata to
OrthoConfig `LocalizeCmd::with_base` and parsing/error conversion to
`parse_localized_command`. Keep the existing project public trait and helper
signatures. Retain only the adapter needed for the root/command ID mapping,
`version` argument, and `merge-help` suffix. Do not call the parser's
localization path twice.

Stage E audits `load_and_merge_subcommand` and the CLI-to-domain conversion.
Keep the existing `cmds` namespaces and selected-command merge; ensure errors
map to `CliConfigError` before startup. Use child processes for configuration
environment values and use `MapEnv` only with a discovery API that accepts an
injected source. Do not add `OrthoError::MissingRequiredValues` or parse
human-readable error text as a protocol.

Stage F updates `docs/users-guide.md`, `docs/developers-guide.md`,
`docs/ortho-config-users-guide.md`, and the relevant localization section of
`docs/spycatcher-harness-design.md`. Record the lack of a current metadata
consumer as a separate follow-up in this plan; do not add a new command or
catalogue. Leave existing v0.8.0 ExecPlans unchanged.

## Milestones and plateaus

- Identifier and outcome: `EP-M1`, a coherent dependency and compiler-floor
  upgrade. Requirements and gaps: `EP-REQ-DEP`. Acceptance evidence: Cargo
  resolves runtime and macro packages to 0.9.0; the locked graph checks on Rust
  1.89.0; current feature selection remains intact. Conformance check: no new
  dependency, feature, or CLI/domain coupling. Recovery: revert the milestone
  commit if the release graph cannot satisfy the declared floor. Remaining
  gaps: parser and configuration regression tests. Compatibility decision: no
  source API change.
- Identifier and outcome: `EP-M2`, a coherent localized parser migration.
  Requirements and gaps: `EP-REQ-LOC`. Acceptance evidence: old helper
  signatures compile and exact localized, fallback, help, version, and error
  tests pass. Conformance check: metadata and errors are each localized once;
  project-owned IDs and adapter boundaries remain. Recovery: revert the
  milestone commit without disturbing the dependency milestone. Remaining gaps:
  complete layering and discovery error matrix. Compatibility decision: keep
  the named public localization helpers and Fluent catalogue contract required
  by issue #123.
- Identifier and outcome: `EP-M3`, a coherent configuration/error regression
  suite for all selected commands. Requirements and gaps: `EP-REQ-LAYER` and
  `EP-REQ-ERR`. Acceptance evidence: named unit and BDD tests pass with
  child-controlled environment values and distinct error fixtures. Conformance
  check: defaults < file < env < CLI, namespace isolation, and record-only
  upstream values remain intact. Recovery: revert test and required production
  fixes as one milestone. Remaining gaps: final guide and plan review.
  Compatibility decision: no configuration API change.
- Identifier and outcome: `EP-M4`, a documented, fully gated migration ready
  for a draft PR. Requirements and gaps: `EP-REQ-DOC` and all earlier evidence.
  Acceptance evidence: four requested tests, all full repository gates, and
  CodeRabbit concerns are clear; changes are committed and pushed. Conformance
  check: guides match behaviour and historical plans are untouched. Recovery:
  make follow-up fixes as new gated commits on this branch. Remaining gaps:
  hosted checks may still be running after PR creation. Compatibility decision:
  document metadata derivation as a separate future item that requires an
  actual consumer.

## Concrete steps

Run commands from the repository root. First add tests, then run the four
focused targets and capture the intended pre-fix failures. Upgrade and lock the
dependency with Cargo using the shared default cache. Install Rust 1.89.0 if
needed, verify the runtime/macro graph, compile with
`cargo +1.89.0 check --locked --workspace --all-targets --all-features`, and
check project code with `make typecheck`.

After code and documentation changes, run `make fmt`, inspect and remove any
out-of-scope formatter churn, then run these gates sequentially, saving output
under `/tmp`:

```sh
make check-fmt
make typecheck
make lint
make test
make spelling
make markdownlint
make nixie
```

Run the four requested focused suites before full gates:

```sh
cargo test --test cli_layering_unit
cargo test --test harness_cli_layering_bdd
cargo test --test cli_localization_unit
cargo test --test binary_localization_e2e
```

At each milestone, commit only after its applicable deterministic gates pass,
push the commit, and run `coderabbit review --agent`. Clear each valid concern
before moving to the next milestone. If the review is rate-limited, use the
requested `vsleep` interval in the foreground before retrying.

## Validation and acceptance

The four focused test targets above must pass. The full sequence of formatting,
typecheck, lint, test, spelling, Markdown, and Mermaid gates must pass on the
final commit. The CI floor check must pass with Rust 1.89.0 and the committed
lockfile. The resolved runtime and macro packages must both be 0.9.0; YAML and
the optional metrics feature remain disabled.

The accepted CLI outcomes are concrete: `--help` and `--version` return
successful display requests; ordinary invalid arguments fail with localized
stderr and a non-zero exit; disabling localization or failing to build its
resources returns stock Clap output. Every missing optional configuration uses
defaults, while malformed or unreadable discovered configuration fails before
harness startup. Record, replay, and verify each retain their namespace and
precedence, and replay/verify never inherit record upstream settings.

For Red-Green-Refactor, save focused failing evidence before changing
production code when a real behaviour regression is identified. Record green
baseline evidence for contracts that already hold on v0.8.0, then keep them
green across the migration. Save the passing focused result after each fix and
the final full gate logs. A test fixture must use visibly distinct file,
environment, and CLI values; the expected value must fail under a reversed
merge order. Capture CodeRabbit results separately from deterministic gates and
hosted CI status.

Quality criteria:

- Tests: all four named targets and the full workspace suite pass.
- Verification: `EP-INV-DEP`, `EP-INV-LOC`, `EP-INV-LAYER`, `EP-INV-ERR`, and
  `EP-INV-BOUNDARY` have evidence above.
- Lint/typecheck: `make typecheck` and `make lint` pass without warnings.
- Formatting/docs: `make check-fmt`, `make spelling`, and
  `make markdownlint` pass.
- Diagrams: `make nixie` passes.
- Review: each CodeRabbit milestone review has no unresolved valid concern.
- Publication: a pushed draft PR title includes `(#123)`, its summary states
  `Closes #123`, and its terminal `## References` section contains the Lody
  session link.

Quality method: deterministic repository gates precede each CodeRabbit review;
record exact results and failures in this plan. A queued or skipped hosted
check is not a completed check.

## Revision note

- (2026-09-30) Create the migration plan after inspecting the current adapter,
  the task requirements, and the tagged v0.9.0 APIs. Use the command-based
  localized parser path and plan isolated child environments for configuration
  values. Pass documentation gates before starting the code changes; implement
  and verify the migration in the next stages.
- (2026-09-30) Add controlled child-process test support and broader precedence,
  discovery-error, record-only upstream, and localization-environment coverage.
  The baseline behaviour tests passed on v0.8.0. Full deterministic gates
  passed, and the complete-file CodeRabbit review found only trivial test
  diagnostics and duplication concerns. Those concerns were addressed and all
  requested focused targets passed again. Re-run full gates and CodeRabbit
  before committing this milestone.
- (2026-10-04) Rebase the issue series onto `origin/main`, preserve the newer
  CI and Make checks, and resolve the lockfile to main's exact version during
  replay. Regenerate the merged lockfile and rerun local gates plus CodeRabbit
  before force-pushing the rewritten branch.
- (2026-10-10) Rebase the seven retained issue commits from `58e54ff` onto
  `d5fc21f`. Preserve the newer Rust build standard and workflow tooling; use
  main's lockfile during replay; regenerate `typos.toml` with the new gate and
  omit the superseded generated-only commit. Complete the merged lockfile,
  deterministic gates, and CodeRabbit review before committing or publishing.
