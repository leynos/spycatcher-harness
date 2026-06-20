//! Property tests for CLI command localization invariants.
//!
//! This module complements the example-driven CLI localization unit tests and
//! process-level binary snapshots by generating small command trees and
//! argument names. The properties assert that `LocalizeCmd::localize()` keeps
//! command structure intact, remains idempotent for the bundled catalogue, and
//! preserves the offending argument text when clap errors are localized.

use clap::{Arg, Command};
use i18n_embed::unic_langid::langid;
use ortho_config::localize_clap_error_with_command;
use proptest::prelude::*;
use spycatcher_harness::cli::localization::LocalizeCmd;
use spycatcher_harness::cli::localizer::build_cli_localizer;

#[derive(Clone, Debug)]
struct CommandSpec {
    name: String,
    arg: String,
    subcommands: Vec<Self>,
}

fn identifier() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{1,8}".prop_map(String::from)
}

fn command_spec() -> impl Strategy<Value = CommandSpec> {
    command_spec_leaf().prop_recursive(2, 8, 2, |inner| {
        (
            identifier(),
            identifier(),
            proptest::collection::vec(inner, 0..3),
        )
            .prop_map(|(name, arg, subcommands)| CommandSpec {
                name,
                arg,
                subcommands,
            })
    })
}

fn command_spec_leaf() -> impl Strategy<Value = CommandSpec> {
    (identifier(), identifier()).prop_map(|(name, arg)| CommandSpec {
        name,
        arg,
        subcommands: Vec::new(),
    })
}

fn command_from_spec(spec: &CommandSpec) -> Command {
    spec.subcommands.iter().fold(
        Command::new(spec.name.clone()).arg(Arg::new(spec.arg.clone()).long(spec.arg.clone())),
        |command, child| command.subcommand(command_from_spec(child)),
    )
}

fn shape(command: &Command) -> Vec<(String, Vec<String>)> {
    let mut entries = vec![(
        command.get_name().to_owned(),
        command
            .get_arguments()
            .map(|arg| arg.get_id().to_string())
            .collect(),
    )];
    for subcommand in command.get_subcommands() {
        entries.extend(shape(subcommand));
    }
    entries
}

fn command_render(command: &mut Command) -> Result<String, TestCaseError> {
    let mut rendered = Vec::new();
    command
        .write_long_help(&mut rendered)
        .map_err(|error| TestCaseError::fail(error.to_string()))?;
    String::from_utf8(rendered).map_err(|error| TestCaseError::fail(error.to_string()))
}

proptest! {
    #[test]
    fn localize_preserves_generated_command_structure(spec in command_spec()) {
        let command = command_from_spec(&spec);
        let expected_shape = shape(&command);
        let localizer = build_cli_localizer(langid!("en-US"));

        let localized = command.localize(localizer.as_ref());

        prop_assert_eq!(shape(&localized), expected_shape);
    }

    #[test]
    fn localize_is_idempotent_for_bundled_cli_copy(spec in command_spec()) {
        let command = command_from_spec(&spec).version("0.1.0");
        let localizer = build_cli_localizer(langid!("en-US"));
        let mut once = command.clone().localize(localizer.as_ref());
        let mut twice = command.localize(localizer.as_ref()).localize(localizer.as_ref());

        prop_assert_eq!(command_render(&mut once)?, command_render(&mut twice)?);
    }

    #[test]
    fn localized_unknown_argument_preserves_argument_text(
        unknown in identifier().prop_filter("must differ from known flag", |value| value != "known"),
    ) {
        let localizer = build_cli_localizer(langid!("en-US"));
        let command = Command::new("spycatcher-harness").arg(Arg::new("known").long("known"));
        let unknown_argument = format!("--{unknown}");
        let error = command
            .clone()
            .try_get_matches_from(["spycatcher-harness", unknown_argument.as_str()])
            .expect_err("generated unknown argument should fail");

        let localized = localize_clap_error_with_command(error, localizer.as_ref(), Some(&command));
        let rendered = localized.to_string();

        prop_assert!(rendered.contains("unknown argument"));
        prop_assert!(rendered.contains(&unknown_argument));
    }
}
