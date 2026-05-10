# Embed Fluent resources and inject message rendering

This ExecPlan (execution plan) is a living document. The sections
`Constraints`, `Tolerances`, `Risks`, `Progress`, `Surprises & Discoveries`,
`Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work
proceeds.

Status: COMPLETE

## Purpose / big picture

Roadmap item 1.4.1 establishes the library side of localization for the
Spycatcher harness. After this work, the `spycatcher_harness` library owns and
embeds its Fluent Translation List (FTL) resources, exposes public rendering
helpers that accept an application-provided `FluentLanguageLoader`, and keeps
locale negotiation and loader construction outside the library.

The visible outcome is deliberately modest. Library users can configure one
Fluent loader in their application, load the library's embedded resources into
that loader, and pass the loader to `spycatcher_harness` rendering helpers to
obtain localised diagnostics. The library must not create a process-global
language loader, read the process locale, or make the command-line interface
(CLI) responsible for library-owned message IDs.

This plan records the completed implementation, review follow-ups, validation
results, and remaining context needed to understand roadmap item 1.4.1.

## Constraints

Hard invariants that must hold throughout implementation. Violation requires
escalation, not workarounds.

- This plan implements only roadmap item 1.4.1. It must not implement 1.4.2
  locale configuration layering or 1.4.3 localised CLI help and parse errors,
  except where a narrow compatibility seam is needed for 1.4.1.
- The roadmap item in `docs/roadmap.md` remains unchecked while this plan is
  only drafted. Mark 1.4.1 done only after the feature implementation is
  complete and gated.
- The library must not construct a process-global `FluentLanguageLoader` or
  perform locale detection. Loader creation belongs to the application or tests.
- Rendering APIs must accept `&FluentLanguageLoader` supplied by the caller.
- Domain-facing APIs must remain semantic. Continue returning typed values such
  as `HarnessError`; do not replace them with preformatted strings.
- Opaque errors such as `eyre::Report` remain at application boundaries only.
  Library public APIs must continue returning typed `HarnessError` values.
- Use crate-local modules before adding crates or broad abstractions. This is a
  single-package library-plus-binary crate, not a Cargo workspace split.
- All dependencies in `Cargo.toml` must use caret-compatible requirements.
  Wildcard or open-ended requirements are forbidden.
- Every module must start with `//!` documentation, every public item must have
  Rustdoc, and no source file may exceed 400 lines.
- Comments and documentation must use en-GB Oxford spelling, except when
  quoting external API names.
- Use `rstest` for unit tests and `rstest-bdd` for behavioural tests where the
  behaviour is externally observable.
- Do not run format, lint, or test commands in parallel. Use Makefile targets
  and capture long outputs with `tee` logs under `/tmp`.
- Use the relevant skills while implementing: `leta` for code navigation,
  `rust-router` to choose Rust-specific guidance, `rust-types-and-apis` for
  public rendering signatures, `arch-crate-design` for crate/module boundaries,
  `rust-errors` if error shape changes, and `hexagonal-architecture` to keep
  rendering ports separate from application loader construction.

## Tolerances

Thresholds that trigger escalation when breached.

- Scope: if implementation requires touching more than 12 files or more than
  800 net lines, stop and ask for approval before continuing.
- Public API: if `start_harness(cfg)` must change signature, stop and present
  alternatives. Prefer adding explicit rendering helpers rather than changing
  harness startup unless implementation proves startup output needs rendering
  now.
- Dependencies: adding `i18n-embed` and `rust-embed` is expected. If another
  runtime dependency is needed, stop and explain why.
- Behavioural scope: if localised CLI help, parse errors, or OrthoConfig
  locale precedence must be implemented to satisfy tests, stop. Those belong to
  1.4.2 and 1.4.3.
- Test iterations: if `make test`, `make lint`, or `make check-fmt` still fail
  after three fix-up passes, stop and document the failure.
- Ambiguity: if multiple valid message ID naming schemes or fallback policies
  would create different public contracts, stop and request a decision.

