# Architectural decision record (ADR) 0001: Add a diagnostic switch for CLI localization

## Status

Accepted on 2026-06-04: add a diagnostic environment switch that forces stock
CLI help and parse-error output. See [Amendments](#amendments) for the later
filename and status-summary update, which did not change the decision.

## Date

2026-11-03

## Context and problem statement

The `spycatcher-harness` binary now localizes `clap` help, version, and
parse-error text before full command-line parsing completes. That early phase
uses a best-effort locale from environment variables and falls back to stock
`clap` text through `ortho_config::NoOpLocalizer` when the Fluent-backed
localizer cannot be built.

Tests need a deterministic way to exercise the process-level fallback path.
Operators also need a simple way to rule out localized CLI assets while
diagnosing startup problems.

## Decision

Introduce `SPYCATCHER_HARNESS_DISABLE_LOCALIZATION=1`. When this environment
variable is set to an explicit truthy value, the binary bypasses Fluent-backed
CLI localization and uses `NoOpLocalizer` for help, version, and parse-error
rendering.

The switch affects only CLI parser output. Harness library errors still use the
merged `locale` and `fallback_locale` configuration after parsing.

## Consequences

The fallback path is testable through the compiled binary without corrupting or
removing embedded assets. The environment variable is intentionally outside the
layered subcommand configuration model because it is a diagnostic escape hatch,
not an application preference.

Future non-English CLI catalogues must continue to pass the same fallback
tests, so this switch remains a reliable way to recover stock `clap`
diagnostics.

## Amendments

### 2026-11-03: canonical filename and status summary

This decision was published as ADR 001 at
`docs/adr/2026-06-04-cli-localization-disable-switch.md`. Its canonical path
now uses ADR 0001 to follow the four-digit filename convention; the ordinal
remains 1. The expanded Status summary records the original acceptance date and
decision. This is a documentation-format update, not a change to the accepted
decision.
