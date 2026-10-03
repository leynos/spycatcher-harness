//! Unit tests for the readers in `support.rs`, over fixed command lines.

use rstest::rstest;

use super::{
    Host, assignment, commands, dry_run, expanded, make_rustflags, runs_cargo_or_whitaker,
};

#[rstest]
#[case::single("cargo build --lib", &["cargo build --lib"])]
#[case::and("RUSTFLAGS=\"-a\" cargo doc && cargo clippy", &["RUSTFLAGS=\"-a\" cargo doc", "cargo clippy"])]
#[case::branch(
    "if command -v whitaker >/dev/null; then X=1 whitaker --all; else echo no; fi",
    &["command -v whitaker >/dev/null", "X=1 whitaker --all", "echo no", "fi"],
)]
fn commands_split_chained_and_branching_lines(#[case] line: &str, #[case] expected: &[&str]) {
    assert_eq!(commands(line).expect("split the line"), expected);
}

#[rstest]
#[case("cargo test", true)]
#[case(
    "RUSTFLAGS=\"-D warnings\" PATH=\"/x:$PATH\" /bin/whitaker --all",
    true
)]
#[case("command -v whitaker", false)]
#[case("echo whitaker not found", false)]
fn only_a_running_cargo_or_whitaker_counts(#[case] command: &str, #[case] expected: bool) {
    assert_eq!(runs_cargo_or_whitaker(command), expected);
}

#[test]
fn a_similarly_named_variable_is_not_a_rustflags_assignment() {
    let found = assignment("CARGO_ENCODED_RUSTFLAGS=\"-Zx\" cargo test", None);
    assert!(found.expect("read the command").is_none());
}

#[rstest]
#[case::unterminated("RUSTFLAGS=\"-a cargo test")]
#[case::unquoted("RUSTFLAGS=-a cargo test")]
fn a_malformed_rustflags_assignment_is_an_error(#[case] command: &str) {
    assert!(
        assignment(command, None).is_err(),
        "read `{command}` as valid"
    );
}

#[test]
fn a_target_the_makefile_lacks_is_an_error() {
    let found = make_rustflags("no-such-target-for-the-contract", Host::Linux, None);
    assert!(found.is_err(), "read a rule that does not exist");
}

#[rstest]
#[case::double_quoted("X=\"a && b\" cargo test", &["X=\"a && b\" cargo test"])]
#[case::single_quoted("echo 'a; b' && cargo test", &["echo 'a; b'", "cargo test"])]
#[case::escaped_quote("X=\"a \\\" && b\" cargo test", &["X=\"a \\\" && b\" cargo test"])]
fn a_separator_inside_quotes_does_not_split(#[case] line: &str, #[case] expected: &[&str]) {
    assert_eq!(commands(line).expect("split the line"), expected);
}

#[test]
fn an_unterminated_quote_is_an_error() {
    assert!(commands("X=\"a && cargo test").is_err());
}

#[test]
fn an_empty_or_blank_caller_value_is_carried_by_any_flags() {
    use super::Flags;
    let flags = Flags::from_words(&["-Zthreads=8"]);
    assert!(flags.carries_run(""));
    assert!(flags.carries_run("   "));
    assert!(!flags.carries_run("--cfg x"));
}

#[test]
fn a_failed_command_reports_its_status_and_stderr() {
    let from_make = dry_run("no-such-target-for-the-contract", Host::Linux, None)
        .expect_err("make has no such rule")
        .to_string();
    assert!(
        from_make.contains("failed (") && from_make.contains("No rule"),
        "{from_make}"
    );
    let from_shell = expanded("${", None)
        .expect_err("bad substitution")
        .to_string();
    assert!(from_shell.contains("could not expand"), "{from_shell}");
}
