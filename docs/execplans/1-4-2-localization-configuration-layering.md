# Add localization configuration layering

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

## Purpose / big picture

Roadmap item `1.4.2` makes locale selection a deterministic part of binary
startup rather than an ad hoc application concern. After this work is
implemented, a user can set `locale` and `fallback_locale` for
`spycatcher-harness` through the same layered configuration model already used
for subcommands: command-line arguments override environment variables,
environment variables override configuration files, and configuration files
override defaults.

The observable behaviour is simple: loading `spycatcher-harness replay` with
different combinations of CLI flags, `SPYCATCHER_HARNESS_*` environment
variables, and `.spycatcher_harness.toml` values produces one
`HarnessConfig.localization` value with predictable precedence. Starting the
binary builds one authoritative `FluentLanguageLoader` from that configuration
and reuses it for localized rendering at the application boundary. The library
crate remains locale-agnostic and continues to receive a loader by dependency
injection when it needs to render localized text.

This plan must be approved before implementation begins. Do not mark roadmap
item `1.4.2` done until the implementation, tests, documentation, CodeRabbit
review, and commit gates are complete.

## Constraints

- Preserve the public library boundary established by roadmap item `1.4.1`.
  `src/i18n.rs` must continue to expose `HarnessLocalizations` and
  `localize_harness_error(&FluentLanguageLoader, &HarnessError)` without
  constructing or storing a process-global loader.
- Keep localization configuration loading in the CLI/application adapter.
  Domain and cassette logic must not import `clap`, `ortho_config`, Figment, or
  environment access APIs.
- Use the existing OrthoConfig precedence model from roadmap item `1.1.2`.
  The required precedence is `CLI > env > config files > defaults`.
- Keep roadmap item `1.4.3` out of scope. This work may prepare a startup
  loader, but must not require localized `clap` help or parse-error rendering
  through OrthoConfig localizer hooks.
- Use `rstest` for unit tests and `rstest-bdd` for behavioural tests where
  externally observable command-line or startup behaviour is covered.
- Do not mutate process environment directly in tests. Use
  `ortho_config::figment::Jail`, dependency injection, or existing test helpers.
- Use `camino::Utf8PathBuf` for path fields. Do not introduce `std::fs` or
  `std::path` into new filesystem-facing application code unless there is no
  capability-oriented alternative.
- Keep each Rust source file at or below 400 lines. If adding tests would push
  an existing file over that limit, create a focused sibling test module.
- Documentation must use en-GB-oxendict spelling and must remain wrapped to the
  repository's Markdown style.
- Run `coderabbit review --agent` after each major milestone during
  implementation and clear all concerns before moving to the next milestone.
- Gate each commit. For code changes, run the relevant Makefile targets through
  `tee` logs before committing.

## Tolerances (exception triggers)

- Scope: if implementation requires modifying more than 12 tracked files or
  more than 700 net lines, stop and ask for approval before continuing.
- Public API: if a public library API signature must change, stop and ask for
  approval. Adding a small binary/application helper module is allowed.
- Configuration shape: if OrthoConfig cannot support `locale` and
  `fallback_locale` with the required precedence without a new global loading
  pass, stop after documenting the exact conflict and propose the least
  invasive options.
- Dependencies: if a new runtime dependency beyond the existing
  `i18n-embed`/`unic-langid` stack is needed, stop and ask for approval. A new
  dev-dependency for tests also requires an explicit decision log entry.
- Locale assets: if implementation needs non-English Fluent resources to prove
  fallback behaviour, stop and ask whether to add a minimal test-only locale or
  defer multi-locale assets to a later roadmap item.
- Tests: if `make lint` or `make test` still fails after five focused repair
  attempts, stop, record the failure transcript, and ask for direction.
- Review: if `coderabbit review --agent` reports a concern that conflicts with
  this plan, stop and update the `Decision Log` before changing approach.

## Risks