## Risks

- Risk: `i18n-embed` exposes several setup patterns, and the wrong one could
  make the library construct its own loader. Severity: high Likelihood: medium
  Mitigation: follow `docs/localizable-rust-libraries-with-fluent.md`: embed
  assets in the library, expose an asset type and rendering helpers, but leave
  loader construction and language selection to the caller.

- Risk: changing `start_harness(cfg)` to accept a loader would create broad
  API churn and mix startup with rendering. Severity: medium Likelihood: medium
  Mitigation: first implement standalone rendering APIs in `src/i18n.rs`.
  Escalate before changing lifecycle entry points.

- Risk: current error strings are embedded in `thiserror` display attributes
  and tests assert those English strings. Severity: medium Likelihood: high
  Mitigation: keep `Display` as the non-localised semantic fallback. Add
  separate localised rendering APIs so existing error matching remains stable.

- Risk: the record-mode HTTP error body in `src/server/record_handler.rs` is
  externally observable and currently uses hard-coded text. Severity: medium
  Likelihood: medium Mitigation: do not localize server responses in this
  milestone unless a caller-injected loader can be threaded without public
  startup churn. Document deferred server injection if necessary.

- Risk: dependency versions or feature flags may differ from examples in the
  design guide. Severity: low Likelihood: medium Mitigation: inspect current
  crate documentation while implementing if an API mismatch appears, then
  record the exact decision in this plan and the design document.

## Progress

- [x] (2026-05-08 11:23Z) Read `AGENTS.md` and loaded required planning
  skills: `execplans`, `leta`, `rust-router`, `hexagonal-architecture`,
  `rust-types-and-apis`, `arch-crate-design`, `commit-message`, and
  `pr-creation`.
- [x] (2026-05-08 11:24Z) Renamed branch from
  `feat/fluent-loader-execplan` to
  `1-4-1-fluent-resources-and-loader-injected-message-rendering`.
- [x] (2026-05-08 11:24Z) Created context pack `pk_4hga75jb` for Wyvern
  planning collaboration.
- [x] (2026-05-08 11:25Z) Used a Wyvern agent team to inspect roadmap/design
  intent, source layout, and testing/documentation guidance.
- [x] (2026-05-08 11:26Z) Drafted this pre-implementation ExecPlan.
- [x] (2026-05-08 12:18Z) Received explicit user approval to implement this
  plan and moved it to execution.
- [x] (2026-05-08 12:31Z) Added direct `i18n-embed` and `rust-embed`
  dependencies, using versions already present in `Cargo.lock`.
- [x] (2026-05-08 12:31Z) Added embedded English library FTL assets under
  `i18n/en-US/spycatcher-harness.ftl`.
- [x] (2026-05-08 12:32Z) Implemented `HarnessLocalizations` and
  `localize_harness_error(&FluentLanguageLoader, &HarnessError)` in
  `src/i18n.rs`.
- [x] (2026-05-08 12:40Z) Added `rstest` unit cases for every current
  `HarnessError` variant and an unhappy-path unloaded-loader fallback case.
- [x] (2026-05-08 12:40Z) Added public API doctest examples for
  `HarnessLocalizations` and `localize_harness_error`.
- [x] (2026-05-08 12:40Z) Recorded that `rstest-bdd` is not applicable for
  this milestone because no externally observable workflow changes.
- [x] (2026-05-08 12:42Z) Updated `docs/spycatcher-harness-design.md`,
  `docs/users-guide.md`, and `docs/developers-guide.md` with API, asset path,
  and loader-injection guidance.
- [x] (2026-05-08 12:51Z) Ran and passed `make fmt`, `make markdownlint`,
  `make nixie`, `make check-fmt`, `make lint`, and `make test`.
- [x] (2026-05-08 12:52Z) Marked roadmap item 1.4.1 and its success criteria
  done after validation passed.
- [x] (2026-05-08 12:56Z) Prepared the implementation commit after all gates
  passed.
