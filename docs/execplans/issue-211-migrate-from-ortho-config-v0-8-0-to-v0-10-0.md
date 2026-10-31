# Migrate from `ortho_config` v0.8.0 to v0.10.0 (#211)

## Why this work matters

Spycatcher Harness currently uses the published `ortho_config` v0.8.0 API.
Moving the existing consumer to v0.10.0 must retain its layered configuration,
discovery failures, localized command-line output, generated documentation, and
process exit and stream contracts. The observable result is a working binary
and library on the v0.10.0 dependency graph, with focused regression coverage
and current migration documentation.

## Starting point and boundaries

The task survey names commit `9b264721287d454762ac4d43874a8bc1d8bd29db`. This
worktree started at `b2a36acdca16c6eae1f0a338508b7218a4e47167`, the current
`origin/main`, which contains that survey commit plus two later
dependency-maintenance commits (`#212` and `#213`). Preserve those base
changes. The worktree was clean before the documentation imports. No remote
branch named `issue-211-migrate-from-ortho-config-v0-8-0-to-v0-10-0` existed
when checked; configure it as the upstream after publishing the final branch.

The repository owns one root Cargo package and lockfile, one Rust library and
binary, and the checked-in `docs/`, `i18n/`, and `tests/` trees. Reconnaissance
found no nested Cargo manifest, build script, vendored implementation,
submodule, standalone lockfile, package alias, patch, or `.cargo/config`.
`ortho_config` is declared as `^0.8.0` and resolves to 0.8.0 alongside
`ortho_config_macros` 0.8.0. The application enables the facade's defaults and
declares no Cargo feature set. It has no direct macro, `cargo-orthohelp`,
`ortho_l10n`, Figment, `uncased`, `xdg`, YAML, or JSON5 dependency. Keep the
direct `metrics` dependency because application code uses it independently.

Keep the domain crate independent of command-line and OrthoConfig mechanics.
Configuration parsing and localization stay in `src/cli/`, with a narrow
project adapter where upstream does not preserve an existing public or
observable contract. Do not add optional profiles, stacked discovery scopes,
agent-native policy, or a standalone localization dependency unless the
existing code requires one to complete this migration.

## Authoritative upstream material

The upstream repository is `leynos/ortho-config`. Tag `v0.10.0` resolved to
annotated tag object `996e5a1c9f22ace4f55bf5dcc196d2c0524262bf`, which peels to
commit `38812c563e3953a490e3efc202609f37e21a0517` (commit date 2026-10-25). The
imported files below were read from that immutable commit and verified
byte-for-byte with `git show`.

| Upstream path                     | Local path                                     | SHA-256                                                            |
| --------------------------------- | ---------------------------------------------- | ------------------------------------------------------------------ |
| `docs/users-guide.md`             | `docs/ortho-config-users-guide.md`             | `84d1c97b77024224dc9220a12d4b749205d9bb2db54849185da21c969dda5575` |
| `docs/v0-10-0-migration-guide.md` | `docs/ortho-config-v0-10-0-migration-guide.md` | `b5331aa6cfcc07b3dcd188c620db98d0a1a1f13b0c8ef96d88913d701bca25bf` |
| `docs/v0-9-0-migration-guide.md`  | `docs/ortho-config-v0-9-0-migration-guide.md`  | `c174c424f91c055201d6a8ec644b0a3b2c7aa681ffbd052d9d6152d78eb2f49d` |

Relative links in the immutable imports intentionally remain unchanged. To
follow one, match its relative target below, use the corresponding immutable
URL, and preserve any fragment from the original link. This mapping resolves
the upstream references without changing the imported bytes. The upstream
source commit is `38812c563e3953a490e3efc202609f37e21a0517`:

