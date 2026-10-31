//! Coverage-profile propagation for subprocess tests.
//!
//! Startup-isolation tests run the compiled binary with a cleared environment
//! so ambient configuration cannot leak in. `env_clear` also drops
//! `LLVM_PROFILE_FILE`, which `cargo llvm-cov` sets so child processes record
//! their own profiles. Without it the spawned binary reports no coverage and
//! the workspace ratchet loses the binary's lines. The variable configures the
//! profiling runtime only, so restoring it leaves the application-visible
//! environment as isolated as the test requires.

use std::process::Command;

/// Restores the inherited coverage profile destination after `env_clear`.
///
/// Call this after `Command::env_clear` and before spawning the child. For
/// example, `preserve_coverage_profile(&mut command)` forwards
/// `LLVM_PROFILE_FILE` when `cargo llvm-cov` has set it, and leaves a plain
/// `cargo test` child untouched.
pub(crate) fn preserve_coverage_profile(command: &mut Command) {
    if let Some(profile_file) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile_file);
    }
}
