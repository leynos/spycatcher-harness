//! Drives the main-publisher rules against fixtures directly.
//!
//! Each case varies one clause of an otherwise complying publisher and
//! asserts the rule names that clause and nothing else, so a rule that
//! fails for the wrong reason cannot pass as one that works.

use anyhow::{Result, ensure};
use rstest::rstest;

use super::{publisher, pull_request_cases::parse, reader, rules};

/// Scenario: push triggers that name other branches, tags, or none.
///
/// Invariant: only a push restricted to `main` is the publisher.
#[rstest]
#[case::main_only("on:\n  push:\n    branches: [main]\njobs: {}\n", true)]
#[case::unfiltered("on:\n  push:\njobs: {}\n", false)]
#[case::scalar("on: push\njobs: {}\n", false)]
#[case::tags("on:\n  push:\n    tags: ['v*']\njobs: {}\n", false)]
#[case::every_branch("on:\n  push:\n    branches: ['**']\njobs: {}\n", false)]
#[case::another_branch("on:\n  push:\n    branches: [develop]\njobs: {}\n", false)]
#[case::main_and_another("on:\n  push:\n    branches: [main, develop]\njobs: {}\n", false)]
fn only_a_push_restricted_to_main_is_the_publisher(
    #[case] source: &str,
    #[case] expected: bool,
) -> Result<()> {
    ensure!(
        rules::publishes_from_main(&parse(source)?) == expected,
        "expected {expected} for {source:?}"
    );
    Ok(())
}

/// A publisher shaped as this repository's is, which every clause accepts.
const PUBLISHER: &str = r#"
on:
  push:
    branches: [main]
  workflow_dispatch:
concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: false
jobs:
  coverage:
    steps:
      - uses: leynos/shared-actions/.github/actions/generate-coverage@abc
        with:
          output-path: lcov.info
          format: lcov
          with-ratchet: 'true'
      - name: Check CodeScene token
        id: codescene-token
        run: echo "available=${{ secrets.CS_ACCESS_TOKEN != '' }}" >> "$GITHUB_OUTPUT"
      - name: Upload
        if: ${{ steps.codescene-token.outputs.available == 'true' && github.ref == 'refs/heads/main' }}
        uses: leynos/shared-actions/.github/actions/upload-codescene-coverage@abc
        with:
          path: lcov.info
          format: lcov
          access-token: ${{ secrets.CS_ACCESS_TOKEN }}
"#;

/// The upload's condition as [`PUBLISHER`] writes it, without its braces.
const GUARD: &str =
    "steps.codescene-token.outputs.available == 'true' && github.ref == 'refs/heads/main'";
/// The token check step as [`PUBLISHER`] writes it.
const CHECK: &str = concat!(
    "      - name: Check CodeScene token\n        id: codescene-token\n",
    "        run: echo \"available=${{ secrets.CS_ACCESS_TOKEN != '' }}\" >> \"$GITHUB_OUTPUT\"\n",
);
/// The line after which a step-level key can be added to the upload.
const UPLOAD_NAME: &str = "      - name: Upload\n";
/// A declaration of the token in an `env`, at the indentation `indent` gives.
fn env_binding(indent: &str) -> String {
    format!("{indent}env:\n{indent}  CS_ACCESS_TOKEN: ${{{{ secrets.CS_ACCESS_TOKEN }}}}\n")
}
/// A job calling a reusable workflow, awaiting the mapping a case gives it.
const REUSABLE: &str = "jobs:\n  forward:\n    uses: ./.github/workflows/elsewhere.yml\n";

/// Applies `(from, to)` to [`PUBLISHER`], failing if it changes nothing.
fn vary(from: &str, to: &str) -> Result<String> {
    let source = PUBLISHER.replacen(from, to, 1);
    ensure!(
        from.is_empty() || source != PUBLISHER,
        "the case changed nothing: {from:?}"
    );
    Ok(source)
}

