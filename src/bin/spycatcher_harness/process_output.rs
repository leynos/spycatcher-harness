//! Writes process display and error output, flushing each stream before returning.

use std::io::Write;

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
