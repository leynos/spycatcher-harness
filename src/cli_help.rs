//! Long-form CLI help text constants for the `spycatcher-harness` binary.
//!
//! This module contains static string constants that are injected into the
//! `clap` argument parser as after-help text. [`CLI_MERGE_HELP`] documents the
//! layered configuration precedence model (CLI > env > config files > defaults)
//! and illustrates the `cmds.<subcommand>` TOML namespace and the double-underscore
//! convention for nested environment keys such as
//! `SPYCATCHER_HARNESS_CMDS_RECORD_UPSTREAM__BASE_URL`.
//!
//! Keep this module free of logic; its sole responsibility is to hold
//! user-facing documentation strings.

/// Help text describing configuration precedence and subcommand-specific keys.
pub(super) const CLI_MERGE_HELP: &str = concat!(
    "Configuration precedence: CLI > env > config files > defaults.\n",
    "Subcommand defaults merge from the `cmds` namespace.\n\n",
    "Example:\n",
    "  [cmds.record]\n",
    "  cassette_name = \"session_a\"\n\n",
    "  [cmds.record.upstream]\n",
    "  kind = \"openrouter\"\n",
    "  base_url = \"https://openrouter.ai/api/v1\"\n",
    "  api_key_env = \"OPENROUTER_API_KEY\"\n\n",
    "Environment prefix: SPYCATCHER_HARNESS_CMDS_<SUBCOMMAND>_...\n",
    "Nested keys use double underscores, e.g.\n",
    "SPYCATCHER_HARNESS_CMDS_RECORD_UPSTREAM__BASE_URL."
);

#[cfg(test)]
mod tests {
    //! Tests for keeping stock and localized long-help content aligned.

    use i18n_embed::unic_langid::langid;
    use ortho_config::{FluentLocalizer, Localizer};

    use super::CLI_MERGE_HELP;

    const CLI_FTL: &str = include_str!("../i18n/en-US/spycatcher-harness.ftl");

    /// Keeps the localized merge help no less informative than stock Clap text.
    ///
    /// Every non-empty stock line must still appear, so a translation cannot
    /// silently drop an option or a hint.
    #[test]
    fn localized_merge_help_matches_stock_merge_help_content() {
        let fluent = FluentLocalizer::builder(langid!("en-US"))
            .with_consumer_resources([CLI_FTL])
            .try_build()
            .expect("bundled CLI catalogue should build");
        let rendered_help = fluent
            .lookup("cli-after_long_help", None)
            .expect("localized merge help should render");

        for expected in non_empty_lines(CLI_MERGE_HELP) {
            assert!(
                non_empty_lines(&rendered_help).contains(&expected),
                "localized merge help should contain stock line: {expected}"
            );
        }
    }

    /// Returns the trimmed, non-blank lines of `text`.
    ///
    /// Blank-line differences between stock and localized help are irrelevant
    /// to the comparison, so they are dropped rather than asserted on.
    fn non_empty_lines(text: &str) -> Vec<&str> {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect()
    }
}
