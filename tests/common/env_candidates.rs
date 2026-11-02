//! Selection of stable ambient variables for spawned child processes.
//!
//! The record-mode and replay BDD suites both need a variable that exists in
//! the test process so they can forward it to a child spawned with
//! `env_clear`. Candidates are tried in preference order, and one that is set
//! but not valid UTF-8 is skipped: it cannot be forwarded as a CLI value, so
//! stopping the search there would fail a suite that has a usable later
//! candidate.

use std::ffi::OsString;

use crate::process_env::read_ambient;

/// Variables set by ordinary shells and CI runners, in preference order.
///
/// Each is stable for the lifetime of a test run, so forwarding it to a child
/// process preserves the parent's meaning.
pub(crate) const STABLE_CANDIDATES: &[&str] = &["HOME", "PWD", "USER", "PATH"];

/// Returns the first candidate that is present and valid UTF-8.
///
/// For example, when `FIRST` holds non-UTF-8 bytes and `SECOND` holds
/// `"value"`, `first_utf8_ambient(&["FIRST", "SECOND"])` returns
/// `Some("SECOND")`.
pub(crate) fn first_utf8_ambient(names: &[&'static str]) -> Option<&'static str> {
    first_utf8_ambient_with(names, read_ambient)
}

/// Selects through an injected reader so the skip rule is testable without
/// mutating the test process environment.
fn first_utf8_ambient_with<F>(names: &[&'static str], read: F) -> Option<&'static str>
where
    F: Fn(&str) -> Option<OsString>,
{
    names
        .iter()
        .copied()
        .find(|name| read(name).is_some_and(|value| value.into_string().is_ok()))
}

#[cfg(all(test, unix))]
mod tests {
    //! Verify that selection skips values which cannot be forwarded as UTF-8.

    use std::{ffi::OsString, os::unix::ffi::OsStringExt as _};

    use super::first_utf8_ambient_with;

    #[test]
    fn selector_skips_a_non_utf8_candidate() {
        let read = |name: &str| match name {
            "FIRST" => Some(OsString::from_vec(vec![0xff])),
            "SECOND" => Some(OsString::from("usable")),
            _ => None,
        };

        assert_eq!(
            first_utf8_ambient_with(&["FIRST", "SECOND"], read),
            Some("SECOND")
        );
    }
}