- The existing CLI loader uses `load_and_merge_subcommand` and naturally reads
  per-command values from `cmds.record`, `cmds.replay`, and `cmds.verify`. The
  design text describes localization as global CLI behaviour. The likely
  implementation is to add the localization fields to each subcommand argument
  struct and map them into `HarnessConfig.localization`, but this may leave
  truly global top-level config file keys unsupported. The implementation must
  verify OrthoConfig behaviour before committing to the final shape.
- `fallback_locale` currently defaults to `"en-US"` as a plain `String`.
  Startup loader construction must parse it into
  `i18n_embed::unic_langid::LanguageIdentifier` and return an actionable error
  if it is invalid.
- The repository currently contains only
  `i18n/en-US/spycatcher-harness.ftl`. Locale negotiation can still be
  deterministic, but tests must be clear about whether they are proving
  configuration selection, language identifier parsing, or actual catalogue
  fallback.
- Loader construction belongs in the binary composition root, but
  `src/bin/spycatcher_harness.rs` is intentionally small. Adding too much logic
  there would make startup harder to test. Prefer a small application helper
  with unit tests if construction logic needs more than trivial wiring.
- The next roadmap item, `1.4.3`, will localize CLI help and parse errors. This
  plan must leave a clean seam for that work without implementing it early.

## Progress

- [x] 2026-05-18: Loaded the `execplans`, `leta`, `rust-router`,
  `domain-cli-and-daemons`, `hexagonal-architecture`, `firecrawl-mcp`,
  `commit-message`, and `pr-creation` skills.
- [x] 2026-05-18: Renamed the branch to
  `1-4-2-localization-configuration-layering`.
- [x] 2026-05-18: Reviewed roadmap item `1.4.2`, design references, current
  CLI layering code, localization code, binary startup, unit tests, BDD tests,
  and user/developer documentation.
- [x] 2026-05-18: Used a Wyvern agent team for planning reconnaissance. One
  agent inspected source and composition-root risks; one inspected
  documentation and test expectations.
- [x] 2026-05-18: Used Firecrawl to check current Rust/Fluent prior art for
  BCP 47 language identifiers, Fluent fallback bundles, and locale negotiation.
- [x] 2026-05-18: Drafted this ExecPlan.
- [x] 2026-05-18: Opened draft PR #43 for approval.
- [x] 2026-05-18: Received approval to proceed with implementation.
- [x] 2026-05-18: Added red `rstest` unit coverage for localization
  defaults, config/env/CLI precedence, all subcommands, and invalid locale
  input.
- [x] 2026-05-18: Added red `rstest-bdd` scenarios for replay locale
  precedence, fallback locale behaviour, and invalid locale configuration.
- [x] 2026-05-18: Ran targeted red tests. `cli_layering_unit` failed because
  localization fields are not merged and `--locale` is not recognized;
  `harness_cli_layering_bdd` failed on the missing `--locale` flag path.
- [x] 2026-05-18: Ran CodeRabbit after Milestone 1; it reported zero
  findings.
- [x] 2026-05-18: Implemented CLI adapter support for `--locale`,
  `--fallback-locale`, `[cmds.<subcommand>.localization]`, nested env keys, and
  language identifier validation at the adapter boundary.
- [x] 2026-05-18: Re-ran targeted CLI layering tests. `cli_layering_unit`
  passed 15 tests and `harness_cli_layering_bdd` passed 7 scenarios.
- [x] 2026-05-18: Ran CodeRabbit after Milestone 2; it reported zero
  findings.
- [x] 2026-05-18: Gated the CLI adapter milestone with `make check-fmt`,
  `make lint`, and `make test`; all passed.
- [x] 2026-05-18: Implemented binary startup localization planning and
  one-time `FluentLanguageLoader` construction from `LocalizationConfig`.
- [x] 2026-05-18: Added `rstest` coverage in the binary for fallback-only
  planning, requested-locale ordering, invalid explicit locale failure, invalid
  fallback failure, and fallback to the embedded English catalogue.