- [x] Mark roadmap item 1.4.1 done after the feature implementation is
  complete.
- [x] (2026-05-10 09:12Z) Addressed review follow-ups for structured
  missing-message detection, I/O source rendering, narrower Fluent isolation
  normalization, mapping simplification, and `rstest` fixtures.
- [x] (2026-05-10 09:24Z) Revalidated review follow-ups with
  `cargo test i18n --all-targets --all-features`, `make fmt`,
  `make markdownlint`, `make nixie`, `make check-fmt`, `make lint`, and
  `make test`.
- [x] (2026-05-10 02:05Z) Addressed follow-up review findings by loading the
  explicit fallback locale in tests and examples, using
  `FluentLanguageLoader::has` for missing-message checks, and syncing the
  dependency snippet with the implemented versions.
- [x] (2026-05-10 02:36Z) Addressed CI review findings by replacing fallible
  fixture `expect` calls with `Result`/`?`, snapshotting localised text output
  with `insta`, adding `proptest` coverage for Fluent isolation mark stripping,
  and correcting the completed ExecPlan opening text.
- [x] (2026-05-10 02:49Z) Revalidated the CI follow-up with focused i18n
  tests, `make check-fmt`, `make markdownlint`, `make nixie`, `make lint`, and
  `make test`. `make fmt` still reports unrelated repository-wide MD013
  findings during its markdown fix step, so unrelated formatter churn was
  restored and the explicit formatting and Markdown gates were used.
- [x] (2026-05-11 08:45Z) Addressed review follow-ups by using en-GB spelling
  in Rustdoc and ExecPlan references, defining command-line interface (CLI) on
  first use, and splitting the localised error snapshot cases into focused
  tests without an inline multi-branch dispatcher.

## Surprises & discoveries

- Observation: `src/i18n.rs` already exists but is only a placeholder module.
  Evidence: it contains module documentation and no items. Impact:
  implementation can stay focused in this module instead of creating a new
  localization package.

- Observation: `HarnessConfig` already includes `LocalizationConfig`, but the
  CLI and binary do not yet consume those fields for localization. Evidence:
  `src/config.rs` defines `LocalizationConfig { locale, fallback_locale }`;
  `src/cli.rs` loads command config without localiser wiring. Impact: this plan
  must avoid pulling 1.4.2 configuration layering into 1.4.1.

- Observation: the current crate is a single package with both `[lib]` and
  `[[bin]]`, not a multi-crate workspace. Evidence: the root `Cargo.toml`
  defines `spycatcher_harness` and `spycatcher-harness`. Impact: embedded
  resources and rendering helpers belong in the existing library crate.

- Observation: `Cargo.lock` already contained `i18n-embed 0.16.0` and
  `rust-embed 8.11.0` through `rstest-bdd`, so adding them as direct runtime
  dependencies did not require selecting newer crate versions. Impact: this
  keeps dependency movement minimal while making the library's runtime
  localization contract explicit.

- Observation: `FluentLanguageLoader` wraps interpolated variables in Unicode
  bidirectional isolation marks by default. Evidence: focused
  `cargo test i18n --all-targets --all-features` failed with strings containing
  `U+2068` and `U+2069` around dynamic fields. Impact: the rendering helper
  normalizes those marks out for harness diagnostics so localised output
  remains suitable for assertions, logs, and command-line surfaces without
  mutating the caller-owned loader.

- Observation: `FluentLanguageLoader` exposes `has(message_id)`, which checks
  whether a message is available in any loaded language bundle. Evidence: the
  `i18n-embed 0.16.0` API provides `FluentLanguageLoader::has`. Impact: missing
  harness messages can be detected structurally before rendering instead of
  comparing against the loader's English fallback string.

## Decision log

- Decision: keep this plan pre-implementation and require explicit approval
  before coding. Rationale: the user explicitly stated that the plan must be
  approved before implementation. The roadmap checkbox therefore remains
  unchecked in this planning branch. Date/Author: 2026-05-08 / agent