/// Asserts `findings` name exactly the `expected` clauses, one each.
fn names_exactly(findings: &[String], expected: &[&str]) -> Result<()> {
    ensure!(
        findings.len() == expected.len()
            && expected
                .iter()
                .all(|clause| findings.iter().any(|f| f.contains(clause))),
        "expected findings naming {expected:?}, saw {findings:?}"
    );
    Ok(())
}

/// Scenario: the upload's condition is varied.
///
/// Invariant: each variation is named. The disjunction keeps both required
/// conjuncts whole and hides its `||` behind a narrowing conjunct, so only
/// the refusal of `||` catches it; the narrowing conjunct alone is allowed.
#[rstest]
#[case::complies("", "", &[][..])]
#[case::narrowed(GUARD, &format!("{GUARD} && github.actor != 'x'"), &[][..])]
#[case::disjunction(
    GUARD,
    &format!("{GUARD} && github.actor != 'x' || github.event_name == 'workflow_dispatch'"),
    &["with `||`"][..],
)]
#[case::no_ref_guard(GUARD, "steps.codescene-token.outputs.available == 'true'", &["github.ref =="][..])]
#[case::no_check_guard(GUARD, "github.ref == 'refs/heads/main'", &["earlier token check"][..])]
#[case::other_step_id(GUARD, &GUARD.replace("steps.codescene-token", "steps.other"), &["earlier token check"][..])]
#[case::no_condition(
    &format!("        if: ${{{{ {GUARD} }}}}\n"),
    "",
    &["no condition"][..],
)]
fn the_upload_condition_is_judged(
    #[case] from: &str,
    #[case] to: &str,
    #[case] expected: &[&str],
) -> Result<()> {
    names_exactly(
        &publisher::publisher_findings(&parse(&vary(from, to)?)?),
        expected,
    )
}

/// Scenario: the token check is deleted, changed, neutralized or moved.
///
/// Invariant: each is named. With the check gone the upload's own condition
/// is simply false and publishing stops in silence, so the check's command is
/// compared whole, as the step's sole command, with no `if:`.
#[rstest]
#[case::deleted(CHECK, "", &["earlier token check"][..])]
#[case::other_command(
    "available=${{ secrets.CS_ACCESS_TOKEN != '' }}",
    "available=true",
    &["earlier token check"][..],
)]
#[case::neutralised("run: echo", "run: false && echo", &["earlier token check", "referenced outside"][..])]
#[case::conditional(
    "        id: codescene-token\n",
    "        id: codescene-token\n        if: false\n",
    &["earlier token check", "referenced outside"][..],
)]
#[case::bound_in_env(
    "        id: codescene-token\n",
    &format!("        id: codescene-token\n{}", env_binding("        ")),
    &["earlier token check", "referenced outside", "binds CS_ACCESS_TOKEN"][..],
)]
fn the_token_check_is_judged(
    #[case] from: &str,
    #[case] to: &str,
    #[case] expected: &[&str],
) -> Result<()> {
    names_exactly(
        &publisher::publisher_findings(&parse(&vary(from, to)?)?),
        expected,
    )
}

/// Scenario: the token check is moved after the upload.
///
/// Invariant: it no longer guards the upload, since a step's outputs are
/// visible only to later steps, and the move is named.
#[test]
fn a_check_after_the_upload_does_not_guard_it() -> Result<()> {
    let source = format!("{}{CHECK}", vary(CHECK, "")?);
    names_exactly(
        &publisher::publisher_findings(&parse(&source)?),
        &["earlier token check"],
    )
}