- [x] 2026-05-18: Ran targeted startup loader tests. `cargo test --bin
  spycatcher-harness` passed 5 tests and `cargo test --test
  cli_layering_unit` passed 15 tests.
- [x] 2026-05-18: Ran CodeRabbit after Milestone 3; it reported zero
  findings.
- [x] 2026-05-18: Gated the startup loader milestone with `make check-fmt`,
  `make lint`, and `make test`; all passed.
- [x] 2026-05-18: Updated user, developer, and design documentation with the
  implemented subcommand-local localization config shape, double-underscore
  nested env keys, startup loader ownership, and invalid language identifier
  behaviour.
- [x] 2026-05-18: Ran documentation validation. Changed docs passed targeted
  `markdownlint-cli2`; `make markdownlint` and `make nixie` passed.
  `make fmt` still reports pre-existing repository-wide Markdown line-length
  failures from the legacy `markdownlint --fix` tool, so unrelated formatter
  churn was reverted.
- [x] 2026-05-18: Ran CodeRabbit after Milestone 4; it reported zero
  findings.
- [x] 2026-05-18: Ran final gates. `make check-fmt`, `make lint`,
  `make test`, `make markdownlint`, and `make nixie` passed.
- [x] 2026-05-18: Marked roadmap item `1.4.2` and its success criteria done.
- [x] 2026-05-18: Ran final CodeRabbit review; it reported zero findings.
- [x] 2026-05-22: Rechecked review warnings with Wyvern and Scribe agents.
  Added binary-level locale flag coverage, converted startup error rendering
  to an `insta` snapshot, and added debug/warn tracing around CLI localization
  override selection and invalid identifier rejection.
- [x] 2026-05-22: Revalidated with `cargo test --test
  binary_localization_e2e`, `cargo test --bin spycatcher-harness`, `cargo
  test --test cli_layering_unit`, `make check-fmt`, `make lint`, and
  `make test`; all passed. `coderabbit review --agent` could not complete
  because the service returned a recoverable rate-limit error.
- [x] Implement the plan milestone by milestone.
- [x] After implementation, mark roadmap item `1.4.2` done.

## Surprises & Discoveries

- `src/config.rs` already contains `LocalizationConfig { locale:
  Option<String>, fallback_locale: String
  }` with the expected default fallback locale of `"en-US"`.
- `src/cli.rs` currently maps only `listen`, `cassette_dir`, and
  `cassette_name` through `CommonOverrides`; localization fields are present in
  domain config but absent from CLI argument structs and mapping.
- `src/bin/spycatcher_harness.rs` currently loads merged config, builds a
  Tokio runtime, starts the harness, and shuts it down. It does not construct
  any language loader.
- Existing BDD coverage for CLI layering is already concentrated in
  `tests/features/harness_cli_layering.feature` and
  `tests/harness_cli_layering_bdd.rs`, which is the right place to extend
  externally observable configuration precedence scenarios.
- Firecrawl found Rust prior art around `unic-langid`, `fluent-locale`, and
  `fluent-fallback`. The repository already depends on the Fluent stack used by
  `i18n-embed`, so this plan does not assume a new negotiation dependency.
- The BDD feature file is not automatically treated as a Cargo recompilation
  input by the test binary. If only the `.feature` file changes, touch or edit
  `tests/harness_cli_layering_bdd.rs` before rerunning targeted BDD tests so
  generated scenarios reflect the current feature text.
- OrthoConfig's nested environment protocol uses a double underscore between
  path segments. The supported env keys are therefore
  `SPYCATCHER_HARNESS_CMDS_<SUBCOMMAND>_LOCALIZATION__LOCALE` and
  `SPYCATCHER_HARNESS_CMDS_<SUBCOMMAND>_LOCALIZATION__FALLBACK_LOCALE`.
- The binary can construct and hold one authoritative language loader today
  even though most localized boundary rendering belongs to roadmap item
  `1.4.3`. Passing the loader into `run_harness` makes that ownership visible
  without introducing process-global state.
