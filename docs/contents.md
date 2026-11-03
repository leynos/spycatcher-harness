# Documentation contents

[Documentation contents](contents.md) is the index for the project's
documentation set.

## Project guidance

- [Developer's guide](developers-guide.md) explains contributor workflows and
  implementation conventions.
- [Documentation style guide](documentation-style-guide.md) defines the
  project's documentation conventions and standard document formats.
- [Scripting standards](scripting-standards.md) records repository rules for
  scripts and automation.

## User and technical guides

- [User's guide](users-guide.md) explains the harness's user-facing workflows.
- [OrthoConfig user's guide](ortho-config-users-guide.md) documents the
  configuration library integration.
- [Replay matching guide](replay-matching-guide.md) explains replay matching
  behaviour and diagnostics.
- [Testing guide](testing-guide.md) describes the repository's testing
  strategy and commands.
- [Rust testing with rstest fixtures](rust-testing-with-rstest-fixtures.md)
  covers fixture-based Rust tests.
- [rstest-bdd user's guide](rstest-bdd-users-guide.md) documents behaviour-
  driven testing practices used by the project.
- [Rust doctest dry guide](rust-doctest-dry-guide.md) explains the doctest
  approach used by the Rust codebase.
- [Reliable testing through dependency injection](reliable-testing-in-rust-via-dependency-injection.md)
  describes testable boundaries and dependency injection.
- [Localizable Rust libraries with Fluent](localizable-rust-libraries-with-fluent.md)
  explains loader-injected localization for Rust libraries.
- [Complexity antipatterns and refactoring strategies](complexity-antipatterns-and-refactoring-strategies.md)
  catalogues complexity signals and practical refactoring approaches.
- [OrthoConfig v0.9.0 migration guide](ortho-config-v0-9-0-migration-guide.md)
  covers the migration to OrthoConfig v0.9.0.
- [OrthoConfig v0.10.0 migration guide](ortho-config-v0-10-0-migration-guide.md)
  covers the migration to OrthoConfig v0.10.0.

## Design, decisions, and delivery

- [Spycatcher harness design](spycatcher-harness-design.md) explains the
  system architecture and its evolution.
- [Spycatcher harness roadmap](roadmap.md) tracks delivery phases, tasks, and
  acceptance criteria.
- [ADR 0001: Diagnostic switch for CLI localization](adrs/adr-0001-add-a-diagnostic-switch-for-cli-localization.md)
  records the CLI localization fallback decision.
- [ADR 0002: Rust build standard](adrs/adr-0002-adopt-the-rust-build-standard.md)
  records the repository's Rust build configuration decision.
- [Debugging plan for fake-program text-busy failures](debugging/debugging-plan-2026-10-14-fake-program-text-busy.md)
  captures investigation notes for a recurring test failure.

## Execution plans

The [execution plans](execplans/) describe bounded delivery work and its
validation evidence.

- [1.1.1: Library and binary crate skeleton](execplans/1-1-1-library-and-binary-crate-skeleton-for-harness.md)
- [1.1.2: Layered configuration loading](execplans/1-1-2-layered-configuration-loading-for-subcommands.md)
- [1.2.1: Cassette schema versioning](execplans/1-2-1-cassette-schema-versioning.md)
- [1.2.2: Canonical request generation and stable hashing](execplans/1-2-2-implement-canonical-request-generation-and-stable-hashing.md)
- [1.2.3: Sequential and keyed replay matching](execplans/1-2-3-deliver-strict-sequential-and-keyed-replay-matching-modes.md)
- [1.3.1: Chat completions record-mode proxying](execplans/1-3-1-chat-completions-record-mode-proxying.md)
- [1.3.2: Non-stream replay for chat completions](execplans/1-3-2-non-stream-replay-for-post-chat-completions.md)
- [1.4.1: Fluent resources and loader-injected messages](execplans/1-4-1-fluent-resources-and-loader-injected-message-rendering.md)
- [1.4.2: Localization configuration layering](execplans/1-4-2-localization-configuration-layering.md)
- [1.4.3: Localized help and parse errors](execplans/1-4-3-localize-help-and-parse-errors-via-ortho-config.md)
- [2.1.1: SSE parsing and recording](execplans/2-1-1-sse-parser-and-recorder-for-open-ai-streams.md)
- [2.1.2: OpenRouter comment-frame replay](execplans/2-1-2-preserve-and-replay-open-router-comment-frames.md)
- [Adopt OrthoConfig v0.8.0](execplans/adopt-ortho-config-v0-8-0.md)
- [Migrate OrthoConfig v0.8.0 to v0.10.0](execplans/issue-211-migrate-from-ortho-config-v0-8-0-to-v0-10-0.md)
