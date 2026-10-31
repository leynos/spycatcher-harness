//! Shared table-case types for the CLI layering unit tests.
//!
//! Keeping the case shapes here lets the cassette-precedence and localization
//! suites share one description of a layering scenario without duplicating
//! struct definitions or inviting stringly typed subcommand parameters.

use spycatcher_harness::config;

/// One cassette-name precedence case.
pub(super) struct CassetteNameCase {
    /// Command-line arguments passed to the loader.
    pub(super) argv: &'static [&'static str],
    /// Optional config-file fixture.
    pub(super) config_file: Option<&'static str>,
    /// Environment overrides applied above the config file.
    pub(super) env_vars: &'static [(&'static str, &'static str)],
    /// Cassette name expected after merging.
    pub(super) expected_cassette_name: &'static str,
}

/// One localization precedence case.
pub(super) struct LocalizationPrecedenceCase {
    /// Command-line arguments passed to the loader.
    pub(super) argv: &'static [&'static str],
    /// Optional config-file fixture.
    pub(super) config_file: Option<&'static str>,
    /// Environment overrides applied above the config file.
    pub(super) env_vars: &'static [(&'static str, &'static str)],
    /// Primary locale expected after merging.
    pub(super) expected_locale: Option<&'static str>,
    /// Fallback locale expected after merging.
    pub(super) expected_fallback_locale: &'static str,
}

/// CLI subcommands exercised by the layering tests.
#[derive(Clone, Copy, Debug)]
pub(super) enum Subcommand {
    /// Record mode.
    Record,
    /// Replay mode.
    Replay,
    /// Verify mode.
    Verify,
}

impl Subcommand {
    /// Name used on the command line and in configuration paths.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Replay => "replay",
            Self::Verify => "verify",
        }
    }

    /// Mode the loader should report for this subcommand.
    pub(super) const fn mode(self) -> config::Mode {
        match self {
            Self::Record => config::Mode::Record,
            Self::Replay => config::Mode::Replay,
            Self::Verify => config::Mode::Verify,
        }
    }
}
