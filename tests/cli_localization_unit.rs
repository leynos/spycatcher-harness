//! Unit tests for CLI help and parse-error localization.
//!
//! These tests exercise the project-owned `LocalizeCmd` trait and localized
//! parsing helper without invoking the compiled binary.

use clap::Command;
use i18n_embed::unic_langid::langid;
use ortho_config::{
    FluentLocalizer, LocalizationArgs, Localizer, NoOpLocalizer, localize_clap_error_with_command,
};
use rstest::rstest;
use spycatcher_harness::cli::load_subcommand_config_from_iter_with_localizer;
use spycatcher_harness::cli::localization::LocalizeCmd;
use spycatcher_harness::cli::localizer::{build_cli_localizer, build_cli_localizer_from_resources};

#[path = "support/isolated_cli_process.rs"]
mod isolated_cli_process;

#[path = "cli_localization_unit/locale_scenarios.rs"]
mod locale_scenarios;

const CLI_FTL: &str = include_str!("../i18n/en-US/spycatcher-harness.ftl");

#[rstest]
fn bundled_cli_catalogue_builds() {
    FluentLocalizer::builder(langid!("en-US"))
        .with_consumer_resources([CLI_FTL])
        .try_build()
        .expect("bundled CLI catalogue should build");
}

#[rstest]
#[case("cli-about")]
#[case("cli-long-about")]
#[case("cli-usage")]
#[case("cli-version")]
#[case("cli-merge-help")]
#[case("cli-record-about")]
#[case("cli-replay-about")]
#[case("cli-verify-about")]
fn cli_catalogue_entries_are_available(#[case] id: &str) {
    let localizer = build_cli_localizer(langid!("en-US"));
    let mut args = LocalizationArgs::new();
    args.insert("binary", "spycatcher-harness".into());
    args.insert("version", "0.1.0".into());

    let rendered = localizer.lookup(id, Some(&args));

    assert!(rendered.is_some_and(|text| !text.trim().is_empty()));
}

#[rstest]
#[case("clap-error-missing-argument")]
#[case("clap-error-unknown-argument")]
#[case("clap-error-invalid-value")]
#[case("clap-error-invalid-subcommand")]
#[case("clap-error-missing-subcommand")]
fn clap_error_catalogue_entries_are_available(#[case] id: &str) {
    let localizer = build_cli_localizer(langid!("en-US"));
    let mut args = LocalizationArgs::new();
    args.insert("argument", "--cassette-name".into());
    args.insert("value", "bad".into());
    args.insert("valid_values", "record, replay, verify".into());
    args.insert("subcommand", "bad".into());
    args.insert("valid_subcommands", "record, replay, verify".into());

    let rendered = localizer.lookup(id, Some(&args));

    assert!(rendered.is_some_and(|text| !text.trim().is_empty()));
}

#[rstest]
fn noop_localizer_leaves_command_copy_unchanged() {
    let command = Command::new("demo").about("Stock about");

    let localized = command.localize(&NoOpLocalizer::new());

    assert_eq!(
        localized.get_about().map(ToString::to_string),
        Some(String::from("Stock about"))
    );
}

#[rstest]
fn fluent_localizer_updates_command_copy() {
    let command = Command::new("spycatcher-harness")
        .about("Stock about")
        .version("0.0.0");
    let localizer = build_cli_localizer(langid!("en-US"));

    let localized = command.localize(localizer.as_ref());

    assert_eq!(
        localized.get_about().map(ToString::to_string),
        Some(String::from(
            "Deterministic record/replay harness for LLM API testing"
        ))
    );
    let version = localized
        .get_version()
        .map(str::to_owned)
        .expect("localized command should keep version text");
    assert!(
        version.contains("0.0.0"),
        "localized version should include the package version, got: {version}"
    );
    assert!(
        localized
            .get_after_long_help()
            .is_some_and(|help| help.to_string().contains("Configuration precedence"))
    );
}

#[rstest]
fn fluent_localizer_updates_subcommand_copy() {
    let localizer = FluentLocalizer::builder(langid!("en-US"))
        .with_consumer_resources([
            "cli-about = Root localized\ncli-replay-about = Replay localized by catalogue\n",
        ])
        .try_build()
        .expect("test CLI catalogue should build");
    let command =
        Command::new("spycatcher-harness").subcommand(Command::new("replay").about("Stock replay"));

    let localized = command.localize(&localizer);
    let replay = localized
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == "replay")
        .expect("replay subcommand should remain present");

    assert_eq!(
        replay.get_about().map(ToString::to_string),
        Some(String::from("Replay localized by catalogue"))
    );
}

