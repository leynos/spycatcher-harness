//! Contract test for where CI installs `mold` relative to the gates.
//!
//! The Makefile restates the build standard's `mold` flag for its gate
//! targets, and coverage links Linux builds with it, so the coverage job must
//! install `mold` before `make lint` and before the coverage step. The test
//! reads `ci.yml` as text, ignoring comments, and its reader is held by unit
//! tests over fixed workflows, including a late install and none.
//!
//! File access goes through a `cap_std` directory handle rooted at the crate
//! manifest directory.

use std::error::Error;

use cap_std::{ambient_authority, fs::Dir};

/// The result of a reader, which the tests unwrap.
type Read<T> = Result<T, Box<dyn Error>>;

/// Reads a file relative to the crate manifest directory.
fn read(path: &str) -> Read<String> {
    let root = Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority())?;
    Ok(root.read_to_string(path)?)
}

/// Returns the number of leading spaces on a line.
fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Returns whether a line is blank or a comment, and so evidence of nothing.
fn is_inert(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.is_empty() || trimmed.starts_with('#')
}

/// Returns the lines of the job that contains the first line holding `needle`.
///
/// A job is a two-space-indented key under `jobs:`, running to the next one.
fn job_containing<'a>(workflow: &'a str, needle: &str) -> Vec<&'a str> {
    let lines: Vec<&str> = workflow.lines().collect();
    let starts_job = |line: &str| indent(line) == 2 && line.trim_end().ends_with(':');
    let bounds = lines
        .iter()
        .position(|line| !is_inert(line) && line.contains(needle))
        .and_then(|found| {
            let start = (0..=found).rfind(|&i| lines.get(i).is_some_and(|l| starts_job(l)))?;
            let end = (found + 1..lines.len())
                .find(|&i| lines.get(i).is_some_and(|l| starts_job(l)))
                .unwrap_or(lines.len());
            Some(start..end)
        });
    bounds
        .and_then(|range| lines.get(range))
        .map(<[&str]>::to_vec)
        .unwrap_or_default()
}

/// Returns the offset of the first line that installs mold: an `apt` install
/// naming it, or setup-rust's `install-mold` input set to true.
fn mold_install_offset(job: &[&str]) -> Option<usize> {
    job.iter().position(|line| {
        let words = line.trim();
        !is_inert(line)
            && ((words.contains("apt") && words.contains("install") && words.contains("mold"))
                || (words.starts_with("install-mold:") && words.contains("true")))
    })
}

/// Returns the offset of the first non-comment line holding `needle`.
fn offset_of(job: &[&str], needle: &str) -> Option<usize> {
    job.iter()
        .position(|line| !is_inert(line) && line.contains(needle))
}

#[test]
fn ci_installs_mold_before_lint_and_coverage() {
    let workflow = read(".github/workflows/ci.yml").expect("read ci.yml");
    let job = job_containing(&workflow, "generate-coverage@");
    let installed = mold_install_offset(&job).expect("the coverage job never installs mold");
    for needle in ["make lint", "generate-coverage@"] {
        let at = offset_of(&job, needle).expect("the job lacks a gate step");
        assert!(installed < at, "mold is installed after `{needle}`");
    }
}

/// A workflow whose coverage step carries its overrides in `env`, as required.
#[cfg(test)]
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

#[test]
fn the_mold_reader_rejects_a_late_or_missing_install() {
    let good = job_containing(GOOD_WORKFLOW, "generate-coverage@");
    let good_install = mold_install_offset(&good).expect("the good workflow installs mold");
    assert!(good_install < offset_of(&good, "make lint").expect("lint"));
    let late_text = GOOD_WORKFLOW
        .replace("run: sudo apt-get install --yes mold", "run: echo skipped")
        .replace(
            "      - run: make lint\n",
            "      - run: make lint\n      - run: sudo apt-get install mold\n",
        );
    let late = job_containing(&late_text, "generate-coverage@");
    let late_install = mold_install_offset(&late).expect("the late workflow installs mold");
    assert!(late_install > offset_of(&late, "make lint").expect("lint"));
    let none_text = GOOD_WORKFLOW.replace("sudo apt-get install --yes mold", "true");
    assert!(mold_install_offset(&job_containing(&none_text, "generate-coverage@")).is_none());
}
