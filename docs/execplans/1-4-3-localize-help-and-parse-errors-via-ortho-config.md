# Localize CLI help, parse errors, and version output via OrthoConfig localizer hooks

This ExecPlan (execution plan) is a living document. The sections `Constraints`,
`Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`, `Decision Log`,
and `Outcomes & Retrospective` must be kept up to date as work proceeds.

Status: COMPLETE

## Purpose / big picture

Roadmap item `1.4.3` makes the `spycatcher-harness` binary render `clap` help,
version, and parse-error output through the localization stack already used to
render harness library errors. After this work is implemented, a user running
`spycatcher-harness --help`, `spycatcher-harness replay --help`,
`spycatcher-harness verify --not-a-flag`, or a missing-argument scenario sees
text that:

- comes from an embedded en-US Fluent catalogue authored by this project,
  rather than from `clap`'s built-in English strings; and
- is rendered through an `ortho_config::Localizer` implementation so that any
  future locale assets the project ships will replace the same strings without
  further code changes; and
- falls back to `clap`'s stock English text when localization assets cannot be
  loaded, instead of failing the process.

The observable behaviour for a novice reviewer is:

- `spycatcher-harness --help` emits a deterministic snapshot of the bundled
  localized usage text.
- `spycatcher-harness replay --not-a-flag` exits with a non-zero status and
  prints an error string that matches the bundled localized
  `clap-error-unknown-argument` entry.
- If the embedded localization assets fail to load (simulated in tests), the
  binary still parses arguments and still emits help, version, and parse errors
  using `clap`'s stock text. No process-level failure is introduced by the
  localization stack itself.

This plan was approved before implementation began. Roadmap item `1.4.3` was
marked complete after implementation, tests, documentation, CodeRabbit review,
and commit gates were complete.

## Constraints

- Preserve the public library boundary established by roadmap item `1.4.1`.
  `src/i18n.rs` must continue to expose `HarnessLocalizations` and
  `localize_harness_error(&FluentLanguageLoader, &HarnessError)` without
  constructing or storing a process-global loader. The library crate must not
  depend on `ortho_config::Localizer`. Clap localization is an application
  adapter concern.
- Preserve the locale precedence and binary-owned `FluentLanguageLoader`
  introduced by roadmap item `1.4.2`. The new code path may build an additional
  `FluentLocalizer` for clap text, but the binary must still own exactly one
  authoritative `FluentLanguageLoader` for harness library rendering.
- Keep configuration loading in the CLI/application adapter. Domain and
  cassette logic must not import `clap`, `ortho_config`, Figment, or
  environment access APIs.
- Help, version, and parse errors must render via the
  `ortho_config::Localizer` pipeline; raw `clap::Error::to_string()` must not
  be the user-facing output path.
- The binary must fall back to `ortho_config::NoOpLocalizer` when the
  Fluent-backed localizer cannot be constructed. The CLI must remain usable in
  that condition; the only loss is localized message lookup.
- Use `rstest` for unit tests, `rstest-bdd` for behavioural tests where
  externally observable command-line behaviour is covered, and `insta` for
  snapshotting localized help and parse-error output. End-to-end tests for the
  binary must exercise the real `spycatcher-harness` binary, as
  `tests/binary_localization_e2e.rs` already does, so the help and parse-error
  paths are validated through `std::process::Command`.
- Do not mutate process environment directly in tests. Use
  `ortho_config::figment::Jail`, dependency injection, or existing test helpers
  in `tests/support`.
- Use `camino::Utf8PathBuf` for path fields. Do not introduce `std::fs` or
  `std::path` into new filesystem-facing application code unless there is no
  capability-oriented alternative.
- Keep each Rust source file at or below 400 lines. If adding tests would push
  an existing file over that limit, create a focused sibling test module.
- Documentation must use en-GB-oxendict spelling and must remain wrapped to
  the repository's Markdown style (`docs/documentation-style-guide.md`).
- Run `coderabbit review --agent` after each major milestone during
  implementation and clear all concerns before moving to the next milestone.
- Gate each commit. For code changes, run the relevant Makefile targets
  through `tee` logs before committing.

## Tolerances (exception triggers)

- Scope: if implementation requires modifying more than 15 tracked files or
  more than 900 net lines, stop and ask for approval before continuing.
- Public library API: if a public library API signature must change, stop and
  ask for approval. Adding a small binary or application helper module is
  allowed. Adding a new `pub` item in `src/cli.rs` requires an explicit
  decision-log entry naming the consumer.
- Configuration shape: if OrthoConfig localizer hooks cannot be used without a
  new top-level configuration surface, stop after documenting the exact
  conflict and propose the least invasive options before changing the binary's
  startup contract.
- Dependencies: if a new runtime dependency beyond the existing
  `ortho_config`, `i18n-embed`, `unic-langid`, `fluent`, and `clap` stack is
  needed, stop and ask for approval. A new dev-dependency for tests also
  requires an explicit decision-log entry.
- Locale assets: if implementation needs non-English Fluent resources to
  prove fallback behaviour, stop and ask whether to add a minimal test-only
  locale or defer multi-locale assets to a later roadmap item.
- Iterations: if any of `make check-fmt`, `make lint`, `make test`,
  `make markdownlint`, or `make nixie` still fails after five focused repair
  attempts, stop, record the failure transcript, and ask for direction.
- Review: if `coderabbit review --agent` reports a concern that conflicts
  with this plan, stop and update the `Decision Log` before changing approach.

## Risks

- The OrthoConfig `localize_clap_error_with_command` helper shipped bundled
  translations for four `clap-error-*` IDs in its embedded catalogue:
  `clap-error-missing-argument`, `clap-error-unknown-argument`,
  `clap-error-invalid-value`, `clap-error-missing-subcommand`. During this work,
  `i18n/en-US/spycatcher-harness.ftl` was expanded with a focused superset of
  `clap-error-*` IDs for the `ErrorKind` cases the harness surfaces today.
  Tests now assert that those IDs render through the harness bundle rather than
  relying on OrthoConfig defaults. Residual severity: low. Residual likelihood:
  low.
