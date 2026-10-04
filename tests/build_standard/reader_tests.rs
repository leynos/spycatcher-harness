//! Unit tests for the readers in `support.rs`, over fixed command lines.

use rstest::rstest;

use super::{
    Host, assignment, dry_run, expand_value,
    flags::table_flags,
    make_rustflags,
    shell::{commands, runs_cargo_or_whitaker},
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
#[case("X='a b' cargo test", true)]
#[case("command -v whitaker", false)]
#[case("echo whitaker not found", false)]
fn only_a_running_cargo_or_whitaker_counts(#[case] command: &str, #[case] expected: bool) {
    assert_eq!(runs_cargo_or_whitaker(command).expect("read"), expected);
}

#[test]
fn a_similarly_named_variable_is_not_a_rustflags_assignment() {
    let found = assignment("CARGO_ENCODED_RUSTFLAGS=\"-Zx\" cargo test", None);
    assert!(found.expect("read the command").is_none());
}

#[rstest]
#[case::double("RUSTFLAGS=\"-a cargo test")]
#[case::single("RUSTFLAGS='-a cargo test")]
fn an_unterminated_assignment_is_an_error(#[case] command: &str) {
    assert!(
        assignment(command, None).is_err(),
        "read `{command}` as valid"
    );
}

#[test]
fn only_a_leading_assignment_counts() {
    let after = assignment("cargo test -- RUSTFLAGS=\"-a\"", None).expect("read");
    assert!(after.is_none(), "an argument was read as an assignment");
    let single = assignment("RUSTFLAGS='-a $x' cargo test", None).expect("read");
    assert!(
        single.is_some_and(|flags| flags.names("$x")),
        "a single-quoted value was expanded"
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

#[rstest]
#[case::split_stored_joined_caller(&["-C", "debuginfo=1"], "-Cdebuginfo=1")]
#[case::joined_stored_split_caller(&["-Cdebuginfo=1"], "-C debuginfo=1")]
fn the_two_spellings_of_a_codegen_flag_compare_equal(
    #[case] stored: &[&str],
    #[case] caller: &str,
) {
    use super::Flags;
    assert!(Flags::from_words(stored).carries_run(caller));
}

#[test]
fn a_failed_make_reports_its_context_status_and_both_streams() {
    let message = dry_run("no-such-target-for-the-contract", Host::Linux, None)
        .expect_err("make has no such rule")
        .to_string();
    for wanted in [
        "make -n no-such-target-for-the-contract",
        "Linux",
        "failed (",
        "stdout:",
        "stderr:",
        "No rule",
    ] {
        assert!(
            message.contains(wanted),
            "`{wanted}` missing from: {message}"
        );
    }
}

#[rstest]
#[case::none("${RUSTFLAGS:+$RUSTFLAGS }-D warnings", None, "-D warnings")]
#[case::caller(
    "${RUSTFLAGS:+$RUSTFLAGS }-D warnings",
    Some("--cfg x"),
    "--cfg x -D warnings"
)]
#[case::empty_caller("${RUSTFLAGS:+$RUSTFLAGS }-D warnings", Some(""), "-D warnings")]
#[case::forwarded("${RUSTFLAGS-}", Some("--cfg x"), "--cfg x")]
fn the_modelled_expansions_are_evaluated(
    #[case] value: &str,
    #[case] inherited: Option<&str>,
    #[case] expected: &str,
) {
    assert_eq!(expand_value(value, inherited).expect("expand"), expected);
}

#[rstest]
#[case::command_substitution("$(touch pwned) -D warnings")]
#[case::backtick("`id`")]
#[case::other_variable("$HOME")]
#[case::backslash("a \\\" b")]
fn an_unmodelled_expansion_is_an_error_not_a_guess(#[case] value: &str) {
    assert!(expand_value(value, None).is_err(), "expanded `{value}`");
}

#[test]
fn an_escaped_quote_does_not_end_the_assigned_value() {
    let found = assignment("RUSTFLAGS=\"a \\\" b\" cargo test", None);
    assert!(found.is_err(), "the escaped quote ended the value early");
    let hidden = commands("X=\"a \\\" && b\" cargo test").expect("split");
    assert_eq!(hidden.len(), 1);
}

#[test]
fn a_flag_list_missing_or_reordering_the_callers_words_does_not_carry_them() {
    use super::Flags;
    let flags = Flags::from_words(&["-Zthreads=8", "--cfg", "x"]);
    assert!(!flags.carries_run("--cfg y"), "carried a missing word");
    assert!(!flags.carries_run("x --cfg"), "carried reordered words");
    assert!(flags.is_exactly("-Zthreads=8 --cfg x"));
    assert!(!flags.is_exactly("--cfg x"));
}

#[rstest]
#[case::string("rustflags = \"-Zthreads=8 -Cdebuginfo=1\"", Some(2))]
#[case::array("rustflags = [\"-Zthreads=8\", \"-C\", \"debuginfo=1\"]", Some(2))]
#[case::absent("other = 1", None)]
fn the_accepted_rustflags_shapes_read_as_flags(#[case] table: &str, #[case] count: Option<usize>) {
    let value: toml::Value = toml::from_str(table).expect("parse the fixture");
    let found = table_flags("build", &value).expect("read the table");
    assert_eq!(
        found.map(|flags| flags.names("-Zthreads=8")),
        count.map(|_| true)
    );
}

#[rstest]
#[case::number("rustflags = 3")]
#[case::non_string_member("rustflags = [\"-Zthreads=8\", 4]")]
fn a_malformed_rustflags_table_is_an_error_naming_the_file_and_key(#[case] table: &str) {
    let value: toml::Value = toml::from_str(table).expect("parse the fixture");
    let message = table_flags("build", &value)
        .expect_err("malformed")
        .to_string();
    assert!(
        message.contains(".cargo/config.toml") && message.contains("[build]"),
        "{message}"
    );
}