| Upstream-relative target                                 | Immutable upstream target                                                                                                                          |
| -------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| `users-guide.md#...`                                     | `https://github.com/leynos/ortho-config/blob/38812c563e3953a490e3efc202609f37e21a0517/docs/users-guide.md#...`                                     |
| `v0-10-0-migration-guide.md#...`                         | `https://github.com/leynos/ortho-config/blob/38812c563e3953a490e3efc202609f37e21a0517/docs/v0-10-0-migration-guide.md#...`                         |
| `v0-9-0-migration-guide.md`                              | `https://github.com/leynos/ortho-config/blob/38812c563e3953a490e3efc202609f37e21a0517/docs/v0-9-0-migration-guide.md`                              |
| `../examples/hello_world/`                               | `https://github.com/leynos/ortho-config/tree/38812c563e3953a490e3efc202609f37e21a0517/examples/hello_world/`                                       |
| `../ortho_l10n/README.md`                                | `https://github.com/leynos/ortho-config/blob/38812c563e3953a490e3efc202609f37e21a0517/ortho_l10n/README.md`                                        |
| `../README.md#...`                                       | `https://github.com/leynos/ortho-config/blob/38812c563e3953a490e3efc202609f37e21a0517/README.md#...`                                               |
| `../CHANGELOG.md`                                        | `https://github.com/leynos/ortho-config/blob/38812c563e3953a490e3efc202609f37e21a0517/CHANGELOG.md`                                                |
| `adrs/adr-0004-cargo-external-subcommand-entry-point.md` | `https://github.com/leynos/ortho-config/blob/38812c563e3953a490e3efc202609f37e21a0517/docs/adrs/adr-0004-cargo-external-subcommand-entry-point.md` |
| `cargo-orthohelp-design.md`                              | `https://github.com/leynos/ortho-config/blob/38812c563e3953a490e3efc202609f37e21a0517/docs/cargo-orthohelp-design.md`                              |
| `cargo-orthohelp-api-guide.md`                           | `https://github.com/leynos/ortho-config/blob/38812c563e3953a490e3efc202609f37e21a0517/docs/cargo-orthohelp-api-guide.md`                           |

The v0.8.0-to-v0.9.0 and v0.9.0-to-v0.10.0 guides and the v0.8.0, v0.9.0,
v0.9.1, and v0.10.0 changelog sections are the migration authority. The guides
distinguish released features from roadmap items; do not rely on text marked
planned or not yet available.

## Behavioural contract

The migration preserves these observable invariants:

1. Configuration precedence remains defaults, files, environment, then
   explicit command-line values. Explicit values equal to Clap defaults must
   still win. If any field needs its Clap default to remain below file or
   environment values, use `cli_default_as_absent` and test each tier.
2. `record`, `replay`, and `verify` continue to read only their own
   `SPYCATCHER_HARNESS_` command namespace; record-only configuration never
   leaks into replay or verify.
3. Missing optional configuration remains distinguishable from malformed or
   unreadable files; a discovered bad file remains visible as an error.
4. Localized help, version, and parse failures retain their current message
   text, locale selection, exit status, stdout/stderr placement, and command
   context. `LocalizeCmd` remains as a narrow adapter if the upstream API
   cannot express an existing contract.
5. Boolean false values use `--flag=false`; `--flag false` remains invalid.
6. Generated documentation and any IR version checks use the v0.10.0 schema
   and `ortho_config::docs::ORTHO_DOCS_IR_VERSION`, not a hard-coded value.
7. Existing tests remain present unless a test is replaced by equivalent or
   stronger isolated coverage; any inventory difference is explained.

## Migration applicability matrix

The matrix is completed with evidence during implementation. `Pending` means
the repository inventory has not yet confirmed applicability.