- OrthoConfig's `FluentLocalizer` and `i18n_embed::fluent::FluentLanguageLoader`
  cannot share a `FluentBundle` directly. The harness will need to compile its
  embedded en-US Fluent text twice (once for library error rendering through
  `FluentLanguageLoader`, once for clap text through `FluentLocalizer`).
  Severity: low. Likelihood: high. Mitigation: keep the FTL source as a single
  embedded asset, but pass the same `include_str!` text into the
  `FluentLocalizer` builder so a translator-facing change only happens once.
- Localization must be available *before* CLI parsing, but the requested
  locale itself is parsed from CLI flags. The binary therefore has to make a
  best-effort locale choice (fallback locale + any environment variable hint)
  before clap parsing, then optionally upgrade the locale plan for downstream
  harness rendering once the merged configuration is known. Severity: medium.
  Likelihood: high. Mitigation: codify a two-phase localization policy in the
  binary (see Plan of work, Milestone 4) and assert it via unit tests.
- The OrthoConfig `clap-error-*` ID set is hard-coded; if a future clap
  release adds new `ErrorKind` variants, the mapping will silently fall back to
  stock text for the new kinds. Severity: low. Likelihood: low. Mitigation: add
  an `rstest` matrix that exercises each `clap::error::Kind` variant we care
  about and asserts which IDs we ship.
- The existing `tests/binary_localization_e2e.rs` exercises `--locale` and
  `--fallback-locale` startup behaviour by invoking the compiled binary. New
  end-to-end coverage for `--help`/`--version`/parse failures will increase
  test run time. Severity: low. Likelihood: medium. Mitigation: keep end-to-end
  coverage to small representative scenarios and rely on `insta` snapshots plus
  pure-Rust unit tests for the bulk of assertions.

## Progress

- [x] (2026-05-28T10:00Z) Loaded the `execplans` skill.
- [x] (2026-05-28T10:05Z) Renamed the working branch to
  `1-4-3-localize-help-and-parse-errors-via-ortho-config`.
- [x] (2026-05-28T10:15Z) Used parallel reconnaissance agents to inspect the
  current CLI adapter, binary, localization assets, tests, and OrthoConfig
  upstream localizer API.
- [x] (2026-05-28T10:30Z) Drafted this ExecPlan.
- [x] Open draft PR and request approval.
- [x] (2026-06-04T00:00Z) Received user instruction to proceed with
  implementation of this ExecPlan; treating that as explicit approval to move
  from draft to implementation.
- [x] (2026-06-04T00:00Z) Milestone 1: tests authored for catalogue lookups,
  command localization, localized parse errors, binary help/error output, and
  `NoOpLocalizer` fallback. The implementation proceeded in the same commit
  because deterministic gates were required before CodeRabbit review.
- [x] (2026-06-04T00:00Z) Milestone 2: project-owned `LocalizeCmd` and
  `try_parse_localized_from_iter` helper added under `src/cli/`.
- [x] (2026-06-04T00:00Z) Milestone 3: bundled `cli-*` and `clap-error-*` FTL
  assets added to `i18n/en-US/spycatcher-harness.ftl`.
- [x] (2026-06-04T00:00Z) Milestone 4: localizer construction with
  deterministic `NoOpLocalizer` fallback wired into CLI parsing.
- [x] (2026-06-04T00:00Z) Implemented the production CLI localization path:
  `LocalizeCmd`, localized parse helper, Fluent-backed CLI localizer,
  diagnostic disable switch, catalogue entries, unit tests, and binary e2e
  tests are present. Targeted `cargo test --test cli_localization_unit`,
  `cargo test --test cli_layering_unit`, and
  `cargo test --test binary_localization_e2e` pass.
- [x] (2026-06-04T00:00Z) Milestone 5: documentation update
  (`docs/users-guide.md`, `docs/developers-guide.md`,
  `docs/spycatcher-harness-design.md`).
- [x] (2026-06-04T00:00Z) Milestone 6: final validation and roadmap update.
  `make check-fmt`, `make lint`, `make test`, `make markdownlint`, and
  `make nixie` passed. CodeRabbit review first reported one trivial ADR comma
  fix; after applying it and re-running documentation gates, CodeRabbit
  returned zero findings.
- [x] (2026-06-16T00:00Z) Review follow-up: verified the reported version
  overclaim against the current code, implemented `--version` with localized
  command-copy support, and added snapshots for version, missing-subcommand,
  and invalid-value output.
- [x] (2026-06-16T00:00Z) Review follow-up: added the planned proptest
  invariant for generated language identifiers and localizer fallback, plus a
  merge-help content parity test to guard the stock and Fluent copies.
- [x] (2026-06-16T00:00Z) Review follow-up: recorded the explicit decision to
  rely on binary e2e snapshots instead of adding duplicate `rstest-bdd`
  scenarios for this CLI localization surface.
- [x] (2026-06-16T00:00Z) Review follow-up validation passed:
  `make check-fmt`, `make typecheck`, `make lint`, `make test`,
  `make markdownlint`, and `make nixie`. CodeRabbit review completed with zero
  findings.
- [x] (2026-06-17T00:00Z) Failed-check follow-up: re-verified the reported
  items against current code. The fallback locale retry, truthy disable switch,
  public config-loading shim, binary snapshot env isolation, ADR structure,
  users' guide disable-switch wording, and version-scope documentation were
  already present. The still-valid items were the missing public re-export
  mention in `src/cli/localization.rs`, accepting `on` as an explicit truthy
  disable value, logging when the diagnostic disable switch is active, and a
  small property test for generated early-locale candidates.
- [x] (2026-06-17T00:00Z) Second failed-check follow-up: removed the
  conditional wording around the shipped diagnostic env switch, documented all
  truthy disable values, isolated locale environment variables in binary
  snapshots, enabled clap's `string` feature to remove the localized version
  string leak, and aligned design diagrams with `camino::Utf8PathBuf`.

Use timestamps to measure rates of progress and detect tolerance breaches.

