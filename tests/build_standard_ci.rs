//! Contract test for where CI installs `mold` relative to the gates.
//!
//! The Makefile restates the build standard's `mold` flag for its gate targets,
//! so the job must install `mold` before `make lint`, and before the coverage
//! step, which builds the same tree on the same runner. Coverage takes the
//! configured linker flags only when its effective `RUSTFLAGS` leave them in
//! place (CI's setup-rust exports `RUSTFLAGS`, which displaces them), so the
//! requirement is that the linker be present for every step that may link, not
//! that coverage use it. The test reads `ci.yml` as text, ignoring comments, and
//! its reader (`build_standard/workflow.rs`) is held by tests over fixed
//! workflows, including a late install, none, and text that is not a command.
//!
//! File access goes through a `cap_std` directory handle rooted at the crate
//! manifest directory.

use std::error::Error;

use cap_std::{ambient_authority, fs::Dir};
use rstest::rstest;

#[path = "build_standard/workflow.rs"]
mod workflow;

use workflow::Job;

/// The result of a reader, which the tests unwrap.
type Read<T> = Result<T, Box<dyn Error>>;

/// Reads a file relative to the crate manifest directory.
fn read(path: &str) -> Read<String> {
    let root = Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority())?;
    Ok(root.read_to_string(path)?)
}

/// The coverage job's workflow, with one slot for the mold installation.
const GOOD_WORKFLOW: &str = concat!(
    "jobs:\n",
    "  build-test:\n",
    "    steps:\n",
    "      - name: Install mold linker\n",
    "        run: sudo apt-get install --yes mold\n",
    "      - run: make lint\n",
    "      - name: Coverage\n",
    "        uses: org/actions/generate-coverage@abc\n",
    "        env:\n",
    "          CARGO_UNSTABLE_CODEGEN_BACKEND: \"true\"\n",
    "          CARGO_PROFILE_DEV_CODEGEN_BACKEND: llvm\n",
    "        with:\n",
    "          format: lcov\n",
);

/// The installation step of [`GOOD_WORKFLOW`], which the fixtures replace.
const INSTALL_STEP: &str =
    "      - name: Install mold linker\n        run: sudo apt-get install --yes mold\n";

/// The coverage job of a workflow text.
fn coverage_job(workflow: &str) -> Job<'_> {
    Job::containing(workflow, "generate-coverage@")
}

/// The one validator of the install order: mold is installed, and before every
/// gate step that may link. It is the check on the real workflow and on every
/// fixture, so a fixture that fails it is a fixture the real check would fail.
fn order_problem(workflow: &str) -> Option<String> {
    let job = coverage_job(workflow);
    let installed = match job.mold_install_offset() {
        Ok(Some(installed)) => installed,
        Ok(None) => return Some("the coverage job never installs mold".to_owned()),
        Err(error) => return Some(format!("unrecognised workflow form: {error}")),
    };
    for needle in ["make lint", "generate-coverage@"] {
        let Some(at) = job.offset_of(needle) else {
            return Some(format!("the job lacks the `{needle}` gate step"));
        };
        if installed >= at {
            return Some(format!("mold is installed after `{needle}`"));
        }
    }
    None
}

#[test]
fn ci_installs_mold_before_lint_and_coverage() {
    let workflow = read(".github/workflows/ci.yml").expect("read ci.yml");
    assert_eq!(order_problem(&workflow), None);
}

#[test]
fn the_validator_accepts_the_good_workflow() {
    assert_eq!(order_problem(GOOD_WORKFLOW), None);
}

/// Each fixture breaks the install order in one way, and the validator that
/// checks the real workflow must name it: no install, and an install that comes
/// after lint.
#[rstest]
#[case::missing(
    GOOD_WORKFLOW.replace("sudo apt-get install --yes mold", "true"),
    "never installs mold"
)]
#[case::after_lint(
    GOOD_WORKFLOW
        .replace("run: sudo apt-get install --yes mold", "run: echo skipped")
        .replace(
            "      - run: make lint\n",
            "      - run: make lint\n      - run: sudo apt-get install mold\n",
        ),
    "after `make lint`"
)]
fn the_validator_rejects_a_missing_or_late_install(
    #[case] workflow: String,
    #[case] problem: &str,
) {
    let found = order_problem(&workflow).expect("the validator accepted a broken order");
    assert!(found.contains(problem), "{found}");
}