| Guide change                                                                | Initial disposition                     | Evidence / action                                                                                                                                                                                                                                                                                                                                                                                                  |
| --------------------------------------------------------------------------- | --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Update runtime, direct macro, and installed `cargo-orthohelp` versions      | Required runtime only                   | Change `ortho_config`; macros are facade-transitive. No installed tool or workflow is in this repo.                                                                                                                                                                                                                                                                                                                |
| Explicit CLI values now win even when restating defaults                    | Required behavioural audit              | Clap structs use optional values and no `default_value`; test an explicit value equal to the application default against lower layers.                                                                                                                                                                                                                                                                             |
| Fluent generated identifiers and IR 2.0                                     | Required catalogue audit                | Preserve the `cli` base with upstream `with_base("cli")`; re-key `cli-long-about` to `cli-long_about` and `cli-merge-help` to `cli-after_long_help` for v0.10.0's underscore-preserving suffixes. Keep a per-command adapter that supplies the legacy `$binary` and `$version` arguments to every command-copy lookup. No `OrthoConfigLocalization` derive, dotted generated keys, or pinned IR checks were found. |
| `AgentContext.profiles` uses `ProfilesDeclaration`                          | Not applicable                          | No `AgentContext` construction exists.                                                                                                                                                                                                                                                                                                                                                                             |
| `DocMetadata` gains `profiles`; `cargo-orthohelp` stdout changes            | Not applicable to generated artefacts   | No `DocMetadata` literal, bridge copy, installed/invoked `cargo-orthohelp`, package metadata policy, or generated IR snapshot exists. CLI help snapshots remain applicable; there is no agent-context or policy stdout contract to migrate.                                                                                                                                                                        |
| Boolean flags accept `--flag=false`                                         | No configuration boolean flags found    | CLI argument structs contain no boolean fields or boolean defaults; audit help, examples, and scripts before finalizing.                                                                                                                                                                                                                                                                                           |
| Named configuration profiles                                                | Optional                                | Do not adopt without an existing consumer requirement.                                                                                                                                                                                                                                                                                                                                                             |
| Stacked discovery scopes                                                    | Optional                                | Do not change first-wins discovery policy.                                                                                                                                                                                                                                                                                                                                                                         |
| Source-injected configuration                                               | Adopt in tests and expose a CLI adapter | Replace all four `figment::Jail` call sites with `MapEnv`, `SubcommandFileContext`, and the shipped `_with_sources_at` loader. The BDD scenarios share the same isolated helper; both XDG roots are fixture-controlled so missing values cannot fall back to `$HOME/.config` or `/etc/xdg`.                                                                                                                        |
| Clap-parser-faithful string defaults                                        | Optional                                | No `default_value` or `default_value_t` occurs on configuration arguments.                                                                                                                                                                                                                                                                                                                                         |
| Agent-native behaviour metadata and policy                                  | Optional                                | No policy adoption absent existing workflow.                                                                                                                                                                                                                                                                                                                                                                       |
| Cargo external-subcommand helper                                            | Optional                                | Adopt only if it replaces current Cargo command glue.                                                                                                                                                                                                                                                                                                                                                              |
| Standalone `ortho_l10n` runtime                                             | Optional                                | Keep facade imports unless code truly uses only localization.                                                                                                                                                                                                                                                                                                                                                      |
| Facade localization re-exports (`FluentValue`, `Localizer`, Fluent runtime) | No extra direct runtime dependency      | The CLI still needs the configuration facade and does not directly depend on `fluent-bundle`; existing facade imports remain valid through re-exports.                                                                                                                                                                                                                                                             |
| v0.9 discovery errors                                                       | Required through the selected loader    | Test absent configuration, malformed TOML, and unreadable candidate as distinct outcomes.                                                                                                                                                                                                                                                                                                                          |
| v0.9 YAML 1.2 semantics                                                     | Not applicable                          | The app uses TOML; the manifest enables no YAML or JSON5 feature.                                                                                                                                                                                                                                                                                                                                                  |
| v0.9 discovery derive metadata / subcommand documentation                   | Not applicable                          | The app uses handwritten Clap argument structs and no `OrthoConfigDocs` derive or generator.                                                                                                                                                                                                                                                                                                                       |
| v0.9 combined localized parse API                                           | Required comparison                     | Use shipped `parse_localized_command`; keep a narrow adapter for the legacy `cli` message root and public path.                                                                                                                                                                                                                                                                                                    |
| v0.9 runtime re-exports, tracing and metrics                                | Audit                                   | No duplicate parser dependencies exist. Keep `metrics`, which the harness uses directly.                                                                                                                                                                                                                                                                                                                           |
| Proposed, unreleased error variants                                         | Not applicable                          | Do not use APIs absent from the published release.                                                                                                                                                                                                                                                                                                                                                                 |

## Verification plan

The precedence relation is checked with independent file, environment, and CLI
inputs so each source wins only over lower tiers. Subcommand tests assert both
each namespace's selected values and the absence of cross-command leaks.
Localized parsing is checked at the CLI boundary for help, version, invalid
values, unknown arguments, missing subcommands, status code, and stdout/stderr.
Discovery tests distinguish no candidate from malformed and unreadable
candidates. Documentation checks compare IR version constants and generated
message keys rather than approving broad snapshot changes.

Before editing dependencies or tests, read the imported guides and record the
baseline test inventory. Run `cli_layering_unit`, `harness_cli_layering_bdd`,
`cli_localization_unit`, and `binary_localization_e2e` explicitly. Then run the
repository's sequential gates: `make fmt`, `make check-fmt`, `make lint`,
`make test`, `make markdownlint`, and `make nixie`. Log each command with `tee`
and `set -o pipefail`; do not run the gates concurrently. Use the pinned
toolchain unless evidence requires a dated nightly change, and record the exact
compiler identity used.

Inspect `cargo metadata`, `cargo tree -i ortho_config`, `cargo tree -d`, and
the final lockfile after migration. Explain necessary transitive changes and
any old package versions still selected through another dependency chain.

## Progress