- Exact CLI/env/file provenance for localization values is not exposed by the
  current OrthoConfig adapter boundary. The follow-up tracing therefore records
  explicit CLI overrides versus merged non-CLI values, plus application of the
  selected override and invalid language identifier rejection.

## Decision Log

- Decision: treat this as an application adapter change, not a library-domain
  change. Rationale: the design document explicitly says the library must avoid
  locale detection and process-global loaders, while the application is the
  single authority for locale negotiation. Date/Author: 2026-05-18 / agent.
- Decision: initially plan to extend the existing subcommand argument structs
  rather than replace the configuration loader. Rationale: roadmap item `1.1.2`
  already proved this path for `CLI > env > config files > defaults` and
  `cmds.<subcommand>` merging. Date/Author: 2026-05-18 / agent.
- Decision: keep localized `clap` help and parse errors out of scope.
  Rationale: those behaviours are success criteria for roadmap item `1.4.3`,
  and implementing them here would blur the approval boundary. Date/Author:
  2026-05-18 / agent.
- Decision: use tests to define deterministic fallback semantics before
  implementation. Rationale: locale negotiation is policy, and ambiguous
  fallback behaviour is the highest-risk part of this task. Date/Author:
  2026-05-18 / agent.
- Decision: use OrthoConfig's existing double-underscore nested environment
  key convention for localization fields. Rationale: attempting to alias
  `LOCALIZATION_LOCALE` onto the top-level CLI field caused duplicate-field
  merge errors when CLI and env both set `fallback_locale`; the existing nested
  convention is already documented by the CLI help for `upstream` fields.
  Date/Author: 2026-05-18 / agent.

## Outcomes & Retrospective

Implemented the localization configuration layering for the binary
application. `locale` and `fallback_locale` now load through the existing
subcommand layering model: CLI flags override nested environment variables,
environment variables override `[cmds.<subcommand>.localization]` config file
values, and defaults provide `locale = None` with `fallback_locale = "en-US"`.

Startup now constructs one binary-owned `FluentLanguageLoader` from
`LocalizationConfig`, loads the embedded harness catalogue, and carries that
loader through the startup boundary for future localized rendering. The
library remains loader-injected and does not construct process-global
localization state.

Validation passed for code formatting, linting, full tests, Markdown linting,
diagram validation, and CodeRabbit review. The only residual tooling caveat is
that `make fmt` still reports unrelated pre-existing line-length errors from
the legacy Markdown fixer, even though `make markdownlint` passes.

Post-review validation on 2026-05-22 added process-level binary coverage for
`--locale` and `--fallback-locale`, snapshot coverage for startup-localized
error rendering, and narrow localization tracing in the CLI adapter. A fresh
CodeRabbit run was attempted but blocked by service rate limiting rather than
by a reported code concern.

## Relevant documentation and skills

Read these project documents before implementation:

- `docs/roadmap.md`, roadmap item `1.4.2`.
- `docs/spycatcher-harness-design.md`, especially
  "Localization architecture", "Configuration via OrthoConfig", and "CLI
  integration and configuration".
- `docs/users-guide.md`, especially "Configuration" and "Localizing library
  messages".
- `docs/developers-guide.md`, especially "Internal module layout" and the
  `i18n` module guidance.
- `docs/ortho-config-users-guide.md`, especially layered loading,
  subcommand configuration, display-request handling, and localizer sections.
- `docs/rust-testing-with-rstest-fixtures.md`.
- `docs/reliable-testing-in-rust-via-dependency-injection.md`.
- `docs/rstest-bdd-users-guide.md`.
- `docs/rust-doctest-dry-guide.md`.
- `docs/complexity-antipatterns-and-refactoring-strategies.md`.
- `docs/documentation-style-guide.md` before adding or changing
  documentation.

Use these skills when implementing:

- `leta` for code navigation and symbol references.
- `rust-router` to select any additional Rust skill needed by the actual
  implementation pressure.