#[rstest]
#[case::real_install("      - run: sudo apt-get install --yes mold\n", true)]
#[case::inert_echo("      - run: echo sudo apt-get install mold\n", false)]
#[case::step_first_if(
    "      - if: false\n        run: sudo apt-get install --yes mold\n",
    false
)]
#[case::later_if(
    "      - run: sudo apt-get install --yes mold\n        if: false\n",
    false
)]
#[case::quoted_echo("      - run: echo \"x && sudo apt-get install mold\"\n", false)]
#[case::description_text("      - description: sudo apt-get install mold\n", false)]
#[case::input_quoted_true(
    "      - uses: leynos/shared-actions/.github/actions/setup-rust@abc\n        with:\n          install-mold: 'true'\n",
    true
)]
#[case::input_not_exactly_true(
    "      - uses: leynos/shared-actions/.github/actions/setup-rust@abc\n        with:\n          install-mold: untrue\n",
    false
)]
#[case::input_in_env_not_with(
    "      - uses: leynos/shared-actions/.github/actions/setup-rust@abc\n        env:\n          install-mold: true\n",
    false
)]
#[case::lookalike_action(
    "      - uses: org/not-setup-rust-really@abc\n        with:\n          install-mold: true\n",
    false
)]
#[case::block_run(
    "      - run: |\n          sudo apt-get update\n          sudo apt-get install mold\n",
    true
)]
#[case::setup_rust_input(
    "      - uses: leynos/shared-actions/.github/actions/setup-rust@abc\n        with:\n          install-mold: true\n",
    true
)]
#[case::unrelated_action_input(
    "      - uses: org/other-action@abc\n        with:\n          install-mold: true\n",
    false
)]
fn the_install_reader_counts_only_runnable_installs(#[case] step: &str, #[case] counts: bool) {
    let workflow = GOOD_WORKFLOW.replace(INSTALL_STEP, step);
    let found = coverage_job(&workflow)
        .mold_install_offset()
        .expect("a recognised form");
    assert_eq!(found.is_some(), counts, "{step}");
}

/// The reader judges this repository's own workflow forms and rejects any other
/// with a named error, so a form it cannot model is never read as an install or
/// as no install.
#[rstest]
#[case::folded_echo(
    "      - run: >\n          echo skipped\n          sudo apt-get install mold\n",
    "folded scalar"
)]
#[case::folded_install(
    "      - run: >\n          sudo apt-get install\n          --yes mold\n",
    "folded scalar"
)]
#[case::comment_after_false(
    "      - uses: leynos/shared-actions/.github/actions/setup-rust@abc\n        with:\n          install-mold: false # true\n",
    "trailing comment"
)]
#[case::comment_after_true(
    "      - uses: leynos/shared-actions/.github/actions/setup-rust@abc\n        with:\n          install-mold: true # needed\n",
    "trailing comment"
)]
fn the_reader_rejects_a_form_it_does_not_recognise(#[case] step: &str, #[case] error: &str) {
    let workflow = GOOD_WORKFLOW.replace(INSTALL_STEP, step);
    let message = coverage_job(&workflow)
        .mold_install_offset()
        .expect_err("the reader accepted an unrecognised form");
    assert!(message.contains(error), "{message}");
}

/// The validator turns an unrecognised form into a rejection of the workflow, so
/// the real check fails on it as well.
#[test]
fn the_validator_rejects_a_workflow_with_an_unrecognised_form() {
    let workflow = GOOD_WORKFLOW.replace(INSTALL_STEP, "      - run: >\n          true\n");
    let found = order_problem(&workflow).expect("the validator accepted a folded scalar");
    assert!(found.contains("unrecognised workflow form"), "{found}");
}