- [x] Inspect repository instructions, worktree state, base commit, and
  compiler identity. Current base is `b2a36acd`; the issue survey baseline
  `9b264721` is an ancestor. The pinned toolchain is `nightly-2026-02-26`;
  `rustc -Vv` reports 1.95.0-nightly commit
  `1ed488274bec5bf5cfe6bf7a1cc089abcc4ebd68` dated 2026-02-25.
- [x] Fetch and resolve upstream v0.10.0 tag; tag object and peeled commit
  match the supplied identifiers.
- [x] Import and verify the v0.10.0 users' guide and both migration guides.
- [x] Complete manifest, toolchain, CI, source, localization, and test-call
  inventory. Four Jail call sites were found across three tests; no direct
  process-environment mutation exists. The issue's count of twelve differs from
  the source-site count and likely counts scenario uses of those helpers.
- [x] Capture baseline inventory for the four required focused suites:
  `cli_layering_unit` 20/20, `harness_cli_layering_bdd` 8/8,
  `cli_localization_unit` 41/41, and `binary_localization_e2e` 16/16; none
  skipped. Baseline `make test` ran 452 tests across 16 binaries (452 passed, 0
  skipped) and 27 doctests (27 passed, 4 ignored).
- [x] Apply the dependency, source, test, and local documentation changes
  within the behavioural contract. Set the declared MSRV to 1.89.0 and retain
  the working `nightly-2026-02-26` pin. Enable `cap-std` 3's `fs_utf8` feature
  directly because OrthoConfig 0.10 now depends on cap-std 4 and no longer
  feature-unifies the application's cap-std 3 use.
- [x] Correct the active `rstest-bdd` user's guide's stale 1.88 application
  MSRV claim; it now distinguishes the dependency's floor from this consumer's
  Rust 1.89.0 requirement.
- [x] The first post-migration format, lint, typecheck, four focused suites, and
  full `make test` passed. The four focused suites passed 25, 8, 30, and 16
  tests respectively; `make test` passed 458 tests and 28 doctests, with 4
  ignored. `make markdownlint` then found one en-GB spelling error in the
  updated developer guide; the correction is included in the next full gate
  pass. `make nixie` remains pending until that sequence reaches it.
- [x] Repeat the full pinned-toolchain sequence after the spelling correction:
  `make fmt`, `make check-fmt`, `make lint`, `make typecheck`, the four focused
  suites, `make test`, `make markdownlint`, and `make nixie` all passed.
- [x] Validate the declared MSRV with `rustc 1.89.0` (commit
  `29483883eed69d5fb4db01964cdf2af4d86e9cb2`, 2025-08-04). Stable Cargo cannot
  accept the repository's nightly-only `-Zthreads=8`; direct Cargo validation
  therefore omitted only that performance flag, retained the configured linker
  flag and `RUSTC_WRAPPER`, and left the build-standard contract tests' default
  Makefile view intact. All 458 all-target tests and 28 doctests passed (4
  ignored), and `cargo check --workspace --all-targets --all-features` passed.
- [x] Resolve CodeRabbit's first valid findings and rerun the complete
  pinned-toolchain sequence. The focused suites again passed 25, 8, 30, and 16
  tests; `make test` passed 458 tests and 28 doctests (4 ignored), and Markdown
  and Mermaid checks passed. The stable 1.89.0 all-target tests, doctests, and
  check were repeated after these test-source changes and passed with the same
  counts.
- [x] Inspect the final dependency graph. The sole runtime, macro, and
  localization packages are all 0.10.0 from crates.io. The lockfile also has
  cap-std 3.4.5 for the application's existing `fs_utf8` use and cap-std 4.0.3
  through `ortho_config` and `rstest-bdd-macros`; TOML 0.8.23 remains through
  `ortho_config -> figment`, while TOML 0.5.11 remains through
  `i18n-embed-impl -> find-crate`. There are no Git-sourced OrthoConfig
  packages or duplicate OrthoConfig versions.
- [x] Address the second review's valid findings in the application user's
  guide and add a child-process test for process-backed precedence.
- [x] Run the final pinned-toolchain gates sequentially. `make fmt`,
  `make check-fmt`, `make lint` (including Whitaker), `make typecheck`, the
  five focused suites, `make test`, `make markdownlint`, and `make nixie` all
  passed. Focused counts were 25, 8, 30, 16, and 4, with no skips; the full
  suite passed 462 tests and 28 doctests, with 4 ignored.
