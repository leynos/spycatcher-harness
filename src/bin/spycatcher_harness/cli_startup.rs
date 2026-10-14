//! CLI parsing and process-exit orchestration using the selected CLI localizer.
//!
//! Output sinks and subscriber ownership remain in the separate process-output
//! child; this module keeps the parse span alive through result handling.

use std::sync::Arc;

use eyre::WrapErr;
use ortho_config::{Localizer, NoOpLocalizer};
use spycatcher_harness::HarnessConfig;
use spycatcher_harness::cli::localizer::{
    build_cli_localizer, early_locale_plan, is_cli_localization_disabled,
};
use spycatcher_harness::cli::{CliConfigError, load_subcommand_config_with_localizer};

use super::process_output::{write_display_output, write_error_output};

/// Loads merged configuration while retaining a span through result handling.
/// For example, `--help` writes localized stdout and exits with status zero;
/// invalid arguments write stderr and exit with status two.
///
/// # Errors
/// Returns configuration or output-write failures with their existing context.
pub(super) fn load_config_or_display_output() -> eyre::Result<HarnessConfig> {
    let (localizer_kind, localizer) = select_cli_localizer();
    log_selected_localizer(localizer_kind);
    let span = cli_parse_span(localizer_kind);
    let _guard = span.enter();
    handle_cli_config_result(
        load_subcommand_config_with_localizer(localizer.as_ref()),
        localizer_kind,
    )
}

/// Records the selected localizer before parsing starts.
/// For example, `"noop"` records disabled CLI localization without a new span.
fn log_selected_localizer(localizer_kind: &str) {
    tracing::debug!(target: "spycatcher_harness", localizer_kind, "selected CLI localizer");
}

/// Creates the parse span without entering it; the caller controls its lifetime.
/// For example, `"fluent"` labels the span with the Fluent localizer kind.
fn cli_parse_span(localizer_kind: &str) -> tracing::Span {
    tracing::debug_span!(target: "spycatcher_harness", "parse_cli", localizer_kind)
}

/// Dispatches configuration success, display requests, and parse failures.
/// For example, a display request writes stdout and exits zero, while a parse
/// failure writes stderr and exits two; neither successful write returns.
///
/// # Errors
/// Returns write failures or other configuration errors with existing context.
fn handle_cli_config_result(
    result: Result<HarnessConfig, CliConfigError>,
    localizer_kind: &str,
) -> eyre::Result<HarnessConfig> {
    match result {
        Ok(config) => {
            log_parsed_configuration(localizer_kind);
            Ok(config)
        }
        Err(CliConfigError::DisplayRequested { output }) => {
            display_cli_output(&output, localizer_kind)
        }
        Err(CliConfigError::CliParse(error)) => display_cli_parse_error(&error, localizer_kind),
        Err(error) => Err(error).wrap_err("failed to load merged command config"),
    }
}

/// Records configuration success before returning it to the entry point.
/// For example, `"fluent"` identifies the localizer that parsed the configuration.
fn log_parsed_configuration(localizer_kind: &str) {
    tracing::debug!(target: "spycatcher_harness", localizer_kind, "parsed CLI configuration");
}

/// Writes a display request to stdout and exits zero after a successful flush.
/// For example, help output uses stdout; this function only returns on failure.
///
/// # Errors
/// Returns output-write failures with `failed to write CLI output` context.
fn display_cli_output(output: &str, localizer_kind: &str) -> eyre::Result<HarnessConfig> {
    tracing::debug!(target: "spycatcher_harness", localizer_kind, "parsed CLI display request");
    write_display_output(output).wrap_err("failed to write CLI output")?;
    std::process::exit(0);
}

/// Writes a parse failure to stderr and exits two after a successful flush.
/// For example, an unknown argument emits its error kind before the error text.
///
/// # Errors
/// Returns output-write failures with `failed to write CLI error output` context.
fn display_cli_parse_error(
    error: &clap::Error,
    localizer_kind: &str,
) -> eyre::Result<HarnessConfig> {
    tracing::debug!(target: "spycatcher_harness", localizer_kind, error_kind = ?error.kind(), "parsed CLI error");
    write_error_output(&error.to_string()).wrap_err("failed to write CLI error output")?;
    std::process::exit(2);
}

/// Chooses disabled CLI localization or Fluent using the early environment plan.
/// For example, a truthy disable switch selects `"noop"`; otherwise `"fluent"`
/// uses the primary locale, fallback locale, then English selection sequence.
fn select_cli_localizer() -> (&'static str, Arc<dyn Localizer>) {
    if is_cli_localization_disabled() {
        ("noop", Arc::new(NoOpLocalizer::new()))
    } else {
        ("fluent", build_cli_localizer(early_locale_plan()))
    }
}
