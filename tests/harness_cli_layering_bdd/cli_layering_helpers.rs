//! Helper utilities for layered CLI BDD scenarios.
//!
//! The step definitions live in `harness_cli_layering_bdd.rs`; this module
//! keeps command construction, configuration fragments, and result extraction
//! small enough to satisfy repository health rules.

use spycatcher_harness::HarnessConfig;

use super::CliLayeringWorld;

/// Replaces the current command argv with `args`.
pub(super) fn set_command(cli_layering_world: &CliLayeringWorld, args: Vec<String>) {
    cli_layering_world.argv.set(args);
}

/// Appends a TOML configuration `fragment` to the scenario config file.
pub(super) fn append_config(cli_layering_world: &CliLayeringWorld, fragment: &str) {
    let mut current = cli_layering_world.config_file.take().unwrap_or_default();
    current.push_str(fragment);
    cli_layering_world.config_file.set(current);
}

/// Appends a `[cmds.replay.localization]` TOML fragment setting `field` to `value`.
///
/// # Example
///
/// ```ignore
/// append_replay_localization_field(&world, "locale", "en-GB");
/// // Appends:
/// // [cmds.replay.localization]
/// // locale = "en-GB"
/// ```
pub(super) fn append_replay_localization_field(
    cli_layering_world: &CliLayeringWorld,
    field: &str,
    value: &str,
) {
    append_config(
        cli_layering_world,
        &format!("[cmds.replay.localization]\n{field} = \"{value}\"\n"),
    );
}

/// Adds an environment variable key/value pair to the scenario environment.
pub(super) fn push_env(cli_layering_world: &CliLayeringWorld, key: &str, value: &str) {
    let mut vars = cli_layering_world.env_vars.take().unwrap_or_default();
    vars.push((String::from(key), String::from(value)));
    cli_layering_world.env_vars.set(vars);
}

/// Trims leading and trailing double quotes from `value` into an owned string.
pub(super) fn trim_surrounding_quotes(value: &str) -> String {
    value.trim_matches('"').to_owned()
}

/// Supported subcommands used by BDD command-builder helpers.
#[derive(Clone, Copy)]
pub(super) enum Subcommand {
    Record,
    Replay,
    Verify,
}

impl Subcommand {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Record => "record",
            Self::Replay => "replay",
            Self::Verify => "verify",
        }
    }
}

/// Supported CLI flags used by BDD command-builder helpers.
#[derive(Clone, Copy)]
pub(super) enum CliFlag {
    CassetteName,
    FallbackLocale,
    Locale,
}

impl CliFlag {
    const fn as_str(self) -> &'static str {
        match self {
            Self::CassetteName => "--cassette-name",
            Self::FallbackLocale => "--fallback-locale",
            Self::Locale => "--locale",
        }
    }
}

/// Builds a command containing `subcommand`, `flag`, and `value`.
///
/// # Example
///
/// ```ignore
/// set_flag_command(&world, Subcommand::Replay, CliFlag::Locale, "en-GB");
/// // Sets argv to:
/// // ["spycatcher-harness", "replay", "--locale", "en-GB"]
/// ```
pub(super) fn set_flag_command(
    cli_layering_world: &CliLayeringWorld,
    subcommand: Subcommand,
    flag: CliFlag,
    value: &str,
) {
    set_command(
        cli_layering_world,
        vec![
            String::from("spycatcher-harness"),
            String::from(subcommand.as_str()),
            String::from(flag.as_str()),
            String::from(value),
        ],
    );
}

/// Sets argv to the binary name and `subcommand` only.
///
/// # Example
///
/// ```ignore
/// set_subcommand_only(&world, Subcommand::Replay);
/// // Sets argv to:
/// // ["spycatcher-harness", "replay"]
/// ```
pub(super) fn set_subcommand_only(cli_layering_world: &CliLayeringWorld, subcommand: Subcommand) {
    set_command(
        cli_layering_world,
        vec![
            String::from("spycatcher-harness"),
            String::from(subcommand.as_str()),
        ],
    );
}

/// Returns the loaded config, or panics with `context` if loading failed.
///
/// # Example
///
/// ```ignore
/// let config = expect_loaded_config(&world, "replay");
/// // Returns the stored HarnessConfig, or panics with replay context on error.
/// ```
pub(super) fn expect_loaded_config(
    cli_layering_world: &CliLayeringWorld,
    context: &str,
) -> HarnessConfig {
    let outcome = cli_layering_world
        .result
        .with_ref(Clone::clone)
        .unwrap_or_else(|| Err(String::from("result slot missing")));
    match outcome {
        Ok(config) => config,
        Err(error) => panic!("expected {context} configuration, load failed: {error}"),
    }
}