- Decision: model 1.4.1 around a narrow `src/i18n.rs` public API rather than
  changing `start_harness(cfg)`. Rationale: the success criteria require
  loader-injected message rendering, not startup-time localization. A narrow
  rendering API protects the existing public lifecycle contract and leaves
  binary loader construction for 1.4.2. Date/Author: 2026-05-08 / agent

- Decision: keep `HarnessError` display messages as non-localised fallbacks and
  add localised rendering as a separate operation. Rationale: typed errors are
  part of the design contract. Replacing their `Display` implementation with
  loader-dependent behaviour is impossible without global state and would break
  existing tests. Date/Author: 2026-05-08 / agent

- Decision: treat server HTTP error-body localization as conditional in this
  milestone. Rationale: localizing those bodies requires loader propagation
  through server startup. If that requires changing `start_harness(cfg)`, it
  breaches the public API tolerance and must be approved first. Date/Author:
  2026-05-08 / agent

- Decision: begin implementation from the approved plan without changing the
  public `start_harness(cfg)` signature. Rationale: the user's 2026-05-08
  approval explicitly asks to proceed with the planned functionality, and the
  plan's narrow library rendering API remains within tolerance. Date/Author:
  2026-05-08 / agent

- Decision: use the existing lockfile versions `i18n-embed = "0.16.0"` and
  `rust-embed = "8.11.0"` rather than the older example versions in the plan.
  Rationale: both crates are already resolved by the repository through
  `rstest-bdd`; direct dependencies should expose the runtime contract without
  unnecessary version churn. Date/Author: 2026-05-08 / agent

- Decision: strip Fluent bidirectional isolation marks from strings returned
  by `localize_harness_error`. Rationale: disabling isolation would require
  mutating the application-owned loader, while leaving marks in harness
  diagnostics would make logs and test assertions surprising. The helper keeps
  the injected-loader boundary intact and normalizes only its own returned
  diagnostic string. Date/Author: 2026-05-08 / agent

- Decision: replace fallback-string comparison with loaded-message detection.
  Rationale: comparing against `i18n-embed`'s `"No localization for id: ..."`
  string couples harness behaviour to upstream wording. Checking loaded Fluent
  message IDs through `FluentLanguageLoader::has` is a structured signal from
  the injected loader. Date/Author: 2026-05-10 / agent

- Decision: examples and tests pass the requested fallback locale explicitly to
  `i18n_embed::select`. Rationale: using the same explicit locale value for
  loader construction and resource selection avoids relying on
  `current_languages()` state before resources have been loaded, and gives
  callers a copyable setup that reliably loads bundled Fluent messages.
  Date/Author: 2026-05-10 / agent

- Decision: normalize only Fluent isolation pairs surrounding argument values,
  not every isolation mark in the rendered string. Rationale: Fluent inserts
  `U+2068` and `U+2069` around placeables; replacing only
  `LRI + argument_value + PDI` preserves intentional isolation marks present in
  localised text or user-provided content. Date/Author: 2026-05-10 / agent

- Decision: include `HarnessError::Io` source details in localised rendering.
  Rationale: the localised message should not drop diagnostic context that is
  available in the underlying typed error. Date/Author: 2026-05-10 / agent

- Decision: do not add a new `rstest-bdd` scenario for 1.4.1. Rationale: this
  implementation exposes a public library helper and embedded resources only;
  it does not change HTTP responses, CLI output, persistence, network
  contracts, or other system-level behaviour. Unit tests plus doctests cover
  the observable library contract. Date/Author: 2026-05-08 / agent

## Outcomes & retrospective

Roadmap item 1.4.1 is implemented. The library now owns embedded Fluent
resources at `i18n/en-US/spycatcher-harness.ftl`, exposes
`HarnessLocalizations` for applications to load into their own
`FluentLanguageLoader`, and exposes
`localize_harness_error(&FluentLanguageLoader, &HarnessError)` for localised
diagnostic rendering. The implementation does not create a process-global
loader, does not perform locale detection, and does not change
`start_harness(cfg)`.

