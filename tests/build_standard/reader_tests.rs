//! Unit tests for the readers in `support.rs`, over fixed command lines.

use rstest::rstest;

use super::{
    Host, assignment, dry_run, dry_run_with, expand_value,
    flags::{ConfigText, Flags, Source, read},
    make_rustflags, rustflags_in,
    shell::Line,
};

#[rstest]
#[case::single("cargo build --lib", &["cargo build --lib"])]
#[case::and("RUSTFLAGS=\"-a\" cargo doc && cargo clippy", &["RUSTFLAGS=\"-a\" cargo doc", "cargo clippy"])]
#[case::branch(
    "if command -v whitaker >/dev/null; then X=1 whitaker --all; else echo no; fi",
    &["command -v whitaker >/dev/null", "X=1 whitaker --all", "echo no", "fi"],
)]
fn commands_split_chained_and_branching_lines(#[case] line: &str, #[case] expected: &[&str]) {
    assert_eq!(Line(line).commands().expect("split the line"), expected);
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
    assert_eq!(
        Line(command).runs_cargo_or_whitaker().expect("read"),
        expected
    );
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
    assert_eq!(Line(line).commands().expect("split the line"), expected);
}

#[test]
fn an_unterminated_quote_is_an_error() {
    assert!(Line("X=\"a && cargo test").commands().is_err());
}

#[test]
fn an_empty_or_blank_caller_value_is_carried_by_any_flags() {
    let flags = Flags::from_words(&["-Zthreads=8"]);
    assert!(flags.carries(&Flags::from_text("")));
    assert!(flags.carries(&Flags::from_text("   ")));
    assert!(!flags.carries(&Flags::from_text("--cfg x")));
}

#[rstest]
#[case::split_stored_joined_caller(&["-C", "debuginfo=1"], "-Cdebuginfo=1")]
#[case::joined_stored_split_caller(&["-Cdebuginfo=1"], "-C debuginfo=1")]
fn the_two_spellings_of_a_codegen_flag_compare_equal(
    #[case] stored: &[&str],
    #[case] caller: &str,
) {
    assert!(Flags::from_words(stored).carries(&Flags::from_text(caller)));
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
    let hidden = Line("X=\"a \\\" && b\" cargo test")
        .commands()
        .expect("split");
    assert_eq!(hidden.len(), 1);
}

#[test]
fn a_flag_list_missing_or_reordering_the_callers_words_does_not_carry_them() {
    let flags = Flags::from_words(&["-Zthreads=8", "--cfg", "x"]);
    assert!(
        !flags.carries(&Flags::from_text("--cfg y")),
        "carried a missing word"
    );
    assert!(
        !flags.carries(&Flags::from_text("x --cfg")),
        "carried reordered words"
    );
    assert!(flags.equals(&Flags::from_text("-Zthreads=8 --cfg x")));
    assert!(!flags.equals(&Flags::from_text("--cfg x")));
}

#[rstest]
#[case::string(
    "rustflags = \"-Zthreads=8 -Cdebuginfo=1\"",
    Some("-Zthreads=8 -Cdebuginfo=1")
)]
#[case::array(
    "rustflags = [\"-Zthreads=8\", \"-C\", \"debuginfo=1\"]",
    Some("-Zthreads=8 -Cdebuginfo=1")
)]
#[case::absent("other = 1", None)]
fn the_accepted_rustflags_shapes_read_as_the_complete_flag_list(
    #[case] table: &str,
    #[case] expected: Option<&str>,
) {
    let value: toml::Value = toml::from_str(table).expect("parse the fixture");
    let found = (Source {
        key: "build",
        table: &value,
    })
    .flags()
    .expect("read the table");
    match (found, expected) {
        (Some(flags), Some(words)) => assert!(flags.equals(&Flags::from_text(words)), "{flags:?}"),
        (None, None) => {}
        (read, wanted) => panic!("read {read:?}, wanted {wanted:?}"),
    }
}

#[rstest]
#[case::number("rustflags = 3")]
#[case::non_string_member("rustflags = [\"-Zthreads=8\", 4]")]
fn a_malformed_rustflags_table_is_an_error_naming_the_file_and_key(#[case] table: &str) {
    let value: toml::Value = toml::from_str(table).expect("parse the fixture");
    let message = (Source {
        key: "build",
        table: &value,
    })
    .flags()
    .expect_err("malformed")
    .to_string();
    assert!(
        message.contains(".cargo/config.toml") && message.contains("[build]"),
        "{message}"
    );
}