## Surprises & Discoveries

- `Command::localize` is *not* a public OrthoConfig API. It is an extension
  trait `LocalizeCmd` defined in `examples/hello_world/src/cli/localization.rs`
  in the OrthoConfig repository. Likewise, `try_parse_localized_env` is an
  inherent method on the example's `CommandLine` type. The harness must own
  equivalent helpers. Evidence: OrthoConfig `v0.8.0` source tree on GitHub.
  Impact: the plan adds an explicit milestone for a small project-owned
  `LocalizeCmd` trait and a parsing helper. Documentation referencing
  `Command::localize` and `try_parse_localized_env` must clarify they are
  project-owned in this repository.
- The `Localizer`, `Command::localize`, and `try_parse_localized_*` symbols
  named in earlier planning notes were not all exported by the `ortho_config`
  crate. Only the `Localizer` trait, `LocalizationArgs<'a>` alias,
  `FluentLocalizer`, `NoOpLocalizer`, `localize_clap_error_with_command`, and
  `clap_error_formatter` were available as public crate API. The project
  resolved this by adding its own `LocalizeCmd` extension trait and localized
  parsing helpers under `src/cli/`, then documenting that boundary in
  `docs/developers-guide.md`.
- `ortho_config::LocalizationArgs<'a>` is a type alias for
  `HashMap<&'a str, fluent_bundle::FluentValue<'a>>`, not a struct.
  Implementation must use it as a map.
- `localize_clap_error_with_command` short-circuits `DisplayHelp` and
  `DisplayVersion` error kinds unchanged. Help and version localization is
  driven by the `LocalizeCmd` trait alone, not by the error helper. Impact: the
  help and version code paths must run *before* `try_get_matches` returns in
  order to substitute localized strings on the `clap::Command` itself.
- `localize_clap_error_with_command` builds a new `ClapError::raw(...)` when
  it has a translation, discarding the original clap context. We must accept
  that the localized rendering will not preserve clap's coloured suggestion
  output for those kinds. Impact: the localized text must be self-sufficient
  (include the offending value, available subcommands, etc.) and the snapshot
  tests must lock that down.
- `ortho_config`'s embedded `messages.ftl` only ships translations for four
  `clap-error-*` IDs; everything else falls back to stock English. Impact: the
  harness must ship its own superset of `clap-error-*` IDs in
  `i18n/en-US/spycatcher-harness.ftl` to make sure today's behaviour is
  observably localized rather than passing through unchanged.
- `i18n_embed::fluent::FluentLanguageLoader` does not expose a way to hand
  out a `FluentBundle<Arc<FluentResource>>`, so it cannot share its compiled
  bundle with `FluentLocalizer`. The Fluent source is compiled twice. The
  embedded `include_str!` source remains a single asset.
- Fluent treats indented bracketed lines such as `[cmds.record]` inside a
  multiline message as variant syntax. The localized merge-help example uses
  escaped bracket placeables so the rendered text remains valid TOML while the
  catalogue stays parser-friendly.
- The user-facing binary path for localized parse errors must not wrap
  `CliConfigError::CliParse` with `eyre`, because that duplicates the Clap
  diagnostic and adds a source location. The binary now writes localized Clap
  parse errors directly to stderr and exits with code 2.
- `clap::Command::version` stores a static builder string, unlike help text
  fields that accept owned strings. The localized version helper therefore
  converts the rendered version component into the static string shape clap
  requires before parsing.
- Fluent inserts bidirectional isolation marks around interpolated values such
  as `{ $binary }`, `{ $argument }`, and `{ $valid_subcommands }`. The root
  usage snapshots intentionally preserve those marks; subcommand usage remains
  plain stock clap usage when no localized subcommand usage entry is present.
- Later failed-check output repeated several stale concerns from the earlier
  review after those fixes had already landed. The code already preserved the
  original `load_subcommand_config_from_iter(iter)` public signature, isolated
  binary localization snapshots with `env_remove(DISABLE_LOCALIZATION_ENV)`,
  retried `SPYCATCHER_HARNESS_FALLBACK_LOCALE` when the primary early locale
  was invalid, and documented the disable switch as affecting help, version,
  and parse-error output through `NoOpLocalizer`.

## Decision Log

- Decision: treat this as an application/adapter change. The library crate
  must not learn about `ortho_config::Localizer` or `clap`. Rationale: matches
  the architectural boundary set by roadmap items `1.4.1` and `1.4.2`.
  Date/Author: 2026-05-28 / agent.
- Decision: define a project-owned `LocalizeCmd` extension trait on
  `clap::Command` inside `src/cli/localization.rs` (or a sibling module),
  modelled on the OrthoConfig hello_world example, rather than calling a
  non-existent `Command::localize` from `ortho_config`. Rationale: the upstream
  API does not expose this trait; copying the small, well-understood pattern
  keeps us aligned with the documented OrthoConfig CLI localization recipe
  without inventing a new abstraction. Date/Author: 2026-05-28 / agent.
- Decision: define a project-owned `try_parse_localized_from_iter(...)`
  helper that mirrors the OrthoConfig hello_world example's
  `try_parse_localized_env`. Rationale: the helper is the natural seam where
  `LocalizeCmd::localize`, `try_get_matches_from_mut`, and
  `localize_clap_error_with_command` compose. Owning it in the binary keeps the
  localization life cycle visible. Date/Author: 2026-05-28 / agent.
- Decision: build the localizer *before* CLI parsing using a best-effort
  locale plan derived from the OrthoConfig fallback locale default and any
  environment variable hint, then continue to build the application-phase
  `FluentLanguageLoader` from the fully merged `LocalizationConfig` after CLI
  parsing. Rationale: the requested locale is itself a CLI flag, so the
  localizer used for parsing must be available before clap runs. Application
  localization can use a more specific locale once the merged config is known.
  Date/Author: 2026-05-28 / agent.
