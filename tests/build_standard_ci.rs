//! Contract test for where CI installs `mold` relative to the gates.
//!
//! The Makefile restates the build standard's `mold` flag for its gate targets,
//! so the job must install `mold` before `make lint`, and before the coverage
//! step, which builds the same tree on the same runner. (CI's setup-rust
//! exports `RUSTFLAGS`, which displaces the configured linker flags for
//! coverage, but the linker still has to be present for every step that links.)
//! The test reads `ci.yml` as text, ignoring comments, and its reader is held by
//! tests over fixed workflows, including a late install and none.
//!
//! File access goes through a `cap_std` directory handle rooted at the crate
//! manifest directory.

use std::error::Error;

use cap_std::{ambient_authority, fs::Dir};
use rstest::rstest;

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

/// Splits a shell command line at every `&&` that sits outside quotes.
fn chain(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut start, mut quote) = (0, None);
    let bytes = text.as_bytes();
    for (at, c) in text.char_indices() {
        match (quote, c) {
            (Some(open), _) if open == c => quote = None,
            (None, '"' | '\'') => quote = Some(c),
            (None, '&') if bytes.get(at + 1) == Some(&b'&') && at >= start => {
                parts.push(text.get(start..at).unwrap_or_default());
                start = at + 2;
            }
            _ => {}
        }
    }
    parts.push(text.get(start..).unwrap_or_default());
    parts
}

/// Returns whether one command runs `apt`/`apt-get ... install` naming mold:
/// the command itself, not an `echo` of one.
fn segment_installs_mold(segment: &str) -> bool {
    let words: Vec<&str> = segment
        .split_whitespace()
        .skip_while(|w| *w == "sudo")
        .collect();
    words
        .first()
        .is_some_and(|first| matches!(*first, "apt" | "apt-get"))
        && words.contains(&"install")
        && words.contains(&"mold")
}

/// Returns the bounds of the step holding line `at`: from its `- ` line to the
/// next step at the same or a shallower indent.
fn step_bounds(job: &[&str], at: usize) -> Option<std::ops::Range<usize>> {
    let starts_step = |line: &str| line.trim_start().starts_with("- ");
    let start = (0..=at).rfind(|&i| job.get(i).copied().is_some_and(starts_step))?;
    let start_indent = job.get(start).map_or(0, |line| indent(line));
    let end = (start + 1..job.len())
        .find(|&i| {
            job.get(i)
                .copied()
                .is_some_and(|line| starts_step(line) && indent(line) <= start_indent)
        })
        .unwrap_or(job.len());
    Some(start..end)
}

/// Returns whether the step holding line `at` carries an `if:` condition, so
/// its commands may be skipped. The condition can be the step's first key.
fn step_is_conditional(job: &[&str], at: usize) -> bool {
    step_bounds(job, at)
        .and_then(|bounds| job.get(bounds))
        .is_some_and(|step| {
            step.iter().any(|line| {
                line.trim_start()
                    .trim_start_matches("- ")
                    .starts_with("if:")
            })
        })
}

/// Returns whether the step holding line `at` runs the setup-rust action.
fn step_uses_setup_rust(job: &[&str], at: usize) -> bool {
    step_bounds(job, at)
        .and_then(|bounds| job.get(bounds))
        .is_some_and(|step| {
            step.iter().any(|line| {
                let text = line.trim_start().trim_start_matches("- ");
                text.starts_with("uses:") && text.contains("setup-rust")
            })
        })
}

