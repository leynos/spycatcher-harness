# Add a diagnostic switch for CLI localization

Status: accepted

## Context

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
variable is present, the binary bypasses Fluent-backed CLI localization and uses
`NoOpLocalizer` for help, version, and parse-error rendering.

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