- Decision: on Fluent localizer construction failure, the binary falls back
  to `ortho_config::NoOpLocalizer` and emits a `tracing::warn!` describing the
  cause. The CLI continues. Rationale: this is the explicit roadmap success
  criterion and matches the OrthoConfig hello_world fallback pattern.
  Date/Author: 2026-05-28 / agent.
- Decision: keep multilingual coverage out of scope for `1.4.3`. The bundled
  catalogue remains English-only; the implementation only proves that the
  *mechanism* works and that fallback is deterministic. Adding non-English
  assets is roadmap material beyond `1.4.3`. Date/Author: 2026-05-28 / agent.
- Decision: share the FTL *source* between `FluentLanguageLoader` and
  `FluentLocalizer` via `include_str!` but accept two compiled bundles.
  Rationale: `i18n-embed` does not expose a method to share its compiled
  bundle, and a custom Localizer adapter over `FluentLanguageLoader` would
  bypass OrthoConfig's bundled `clap-error-*` defaults. Date/Author: 2026-05-28
  / agent.
- Decision: introduce `SPYCATCHER_HARNESS_DISABLE_LOCALIZATION` as a
  diagnostic switch that forces the CLI parser through `NoOpLocalizer`.
  Rationale: it provides a deterministic process-level fallback test and a
  practical operator escape hatch when diagnosing localization assets.
  Date/Author: 2026-06-04 / agent.
- Decision: surface `CliConfigError::CliParse` directly from the binary
  process boundary instead of wrapping it in `eyre`. Rationale: Clap parse
  errors are already complete user-facing diagnostics; wrapping them made the
  output noisy and duplicated. Date/Author: 2026-06-04 / agent.
- Decision: implement `--version` rather than reverting the version scope from
  the roadmap and guides. Rationale: the documentation already describes
  version output as part of the pre-parse command localization path, and clap
  can provide the flag with `#[command(version)]`. The localized catalogue
  supplies the version component while clap renders the final
  `spycatcher-harness 0.1.0` line. Date/Author: 2026-06-16 / agent.
- Decision: do not add a separate `rstest-bdd` feature for CLI localization.
  Rationale: `tests/binary_localization_e2e.rs` exercises the compiled binary
  through the real process boundary and now snapshots localized help, version,
  unknown-argument output, and the `NoOpLocalizer` diagnostic fallback.
  Duplicating those same paths through BDD step definitions would add
  maintenance without increasing behaviour coverage. Date/Author: 2026-06-16
  / agent.
- Decision: keep both `src/cli_help.rs::CLI_MERGE_HELP` and the
  `cli-merge-help` Fluent entry. Rationale: the constant remains the stock
  clap fallback when localization is disabled, while the Fluent entry is the
  localized path. A unit test renders the Fluent entry and asserts every
  non-empty stock line is still present, allowing harmless whitespace
  differences without silent content drift. Date/Author: 2026-06-16 / agent.

## Outcomes & Retrospective

Implemented localized CLI help, version, and parse-error rendering through a
project-owned `LocalizeCmd` trait, `try_parse_localized_from_iter`, and a
Fluent-backed `ortho_config::Localizer` built from the existing en-US
catalogue. The binary now builds that CLI localizer before argument parsing,
then builds the authoritative `FluentLanguageLoader` after merged configuration
is available for harness library errors.

The fallback path is deterministic. Invalid localizer resources fall back to
`NoOpLocalizer`, and the diagnostic `SPYCATCHER_HARNESS_DISABLE_LOCALIZATION=1`
switch proves stock `clap` output through the real binary. Parse errors are
written directly as Clap diagnostics on stderr with exit code 2, avoiding
duplicated `eyre` reports.

Validation passed with `make check-fmt`, `make lint`, `make test`,
`make markdownlint`, and `make nixie`. CodeRabbit review completed with zero
findings after one trivial ADR punctuation fix. Follow-up validation on
2026-06-16 added targeted `cargo test --test cli_localization_unit` and
`cargo test --test binary_localization_e2e` runs covering the new version and
parse-error snapshots before `make check-fmt`, `make typecheck`, `make lint`,
`make test`, `make markdownlint`, `make nixie`, and `coderabbit review --agent`
all passed.

## Relevant documentation and skills

Read these project documents before implementation:

- `docs/roadmap.md`, roadmap item `1.4.3`.
- `docs/spycatcher-harness-design.md`, especially "Localization architecture",
  "Configuration via OrthoConfig", and "CLI shape".
- `docs/users-guide.md`, especially the configuration and "Localizing library
  messages" sections.
- `docs/developers-guide.md`, especially internal module layout and the
  `i18n` module guidance.
- `docs/ortho-config-users-guide.md`, especially "Localizing CLI copy",
  display-request handling, and the Fluent-backed localizer section.
- `docs/localizable-rust-libraries-with-fluent.md` for the library-side
  Fluent stack.
- `docs/rust-testing-with-rstest-fixtures.md`.
- `docs/reliable-testing-in-rust-via-dependency-injection.md`.
- `docs/rstest-bdd-users-guide.md`.
- `docs/rust-doctest-dry-guide.md`.
- `docs/complexity-antipatterns-and-refactoring-strategies.md`.
- `docs/documentation-style-guide.md` before adding or changing
  documentation.

Useful upstream references identified during reconnaissance (cite these in the
implementation commit messages when relevant):

- OrthoConfig `v0.8.0` localizer source:
  `ortho_config/src/localizer/{mod,fluent,clap_error}.rs`.
- OrthoConfig hello_world example:
  `examples/hello_world/src/cli/localization.rs` and
  `examples/hello_world/src/localizer.rs` (the source of the `LocalizeCmd` and
  fallback patterns we are reproducing).
- OrthoConfig embedded catalogue:
  `ortho_config/locales/en-US/messages.ftl`.
- `i18n-embed` `FluentLanguageLoader` reference on `docs.rs`.
- `clap_rs/clap` discussion #4512 on first-party i18n status.

Use these skills when implementing:

- `leta` for code navigation and symbol references.
- `rust-router` to select any additional Rust skill the actual implementation
  pressure requires.
- `domain-cli-and-daemons` because this task changes CLI argument parsing,
  help rendering, and binary startup behaviour.