- `domain-cli-and-daemons` because this task changes binary startup and CLI
  configuration.
- `rust-errors` if new startup/configuration errors are introduced.
- `rust-types-and-apis` if a new locale negotiation value type or public API
  surface becomes necessary.
- `hexagonal-architecture` to keep domain policy separate from CLI and binary
  adapters.
- `nextest` when running filtered Rust tests through the project's Makefile
  target or direct targeted commands.
- `commit-message` for every commit.
- `pr-creation` when opening or revising the draft PR.

## Repository orientation

`src/config.rs` defines `HarnessConfig` and the existing `LocalizationConfig`.
The default localization config is currently `locale: None` and
`fallback_locale: "en-US"`.

`src/cli.rs` is the CLI adapter for layered configuration loading. It defines
`RecordArgs`, `ReplayArgs`, and `VerifyArgs`, loads each subcommand through
`ortho_config::load_and_merge_subcommand`, and maps merged arguments into
`HarnessConfig` through `build_config` and `apply_overrides`.

`src/i18n.rs` embeds library Fluent resources through `HarnessLocalizations`
and renders `HarnessError` values through an injected `FluentLanguageLoader`.
Do not move loader ownership into this module.

`src/bin/spycatcher_harness.rs` is the binary composition root. It currently
loads config through `load_subcommand_config()`, handles help/version display,
builds a current-thread Tokio runtime, calls `start_harness(config)`, and then
shuts the harness down. This is the right boundary for creating and reusing the
application's authoritative language loader.

`tests/cli_layering_unit.rs` contains `rstest` unit coverage for configuration
precedence using `figment::Jail`.

`tests/features/harness_cli_layering.feature` and
`tests/harness_cli_layering_bdd.rs` contain `rstest-bdd` behavioural coverage
for externally observable CLI layering.

## Implementation plan

### Milestone 1: confirm configuration shape and write failing tests

Start by confirming whether OrthoConfig can load localization fields in the
same per-subcommand merge path used today. Use targeted experiments in tests,
not production code. The desired file shape is:

```toml
[cmds.replay.localization]
locale = "en-GB"
fallback_locale = "en-US"
```

The desired environment variables follow the existing prefix and subcommand
path:

```plaintext
SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__LOCALE=en-GB
SPYCATCHER_HARNESS_CMDS_REPLAY_LOCALIZATION__FALLBACK_LOCALE=en-US
```

If OrthoConfig also supports top-level global localization without a separate
loading pass, document that discovery and decide whether to support both
top-level and `cmds.<subcommand>` values. If it does not, keep the first
implementation scoped to the existing `cmds.<subcommand>` shape and update user
docs so the actual supported shape is unambiguous.

Add `rstest` cases to `tests/cli_layering_unit.rs` proving:

- default replay config yields `locale == None` and
  `fallback_locale == "en-US"`;
- config file values populate both fields;
- env values override config file values;
- CLI values override env and config file values;
- the same common localization override path works for `record`, `replay`, and
  `verify`;
- invalid CLI language identifiers fail deterministically once validation is
  introduced.

Add BDD scenarios to `tests/features/harness_cli_layering.feature` and step
definitions in `tests/harness_cli_layering_bdd.rs` for user-visible precedence:

- replay locale precedence favours CLI over env and file;
- fallback locale is used when no explicit locale is configured;
- invalid locale configuration fails with an actionable error marker.

Go/no-go: the new tests must fail for expected reasons before production
implementation. They should fail because fields are not yet parsed or
validated, not because the test harness is malformed.

Run targeted tests through `tee`:

```bash
cargo test --test cli_layering_unit \
  2>&1 | tee /tmp/test-1-4-2-cli-unit-red.out
cargo test --test harness_cli_layering_bdd \
  2>&1 | tee /tmp/test-1-4-2-cli-bdd-red.out
```

Run `coderabbit review --agent` after this milestone and clear any concerns
before proceeding.

### Milestone 2: add locale fields to the CLI adapter

