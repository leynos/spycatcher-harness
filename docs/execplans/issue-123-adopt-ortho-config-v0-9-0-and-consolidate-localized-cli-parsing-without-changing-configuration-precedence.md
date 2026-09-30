# Adopt OrthoConfig v0.9.0 and consolidate CLI localization

This ExecPlan is a living document. Keep `Constraints`, `Tolerances`, `Risks`,
`Progress`, `Surprises & discoveries`, `Decision log`,
`Outcomes & retrospective`, `Conformance basis`, and `Verification plan`
current.

Status: IN PROGRESS

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
- Dependencies: stop if a new direct dependency or an optional OrthoConfig
  feature is required.
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
- [ ] Add regression tests first and record their expected pre-change failure.
- [ ] Upgrade the runtime and lockfile, align the declared Rust floor, and
  verify the locked graph on Rust 1.89.0.
- [ ] Replace duplicate localized parsing with v0.9.0 APIs while retaining the
  public adapter and exact catalogue behaviour.
- [ ] Prove all-command configuration layering, error propagation, and startup
  isolation with controlled child environments.
- [ ] Update user and developer documentation, implementation notes, and this
  plan; keep historical v0.8.0 plans unchanged.
- [ ] Run the four named test targets and all full repository gates in
  sequence; run CodeRabbit after each validated implementation milestone.
- [ ] Commit and push each milestone; create a draft PR that closes #123 and
  ends with the required Lody session reference.

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

## Outcomes & retrospective

Implementation is in progress. Add validated outcomes, remaining gaps, and
lessons at each milestone boundary. Do not mark this plan complete until each
trace link in `Conformance basis` has evidence and every discovery has been
reconciled with the design and ADRs.

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
  `tests/cli_localization_unit.rs`, `tests/binary_localization_e2e.rs`, and the
  existing snapshots. Evidence: run `cargo test --test cli_localization_unit`
  and `cargo test --test binary_localization_e2e`; help/version exit
  successfully, errors fail on stderr, and stock output works with
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
  Method: loader regression tests and controlled-process behavioral tests.
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

Stage B adds failing tests before production edits. Extend the localization
tests for legacy IDs, version/merge-help placeholders, conversion errors, and
stock fallback. Extend layering scenarios with controlled precedence cases for
all three commands and malformed/missing configuration outcomes. Environment
value tests must execute in a child process with only the required variables
set; do not use parent-process environment mutation or claim `MapEnv` injects
the value layer of `load_and_merge_subcommand`.

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
  check: guides match behavior and historical plans are untouched. Recovery:
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

For Red-Green-Refactor, save the focused failing test evidence before changing
production code, the passing focused result after each fix, and the final full
gate logs. A test fixture must use visibly distinct file, environment, and CLI
values; the expected value must fail under a reversed merge order. Capture
CodeRabbit results separately from deterministic gates and hosted CI status.

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