- `rust-errors` if new application errors are introduced or existing ones
  change shape.
- `rust-types-and-apis` if a public CLI type surface needs design pressure.
- `hexagonal-architecture` to keep domain policy separate from clap, Fluent,
  and OrthoConfig adapters.
- `nextest` when running filtered Rust tests through the project's Makefile
  target or direct targeted commands.
- `commit-message` for every commit.
- `pr-creation` when opening or revising the draft PR.

## Repository orientation

`src/cli.rs` is the CLI adapter for layered configuration loading. It defines
the `Cli` struct, the `Commands` enum, `RecordArgs`, `ReplayArgs`, and
`VerifyArgs`, and the `load_subcommand_config_from_iter` entry point. Help and
version requests are detected in `parse_cli_from_iter` and surfaced as
`CliConfigError::DisplayRequested { output }` (`src/cli.rs:112`).

`src/cli_args.rs` defines `LocalizationArgs` and `RecordUpstreamArgs` used by
each subcommand for OrthoConfig merging.

`src/cli_help.rs` exposes the constant `CLI_MERGE_HELP`, currently injected
into the root command via `#[command(after_long_help = CLI_MERGE_HELP)]`
(`src/cli.rs:158`). This string is plain English and must become a localized
asset under this work.

`src/cli/localization.rs` already hosts CLI-side localization helpers
(`CommonOverrides`, `select_localization_override`,
`validate_language_identifier`). This is the natural module for the new
`LocalizeCmd` extension trait and `try_parse_localized_from_iter` helper.

`src/i18n.rs` embeds the library-owned Fluent catalogue through
`HarnessLocalizations` and the `rust_embed` derive over `i18n/`. The same
embedded asset path will be reused for clap-side strings. Do not move loader
ownership into this module.

`src/bin/spycatcher_harness.rs` is the binary composition root. It currently
calls `load_subcommand_config()`, handles `DisplayRequested`, builds a
`FluentLanguageLoader` from the merged `LocalizationConfig`, builds the Tokio
runtime, and starts the harness. This is the right boundary for constructing
the new `Localizer` *before* CLI parsing.

`i18n/en-US/spycatcher-harness.ftl` is the only Fluent catalogue today. It
contains eight `harness-error-*` message IDs. This file will gain new `cli-*`
and `clap-error-*` entries during Milestone 3.

`tests/cli_layering_unit.rs` contains `rstest` unit coverage for layered
configuration precedence via `figment::Jail`.

`tests/binary_localization_e2e.rs` invokes the compiled binary via
`std::process::Command` and asserts startup error rendering. The new end-to-end
coverage for `--help`, `--version`, and parse failures will be added alongside.

`tests/features/harness_cli_layering.feature` and
`tests/harness_cli_layering_bdd.rs` contain `rstest-bdd` behavioural coverage
for externally observable CLI layering, including locale precedence. New
scenarios for localized help and parse-error rendering will be added here.

`tests/support/` contains shared test helpers (`test_utils.rs`,
`bdd_fixtures.rs`). New helpers required for end-to-end command exercises
belong in this directory.

## Plan of work

### Milestone 1: failing tests that pin the behaviour

Begin by adding red coverage for the externally observable behaviour. No
production code changes in this milestone.

Add `rstest` unit cases to a new test file `tests/cli_localization_unit.rs`
covering:

- a localizer built from valid bundled Fluent assets returns localized text
  for `cli-about`, `cli-long-about`, and `cli-usage`;
- a localizer built with empty consumer resources still returns OrthoConfig's
  bundled defaults for the four documented `clap-error-*` IDs;
- a `NoOpLocalizer` always returns `None` from `lookup` and the fallback from
  `message`;
- the project-owned `LocalizeCmd::localize` extension trait applied to
  `Cli::command()` mutates `about`, `long_about`, and `override_usage` exactly
  when the localizer returns `Some(...)`, and leaves them untouched otherwise;
- `localize_clap_error_with_command` is invoked for the
  `MissingRequiredArgument`, `UnknownArgument`, `InvalidValue`, and
  `InvalidSubcommand` kinds and the resulting message contains the bundled
  catalogue text and the contextual argument or value.

Add `insta` snapshot cases to `tests/cli_localization_unit.rs` (or a sibling
`tests/snapshots/cli_localization_unit__*` snapshot directory) that pin:

- the rendered `--help` output for the root command;
- the rendered `--help` output for `replay`;
- the rendered output for a missing subcommand;
- the rendered output for an unknown flag (`replay --not-a-flag`);
- the rendered output for an invalid value
  (`record --listen not-a-socket`).

Do not add a separate `rstest-bdd` feature for CLI localization. The Decision
Log records the 2026-06-16 choice to use binary e2e snapshots instead because
they exercise the same user-visible behaviour through the compiled binary.

Extend `tests/binary_localization_e2e.rs` with end-to-end coverage that invokes
the compiled binary via `std::process::Command`:

- `spycatcher-harness --help` exits 0 and prints a deterministic snapshot;
- `spycatcher-harness replay --not-a-flag` exits non-zero and prints the
  localized unknown-argument text;
- `SPYCATCHER_HARNESS_DISABLE_LOCALIZATION=1 spycatcher-harness --help`
  exits 0 and prints clap's stock English text (the env-flag escape hatch is
  introduced in Milestone 4 specifically to test the fallback path; if a
  cleaner test seam is found, document the change in the Decision Log).

Go/no-go: the new tests fail for expected reasons before production
implementation. Failures should be due to missing localized output, not
malformed harnesses. Run targeted tests through `tee`:

```bash
cargo test --test cli_localization_unit \
  2>&1 | tee /tmp/test-1-4-3-cli-localization-unit-red.out
cargo test --test cli_localization_bdd \
  2>&1 | tee /tmp/test-1-4-3-cli-localization-bdd-red.out
cargo test --test binary_localization_e2e \
  2>&1 | tee /tmp/test-1-4-3-binary-e2e-red.out
```

