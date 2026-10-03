//! Project-owned `clap` command localization helpers.
//!
//! This module applies an `OrthoConfig` [`Localizer`] to the command tree before
//! parsing so help and version display requests are rendered from the bundled
//! Fluent catalogue when translations are available.

use clap::{Command, CommandFactory, Parser};
use ortho_config::{LocalizationArgs, Localizer};

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
        let command_id = command_identifier(&self);
        let prepared_command = localize_command_copy(self, &command_id, localizer);
        localize_subcommands(prepared_command, localizer)
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
    let command = C::command().localize(localizer);
    ortho_config::parse_localized_command::<C, _, _>(command, iter, localizer)
        .map(|(parsed, _matches)| parsed)
}

fn command_identifier(command: &Command) -> String {
    match command.get_name() {
        "spycatcher-harness" => ROOT_COMMAND_ID.to_owned(),
        name => format!("{ROOT_COMMAND_ID}-{name}"),
    }
}

fn localize_command_copy(command: Command, command_id: &str, localizer: &dyn Localizer) -> Command {
    let compatibility_localizer = CatalogueCompatibleLocalizer {
        localizer,
        binary: command.get_name().to_owned(),
        version: command.get_version().map(str::to_owned),
    };

    ortho_config::LocalizeCmd::with_base(command, command_id)
        .localize_self(&compatibility_localizer)
}

fn localize_subcommands(command: Command, localizer: &dyn Localizer) -> Command {
    command.mut_subcommands(|subcommand| LocalizeCmd::localize(subcommand, localizer))
}

/// Bridges the current catalogue contract to `OrthoConfig`'s command localizer.
///
/// The project catalogue predates `OrthoConfig`'s metadata identifiers: it uses
/// hyphens in `long-about` and `merge-help` where Clap names the fields
/// `long_about` and `after_long_help`, and supplies both formatting arguments
/// to each command lookup. Keep that translation policy at this adapter
/// boundary while delegating the command-field traversal upstream.
struct CatalogueCompatibleLocalizer<'a> {
    localizer: &'a dyn Localizer,
    binary: String,
    version: Option<String>,
}

impl Localizer for CatalogueCompatibleLocalizer<'_> {
    fn lookup(&self, id: &str, args: Option<&LocalizationArgs<'_>>) -> Option<String> {
        let compatible_id = id
            .strip_suffix("-long_about")
            .map(|prefix| format!("{prefix}-long-about"))
            .or_else(|| {
                id.strip_suffix("-after_long_help")
                    .map(|prefix| format!("{prefix}-merge-help"))
            });
        let mut compatible_args = args.cloned().unwrap_or_default();
        compatible_args.insert("binary", self.binary.clone().into());
        if let Some(version) = &self.version {
            compatible_args.insert("version", version.clone().into());
        } else {
            compatible_args.remove("version");
        }

        self.localizer.lookup(
            compatible_id.as_deref().unwrap_or(id),
            Some(&compatible_args),
        )
    }

    fn locale(&self) -> Option<&i18n_embed::unic_langid::LanguageIdentifier> {
        self.localizer.locale()
    }
}
