//! Project-owned `clap` command localization helpers.
//!
//! This module applies an `OrthoConfig` [`Localizer`] to the command tree before
//! parsing so help and version display requests are rendered from the bundled
//! Fluent catalogue when translations are available.

use clap::{Command, CommandFactory, Parser};
use ortho_config::{LocalizationArgs, Localizer, localize_clap_error_with_command};

const ROOT_COMMAND_ID: &str = "cli";

type CommandStringApplicator = fn(Command, String) -> Command;

/// Extension trait that applies localized copy to a [`Command`].
///
/// # Examples
///
/// ```rust
/// use clap::Command;
/// use ortho_config::NoOpLocalizer;
/// use spycatcher_harness::cli::localization::LocalizeCmd;
///
/// let command = Command::new("demo").about("Stock copy");
/// let localized = command.localize(&NoOpLocalizer::new());
///
/// assert_eq!(localized.get_about().map(ToString::to_string), Some("Stock copy".into()));
/// ```
pub trait LocalizeCmd {
    /// Returns `self` with localized copy applied where lookups exist.
    #[must_use]
    fn localize(self, localizer: &dyn Localizer) -> Self;
}

impl LocalizeCmd for Command {
    fn localize(mut self, localizer: &dyn Localizer) -> Self {
        let command_id = command_identifier(&self);
        self = localize_command_copy(self, &command_id, localizer);
        self = localize_subcommands(self, localizer);
        self
    }
}

/// Parses `iter` with localized command copy and localized parse errors.
///
/// # Examples
///
/// ```rust
/// use clap::Parser;
/// use ortho_config::NoOpLocalizer;
/// use spycatcher_harness::cli::localization::try_parse_localized_from_iter;
///
/// #[derive(Debug, Parser, PartialEq)]
/// struct Example {
///     #[arg(long)]
///     name: String,
/// }
///
/// let parsed = try_parse_localized_from_iter::<Example, _, _>(
///     ["example", "--name", "case"],
///     &NoOpLocalizer::new(),
/// )?;
/// assert_eq!(parsed, Example { name: "case".into() });
/// # Ok::<(), clap::Error>(())
/// ```
///
/// # Errors
///
/// Returns the localized [`clap::Error`] produced by parsing or by converting
/// matches into the target parser type.
pub fn try_parse_localized_from_iter<C, I, T>(
    iter: I,
    localizer: &dyn Localizer,
) -> Result<C, clap::Error>
where
    C: Parser + CommandFactory,
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let mut command = C::command().localize(localizer);
    let matches = command
        .try_get_matches_from_mut(iter)
        .map_err(|error| localize_clap_error_with_command(error, localizer, Some(&command)))?;

    C::from_arg_matches(&matches).map_err(|match_error| {
        let command_error = match_error.with_cmd(&command);
        localize_clap_error_with_command(command_error, localizer, Some(&command))
    })
}

fn command_identifier(command: &Command) -> String {
    match command.get_name() {
        "spycatcher-harness" => ROOT_COMMAND_ID.to_owned(),
        name => format!("{ROOT_COMMAND_ID}-{name}"),
    }
}

fn localize_command_copy(
    mut command: Command,
    command_id: &str,
    localizer: &dyn Localizer,
) -> Command {
    let args = command_args(&command);
    let ctx = LocalizeContext {
        localizer,
        args: Some(&args),
    };
    let fields: &[(&str, CommandStringApplicator)] = &[
        ("-about", |cmd, value| cmd.about(value)),
        ("-long-about", |cmd, value| cmd.long_about(value)),
        ("-version", apply_localized_version),
        ("-usage", |cmd, value| cmd.override_usage(value)),
        ("-merge-help", |cmd, value| cmd.after_long_help(value)),
    ];

    for (suffix, apply) in fields {
        command = apply_localized_string(command, &format!("{command_id}{suffix}"), &ctx, *apply);
    }

    command
}

fn localize_subcommands(command: Command, localizer: &dyn Localizer) -> Command {
    command.mut_subcommands(|subcommand| subcommand.localize(localizer))
}

fn apply_localized_version(command: Command, value: String) -> Command {
    command.version(value)
}

fn command_args(command: &Command) -> LocalizationArgs<'static> {
    let mut args = LocalizationArgs::new();
    args.insert("binary", command.get_name().to_owned().into());
    if let Some(version) = command.get_version() {
        args.insert("version", version.to_owned().into());
    }
    args
}

struct LocalizeContext<'a> {
    localizer: &'a dyn Localizer,
    args: Option<&'a LocalizationArgs<'a>>,
}

fn apply_localized_string(
    command: Command,
    key: &str,
    ctx: &LocalizeContext<'_>,
    apply: CommandStringApplicator,
) -> Command {
    match ctx.localizer.lookup(key, ctx.args) {
        Some(value) => apply(command, value),
        None => command,
    }
}
