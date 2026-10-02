//! Unit tests for the readers in `support.rs`, over fixed command lines.

use rstest::rstest;

use super::{assignment, commands, runs_cargo_or_whitaker};

#[rstest]
#[case::single("cargo build --lib", &["cargo build --lib"])]
#[case::and("RUSTFLAGS=\"-a\" cargo doc && cargo clippy", &["RUSTFLAGS=\"-a\" cargo doc", "cargo clippy"])]
#[case::branch(
    "if command -v whitaker >/dev/null; then X=1 whitaker --all; else echo no; fi",
    &["command -v whitaker >/dev/null", "X=1 whitaker --all", "echo no", "fi"],
)]
fn commands_split_chained_and_branching_lines(#[case] line: &str, #[case] expected: &[&str]) {
    assert_eq!(commands(line), expected);
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