#[rstest]
fn localized_help_display_is_returned_from_config_loader() {
    let localizer = build_cli_localizer(langid!("en-US"));

    let error = load_subcommand_config_from_iter_with_localizer(
        ["spycatcher-harness", "--help"],
        localizer.as_ref(),
    )
    .expect_err("help should be surfaced as display output");

    insta::assert_snapshot!(error.to_string());
}

#[rstest]
fn localized_version_display_is_returned_from_config_loader() {
    let localizer = build_cli_localizer(langid!("en-US"));

    let error = load_subcommand_config_from_iter_with_localizer(
        ["spycatcher-harness", "--version"],
        localizer.as_ref(),
    )
    .expect_err("version should be surfaced as display output");

    insta::assert_snapshot!(error.to_string());
}

#[rstest]
fn localized_missing_subcommand_is_returned_from_config_loader() {
    let localizer = build_cli_localizer(langid!("en-US"));

    let error =
        load_subcommand_config_from_iter_with_localizer(["spycatcher-harness"], localizer.as_ref())
            .expect_err("missing subcommand should fail");

    insta::assert_snapshot!(error.to_string());
}

#[rstest]
fn localized_unknown_argument_is_returned_from_config_loader() {
    let localizer = build_cli_localizer(langid!("en-US"));

    let error = load_subcommand_config_from_iter_with_localizer(
        ["spycatcher-harness", "replay", "--not-a-flag"],
        localizer.as_ref(),
    )
    .expect_err("unknown argument should fail");

    insta::assert_snapshot!(error.to_string());
}

#[rstest]
fn localized_invalid_value_is_returned_from_config_loader() {
    let localizer = build_cli_localizer(langid!("en-US"));

    let error = load_subcommand_config_from_iter_with_localizer(
        ["spycatcher-harness", "record", "--listen", "not-a-socket"],
        localizer.as_ref(),
    )
    .expect_err("invalid listen address should fail");

    insta::assert_snapshot!(error.to_string());
}

#[rstest]
fn noop_localizer_preserves_stock_unknown_argument() {
    let localizer = NoOpLocalizer::new();
    assert_stock_unknown_argument(&localizer).expect("unknown argument should use stock output");
    assert_stock_help_output(&localizer).expect("help should use stock output");
}

fn assert_stock_unknown_argument(localizer: &dyn Localizer) -> Result<(), String> {
    match load_subcommand_config_from_iter_with_localizer(
        ["spycatcher-harness", "replay", "--not-a-flag"],
        localizer,
    ) {
        Err(error) if error.to_string().contains("unexpected argument") => Ok(()),
        Err(error) => Err(format!("unknown argument was not stock output: {error}")),
        Ok(_) => Err(String::from("unknown argument unexpectedly succeeded")),
    }
}

fn assert_stock_help_output(localizer: &dyn Localizer) -> Result<(), String> {
    match load_subcommand_config_from_iter_with_localizer(
        ["spycatcher-harness", "--help"],
        localizer,
    ) {
        Err(error)
            if error
                .to_string()
                .contains("Usage: spycatcher-harness <COMMAND>") =>
        {
            Ok(())
        }
        Err(error) => Err(format!("help did not use stock output: {error}")),
        Ok(_) => Err(String::from("help unexpectedly succeeded as configuration")),
    }
}

#[rstest]
fn localize_clap_error_uses_bundled_unknown_argument_text() {
    let localizer = build_cli_localizer(langid!("en-US"));
    let command = Command::new("demo").arg(clap::arg!(--known <VALUE>));
    let error = command
        .clone()
        .try_get_matches_from(["demo", "--unknown"])
        .expect_err("unknown argument should fail");

    let localized = localize_clap_error_with_command(error, localizer.as_ref(), Some(&command));

    assert!(localized.to_string().contains("unknown argument"));
    assert!(localized.to_string().contains("--unknown"));
}

#[rstest]
fn broken_consumer_resources_fall_back_to_noop_localizer() {
    let localizer = build_cli_localizer_from_resources(langid!("en-US"), ["cli-about = {"]);

    assert!(localizer.lookup("cli-about", None).is_none());
    assert_stock_unknown_argument(localizer.as_ref())
        .expect("broken resources should preserve stock parse output");
    assert_stock_help_output(localizer.as_ref())
        .expect("broken resources should preserve stock help output");
}