- [x] Validate the final candidate on the declared Rust 1.89.0 MSRV. With
  `RUSTC_WRAPPER` and `RUSTDOC` preserved and the unsupported nightly-only
  `-Zthreads=8` omitted, all-target/all-feature tests passed (462), doctests
  passed (28, 4 ignored), and the all-target/all-feature check passed. Compiler
  identity: rustc 1.89.0, commit `29483883eed69d5fb4db01964cdf2af4d86e9cb2`,
  LLVM 20.1.7.
- [x] Run a fresh CodeRabbit review after deterministic and MSRV validation.
  It repeated one minor relative-link finding on the imported upstream guides.
  The finding is inapplicable: the migration instructions explicitly require
  byte-identical imported copies and allow an upstream link map instead of
  importing every companion document. This plan maps those links to the
  immutable upstream commit. The imported copies remain byte-identical.
- [x] Commit and push the validated migration over SSH; set the requested
  branch upstream and create the draft PR. Leave issue #123 open for the
  `Closes #123` automation when the new PR merges; do not merge this change.
- [x] Run the full sequential gate suite on `67c4f84` plus this working tree.
  Six of seven gates passed: `make lint` (Rustdoc, Clippy with `-D warnings`,
  and the Whitaker Dylint suite), `make typecheck`, `make test` (474 nextest
  cases with none skipped; 28 doctests pass with 4 ignored),
  `make test-workflow-contracts`, `make markdownlint`, and `make nixie`.
  `make check-fmt` failed only on `mdtablefix` table layout in this plan;
  `make fmt` reformatted four lines and the repeated check passes.
- [x] Repeat all seven gates after the rebase onto `8308c66`, whose toolchain
  bump to `nightly-2026-10-30` invalidates every earlier compiler result. All
  seven pass: `make check-fmt`, `make lint`, `make typecheck`, `make test` (476
  nextest cases, none skipped; 28 doctests pass, 4 ignored),
  `make test-workflow-contracts`, `make markdownlint`, and `make nixie`.
- [x] Diagnose the hosted coverage failure as a measurement artefact of the new
  subprocess tests rather than lost assertions. `cargo llvm-cov` exports
  `LLVM_PROFILE_FILE` so child processes write their own profiles, but every
  binary test clears its child environment with `Command::env_clear`, which
  dropped the variable. The spawned binary then reported no coverage and its
  lines counted as uncovered. The equivalent tests on `main` spawn the binary
  without clearing the environment, which is why the baseline retained those
  lines.
- [x] Repair coverage measurement with one shared helper,
  `tests/common/coverage_profile.rs`. `preserve_coverage_profile` re-adds only
  `LLVM_PROFILE_FILE` after `env_clear`; all seven spawn sites in
  `binary_localization_e2e`, `binary_cli_layering_e2e`, and
  `binary_startup_isolation` call it, replacing the one local copy. The
  variable configures the profiling runtime only, so the isolation under test
  is unchanged.
- [x] Measure the repaired candidate before the rebase with
  `cargo llvm-cov --workspace --all-targets --lcov`: 3636 of 4043 lines,
  89.93%, which was inside the 1.00 percentage point tolerance of the then
  current 90.10% baseline. The binary reported 79/84, 47/49, 19/19, and 50/51
  lines across its four modules, where the cleared environment previously
  recorded none. This figure is superseded: the later toolchain bump changed
  both the baseline and the measurement basis.
- [x] Re-measure after the rebase with the pinned coverage toolchain, using the
  `cargo llvm-cov` 0.9.0 binary that CI installs from
  `.github/tool-manifest.toml`: 3519 of 4035 lines, 87.21%, against the
  post-bump baseline of 87.42% and its 86.42% floor. The binary now reports
  73/78 lines and `i18n`, `http_exchange`, and `error` report 47/49, 105/105,
  and 14/14.
- [x] Diagnose a false 76.15% result from the locally installed
  `cargo-llvm-cov` 0.8.5. Under the new Cargo build-directory layout that
  `nightly-2026-10-30` uses, 0.8.5 merges only part of the instrumented
  objects, dropping whole unit-test mappings; the report contained 41 files with
  `error` absent and `i18n` at 12/49. Version 0.9.0, which CI pins, reports
  the same tree at 87.21% across 51 files. Local measurements must use the
  pinned binary, not whatever `cargo install` last left on `PATH`.
- [x] Deliver the regenerated `typos.toml` prerequisite separately. It was
  committed on its own branch from `724f3db`, passed `make spelling` twice for
  idempotence, `make check-fmt`, and `make markdownlint`, and merged as PR #227
  (`b0f2c20`), so this branch's review diff carries no generated-file noise.
