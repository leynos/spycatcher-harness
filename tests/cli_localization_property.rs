//! Property tests for CLI command localization invariants.
//!
//! This module complements the example-driven CLI localization unit tests and
//! process-level binary snapshots by generating small command trees and
//! argument names. The properties assert that `LocalizeCmd::localize()` keeps
//! command structure intact, remains idempotent for the bundled catalogue, and
//! preserves the offending argument text when clap errors are localized.

use std::collections::BTreeSet;

use clap::{Arg, Command};
use i18n_embed::unic_langid::langid;
use ortho_config::localize_clap_error_with_command;
use proptest::prelude::*;
use rstest::rstest;
use spycatcher_harness::cli::localization::LocalizeCmd;
use spycatcher_harness::cli::localizer::build_cli_localizer;

/// Keep a raw command name paired with its raw option name before normalization.
///
/// The `help`/`help` identity remains raw until `command_spec_node` reserves the
/// option namespace and its parent assigns a unique sibling name.
#[derive(Clone, Debug)]
struct CommandIdentity {
    /// Raw command name retained when the parent assigns its sibling ordinal.
    command_name: String,
    /// Raw option name placed in the reserved-safe namespace by the node builder.
    option_name: String,
}

#[derive(Clone, Debug)]
struct CommandSpec {
    name: String,
    arg: String,
    subcommands: Vec<Self>,
}

fn identifier() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9-]{1,8}".prop_map(String::from)
}

/// Keep arbitrary raw identifiers outside the fixture's recognized long options.
///
/// A raw `help` becomes `unknown-help`, preserving its text while avoiding both
/// the built-in `--help` action and the fixture's `--known` option.
fn unknown_argument_name(raw: &str) -> String {
    format!("unknown-{raw}")
}

/// Localize genuinely unknown options even when their raw names are reserved.
#[rstest]
#[case::help("help")]
#[case::known("known")]
#[case::version("version")]
fn reserved_raw_names_produce_localized_unknown_arguments(#[case] raw: &str) {
    let localizer = build_cli_localizer(langid!("en-US"));
    let command = Command::new("spycatcher-harness").arg(Arg::new("known").long("known"));
    let unknown_argument = format!("--{}", unknown_argument_name(raw));
    let error = command
        .clone()
        .try_get_matches_from(["spycatcher-harness", unknown_argument.as_str()])
        .expect_err("namespaced option should remain unknown");

    assert_eq!(
        error.kind(),
        clap::error::ErrorKind::UnknownArgument,
        "the raw name must not select help or a recognized option"
    );
    let localized = localize_clap_error_with_command(error, localizer.as_ref(), Some(&command));
    let rendered = localized.to_string();

    assert!(
        rendered.contains("unknown argument"),
        "localized error must describe the unknown option: {rendered}"
    );
    assert!(
        rendered.contains(&unknown_argument),
        "localized error must retain the complete option {unknown_argument}: {rendered}"
    );
}

/// Generate bounded trees without rejecting or removing colliding children.
///
/// Every shrink remaps siblings through `command_spec_node`, so two children
/// both drawn as `b-` retain their order and distinct command names.
fn command_spec() -> impl Strategy<Value = CommandSpec> {
    command_spec_leaf().prop_recursive(2, 8, 2, |inner| {
        (
            identifier(),
            identifier(),
            proptest::collection::vec(inner, 0..3),
        )
            .prop_map(|(name, arg, subcommands)| {
                command_spec_node(
                    CommandIdentity {
                        command_name: name,
                        option_name: arg,
                    },
                    subcommands,
                )
            })
    })
}

/// Generate a terminal command with an argument outside Clap's built-in names.
///
/// A drawn argument `help` becomes `option-help`, retaining random material
/// without colliding with the automatically generated `--help` flag.
fn command_spec_leaf() -> impl Strategy<Value = CommandSpec> {
    (identifier(), identifier()).prop_map(|(name, arg)| {
        command_spec_node(
            CommandIdentity {
                command_name: name,
                option_name: arg,
            },
            Vec::new(),
        )
    })
}

/// Keep every child while assigning sibling names and reserving built-in flags.
///
/// Two children named `b-` become `child-0-b-` and `child-1-b-`; an argument
/// named `version` becomes `option-version` beside Clap's version flag.
fn command_spec_node(identity: CommandIdentity, mut subcommands: Vec<CommandSpec>) -> CommandSpec {
    let CommandIdentity {
        command_name: name,
        option_name: arg,
    } = identity;
    for (index, child) in subcommands.iter_mut().enumerate() {
        child.name = format!("child-{index}-{}", child.name);
    }
    CommandSpec {
        name,
        arg: format!("option-{arg}"),
        subcommands,
    }
}

