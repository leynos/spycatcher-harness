//! Project-owned `clap` command localization helpers.
//!
//! This module applies an `OrthoConfig` [`Localizer`] to the command tree before
//! parsing so help and version display requests are rendered from the bundled
//! Fluent catalogue when translations are available.

use clap::{Command, CommandFactory, Parser};
use ortho_config::{LocalizationArgs, Localizer, localize_clap_error_with_command};

const ROOT_COMMAND_ID: &str = "cli";

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
    command = localize_about(command, command_id, localizer, Some(&args));
    command = localize_long_about(command, command_id, localizer, Some(&args));
    command = localize_usage(command, command_id, localizer, Some(&args));
    localize_after_help(command, command_id, localizer, Some(&args))
}

fn localize_subcommands(command: Command, localizer: &dyn Localizer) -> Command {
    command.mut_subcommands(|subcommand| subcommand.localize(localizer))
}

fn command_args(command: &Command) -> LocalizationArgs<'static> {
    let mut args = LocalizationArgs::new();
    args.insert("binary", command.get_name().to_owned().into());
    args
}

fn localize_about(
    command: Command,
    command_id: &str,
    localizer: &dyn Localizer,
    args: Option<&LocalizationArgs<'_>>,
) -> Command {
    match localizer.lookup(&format!("{command_id}-about"), args) {
        Some(about) => command.about(about),
        None => command,
    }
}

fn localize_long_about(
    command: Command,
    command_id: &str,
    localizer: &dyn Localizer,
    args: Option<&LocalizationArgs<'_>>,
) -> Command {
    match localizer.lookup(&format!("{command_id}-long-about"), args) {
        Some(about) => command.long_about(about),
        None => command,
    }
}

fn localize_usage(
    command: Command,
    command_id: &str,
    localizer: &dyn Localizer,
    args: Option<&LocalizationArgs<'_>>,
) -> Command {
    match localizer.lookup(&format!("{command_id}-usage"), args) {
        Some(usage) => command.override_usage(usage),
        None => command,
    }
}

fn localize_after_help(
    command: Command,
    command_id: &str,
    localizer: &dyn Localizer,
    args: Option<&LocalizationArgs<'_>>,
) -> Command {
    match localizer.lookup(&format!("{command_id}-merge-help"), args) {
        Some(help) => command.after_long_help(help),
        None => command,
    }
}