Run `coderabbit review --agent` after this milestone and clear any concerns
before proceeding. Commit this milestone after `make check-fmt`, `make lint`,
and `make test` pass at the workspace level (the new red tests will still fail;
this milestone commits *only* the failing tests and any necessary plumbing such
as snapshot directories).

### Milestone 2: project-owned `LocalizeCmd` and parsing helper

In `src/cli/localization.rs` (or a sibling module `src/cli/localize_cmd.rs` if
file size pushes over 400 lines), add:

```rust,no_run
use clap::Command;
use ortho_config::Localizer;

/// Extension trait that applies a [`Localizer`] to a [`clap::Command`].
///
/// Modelled on the OrthoConfig hello_world example. Walks subcommands
/// recursively. Overrides `about`, `long_about`, and `override_usage` only
/// when the localizer returns `Some` for the corresponding identifier.
pub trait LocalizeCmd {
    fn localize(self, localizer: &dyn Localizer) -> Self;
}

impl LocalizeCmd for Command {
    fn localize(self, localizer: &dyn Localizer) -> Self {
        // Build identifier from command path, override about / long_about /
        // override_usage when present, recurse into subcommands.
        // Identifier shape: "spycatcher-harness", "spycatcher-harness.replay",
        // "spycatcher-harness.record", "spycatcher-harness.verify".
        // ...
        self
    }
}
```

Also add a small helper:

```rust,no_run
use ortho_config::{Localizer, localize_clap_error_with_command};
use clap::{CommandFactory, Parser};

/// Parses `iter` into `Cli` using `localizer` for help, version, and parse
/// error rendering.
pub fn try_parse_localized_from_iter<C, I, T>(
    iter: I,
    localizer: &dyn Localizer,
) -> Result<C, clap::Error>
where
    C: Parser + CommandFactory,
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut command = C::command().localize(localizer);
    let matches = command
        .try_get_matches_from_mut(iter)
        .map_err(|err| localize_clap_error_with_command(err, localizer, Some(&command)))?;
    C::from_arg_matches(&matches).map_err(|err| {
        let err = err.with_cmd(&command);
        localize_clap_error_with_command(err, localizer, Some(&command))
    })
}
```

Wire `parse_cli_from_iter` in `src/cli.rs` to accept `&dyn Localizer` and call
`try_parse_localized_from_iter`. The function's signature changes from:

```rust,no_run
fn parse_cli_from_iter<I, T>(iter: I) -> Result<Cli, CliConfigError>
```

to:

```rust,no_run
fn parse_cli_from_iter<I, T>(
    iter: I,
    localizer: &dyn ortho_config::Localizer,
) -> Result<Cli, CliConfigError>
```

Cascade the same signature change to `load_subcommand_config_from_iter` and
introduce a new convenience function `load_subcommand_config_with_localizer` to
keep the no-arg `load_subcommand_config()` working for callers that pass a
`NoOpLocalizer`. Document the new entry point.

Go/no-go: the unit tests for `LocalizeCmd` and `try_parse_localized_from_iter`
pass. Existing layered configuration tests still pass.

Run:

```bash
cargo test --test cli_localization_unit \
  2>&1 | tee /tmp/test-1-4-3-cli-localization-unit-m2.out
cargo test --test cli_layering_unit \
  2>&1 | tee /tmp/test-1-4-3-cli-layering-unit-m2.out
cargo test --test harness_cli_layering_bdd \
  2>&1 | tee /tmp/test-1-4-3-cli-layering-bdd-m2.out
```

Run `coderabbit review --agent` and clear concerns. Commit after
`make check-fmt`, `make lint`, and `make test` pass.

### Milestone 3: ship bundled `cli-*` and `clap-error-*` FTL assets

Update `i18n/en-US/spycatcher-harness.ftl` to add:

- `cli-about` — short top-level command description;
- `cli-long-about` — full long-form description (uses `{ $binary }` for the
  binary name; argument keys must match what `LocalizeCmd::localize`
  interpolates);
- `cli-usage` — usage line (uses `{ $binary }`);
- `cli-record-about`, `cli-replay-about`, `cli-verify-about` — per-subcommand
  short descriptions;
- `cli-merge-help` — replaces the current static `CLI_MERGE_HELP` constant
  contents; the `LocalizeCmd::localize` implementation should set
  `after_long_help` from this message when present;
- `clap-error-missing-argument`, `clap-error-unknown-argument`,
  `clap-error-invalid-value`, `clap-error-invalid-subcommand`,
  `clap-error-missing-subcommand`, `clap-error-no-equals`,
  `clap-error-too-many-values`, `clap-error-too-few-values`,
  `clap-error-value-validation`, `clap-error-argument-conflict`,
  `clap-error-invalid-utf8`, `clap-error-io`, and `clap-error-format` — each
  using the OrthoConfig-documented argument keys (`argument`, `value`,
  `valid_values`, `expected`, `actual`, `min`, `subcommand`,
  `valid_subcommands`). Write the en-US text so it reads naturally and conveys
  the same information as the corresponding clap stock message.

Add the existing `CLI_MERGE_HELP` static text into the FTL catalogue as the
`cli-merge-help` entry. Delete the `CLI_MERGE_HELP` constant from
`src/cli_help.rs` if it becomes redundant, or keep a doc-test fixture that
asserts the catalogue and the constant agree, and document the rationale in the
Decision Log.

Add unit tests that:

- assert each bundled `clap-error-*` ID has a non-empty translation in the
  catalogue;
- assert the `cli-*` IDs are non-empty;
- assert `LocalizeCmd::localize` populates `after_long_help` from
  `cli-merge-help`.

Go/no-go: the previously red `insta` snapshots from Milestone 1 now produce
deterministic localized output. Existing layered configuration tests still pass.

Run:

```bash
cargo test --test cli_localization_unit \
  2>&1 | tee /tmp/test-1-4-3-cli-localization-unit-m3.out
cargo test --test cli_localization_bdd \
  2>&1 | tee /tmp/test-1-4-3-cli-localization-bdd-m3.out
```

