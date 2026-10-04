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

#[test]
fn ci_installs_mold_before_lint_and_coverage() {
    let workflow = read(".github/workflows/ci.yml").expect("read ci.yml");
    let job = coverage_job(&workflow);
    let installed = job
        .mold_install_offset()
        .expect("the coverage job never installs mold");
    for needle in ["make lint", "generate-coverage@"] {
        let at = job.offset_of(needle).expect("the job lacks a gate step");
        assert!(installed < at, "mold is installed after `{needle}`");
    }
}

#[test]
fn the_mold_reader_rejects_a_late_or_missing_install() {
    let good = coverage_job(GOOD_WORKFLOW);
    let good_install = good
        .mold_install_offset()
        .expect("the good workflow installs mold");
    assert!(good_install < good.offset_of("make lint").expect("lint"));
    let late_text = GOOD_WORKFLOW
        .replace("run: sudo apt-get install --yes mold", "run: echo skipped")
        .replace(
            "      - run: make lint\n",
            "      - run: make lint\n      - run: sudo apt-get install mold\n",
        );
    let late = coverage_job(&late_text);
    let late_install = late
        .mold_install_offset()
        .expect("the late workflow installs mold");
    assert!(late_install > late.offset_of("make lint").expect("lint"));
    let none_text = GOOD_WORKFLOW.replace("sudo apt-get install --yes mold", "true");
    assert!(coverage_job(&none_text).mold_install_offset().is_none());
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
#[case::block_run(
    "      - run: |\n          sudo apt-get update\n          sudo apt-get install mold\n",
    true
)]
#[case::setup_rust_input(
    "      - uses: org/setup-rust@abc\n        with:\n          install-mold: true\n",
    true
)]
#[case::unrelated_action_input(
    "      - uses: org/other-action@abc\n        with:\n          install-mold: true\n",
    false
)]
fn the_install_reader_counts_only_runnable_installs(#[case] step: &str, #[case] counts: bool) {
    let workflow = GOOD_WORKFLOW.replace(INSTALL_STEP, step);
    assert_eq!(
        coverage_job(&workflow).mold_install_offset().is_some(),
        counts,
        "{step}"
    );
}
