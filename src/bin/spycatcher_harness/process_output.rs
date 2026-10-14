//! Writes process display and error output, flushing each stream before returning.

use std::io::Write;

pub(super) fn write_display_output(output: &str) -> std::io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()
}

pub(super) fn write_error_output(output: &str) -> std::io::Result<()> {
    let mut stderr = std::io::stderr().lock();
    stderr.write_all(output.as_bytes())?;
    stderr.flush()
}