The implementation is validated by `rstest` unit cases covering every current
`HarnessError` variant, an unhappy-path unloaded-loader fallback test, public
API doctests, Markdown linting, diagram validation, Rust formatting checks,
Clippy/Whitaker linting, and the full test suite.

No `rstest-bdd` scenario was added because this milestone did not change an
externally observable workflow such as HTTP responses, CLI output, persistence,
or network contracts. The externally observable contract for this milestone is
the public library rendering API, covered by unit tests and doctests.

## Context and orientation

The repository is a single Rust package. `Cargo.toml` defines a library crate
named `spycatcher_harness` at `src/lib.rs` and a binary named
`spycatcher-harness` at `src/bin/spycatcher_harness.rs`.

The current public lifecycle entry point is:

```rust
pub async fn start_harness(cfg: HarnessConfig) -> HarnessResult<RunningHarness>
```

The central typed error enum is `HarnessError` in `src/error.rs`. It currently
uses `thiserror` display strings for English fallback text. Existing tests
assert these strings, so implementation must not remove or destabilize them
without an explicit decision.

`src/config.rs` already defines:

```rust
pub struct LocalizationConfig {
    pub locale: Option<String>,
    pub fallback_locale: String,
}
```

Those fields exist for later application-level localization work. Roadmap item
1.4.1 is narrower: it adds library-owned resources and rendering helpers that
consume a caller-provided `FluentLanguageLoader`.

`src/i18n.rs` is currently the natural home for this feature. It is public via
`pub mod i18n;` in `src/lib.rs`, but it contains no types or functions yet.

The relevant design documents are:

- `docs/roadmap.md`, item 1.4.1, for success criteria.
- `docs/spycatcher-harness-design.md`, "Localization architecture", for the
  division between library and application responsibilities.
- `docs/spycatcher-harness-design.md`, "Core traits and types", for the
  intended `localize_harness_error(loader, error)` shape.
- `docs/localizable-rust-libraries-with-fluent.md`, for the library asset
  embedding and dependency-injection pattern.
- `docs/rust-testing-with-rstest-fixtures.md`, for unit-test fixtures.
- `docs/rstest-bdd-users-guide.md`, for behavioural scenario conventions.
- `docs/reliable-testing-in-rust-via-dependency-injection.md`, for avoiding
  global state in tests and production code.
- `docs/rust-doctest-dry-guide.md`, for public API examples.
- `docs/complexity-antipatterns-and-refactoring-strategies.md`, for keeping
  helper functions small and responsibility-focused.
- `docs/ortho-config-users-guide.md`, for later application-localiser
  integration. Use it as context, but do not implement 1.4.2 or 1.4.3 here.

## Plan of work

Stage A is approval. Review this plan, adjust tolerances or API decisions if
needed, and do not write implementation code until the plan is explicitly
approved.

Stage B adds dependencies and resources. In `Cargo.toml`, add direct runtime
dependencies on `i18n-embed` and `rust-embed` using caret requirements and the
minimal feature set needed for Fluent. Create a library-owned asset directory
such as `i18n/en-US/spycatcher-harness.ftl`, unless current `rust-embed`
constraints require a different crate-relative path. Add baseline English
messages for every `HarnessError` variant, using stable message IDs such as
`harness-error-invalid-config` and arguments for dynamic fields.

Stage C implements the library localization API in `src/i18n.rs`. Define an
embedded asset type, for example `HarnessLocalizations`, using `RustEmbed`.
Expose the assets so applications can load them into their own loader. Add a
public rendering helper with this shape unless implementation discovers a
better crate idiom:

```rust
pub fn localize_harness_error(
    loader: &FluentLanguageLoader,
    error: &HarnessError,
) -> String
```

If a lower-level helper is useful, add a public or crate-private function that
renders a typed message ID plus arguments through the provided loader. Missing
translations should fall back to `error.to_string()` for error rendering, never
panic.