- [x] Publish the rebased branch onto `origin/main` (`8308c66`) with an
  explicit lease, then confirm hosted CI, the coverage ratchet, CodeScene, and
  review convergence on the published head. The force-push used
  `--force-with-lease` against the recorded old tip `5ebe3b2`, the refreshed
  tracking ref and remote head both read `ceb4291`, and the branch upstream now
  points at `origin/issue-211-migrate-from-ortho-config-v0-8-0-to-v0-10-0`.
- [x] Read the hosted checks on `ceb4291`. The CodeScene Code Health Review
  passes with six quality gates and no findings, and its earlier inline threads
  no longer exist on the rewritten head, so there is nothing left to reply to
  or resolve. Gecko Security Review passes. Sourcery skips because the diff
  exceeds its review limit. Codex reports a usage-limit failure for both its
  review and its security review on this head; that is an external quota, not a
  finding, and it produces no actionable review surface. CodeRabbit started a
  fresh review of the `8308c66...ceb4291` range when the draft was marked
  ready, and the `CI` run `41228543776` is still building.
- [x] Confirm the hosted `CI` run `41228543776` on `ceb4291` succeeded, with
  every step green: format, CV-005 contracts, the comprehensive Make gate,
  spelling, Markdown lint, Mermaid validation, the MSRV 1.89.0 check, lint, and
  `Test and Measure Coverage`. The coverage step logged
  `Current coverage: 87.21%` against `Baseline coverage: 87.4%`, which passes
  the ratchet and matches the pinned local measurement exactly, so the earlier
  coverage regression is fully resolved on the published head.
- [x] Review round on the published head `ceb4291`: five CodeRabbit inline
  findings, three warning rows, and one inconclusive row from the pre-merge
  table. Repairs: corrected the `rstest-bdd` toolchain pin; documented the
  `localizer` parameter in the user's guide; set `HOME` for the process-backed
  layering child; removed the tautological listener assertion while splitting
  `binary_startup_isolation` into a module root and support file; pinned the
  listen-address merge error with an inline snapshot; added docstrings across
  touched functions; documented the toolchain, MSRV, and `cap-std` feature in
  the developer's guide; added a compile-time `const` assertion test for
  `should_forward_header`; and recorded `Cargo.lock` evidence for the
  linked-issue check.
- [x] Split `tests/binary_localization_e2e.rs` into a module root and a support
  module so both files stay within the 400-line limit after the added
  docstrings and the child `HOME`/XDG isolation.
- [x] Correct the compile-time test after the first lint pass: Clippy's
  `assertions_on_constants` rejects runtime `assert!` on constant values, so
  the assertions now sit in a `const` block and remain compile-time checks.
- Gate result: all seven gates pass on the repair-round working tree.
  `make check-fmt` and `make markdownlint` report no changes across 37 Markdown
  files, `make lint` runs Rustdoc, Clippy with `-D warnings`, and the Whitaker
  Dylint suite, `make typecheck` passes, `make test` reports 477 nextest cases
  with none skipped and 28 doctests passing with 4 ignored,
  `make test-workflow-contracts` passes, and `make nixie` validates every
  Mermaid diagram. The first `make lint` attempt failed only on
  `clippy::assertions_on_constants`; the `const` block fix cleared it.
- [x] Publish the repair round as `7cd18aa` on top of `ceb4291` with an
  explicit lease. The push fast-forwarded the remote branch, and the refreshed
  tracking ref, `git ls-remote`, and `git rev-parse HEAD` all read
  `7cd18aa2e807290b844c686b82bcb43514d05f1e`; the branch upstream is
  `origin/issue-211-migrate-from-ortho-config-v0-8-0-to-v0-10-0`.
- [x] Dispose of all five inline threads. Replies citing `7cd18aa` were posted
  through the comment-token pool, CodeRabbit answered each one, and the read
  back thread state shows all five as resolved, with two also marked outdated
  because the repair moved their anchor lines.
- [x] Reconcile the pre-merge table with a focused top-level comment rather
  than a fresh full review. CodeRabbit reassessed the held candidate
  `8308c66...7cd18aa` and reported every row resolved: docstring coverage
  independently measured at 37/37 functions with complexity of at least 2
  (90.82% across all 98 touched functions), the toolchain section present at
  `docs/developers-guide.md:34`, the compile-time test present at
  `src/upstream_tests.rs:130`, and the lockfile gap closed by one 0.10.0 entry
  each for `ortho_config`, `ortho_config_macros`, and `ortho_l10n`.