/// Build two levels of equal-name siblings without dropping either branch.
///
/// A raw `help`/`help` identity retains all seven nodes while sibling names
/// avoid the built-in help subcommand at both levels.
fn duplicate_sibling_tree(identity: CommandIdentity) -> CommandSpec {
    let leaf = command_spec_node(identity.clone(), Vec::new());
    let branch = command_spec_node(identity.clone(), vec![leaf.clone(), leaf]);
    command_spec_node(
        CommandIdentity {
            command_name: "root".to_owned(),
            option_name: identity.option_name,
        },
        vec![branch.clone(), branch],
    )
}

/// Check sibling uniqueness independently of the ordinal naming construction.
///
/// A tree containing two siblings both named `b-` returns false, even if their
/// argument names differ; grandchildren are checked in their own parent scope.
fn has_unique_sibling_names(spec: &CommandSpec) -> bool {
    let names: BTreeSet<_> = spec.subcommands.iter().map(|child| &child.name).collect();
    names.len() == spec.subcommands.len() && spec.subcommands.iter().all(has_unique_sibling_names)
}

/// Retain both differently configured siblings from the persisted shrunk failure.
#[test]
fn shrunk_duplicate_siblings_remain_valid() {
    let first = command_spec_node(
        CommandIdentity {
            command_name: "b-".to_owned(),
            option_name: "a0".to_owned(),
        },
        Vec::new(),
    );
    let second = command_spec_node(
        CommandIdentity {
            command_name: "b-".to_owned(),
            option_name: "aa".to_owned(),
        },
        Vec::new(),
    );
    let spec = command_spec_node(
        CommandIdentity {
            command_name: "a0".to_owned(),
            option_name: "aa".to_owned(),
        },
        vec![first, second],
    );

    assert_eq!(
        spec.subcommands.len(),
        2,
        "neither shrunk sibling may be discarded"
    );
    assert!(
        has_unique_sibling_names(&spec),
        "the shrunk siblings must be distinct"
    );
    assert_eq!(
        spec.subcommands
            .iter()
            .map(|child| child.arg.as_str())
            .collect::<Vec<_>>(),
        vec!["option-a0", "option-aa"],
        "the siblings' different arguments must survive"
    );
    command_from_spec(&spec).version("0.1.0").debug_assert();
}

/// Preserve duplicate-name branches and validate Clap's built-in name boundary.
#[rstest]
#[case::auto_help(CommandIdentity { command_name: "help".to_owned(), option_name: "help".to_owned() })]
#[case::auto_version(CommandIdentity { command_name: "version".to_owned(), option_name: "version".to_owned() })]
fn duplicate_names_remain_valid_in_nested_trees(#[case] identity: CommandIdentity) {
    let spec = duplicate_sibling_tree(identity);

    assert_eq!(
        spec.subcommands.len(),
        2,
        "both top-level siblings must remain"
    );
    assert!(
        spec.subcommands
            .iter()
            .all(|child| child.subcommands.len() == 2),
        "both grandchildren must remain under each sibling"
    );
    assert!(
        has_unique_sibling_names(&spec),
        "siblings must have unique names at every level"
    );
    command_from_spec(&spec).version("0.1.0").debug_assert();
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
    /// Validate every generated tree against Clap without removing nested nodes.
    #[test]
    fn generated_command_trees_satisfy_clap_invariants(spec in command_spec()) {
        prop_assert!(has_unique_sibling_names(&spec));
        command_from_spec(&spec).version("0.1.0").debug_assert();
    }

    /// Keep equal random sibling names valid at two deliberately populated levels.
    #[test]
    fn equal_random_names_preserve_nested_siblings(name in identifier(), arg in identifier()) {
        let spec = duplicate_sibling_tree(CommandIdentity { command_name: name, option_name: arg });

        prop_assert_eq!(spec.subcommands.len(), 2);
        prop_assert!(spec.subcommands.iter().all(|child| child.subcommands.len() == 2));
        prop_assert!(has_unique_sibling_names(&spec));
        command_from_spec(&spec).version("0.1.0").debug_assert();
    }

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
        unknown in identifier().prop_map(|raw| unknown_argument_name(&raw)),
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