In `src/cli.rs`, add a small serializable argument struct for localization, for
example `LocalizationArgs`, with `locale: Option<String>` and
`fallback_locale: Option<String>`. Add it to `RecordArgs`, `ReplayArgs`, and
`VerifyArgs` as a nested field using the same OrthoConfig/Serde pattern as
`RecordUpstreamArgs`.

Add `--locale <LANGID>` and `--fallback-locale <LANGID>` to each subcommand
argument struct. Keep these flags subcommand-local unless OrthoConfig supports
a clean global option path without breaking existing `cmds.<subcommand>`
merging. Map the merged values into `HarnessConfig.localization` through
`CommonOverrides` and `apply_overrides`, or through a new helper if that keeps
the function small.

Validate language identifiers at the adapter boundary using the
`i18n_embed::unic_langid::LanguageIdentifier` parser already used in
`src/i18n.rs` doctests. Store the accepted values as strings in
`LocalizationConfig` unless a later approved decision changes the domain type.
Return `CliConfigError::Merge` or a new focused `CliConfigError` variant with a
message that names the invalid field.

Go/no-go: `tests/cli_layering_unit.rs` and `tests/harness_cli_layering_bdd.rs`
pass, and no public library APIs change.

Run:

```bash
cargo test --test cli_layering_unit \
  2>&1 | tee /tmp/test-1-4-2-cli-unit-green.out
cargo test --test harness_cli_layering_bdd \
  2>&1 | tee /tmp/test-1-4-2-cli-bdd-green.out
```

Run `coderabbit review --agent` and clear concerns before proceeding. Commit
this milestone after `make check-fmt`, `make lint`, and `make test` pass.

### Milestone 3: build one startup language loader

Add a small application-level loader construction seam. Prefer a new module
only if it keeps `src/bin/spycatcher_harness.rs` small and testable; otherwise
use private functions in the binary. The seam should accept
`&LocalizationConfig` and return a `FluentLanguageLoader` or a typed startup
error.

The deterministic policy is:

1. Parse `fallback_locale`; invalid fallback locale is an error because there
   is no safe deterministic fallback beneath it.
2. If `locale` is `Some`, parse it. If parsing fails, return an actionable
   error naming `locale`.
3. Build one `FluentLanguageLoader` with the fallback language identifier.
4. Select embedded `HarnessLocalizations` with the requested locale first when
   present, followed by fallback locale. If no requested locale is present,
   select only fallback locale.
5. Reuse this loader for all localized rendering performed during startup and
   shutdown. Do not create additional loaders in lower-level library modules.

Add unit tests around the construction seam using `rstest`. Cover:

- no explicit locale selects the fallback locale;
- explicit locale plus fallback produces a deterministic preference order;
- invalid explicit locale fails without falling back silently;
- invalid fallback locale fails;
- missing requested catalogue falls back through `i18n-embed` selection while
  the loader remains usable for English library messages.

If the implementation cannot directly inspect selected locale order from
`FluentLanguageLoader`, introduce a tiny internal planning function that parses
configuration into a vector of `LanguageIdentifier` values and test that pure
function. Then keep loader construction as a thin adapter over the planned
locale vector.

Wire the binary so `main` constructs the loader once after config loading and
passes it through the startup/shutdown boundary where localized rendering is
needed. If no localized rendering is used until `1.4.3`, still keep the loader
owned by the binary for the full run and name the variable explicitly so reuse
is visible and testable.

Go/no-go: targeted loader tests pass, CLI tests still pass, and the library
still has no process-global localization state.

Run:

```bash
cargo test --bin spycatcher-harness \
  2>&1 | tee /tmp/test-1-4-2-bin-loader.out
cargo test --test cli_layering_unit \
  2>&1 | tee /tmp/test-1-4-2-cli-unit-loader.out
```

Run `coderabbit review --agent` and clear concerns before proceeding. Commit
this milestone after `make check-fmt`, `make lint`, and `make test` pass.

