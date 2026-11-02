//! Sanctioned ambient process-environment reads for integration tests.
//!
//! Several suites spawn the compiled binary with `env_clear` so ambient
//! configuration cannot leak into the child, and then have to hand the child a
//! few values taken from the test process itself. Reading those values is
//! exactly what `clippy::disallowed_methods` restricts, so every such read goes
//! through this module instead of scattering `#[expect]` attributes across the
//! suites.

use std::ffi::OsString;

/// Reads one ambient variable from the test process, if it is set.
///
/// For example, `read_ambient("LLVM_PROFILE_FILE")` returns whatever
/// `cargo llvm-cov` exported so the caller can forward it to a child process,
/// and returns `None` under a plain `cargo test` run.
#[expect(
    clippy::disallowed_methods,
    reason = "integration tests must read the ambient environment to mirror it into spawned \
              children"
)]
pub(crate) fn read_ambient(name: &str) -> Option<OsString> { std::env::var_os(name) }