/// Returns every executable `run` command of a job, with its line offset: the
/// inline text of `run: cmd`, and each line of a `run: |` or `run: >` block.
/// Text under any other key, such as a `name` or a description, is not a command.
fn run_commands<'a>(job: &[&'a str]) -> Vec<(usize, &'a str)> {
    let mut found = Vec::new();
    let mut block: Option<usize> = None;
    for (at, line) in job.iter().enumerate() {
        if is_inert(line) {
            continue;
        }
        if let Some(key_indent) = block {
            if indent(line) > key_indent {
                found.push((at, line.trim()));
                continue;
            }
            block = None;
        }
        let text = line.trim().trim_start_matches("- ");
        let Some(rest) = text.strip_prefix("run:") else {
            continue;
        };
        let dash = if line.trim_start().starts_with("- ") {
            2
        } else {
            0
        };
        let inline = rest.trim();
        if inline.starts_with(['|', '>']) {
            block = Some(indent(line) + dash);
        } else if !inline.is_empty() {
            found.push((at, inline));
        }
    }
    found
}

/// Returns the offset of the first installation of mold: an `apt` install
/// command that the job actually runs, in a step with no `if:` condition, or
/// setup-rust's `install-mold` input set to true in an unconditional step.
fn mold_install_offset(job: &[&str]) -> Option<usize> {
    let commands = run_commands(job)
        .into_iter()
        .filter(|(at, text)| {
            !step_is_conditional(job, *at) && chain(text).into_iter().any(segment_installs_mold)
        })
        .map(|(at, _)| at);
    let inputs = job
        .iter()
        .enumerate()
        .filter(|(at, line)| {
            let text = line.trim();
            !is_inert(line)
                && text.starts_with("install-mold:")
                && text.contains("true")
                && step_uses_setup_rust(job, *at)
                && !step_is_conditional(job, *at)
        })
        .map(|(at, _)| at);
    commands.chain(inputs).min()
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

#[rstest]
#[case::inert_echo("        run: echo sudo apt-get install mold\n", false)]
#[case::real_install("        run: sudo apt-get install --yes mold\n", true)]
fn only_a_real_install_command_counts(#[case] step: &str, #[case] counts: bool) {
    let workflow = GOOD_WORKFLOW.replace("        run: sudo apt-get install --yes mold\n", step);
    let job = job_containing(&workflow, "generate-coverage@");
    assert_eq!(mold_install_offset(&job).is_some(), counts);
}

#[test]
fn a_conditionally_skipped_install_does_not_count() {
    let workflow = GOOD_WORKFLOW.replace(
        "      - name: Install mold linker\n",
        "      - name: Install mold linker\n        if: false\n",
    );
    let job = job_containing(&workflow, "generate-coverage@");
    assert!(mold_install_offset(&job).is_none());
}

#[rstest]
#[case::step_first_if(
    "      - if: false\n        run: sudo apt-get install --yes mold\n",
    false
)]
#[case::quoted_echo("        run: echo \"x && sudo apt-get install mold\"\n", false)]
#[case::description_text("        description: sudo apt-get install mold\n", false)]
#[case::block_run(
    "        run: |\n          sudo apt-get update\n          sudo apt-get install mold\n",
    true
)]
fn the_install_reader_counts_only_runnable_commands(#[case] step: &str, #[case] counts: bool) {
    let workflow = GOOD_WORKFLOW.replace("        run: sudo apt-get install --yes mold\n", step);
    let job = job_containing(&workflow, "generate-coverage@");
    assert_eq!(mold_install_offset(&job).is_some(), counts, "{step}");
}

#[rstest]
#[case::setup_rust(
    "      - uses: org/setup-rust@abc\n        with:\n          install-mold: true\n",
    true
)]
#[case::unrelated_action(
    "      - uses: org/other-action@abc\n        with:\n          install-mold: true\n",
    false
)]
fn an_install_mold_input_counts_only_on_setup_rust(#[case] step: &str, #[case] counts: bool) {
    let workflow = GOOD_WORKFLOW.replace(
        "      - name: Install mold linker\n        run: sudo apt-get install --yes mold\n",
        step,
    );
    let job = job_containing(&workflow, "generate-coverage@");
    assert_eq!(mold_install_offset(&job).is_some(), counts, "{step}");
}
