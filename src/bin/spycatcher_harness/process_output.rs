//! Routes application diagnostics to stderr and flushes process display/error output.

use std::io::Write;

use eyre::WrapErr as _;

/// Installs the binary's warning and error diagnostics before CLI parsing.
///
/// For example, an invalid early locale produces a warning without ANSI escapes
/// on stderr, while `--help` continues to write its help text to stdout.
///
/// # Errors
///
/// Returns an error if a global tracing subscriber cannot be installed.
pub(super) fn initialize_tracing() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init()
        .map_err(eyre::Report::msg)
        .wrap_err("failed to initialize application diagnostics")
}

/// Writes display bytes to stdout and flushes before the caller can exit.
/// For example, `write_display_output("help\n")` writes those exact bytes and
/// returns `Ok(())` after a successful write and flush.
///
/// # Errors
///
/// Propagates an I/O error if writing or flushing stdout fails.
pub(super) fn write_display_output(output: &str) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()
}

/// Writes error bytes to stderr and flushes before the caller can exit.
/// For example, `write_error_output("error\n")` writes those exact bytes and
/// returns `Ok(())` after a successful write and flush.
///
/// # Errors
///
/// Propagates an I/O error if writing or flushing stderr fails.
pub(super) fn write_error_output(output: &str) -> std::io::Result<()> {
    let mut stderr = std::io::stderr().lock();
    stderr.write_all(output.as_bytes())?;
    stderr.flush()
}