- [x] Confirm the hosted `CI` run `41237614055` on `7cd18aa` succeeded with
  every step green, including the MSRV check, lint, and coverage. The coverage
  step again logged `Current coverage: 87.21%` against the `87.4%` baseline,
  which passes the ratchet.
- Observation: CodeRabbit automatically paused its own reviews on this branch
  after the review round because of the commit volume
  (`reviews.auto_review.auto_pause_after_reviewed_commits`). The pause covers
  automatic reviews only; the four saved pre-merge checks were reassessed on
  `7cd18aa` in explicit conversation, and the final approval command is issued
  on the final head.

## Decisions and discoveries

- Decision: keep the upstream copies byte-for-byte intact and map their
  relative links to the immutable source tree in this plan. Their upstream
  repository has companion documents and examples that do not belong to this
  consumer repository.
- Discovery: CodeGraph MCP tools are not available in this session, despite
  trying the skill's documented tool discovery. Continue with narrow symbol and
  caller searches and mark this as a tooling limitation.
- Discovery: Firecrawl search and docs.rs scrape were used to verify current
  public `cargo-orthohelp` package documentation; the tagged upstream source
  remains authoritative for this migration.
- Baseline: all four requested focused suites and the complete `make test`
  gate passed on `nightly-2026-02-26` / rustc 1.95.0-nightly commit
  `1ed488274bec5bf5cfe6bf7a1cc089abcc4ebd68`. The pinned compiler already
  exceeds the v0.10.0 floor, so no nightly-date change is indicated.
- Validation finding: the first migrated `cli_localization_unit` run showed
  that v0.10 derives `long_about` and `after_long_help` identifiers with
  underscore-preserving suffixes. The catalogue now uses `cli-long_about` and
  `cli-after_long_help`; the only remaining adapter handles the application's
  `$version` interpolation, which the shipped localizer does not supply.
- Validation finding: the first migrated binary suite showed that the shipped
  command localizer attempts the legacy `cli-version` message without its
  required `$version` argument, logging a warning before the compatibility
  adapter runs. A narrow `Localizer` wrapper now defers only `*-version`
  lookups to that adapter. The focused binary suite then passed 16/16 with no
  warning output.
- Validation finding: the first complete post-change suite has six more tests
  than baseline. Five are new layering regressions; eleven old process-global
  localization cases were replaced by twelve source/lazy-selection unit cases,
  for a net increase of one there. No tests were skipped in either baseline or
  migrated nextest runs; four existing doctests remain ignored.
- Validation finding: the first attempted stable `make test` rejected the
  Makefile's nightly-only `-Zthreads=8`. Setting `STANDARD_THREADS_FLAG=` on
  `make` also changed the environment inherited by its Makefile contract tests,
  which correctly reported that the default development targets no longer
  contained the flag. Direct stable Cargo test, doctest, and check commands
  avoided that test-harness distortion while omitting only the unsupported
  performance flag; all passed on Rust 1.89.0.
- CodeRabbit review disposition: the request to rewrite imported relative links
  conflicts with byte-for-byte provenance and is unnecessary because this plan
  explicitly permits and records the immutable upstream link mapping. The
  mapping was checked against every distinct relative target in the three
  imports and now states how to follow each target.
- CodeRabbit review repair: test `MapEnv` sources now set both XDG roots to the
  fixture base before case-specific variables, preventing fallback discovery
  from reading host configuration. The unreadable-file fixture checks whether
  mode `0o000` is actually enforced; it only runs the permission assertion when
  the runner cannot still open the file, such as when DAC override capabilities
  are present. Its result-returning test reports assertion failures through
  `eyre::bail!` to satisfy `clippy::panic_in_result_fn`.
- CodeRabbit review repair: the public user's guide now covers the injected
  loader and the process-backed alternative. A new binary test target invokes
  the normal CLI with an isolated child environment and temporary config file,
  proving precedence through the process-backed loader. Its `record` startup
  errors occur during validation before cassette or listener work.
- Final graph: `cargo tree -i ortho_config` resolves only 0.10.0, and
  `cargo tree -i ortho_config_macros` resolves only the matching 0.10.0 macro.
  `cargo tree -i cap-std@3.4.5`, `cargo tree -i cap-std@4.0.3`, and targeted
  TOML reverse trees confirm the retained dependency chains recorded above.
- Final inventory: the migration suite has 10 more tests than baseline. The
  injected localization selection cases add one net test after replacing
  process-global cases, the config-layering suites add five, and the new
  process-backed precedence target adds four. Baseline and migrated runs have
  no skipped nextest cases; four existing doctests remain ignored.
