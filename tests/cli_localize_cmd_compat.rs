//! Contract tests for the project adapter around `OrthoConfig`'s localized parser.

use std::sync::Mutex;

use clap::{Command, Parser, Subcommand};
use i18n_embed::unic_langid::langid;
use ortho_config::{FluentLocalizer, LocalizationArgs, Localizer};
use rstest::rstest;
use spycatcher_harness::cli::localization::{LocalizeCmd, try_parse_localized_from_iter};

#[rstest]
fn command_adapter_preserves_catalogue_ids_and_formatting_arguments() {
    let localizer = FluentLocalizer::builder(langid!("en-US"))
        .with_consumer_resources([concat!(
            "cli-about = About { $binary } { $version }\n",
            "cli-long-about = Long { $binary } { $version }\n",
            "cli-version = Version { $binary } { $version }\n",
            "cli-usage = Localized usage { $binary } <COMMAND>\n",
            "cli-merge-help = Merge { $binary } { $version }\n",
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
    let localizer = MissingVersionArgumentProbe::default();
    let localized = Command::new("spycatcher-harness")
        .about("Stock about")
        .localize(&localizer);

    assert_eq!(
        localized.get_about().map(ToString::to_string),
        Some(String::from("Localized about"))
    );
    let about_arguments = *localizer
        .about_arguments
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(about_arguments, Some((true, false)));
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

#[derive(Default)]
struct ParseErrorProbe {
    lookups: Mutex<Vec<String>>,
}

#[derive(Default)]
struct MissingVersionArgumentProbe {
    about_arguments: Mutex<Option<(bool, bool)>>,
}

impl Localizer for MissingVersionArgumentProbe {
    fn lookup(&self, id: &str, args: Option<&LocalizationArgs<'_>>) -> Option<String> {
        if id != "cli-about" {
            return None;
        }

        let has_binary = args.is_some_and(|arguments| arguments.contains_key("binary"));
        let has_version = args.is_some_and(|arguments| arguments.contains_key("version"));
        *self
            .about_arguments
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((has_binary, has_version));
        Some(String::from("Localized about"))
    }
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