/// Scenario: the token is bound in an `env` or handed on elsewhere.
///
/// Invariant: each placement is named, however the name is cased. The upload
/// action is composite and passes its step's `env` to the nested steps it
/// runs, so the token belongs in no `env` at any scope, the upload's included.
#[rstest]
#[case::upload_env(UPLOAD_NAME, &format!("{UPLOAD_NAME}{}", env_binding("        ")), &["binds CS_ACCESS_TOKEN", "referenced outside"][..])]
#[case::workflow_env("jobs:\n", &format!("{}jobs:\n", env_binding("")), &["binds CS_ACCESS_TOKEN"][..])]
#[case::job_env("    steps:\n", &format!("{}    steps:\n", env_binding("    ")), &["binds CS_ACCESS_TOKEN"][..])]
#[case::run_step(
    UPLOAD_NAME,
    &format!("      - run: echo ${{{{ secrets.CS_ACCESS_TOKEN }}}}\n{UPLOAD_NAME}"),
    &["referenced outside"][..],
)]
#[case::forwarded_as_an_input(
    "jobs:\n",
    &format!("{REUSABLE}    with:\n      token: ${{{{ secrets.CS_ACCESS_TOKEN }}}}\n"),
    &["reusable workflow"][..],
)]
#[case::forwarded_by_name(
    "jobs:\n",
    &format!("{REUSABLE}    secrets:\n      T: ${{{{ secrets.CS_ACCESS_TOKEN }}}}\n"),
    &["reusable workflow"][..],
)]
#[case::forwarded_by_inheritance("jobs:\n", &format!("{REUSABLE}    secrets: inherit\n"), &["reusable workflow"][..])]
#[case::lower_case_env("    steps:\n", "    env:\n      cs_access_token: ${{ secrets.cs_access_token }}\n    steps:\n", &["binds CS_ACCESS_TOKEN"][..])]
#[case::lower_case_run_step(
    UPLOAD_NAME,
    &format!("      - run: echo ${{{{ secrets.cs_access_token }}}}\n{UPLOAD_NAME}"),
    &["referenced outside"][..],
)]
#[case::computed(
    UPLOAD_NAME,
    &format!("      - run: echo ${{{{ secrets['CS_ACCESS_TOKEN'] }}}}\n{UPLOAD_NAME}"),
    &["computed name"][..],
)]
fn the_token_travels_only_where_allowed(
    #[case] from: &str,
    #[case] to: &str,
    #[case] expected: &[&str],
) -> Result<()> {
    names_exactly(
        &publisher::publisher_findings(&parse(&vary(from, to)?)?),
        expected,
    )
}

/// Scenario: the publisher's concurrency is varied.
///
/// Invariant: a group keyed other than on the ref alone, or any block that
/// may cancel a run in progress, is named. Keyed on the event, an earlier
/// dispatch could finish after a newer push and upload older coverage last.
#[rstest]
#[case::cancels("cancel-in-progress: false", "cancel-in-progress: true", &["may cancel"][..])]
#[case::cancels_by_expression("cancel-in-progress: false", "cancel-in-progress: ${{ true }}", &["may cancel"][..])]
#[case::keyed_on_event("}}-${{ github.ref", "}}-${{ github.event_name }}-${{ github.ref", &["concurrency group"][..])]
#[case::no_group(
    "concurrency:\n  group: ${{ github.workflow }}-${{ github.ref }}\n  cancel-in-progress: false\n",
    "",
    &["concurrency group"][..],
)]
#[case::job_cancels("    steps:\n", "    concurrency:\n      group: x\n      cancel-in-progress: true\n    steps:\n", &["may cancel"][..])]
fn the_publisher_never_cancels(
    #[case] from: &str,
    #[case] to: &str,
    #[case] expected: &[&str],
) -> Result<()> {
    names_exactly(
        &publisher::publisher_findings(&parse(&vary(from, to)?)?),
        expected,
    )
}

/// Scenario: a publisher gains a second, unguarded upload through the CLI,
/// with its command split across a shell continuation.
///
/// Invariant: it is read as an upload, so its missing condition is reported.
#[test]
fn a_continued_cli_upload_is_still_an_upload() -> Result<()> {
    let extra = "      - run: |\n          cs-coverage \\\n            upload --format lcov\n";
    let findings = publisher::publisher_findings(&parse(&format!("{PUBLISHER}{extra}"))?);
    names_exactly(&findings, &["no condition"])
}

/// Scenario: a publisher gains a second upload whose mode is an expression,
/// with no condition.
///
/// Invariant: only the literal `check` mode is not an upload, so this one is
/// judged, and its missing condition is named rather than skipped.
#[test]
fn an_expression_mode_is_still_an_upload() -> Result<()> {
    let extra = concat!(
        "      - uses: leynos/shared-actions/.github/actions/upload-codescene-coverage@abc\n",
        "        with:\n          mode: ${{ 'upload' }}\n",
        "          access-token: ${{ secrets.CS_ACCESS_TOKEN }}\n",
    );
    let findings = publisher::publisher_findings(&parse(&format!("{PUBLISHER}{extra}"))?);
    names_exactly(&findings, &["no condition"])
}