Use `cargo insta accept` to lock in the snapshots after manual review.

Run `coderabbit review --agent` and clear concerns. Commit after
`make check-fmt`, `make lint`, and `make test` pass.

### Milestone 4: localizer construction with deterministic fallback

In a new `src/cli/localizer.rs` module (or extend `src/cli/localization.rs` if
the 400-line ceiling permits), add:

```rust,no_run
use std::sync::Arc;

use i18n_embed::unic_langid::LanguageIdentifier;
use ortho_config::{FluentLocalizer, Localizer, NoOpLocalizer};

/// Constructs a `Localizer` for clap copy and parse errors.
///
/// Returns `Arc<dyn Localizer>` so the same instance is shared by clap
/// parsing and any downstream code that needs to render Fluent identifiers
/// (for example, a future help footer extension).
///
/// On `FluentLocalizer` construction failure the function emits a
/// `tracing::warn!` and returns a `NoOpLocalizer`-backed `Arc` so the CLI
/// remains usable.
pub fn build_cli_localizer(locale: LanguageIdentifier) -> Arc<dyn Localizer> {
    match FluentLocalizer::builder(locale)
        .with_consumer_resources([CLI_FTL])
        .try_build()
    {
        Ok(localizer) => Arc::new(localizer),
        Err(error) => {
            tracing::warn!(?error, "falling back to NoOpLocalizer for CLI localization");
            Arc::new(NoOpLocalizer::new())
        }
    }
}

const CLI_FTL: &str = include_str!("../../i18n/en-US/spycatcher-harness.ftl");
```

Add an `early_locale_plan()` helper that returns the locale to use *before* CLI
parsing. The deterministic rule is:

1. Read `SPYCATCHER_HARNESS_LOCALE` (if set), otherwise
   `SPYCATCHER_HARNESS_FALLBACK_LOCALE` (if set), otherwise the OrthoConfig
   built-in default `"en-US"`.
2. Parse with `unic_langid::LanguageIdentifier`. On parse failure, emit a
   `tracing::warn!` and use `"en-US"`.

Wire the binary so that `main` (in `src/bin/spycatcher_harness.rs`) calls
`build_cli_localizer(early_locale_plan())` before
`load_subcommand_config_with_localizer(localizer.as_ref())`. The existing
`build_language_loader(&config.localization)` call continues to run after CLI
parsing and is unaffected by this milestone.

Add a deliberate test seam for the "localization assets failed to load" branch.
Two acceptable options:

- pass the FTL text as a parameter so a test can pass deliberately broken
  bytes (preferred); or
- introduce an `SPYCATCHER_HARNESS_DISABLE_LOCALIZATION` env switch that
  forces the binary to construct a `NoOpLocalizer`. Users can use this
  user-observable env switch as a diagnostic toggle, and
  `docs/users-guide.md` documents the shipped behaviour.

Add a `proptest` strategy (already used in `src/bin/spycatcher_harness.rs`) to
assert that random BCP 47 language identifiers either build a `FluentLocalizer`
or fall back to `NoOpLocalizer` without panicking. This guards the
deterministic fallback invariant.

Go/no-go: every localized test from Milestones 1 and 3 passes against the
compiled binary; the simulated load-failure path yields stock clap text;
existing locale precedence tests from `1.4.2` still pass.

Run:

```bash
cargo test --test cli_localization_unit \
  2>&1 | tee /tmp/test-1-4-3-cli-localization-unit-m4.out
cargo test --test cli_localization_bdd \
  2>&1 | tee /tmp/test-1-4-3-cli-localization-bdd-m4.out
cargo test --test binary_localization_e2e \
  2>&1 | tee /tmp/test-1-4-3-binary-e2e-m4.out
cargo test --bin spycatcher-harness \
  2>&1 | tee /tmp/test-1-4-3-bin-loader-m4.out
```

Run `coderabbit review --agent` and clear concerns. Commit after
`make check-fmt`, `make lint`, and `make test` pass.

### Milestone 5: documentation

Update `docs/users-guide.md` with a CLI localization section that documents:

- where bundled help text comes from (the embedded en-US Fluent catalogue);
- how parse errors are rendered through OrthoConfig's
  `clap-error-*` mapping;
- the deterministic fallback to stock clap text when localization assets
  fail to load, including the diagnostic env switch if introduced in Milestone
  4;
- the relationship between `--locale`/`--fallback-locale` and CLI text
  rendering (currently no non-English locale is bundled, but the mechanism is
  in place for future locales).

Update `docs/developers-guide.md` with:

- the project-owned `LocalizeCmd` extension trait and
  `try_parse_localized_from_iter` helper, with file paths;
- the rule that the library crate must not depend on
  `ortho_config::Localizer`;
- the two-phase locale story (a best-effort `FluentLocalizer` is built before
  CLI parsing; the authoritative `FluentLanguageLoader` is built after).

Update `docs/spycatcher-harness-design.md` only where the implementation
resolves an existing ambiguity. The "CLI shape" and "Localization architecture"
sections should now reference the project-owned `LocalizeCmd` and the
catalogue-backed `cli-*`/`clap-error-*` IDs. Where the design references
`Command::localize` as an OrthoConfig API, clarify that the project owns the
extension trait.

The diagnostic environment switch introduced in Milestone 4 is documented in
`docs/adr/2026-06-04-cli-localization-disable-switch.md` because it is a
user-visible configuration knob outside the existing
`[cmds.<subcommand>.localization]` shape.

Go/no-go: `make markdownlint` and `make nixie` pass on the updated docs.
Documentation describes the implemented behaviour and does not overpromise
multilingual support.

Run:

```bash
make fmt 2>&1 | tee /tmp/fmt-1-4-3-docs.out
make markdownlint 2>&1 | tee /tmp/markdownlint-1-4-3-docs.out
make nixie 2>&1 | tee /tmp/nixie-1-4-3-docs.out
```

Run `coderabbit review --agent` and clear concerns. Commit after documentation
gates pass.

### Milestone 6: final validation, roadmap update, and PR refresh

