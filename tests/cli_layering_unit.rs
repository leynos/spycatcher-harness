//! Unit tests for CLI layered configuration loading.
//!
//! This suite exercises [`spycatcher_harness::cli::load_subcommand_config_from_iter`]
//! in isolation using explicit file and environment sources.
//!
//! Modules:
//!
//! - [`cassette_precedence`] — cassette-name precedence (CLI > env > config file > default) for
//!   `replay`, and record-only isolation.
//! - [`localization`] — `locale` and `fallback_locale` precedence across all three subcommands,
//!   plus property-based BCP 47 acceptance.
//! - [`error_reporting`] — visible failures for invalid values, malformed discovered files, and
//!   unreadable discovered files.
//! - [`namespaces`] — nested `[cmds.<subcommand>]` tables for record and verify.
//!
//! Related suites:
//! - `tests/harness_cli_layering_bdd.rs` — BDD scenarios for the same surface.
//! - `src/bin/spycatcher_harness.rs` (inline tests) — startup locale-plan and loader construction
//!   unit tests.

#[path = "cli_layering/cases.rs"]
mod cases;
#[path = "cli_layering/cassette_precedence.rs"]
mod cassette_precedence;
#[path = "cli_layering/error_reporting.rs"]
mod error_reporting;
#[path = "cli_layering/localization.rs"]
mod localization;
#[path = "cli_layering/namespaces.rs"]
mod namespaces;
#[path = "cli_layering/support.rs"]
mod support;
#[cfg(unix)]
#[path = "cli_layering/unreadable_file.rs"]
mod unreadable_file;