- Hosted-state result: created draft PR #215 from the requested branch at
  `1642aab0dddbad07c417e6414286ddfbeade4f78`; its body closes #211 and #123
  when merged and links this session. Closed the earlier v0.9 PR #147 as
  superseded by #215. Issue #123 remains open until #215 merges. This entry
  records the original delivery state and is superseded by the publication and
  repair-round entries above, which now read `7cd18aa`.
- Historical push result: at that first publication the local branch,
  `origin/issue-211-migrate-from-ortho-config-v0-8-0-to-v0-10-0`, and GitHub
  head all resolved to `1642aab0dddbad07c417e6414286ddfbeade4f78`. SSH and
  GitHub API identity checks both resolved to `leynos`. This is superseded:
  after the rebase and the repair round, all three refs read
  `7cd18aa2e807290b844c686b82bcb43514d05f1e`.
- CodeRabbit's final review completed with one minor finding requesting local
  navigation for upstream-relative links. The repository instructions allow
  documenting an upstream link map, and the plan records a complete target map
  while preserving imported bytes; no source change is warranted.
- Review follow-up (PR #147 parity): the v0.10.0 adapter now applies upstream
  `localize_self` one command at a time, so each command's copy is looked up
  with its own `$binary` and, when declared, `$version`. This closes the
  compatibility gap with PR #147's `CatalogueCompatibleLocalizer`, which
  supplied both arguments to every command-copy lookup; v0.10.0 by itself passes
  `None` to `about` and only `$binary` to longer metadata. The `cli` catalogue
  root and v0.10.0 underscore suffix spellings are unchanged.
- Review follow-up (test parity): `tests/cli_localize_cmd_compat.rs` ports PR
  #147's adapter compatibility cases (catalogue identifiers, formatting
  arguments, absent-version handling, single conversion-error localization) and
  adds a subcommand binary/version case. `tests/binary_startup_isolation.rs`
  ports its startup side-effect isolation cases: parse failures write only to
  stderr, configuration and locale failures exit one before creating cassettes
  or contacting the listener/upstream, and an unreadable discovered file aborts
  before a valid XDG candidate.
- Review follow-up (CI parity): `.github/workflows/ci.yml` now installs the
  pinned Rust 1.89.0 toolchain and runs
  `cargo +1.89.0 check --locked --workspace --all-targets --all-features`,
  matching PR #147's minimum-supported-version gate.
- Rebase result: the branch now sits on `origin/main` at `724bfde` after five
  intervening upstream commits. The single `.github/workflows/ci.yml` conflict
  was resolved by keeping the newer shared-actions pin together with the MSRV
  steps. The resulting series is `59aea75`, `9be54c0`, `8ee245a`, and
  `67c4f84`; recovery ref `backup/issue-211-pre-rebase-5ebe3b2` retains the
  pre-rewrite tip.
- CodeScene result: the rebased head cleared both advisory findings. The
  duplicated help/version stdout helper became a shared `localized_stdout`
  function, and the layering table cases now pass grouped structs plus a
  `Subcommand` enum instead of long string-heavy argument lists.
  `cs delta origin/main HEAD` reports no issues in the checked changes.
- File-size result: `tests/cli_layering_unit.rs` exceeded the 400-line
  repository limit at 509 lines. It is now a 35-line module root that declares
  `cases`, `cassette_precedence`, `localization`, `error_reporting`, and
  `namespaces` under `tests/cli_layering/`; the largest resulting file is 231
  lines.
- Hosted-state caveat: hosted run `40349444426` on `724f3db` failed the
  coverage ratchet at 86.69% against a 90.10% baseline with a 1.00 percentage
  point tolerance. That run predates the legacy-copy interpolation fix and the
  two added suites (`binary_startup_isolation`, `cli_localize_cmd_compat`), so
  the measurement must be repeated on the final head rather than assumed either
  way.
- Limitation: `unreadable_discovered_file_remains_visible` cannot prove the
  permission failure on every runner. When the process can still open a
  mode-`0o000` file, such as under a DAC-override capability, the helper returns
  `None` and the test returns `Ok(())` without exercising the discovery error.
  The skip is deliberate and visible in the helper's contract; it is not an
  executed assertion, so the permission path remains unverified on such a
  runner.
- Discovery: rust-analyzer reports `E0308` for `&[&str; N]` arguments passed to
  `&[&str]` parameters in this suite, and for `return Ok(())` in the result
  helper, even though Cargo compiles the same calls. The diagnostics are stale
  or spurious; the authoritative result is the scrutineer's Cargo gate run.