Run the full gates sequentially, through `tee` logs:

```bash
make check-fmt 2>&1 | tee /tmp/check-fmt-1-4-3.out
make lint 2>&1 | tee /tmp/lint-1-4-3.out
make test 2>&1 | tee /tmp/test-1-4-3.out
make markdownlint 2>&1 | tee /tmp/markdownlint-1-4-3.out
make nixie 2>&1 | tee /tmp/nixie-1-4-3.out
```

If all gates pass and CodeRabbit has no open concerns, update `docs/roadmap.md`
to mark item `1.4.3` and its success criteria done. Update this ExecPlan's
`Progress`, `Surprises & Discoveries`, `Decision Log`, and
`Outcomes & Retrospective` with final evidence and any deviations from the plan.

Commit the roadmap and final plan updates as the final implementation commit.
Push the branch and refresh the draft PR description so reviewers can see the
implementation status, validation logs, and roadmap completion.

## Validation strategy

The minimum accepted validation for the implementation is:

- new `rstest` unit tests prove the `LocalizeCmd` extension trait, the
  `try_parse_localized_from_iter` helper, and the `build_cli_localizer`
  fallback policy;
- new `insta` snapshots pin localized root help, version, missing-subcommand,
  unknown-argument, and invalid-value text, plus binary help, version, and
  unknown-argument output;
- the Decision Log records why no additional `rstest-bdd` scenarios are added
  for this surface;
- extended `tests/binary_localization_e2e.rs` proves the compiled binary
  emits localized help and parse errors through the real OS process boundary;
- a `proptest` strategy proves random BCP 47 language identifiers do not
  panic and always resolve to either a `FluentLocalizer` or a `NoOpLocalizer`;
- existing harness startup, locale precedence, record-mode, cassette,
  matching, and doctest suites still pass;
- documentation gates pass;
- `coderabbit review --agent` has been run after each major milestone and
  all concerns are resolved.

Kani and Verus are not in scope. The task introduces a small deterministic
parsing and fallback policy, not a state-machine or symbolic invariant. The
`proptest` coverage for language identifier behaviour is sufficient for the
bounded randomness involved.

## Acceptance criteria

The work is complete only when:

- CLI help output uses the project-owned `Command::localize(&localizer)`
  (i.e. `LocalizeCmd::localize`) with a Fluent localizer implementation backed
  by `i18n/en-US/spycatcher-harness.ftl`;
- `clap` parsing failures are rendered via
  `localize_clap_error_with_command(..)` and the bundled `clap-error-*` IDs
  produce the localized text;
- the binary falls back to `ortho_config::NoOpLocalizer` when the Fluent
  localizer cannot be constructed, and the fallback is proved by tests;
- `docs/users-guide.md`, `docs/developers-guide.md`, and
  `docs/spycatcher-harness-design.md` accurately describe the implemented
  behaviour and the project-owned `LocalizeCmd` trait;
- `docs/roadmap.md` marks roadmap item `1.4.3` done;
- all full validation gates pass;
- CodeRabbit has no unresolved concerns; and
- the branch has been pushed to
  `origin/1-4-3-localize-help-and-parse-errors-via-ortho-config` with a draft
  PR that references this ExecPlan in its summary.

## Idempotence and recovery

Each milestone above is committed as a discrete change. Reverting any milestone
in reverse order returns the repository to a fully buildable state. Snapshot
acceptance is the only step that requires interactive review; rerunning
`cargo insta accept` after a deliberate UI text update is safe and reproducible.

If a milestone fails its quality gate, do not advance to the next milestone.
Investigate, fix at root cause, and re-run the gate. Do not silence lints, do
not bypass hooks, and do not edit snapshots by hand without rerunning the tests
they correspond to.

## Interfaces and dependencies

By the end of Milestone 4 the following symbols must exist:

- `crate::cli::localization::LocalizeCmd` — extension trait on
  `clap::Command` with `fn localize(self, localizer: &dyn Localizer) -> Self`.
- `crate::cli::localization::try_parse_localized_from_iter` — generic helper
  that parses any `Parser + CommandFactory` from an iterator of OS strings,
  using `&dyn Localizer` for help, version, and parse-error rendering.
- `crate::cli::localizer::build_cli_localizer` — constructs a
  `FluentLocalizer` from the embedded en-US Fluent text, falling back to
  `NoOpLocalizer` on failure.

  ```rust,no_run
  fn build_cli_localizer(locale: LanguageIdentifier) -> Arc<dyn Localizer>
  ```

- `crate::cli::localizer::early_locale_plan() -> LanguageIdentifier` —
  best-effort locale selection used before CLI parsing.
- `crate::cli::load_subcommand_config_with_localizer` — new public entry point
  that takes a localizer. The existing `load_subcommand_config()` continues to
  work and internally constructs a `NoOpLocalizer` for callers that have not
  migrated.

  ```rust,no_run
  fn load_subcommand_config_with_localizer(
      localizer: &dyn Localizer,
  ) -> Result<HarnessConfig, CliConfigError>
  ```

By the end of Milestone 3 the FTL catalogue `i18n/en-US/spycatcher-harness.ftl`
must contain at minimum:

- `cli-about`, `cli-long-about`, `cli-usage`, `cli-merge-help`;
- `cli-record-about`, `cli-replay-about`, `cli-verify-about`;
- the `clap-error-*` IDs listed in Milestone 3.

External crates used at runtime (no new direct dependencies are introduced; all
are already in `Cargo.toml`):

- `ortho_config` for `Localizer`, `FluentLocalizer`, `NoOpLocalizer`,
  `LocalizationArgs`, and `localize_clap_error_with_command`.
- `i18n-embed` for the existing `FluentLanguageLoader` and `RustEmbed`-backed
  `HarnessLocalizations`.
- `clap` for `Command`, `CommandFactory`, `Parser`, and `ErrorKind`.
- `unic-langid` (re-exported by both `i18n-embed` and `ortho_config`) for
  `LanguageIdentifier`.
- `tracing` for diagnostic warnings on fallback paths.