### Milestone 4: update user, developer, and design documentation

Update `docs/users-guide.md` with a CLI localization section documenting:

- `--locale`;
- `--fallback-locale`;
- the supported environment variable names;
- the supported TOML configuration shape;
- the precedence rule `CLI > env > config files > defaults`;
- deterministic fallback behaviour and invalid language identifier failures.

Update `docs/developers-guide.md` to state that binary startup owns language
loader construction and that library modules must continue to accept injected
loaders rather than creating their own.

Update `docs/spycatcher-harness-design.md` only where the implementation
settles a concrete detail that is currently ambiguous. If the selected config
shape is `cmds.<subcommand>.localization`, make the design and example TOML
match that shape or explicitly explain how global and per-subcommand values
interact.

Create an ADR only if the implementation chooses a materially new policy, such
as supporting both top-level global localization and per-subcommand
localization with a new merge order. If an ADR is created, follow
`docs/documentation-style-guide.md` and link it from the design document.

Go/no-go: documentation describes the actual implemented behaviour and does not
claim `1.4.3` localized help/parse errors are complete.

Run:

```bash
make fmt 2>&1 | tee /tmp/fmt-1-4-2-docs.out
make markdownlint 2>&1 | tee /tmp/markdownlint-1-4-2-docs.out
make nixie 2>&1 | tee /tmp/nixie-1-4-2-docs.out
```

Run `coderabbit review --agent` and clear concerns before proceeding. Commit
this milestone after documentation gates pass.

### Milestone 5: final validation, roadmap update, and PR refresh

Run the full gates sequentially, through `tee` logs:

```bash
make check-fmt 2>&1 | tee /tmp/check-fmt-1-4-2.out
make lint 2>&1 | tee /tmp/lint-1-4-2.out
make test 2>&1 | tee /tmp/test-1-4-2.out
make markdownlint 2>&1 | tee /tmp/markdownlint-1-4-2.out
make nixie 2>&1 | tee /tmp/nixie-1-4-2.out
```

If all gates pass and CodeRabbit has no open concerns, update `docs/roadmap.md`
to mark item `1.4.2` and its success criteria done. Update this ExecPlan's
`Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` with final evidence and any deviations from the plan.

Commit the roadmap and final plan updates as the final implementation commit.
Push the branch and refresh the draft PR description so reviewers can see the
implementation status, validation logs, and roadmap completion.

## Validation strategy

The minimum accepted validation for the implementation is:

- New `rstest` unit tests prove `locale` and `fallback_locale` precedence and
  invalid language identifier handling.
- New `rstest-bdd` scenarios prove externally observable CLI layering and
  fallback behaviour.
- Startup loader tests prove the binary constructs one deterministic loader
  from config and does not silently ignore invalid locale values.
- Existing harness startup, record-mode, cassette, matching, and doctest suites
  still pass.
- Documentation gates pass.
- `coderabbit review --agent` has been run after each major milestone and all
  concerns are resolved.

Property tests, Kani, and Verus are not required for the currently planned
scope because the task introduces a small deterministic precedence and parsing
policy rather than an invariant over a large state space, protocol ordering, or
unbounded business rule. If implementation expands into a general locale
negotiation algorithm over arbitrary preference lists, revisit this decision
and add property tests for order and fallback invariants.

## Acceptance criteria

The work is complete only when:

- `locale` and `fallback_locale` are loadable through
  `CLI > env > config files > defaults`;
- startup locale negotiation is deterministic and tested for fallback
  behaviour;
- one authoritative language loader is created at startup and reused;
- `docs/users-guide.md`, `docs/developers-guide.md`, and
  `docs/spycatcher-harness-design.md` accurately describe the implemented
  behaviour;
- `docs/roadmap.md` marks roadmap item `1.4.2` done;
- all full validation gates pass;
- CodeRabbit has no unresolved concerns; and
- the branch has been pushed to
  `origin/1-4-2-localization-configuration-layering` with a draft PR.