/// Scenario: a publisher that runs the upload action in `check` mode.
///
/// Invariant: that is not an upload, so the omission is reported.
#[test]
fn check_mode_is_not_an_upload() -> Result<()> {
    let source = vary("          path: lcov.info\n", "          mode: check\n")?;
    names_exactly(
        &publisher::publisher_findings(&parse(&source)?),
        &["uploads nothing"],
    )
}

/// Scenario: conditions whose operators sit inside quoted literals.
///
/// Invariant: a `||` inside a string is not a disjunction, and an `&&`
/// inside one does not split a conjunct.
#[rstest]
#[case::quoted_or("${{ github.ref == 'refs/heads/main' && env.X != 'a||b' }}", Some(2))]
#[case::quoted_and("${{ github.ref == 'refs/heads/main' && env.X != 'a&&b' }}", Some(2))]
#[case::bare_or("github.ref == 'refs/heads/main' || true", None)]
fn quoted_operators_are_not_operators(#[case] condition: &str, #[case] expected: Option<usize>) {
    assert_eq!(
        rules::conjuncts(condition).map(|parts| parts.len()),
        expected,
        "for {condition}"
    );
}

/// Scenario: the upload is rewired away from what was measured, or from the
/// token.
///
/// Invariant: each variation is named, and the wired publisher has none.
/// Every other publisher clause passes these variations, because each judges
/// one step at a time.
#[rstest]
#[case::wired("", "", &[][..])]
#[case::other_path("path: lcov.info", "path: other.info", &["which no coverage step writes"][..])]
#[case::other_format("          format: lcov\n          access", "          format: cobertura\n          access", &["which no coverage step writes"][..])]
#[case::no_token("          access-token: ${{ secrets.CS_ACCESS_TOKEN }}\n", "", &["access-token is None"][..])]
#[case::env_token("access-token: ${{ secrets.CS_ACCESS_TOKEN }}", "access-token: ${{ env.CS_ACCESS_TOKEN }}", &["access-token is"][..])]
fn the_upload_sends_what_was_measured(
    #[case] from: &str,
    #[case] to: &str,
    #[case] expected: &[&str],
) -> Result<()> {
    names_exactly(
        &publisher::wiring_findings(&parse(&vary(from, to)?)?),
        expected,
    )
}

/// A ratcheted coverage step, as a called workflow's only job.
const CALLED_WRITER: &str = concat!(
    "on: workflow_call\njobs:\n  measure:\n    steps:\n",
    "      - uses: leynos/shared-actions/.github/actions/generate-coverage@abc\n",
    "        with:\n          with-ratchet: 'true'\n",
);

/// Scenario: a push-triggered workflow reaches a second ratcheted coverage
/// step through a called workflow, in each call spelling.
///
/// Invariant: the called workflow is counted as a baseline writer beside the
/// publisher. A writer rule that read only push-triggered files would count
/// one and pass.
#[rstest]
#[case::dot_prefixed("./")]
#[case::dollar_prefixed("$/")]
fn a_called_baseline_writer_is_counted(#[case] prefix: &str) -> Result<()> {
    let caller = format!(
        "on:\n  push:\n    branches: ['**']\njobs:\n  call:\n    uses: \
         {prefix}.github/workflows/called.yml\n"
    );
    let all: reader::Workflows = [
        ("publisher.yml".to_owned(), parse(PUBLISHER)?),
        ("caller.yml".to_owned(), parse(&caller)?),
        ("called.yml".to_owned(), parse(CALLED_WRITER)?),
    ]
    .into();
    let writers = publisher::baseline_writers(&all);
    ensure!(
        writers == ["called.yml", "publisher.yml"],
        "expected both writers, saw {writers:?}"
    );
    Ok(())
}