#[test]
fn a_file_the_reader_cannot_open_is_named_with_the_operation() {
    let message = read(std::path::Path::new("no/such/file.toml"))
        .expect_err("missing")
        .to_string();
    assert!(message.contains("reading `no/such/file.toml`"), "{message}");
}

#[test]
fn text_that_is_not_toml_is_reported_as_the_configuration() {
    let message = ConfigText("rustflags = [")
        .parse()
        .expect_err("malformed")
        .to_string();
    assert!(message.contains("parsing .cargo/config.toml"), "{message}");
}

#[rstest]
#[case::text_after_a_single_quote("X='a 'b cargo test")]
#[case::text_after_a_double_quote("X=\"a \"b cargo test")]
fn text_stuck_to_a_closing_quote_is_an_error(#[case] command: &str) {
    assert!(Line(command).assignments().is_err(), "read `{command}`");
}

#[rstest]
#[case::later_empty_wins("RUSTFLAGS=\"-a\" RUSTFLAGS=\"\" cargo test", "")]
#[case::later_value_wins("RUSTFLAGS=\"\" RUSTFLAGS=\"-a\" cargo test", "-a")]
fn the_last_rustflags_assignment_wins(#[case] command: &str, #[case] expected: &str) {
    let flags = assignment(command, None).expect("read").expect("assigns");
    assert!(flags.equals(&Flags::from_text(expected)), "{flags:?}");
}

#[test]
fn a_quote_inside_a_bare_word_is_an_error() {
    assert!(Line("X=a' b' cargo test").assignments().is_err());
}

#[test]
fn a_program_that_cannot_be_spawned_fails_closed_with_its_name() {
    let message = dry_run_with("no-such-make-for-the-contract", "lint", Host::Linux, None)
        .expect_err("no such program")
        .to_string();
    for wanted in [
        "cannot run `no-such-make-for-the-contract -n lint`",
        "Linux",
    ] {
        assert!(
            message.contains(wanted),
            "`{wanted}` missing from: {message}"
        );
    }
}

#[rstest]
#[case::unterminated_quote("X=\"a && cargo test\n")]
#[case::unmodelled_expansion("RUSTFLAGS=\"$(touch x)\" cargo test\n")]
#[case::quote_inside_a_word("X=a' b' cargo test\n")]
fn a_parse_failure_names_the_route_that_produced_it(#[case] text: &str) {
    let message = rustflags_in(text, "lint", Host::Linux, None)
        .expect_err("unreadable")
        .to_string();
    assert!(
        message.contains("reading `make -n lint` on Linux"),
        "{message}"
    );
}

/// Writes an executable script into a scratch directory and returns its path.
#[cfg(unix)]
fn fake_program(scratch: &str, script: &str) -> std::path::PathBuf {
    use cap_std::{
        ambient_authority,
        fs::{Dir, OpenOptions, OpenOptionsExt},
    };
    let tmp = Dir::open_ambient_dir(env!("CARGO_TARGET_TMPDIR"), ambient_authority())
        .expect("open the scratch root");
    if let Err(error) = tmp.remove_dir_all(scratch) {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::NotFound,
            "clear the scratch: {error}"
        );
    }
    tmp.create_dir(scratch)
        .expect("create the scratch directory");
    let dir = tmp.open_dir(scratch).expect("open the scratch directory");
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o755);
    let mut file = dir.open_with("make", &options).expect("create the script");
    std::io::Write::write_all(&mut file, script.as_bytes()).expect("write the script");
    std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(scratch)
        .join("make")
}

/// A failing program's context, actual status and both streams survive.
#[cfg(unix)]
#[test]
fn a_failed_run_keeps_its_context_status_and_distinct_streams() {
    let fake = fake_program(
        "failing-make",
        "#!/bin/sh\necho OUT-PAYLOAD\necho ERR-PAYLOAD >&2\nexit 4\n",
    );
    let message = dry_run_with(&fake.display().to_string(), "lint", Host::Linux, None)
        .expect_err("the stand-in fails")
        .to_string();
    for wanted in [
        "-n lint",
        "Linux",
        "exit status: 4",
        "stdout: OUT-PAYLOAD",
        "stderr: ERR-PAYLOAD",
    ] {
        assert!(
            message.contains(wanted),
            "`{wanted}` missing from: {message}"
        );
    }
}
