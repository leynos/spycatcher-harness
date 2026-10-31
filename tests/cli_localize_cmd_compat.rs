//! Contract tests for the project adapter around `OrthoConfig`'s localized parser.
//!
//! The adapter preserves two pre-v0.10 contracts: the `cli.<command>` catalogue
//! identifiers and the interpolation arguments (`$binary`, and `$version` when
//! available) supplied to every command-copy lookup.

use std::sync::Mutex;

use clap::{Command, Parser, Subcommand};
use i18n_embed::unic_langid::langid;
use ortho_config::{FluentLocalizer, LocalizationArgs, Localizer};
use rstest::rstest;
use spycatcher_harness::cli::localization::{LocalizeCmd, try_parse_localized_from_iter};

/// Proves the adapter keeps the pre-v0.10 catalogue ids and arguments.
#[rstest]
fn command_adapter_preserves_catalogue_ids_and_formatting_arguments() {
    let localizer = FluentLocalizer::builder(langid!("en-US"))
        .with_consumer_resources([concat!(
            "cli-about = About { $binary } { $version }\n",
            "cli-long_about = Long { $binary } { $version }\n",
            "cli-version = Version { $binary } { $version }\n",
            "cli-usage = Localized usage { $binary } <COMMAND>\n",
            "cli-after_long_help = Merge { $binary } { $version }\n",
            "cli-replay-about = Replay { $binary }\n",
        )])
        .try_build()
        .expect("compatibility catalogue should build");
    let command = Command::new("spycatcher-harness")
        .about("Stock about")
        .long_about("Stock long about")
        .version("0.1.0")
        .after_long_help("Stock merge help")
        .subcommand(Command::new("replay").about("Stock replay"));

    let localized = command.localize(&localizer);

    assert_eq!(
        localized.get_about().map(ToString::to_string),
        Some(String::from(
            "About \u{2068}spycatcher-harness\u{2069} \u{2068}0.1.0\u{2069}"
        ))
    );
    assert_eq!(
        localized.get_long_about().map(ToString::to_string),
        Some(String::from(
            "Long \u{2068}spycatcher-harness\u{2069} \u{2068}0.1.0\u{2069}"
        ))
    );
    assert_eq!(
        localized.get_version(),
        Some("Version \u{2068}spycatcher-harness\u{2069} \u{2068}0.1.0\u{2069}")
    );
    assert_eq!(
        localized.get_after_long_help().map(ToString::to_string),
        Some(String::from(
            "Merge \u{2068}spycatcher-harness\u{2069} \u{2068}0.1.0\u{2069}"
        ))
    );
    let usage = localized.clone().render_usage().to_string();
    assert!(
        usage.contains("Localized usage")
            && usage.contains("spycatcher-harness")
            && usage.contains("<COMMAND>"),
        "localized usage should be rendered by the adapter: {usage}"
    );
    let replay = localized
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "replay")
        .expect("replay subcommand should remain present");
    assert_eq!(
        replay.get_about().map(ToString::to_string),
        Some(String::from("Replay \u{2068}replay\u{2069}"))
    );
}

#[rstest]
fn compatibility_localizer_forwards_binary_without_missing_version() {
    let localizer = ArgumentProbe::default();
    let localized = Command::new("spycatcher-harness")
        .about("Stock about")
        .localize(&localizer);

    assert_eq!(
        localized.get_about().map(ToString::to_string),
        Some(String::from("Localized about"))
    );
    let recorded = localizer.recorded_arguments("cli-about");
    assert_eq!(
        recorded,
        Some(ArgumentPresence {
            has_binary: true,
            has_version: false
        })
    );
}

#[rstest]
fn subcommand_adapter_uses_subcommand_binary_without_parent_version() {
    let localizer = ArgumentProbe::default();
    let localized = Command::new("spycatcher-harness")
        .version("0.1.0")
        .subcommand(Command::new("replay").about("Stock replay"))
        .localize(&localizer);

    let replay = localized
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "replay")
        .expect("replay subcommand should remain present");
    assert_eq!(
        replay.get_about().map(ToString::to_string),
        Some(String::from("Localized about"))
    );
    assert_eq!(
        localizer.recorded_arguments("cli-replay-about"),
        Some(ArgumentPresence {
            has_binary: true,
            has_version: false
        })
    );
}

#[rstest]
fn localized_parse_uses_upstream_conversion_error_path_once() {
    let localizer = ParseErrorProbe::default();

    let error =
        try_parse_localized_from_iter::<ConversionCli, _, _>(["spycatcher-harness"], &localizer)
            .expect_err("missing subcommand should fail during matches conversion");

    assert!(
        error
            .to_string()
            .starts_with("error: Localized conversion error\n"),
        "conversion errors should use localized text and retain Clap context: {error}"
    );
    let lookups = localizer
        .lookups
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(
        lookups
            .iter()
            .filter(|id| id.as_str() == "clap-error-missing-subcommand")
            .count(),
        1,
        "the conversion error should be localized once"
    );
}

#[derive(Debug, Parser)]
#[command(name = "spycatcher-harness", subcommand_required = false)]
struct ConversionCli {
    #[command(subcommand)]
    command: ConversionCommand,
}

#[derive(Debug, Subcommand)]
enum ConversionCommand {
    Replay,
}

/// Argument keys observed for one command-copy lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArgumentPresence {
    /// Whether the lookup received a `$binary` value.
    has_binary: bool,
    /// Whether the lookup received a `$version` value.
    has_version: bool,
}

#[derive(Default)]
struct ArgumentProbe {
    recorded: Mutex<Vec<(String, ArgumentPresence)>>,
}

impl ArgumentProbe {
    /// Returns the argument presence recorded for `id`, if the adapter
    /// performed that lookup.
    fn recorded_arguments(&self, id: &str) -> Option<ArgumentPresence> {
        self.recorded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|(recorded_id, _)| recorded_id == id)
            .map(|(_, presence)| *presence)
    }
}

impl Localizer for ArgumentProbe {
    fn lookup(&self, id: &str, args: Option<&LocalizationArgs<'_>>) -> Option<String> {
        let presence = ArgumentPresence {
            has_binary: args.is_some_and(|arguments| arguments.contains_key("binary")),
            has_version: args.is_some_and(|arguments| arguments.contains_key("version")),
        };
        self.recorded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((id.to_owned(), presence));
        id.ends_with("-about")
            .then(|| String::from("Localized about"))
    }
}

#[derive(Default)]
struct ParseErrorProbe {
    lookups: Mutex<Vec<String>>,
}

impl Localizer for ParseErrorProbe {
    fn lookup(&self, id: &str, _args: Option<&LocalizationArgs<'_>>) -> Option<String> {
        self.lookups
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(id.to_owned());
        (id == "clap-error-missing-subcommand").then(|| String::from("Localized conversion error"))
    }
}
