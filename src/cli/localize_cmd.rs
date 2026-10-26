//! Project adapter around `OrthoConfig`'s localized Clap parsing.
//!
//! The upstream API owns generated Fluent identifiers and parse-error
//! localization. This module keeps the harness's established `cli` catalogue
//! root and the version interpolation required by its catalogue.

use clap::{Command, CommandFactory, Parser};
use ortho_config::{
    LanguageIdentifier, LocalizationArgs, LocalizeCmd as OrthoLocalizeCmd, Localizer,
    parse_localized_command,
};

/// Catalogue identifier that anchors lookup keys for the root command.
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
    fn localize(self, localizer: &dyn Localizer) -> Self {
        let original = self.clone();
        let command_localizer = CommandCopyLocalizer::new(localizer);
        let command =
            OrthoLocalizeCmd::with_base(self, ROOT_COMMAND_ID).localize(&command_localizer);
        apply_version_compatibility(command, &original, localizer)
    }
}

/// Parses `iter` with localized command copy and localized parse errors.
///
/// The pre-built command path preserves the harness's `cli` catalogue root;
/// `parse_localized_command` then handles both Clap parsing and conversion
/// errors with the same localizer.
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
/// Returns the localized [`clap::Error`] produced by parsing or converting
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
    let command = LocalizeCmd::localize(C::command(), localizer);
    parse_localized_command::<C, _, _>(command, iter, localizer).map(|(parsed, _matches)| parsed)
}

/// Applies the application-specific version interpolation after upstream localization.
///
/// `OrthoConfig` v0.10.0 supplies the `cli` base and underscore-preserving
/// suffixes such as `long_about` and `after_long_help`. The harness's
/// `cli-version` message also interpolates `$version`, which upstream does not
/// provide, so this adapter retains that application-specific value.
fn apply_version_compatibility(
    mut command: Command,
    original: &Command,
    localizer: &dyn Localizer,
) -> Command {
    let command_id = command_identifier(original);
    let args = command_args(original);

    if let Some(value) = localizer.lookup(&format!("{command_id}-version"), Some(&args)) {
        command = command.version(value);
    }
    command.mut_subcommands(|subcommand| {
        if let Some(original_subcommand) = original.find_subcommand(subcommand.get_name()) {
            apply_version_compatibility(subcommand, original_subcommand, localizer)
        } else {
            subcommand
        }
    })
}

/// Builds the legacy catalogue identifier for one command.
fn command_identifier(command: &Command) -> String {
    match command.get_name() {
        "spycatcher-harness" => ROOT_COMMAND_ID.to_owned(),
        name => format!("{ROOT_COMMAND_ID}-{name}"),
    }
}

/// Adapts the legacy version message to the command-tree localization context.
struct CommandCopyLocalizer<'a> {
    /// Localizer used for every command-copy field except legacy version text.
    localizer: &'a dyn Localizer,
}

impl<'a> CommandCopyLocalizer<'a> {
    /// Wraps the application localizer for the upstream command-tree pass.
    const fn new(localizer: &'a dyn Localizer) -> Self {
        Self { localizer }
    }
}

impl Localizer for CommandCopyLocalizer<'_> {
    fn lookup(&self, id: &str, args: Option<&LocalizationArgs<'_>>) -> Option<String> {
        if id.ends_with("-version") {
            None
        } else {
            self.localizer.lookup(id, args)
        }
    }

    fn locale(&self) -> Option<&LanguageIdentifier> {
        self.localizer.locale()
    }
}

/// Provides the interpolation values used by the legacy CLI version message.
fn command_args(command: &Command) -> LocalizationArgs<'static> {
    let mut args = LocalizationArgs::new();
    args.insert("binary", command.get_name().to_owned().into());
    if let Some(version) = command.get_version() {
        args.insert("version", version.to_owned().into());
    }
    args
}
