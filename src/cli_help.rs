//! Long-form CLI help text.

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
