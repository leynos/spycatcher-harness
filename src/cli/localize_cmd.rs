//! Project adapter around `OrthoConfig`'s localized Clap parsing.
//!
//! The upstream API owns generated Fluent identifiers and parse-error
//! localization. This module keeps the harness's established `cli` catalogue
//! root and the legacy interpolation contract: every command-copy lookup
//! receives `$binary` and, when the command declares one, `$version`.

use clap::{Command, CommandFactory, Parser};
use ortho_config::{
    LanguageIdentifier,
    LocalizationArgs,
    LocalizeCmd as OrthoLocalizeCmd,
    Localizer,
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
/// assert_eq!(
///     localized.get_about().map(ToString::to_string),
///     Some("Stock copy".into())
/// );
/// ```
pub trait LocalizeCmd {
    /// Returns `self` with localized copy applied where lookups exist.
    #[must_use]
    fn localize(self, localizer: &dyn Localizer) -> Self;
}

impl LocalizeCmd for Command {
    /// Localizes the harness root command with the stock catalogue root.
    fn localize(self, localizer: &dyn Localizer) -> Self {
        localize_command(self, ROOT_COMMAND_ID, localizer)
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
/// assert_eq!(
///     parsed,
///     Example {
///         name: "case".into()
///     }
/// );
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

/// Localizes one command's own copy, then recurses into its subcommands.
///
/// Upstream v0.10.0 passes no arguments to `about` and only `$binary` to
/// longer metadata. The harness catalogue predates that split: its messages
/// may interpolate both `$binary` and `$version` in any command-copy field.
/// Applying the upstream pass one command at a time keeps the `cli.<command>`
/// identifier path while letting [`CommandCopyLocalizer`] restore the legacy
/// arguments for every lookup.
fn localize_command(command: Command, base: &str, localizer: &dyn Localizer) -> Command {
    let command_localizer = CommandCopyLocalizer::for_command(&command, localizer);
    let localized_command =
        OrthoLocalizeCmd::with_base(command, base).localize_self(&command_localizer);

    localized_command.mut_subcommands(|subcommand| {
        let child_base = format!("{base}.{}", subcommand.get_name());
        localize_command(subcommand, &child_base, localizer)
    })
}

/// Adds the legacy interpolation arguments to one command's copy lookups.
struct CommandCopyLocalizer<'a> {
    /// Localizer that owns the actual catalogue lookups.
    localizer: &'a dyn Localizer,
    /// Binary name used for `$binary` interpolation in this command's copy.
    binary: String,
    /// Version used for `$version` interpolation, when the command declares one.
    version: Option<String>,
}

impl<'a> CommandCopyLocalizer<'a> {
    /// Captures the interpolation values for `command`.
    fn for_command(command: &Command, localizer: &'a dyn Localizer) -> Self {
        Self {
            localizer,
            binary: command.get_name().to_owned(),
            version: command.get_version().map(str::to_owned),
        }
    }
}

impl Localizer for CommandCopyLocalizer<'_> {
    /// Supplies the command's own binary and version to catalogue lookups.
    ///
    /// Values from the delegated localizer are kept, but `$binary` and
    /// `$version` always describe this command so inherited copy cannot
    /// render another command's name.
    fn lookup(&self, id: &str, args: Option<&LocalizationArgs<'_>>) -> Option<String> {
        let mut compatible_args = args.cloned().unwrap_or_default();
        compatible_args.insert("binary", self.binary.clone().into());
        if let Some(version) = &self.version {
            compatible_args.insert("version", version.clone().into());
        } else {
            // A version-less command must not receive a stale `$version` from
            // the caller; Fluent would otherwise render an unexpected value.
            compatible_args.remove("version");
        }

        self.localizer.lookup(id, Some(&compatible_args))
    }

    fn locale(&self) -> Option<&LanguageIdentifier> { self.localizer.locale() }
}