Stage D adds tests. Unit tests in `src/i18n.rs` should construct a test
`FluentLanguageLoader`, load the embedded English resources, and verify happy
paths for each `HarnessError` variant. Add unhappy-path tests for missing
messages or unloaded resources, proving fallback behaviour. Use `rstest`
parameterized cases. Add doctests to public `i18n` APIs showing application
loader injection. Add a `rstest-bdd` feature only if the implementation changes
an externally observable workflow, such as HTTP error responses or binary
output. If rendering remains a library helper only, unit tests plus doctests
are enough.

Stage E updates documentation. In `docs/spycatcher-harness-design.md`, record
the concrete embedded asset path, public rendering helper names, message ID
scheme, and the decision that library rendering is loader-injected. In
`docs/users-guide.md`, add a short library API section showing how application
code loads the embedded assets into its `FluentLanguageLoader` and calls the
rendering helper. If internal conventions need more detail, update
`docs/developers-guide.md` or the closest component architecture section rather
than burying conventions in tests.

Stage F validates and commits. Run formatting and gates sequentially. Fix any
issues without broad refactors. After all gates pass, commit the implementation
as a focused atomic change. Then review changed code for refactoring
opportunities; if refactoring is needed, make it in a separate gated commit.
Only after the feature implementation passes all gates, update
`docs/roadmap.md` to mark item 1.4.1 done and include that in the final
implementation commit or a small follow-up commit.

## Concrete steps

Run all commands from the repository root:

```sh
cd <repository-root>
```

Before implementation, confirm the branch:

```sh
git branch --show-current
```

Expected output:

```plaintext
1-4-1-fluent-resources-and-loader-injected-message-rendering
```

After approval, inspect the current API and resource module:

```sh
leta grep "localize|HarnessError|start_harness" -k function,enum
leta show HarnessError
leta show start_harness
```

Add dependencies in `Cargo.toml`:

```toml
i18n-embed = { version = "0.16.0", features = ["fluent-system"] }
rust-embed = "8.11.0"
```

Use the exact latest compatible versions already acceptable to the project only
after checking the local lockfile and crate API. Keep caret requirements.

Create embedded resources under the chosen library asset path. The first
English FTL file should contain message IDs for all current `HarnessError`
variants. A representative entry is:

```fluent
harness-error-invalid-config = invalid configuration: { $message }
```

Implement tests before or alongside the rendering helper. New tests should fail
before the helper exists and pass after implementation.

When code and docs are ready, run gates sequentially with logs:

```sh
make fmt 2>&1 | tee /tmp/fmt-spycatcher-harness-1-4-1-fluent-resources-and-loader-injected-message-rendering.out
make markdownlint 2>&1 | tee /tmp/markdownlint-spycatcher-harness-1-4-1-fluent-resources-and-loader-injected-message-rendering.out
make nixie 2>&1 | tee /tmp/nixie-spycatcher-harness-1-4-1-fluent-resources-and-loader-injected-message-rendering.out
make check-fmt 2>&1 | tee /tmp/check-fmt-spycatcher-harness-1-4-1-fluent-resources-and-loader-injected-message-rendering.out
make lint 2>&1 | tee /tmp/lint-spycatcher-harness-1-4-1-fluent-resources-and-loader-injected-message-rendering.out
make test 2>&1 | tee /tmp/test-spycatcher-harness-1-4-1-fluent-resources-and-loader-injected-message-rendering.out
```

If `make fmt` changes files, inspect the diff before continuing:

```sh
git diff --stat
git diff -- docs src Cargo.toml
```

When implementation is complete and gated, update the roadmap checkbox:

```markdown
- [x] 1.4.1. Embed library Fluent resources and expose loader-injected message
      rendering.
```

Then commit using a file-based commit message as required by the
`commit-message` skill.

## Validation and acceptance

Acceptance for the implementation is behaviour-based:

- A caller can create its own `FluentLanguageLoader`, load the library's
  embedded FTL resources, and call a `spycatcher_harness::i18n` rendering
  function without the library creating a loader.
- Each `HarnessError` variant renders through the injected loader with dynamic
  fields preserved.
- Missing or unloaded translations fall back deterministically to the existing
  non-localised error display string.
- Searching the source shows no process-global language loader or locale
  detection created by the library.
- Documentation explains how an application injects a loader and how this
  relates to later CLI localization work.
- Roadmap item 1.4.1 is marked done only after the implementation passes all
  gates.

Required automated checks:

- `make fmt` after documentation edits.
- `make markdownlint` after Markdown edits.
- `make nixie` after Markdown edits because design docs contain Mermaid.
- `make check-fmt`.
- `make lint`.
- `make test`.

Unit-test expectations:

- Add `rstest` cases for all current `HarnessError` variants.
- Add at least one unhappy-path case proving fallback when lookup cannot
  produce a localised message.
- Add an assertion that rendering accepts a caller-owned loader by constructing
  the loader in the test and passing it by reference.

Behavioural-test expectations:

- Add `rstest-bdd` only if implementation changes an externally observable
  workflow. Examples include localised HTTP error bodies or binary output.
- If no external workflow changes, document in the plan that BDD coverage is
  intentionally unnecessary for 1.4.1 because the observable contract is a
  public library helper covered by unit tests and doctests.

Property or proof expectations:

- Property tests and bounded model checking are not required for the narrow
  rendering helper unless the implementation introduces a non-trivial invariant
  over message IDs, locales, fallback ordering, or state transitions.
- If such an invariant is introduced, add a bounded `proptest` suite that
  checks fallback totality over generated message IDs and argument maps.
- Formal proof is not required unless new business axioms are introduced.

## Idempotence and recovery

Most steps are additive and safe to repeat. Re-running `make fmt`, lint, and
tests is safe. Re-running dependency edits should be done by inspecting
`Cargo.toml` first so duplicate dependency entries are not created.

If the chosen embedded asset path does not work with `rust-embed`, move the
assets once, update all references in the plan and documentation, and record
the reason in `Decision Log`.

If a validation command fails because a dependency download or cache access is
blocked by the sandbox, rerun the same command with elevated permissions using
the command-execution tool and keep the log path unchanged.

If implementation would require changing `start_harness(cfg)`, stop before
making that change. Record the proposed signature and alternatives in
`Decision Log`, then ask for approval.

## Artifacts and notes

Wyvern agent team findings used for this draft:

- Roadmap and design pass: item 1.4.1 requires embedded library FTL assets,
  loader-injected rendering APIs, and no process-global loaders.
- Source-layout pass: `src/i18n.rs` is a placeholder, `HarnessConfig` already
  includes localization fields, and no checked-in FTL assets exist.
- Testing-doc pass: use `rstest` unit tests, `rstest-bdd` only for externally
  observable behaviour, doctests for public API examples, and Makefile gates.

The shared context pack for the planning activity is `pk_4hga75jb`.

## Interfaces and dependencies

The implementation should leave these public concepts available:

```rust
pub struct HarnessLocalizations;

pub fn localize_harness_error(
    loader: &FluentLanguageLoader,
    error: &HarnessError,
) -> String;
```

The final exact names may change during implementation if the Fluent crate API
or existing project conventions make another name clearer. Any name change must
be recorded in `Decision Log` and reflected in `docs/users-guide.md` and
`docs/spycatcher-harness-design.md`.

The implementation should use:

- `i18n_embed::fluent::FluentLanguageLoader` as the injected loader type.
- `rust_embed::RustEmbed` to embed library-owned FTL assets.
- `rstest` for unit tests.
- `rstest-bdd` only when externally observable behaviour changes.

Revision note: Initial draft created from roadmap item 1.4.1, design
references, current source layout, testing documentation, and Wyvern agent team
findings. This draft authorizes no implementation until explicitly approved.
