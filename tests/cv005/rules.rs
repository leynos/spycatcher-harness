//! The CV-005 judgements about pull-request lanes, and the shared step readers.
//!
//! Each rule returns the reasons a workflow fails, and an empty vector means
//! it complies. Returning reasons rather than a boolean lets the fixture
//! cases assert *which* clause fired, so a rule that fails for the wrong
//! reason cannot pass as one that works. The publisher's rules are in
//! `publisher.rs`.

use serde_yaml::{Mapping, Value};

use super::{
    reader::{self, get, uses},
    text::{computes_a_secret, rendered},
};

/// The action the estate uses to publish coverage to `CodeScene`.
pub const UPLOAD_ACTION: &str = "leynos/shared-actions/.github/actions/upload-codescene-coverage";
/// The action that measures coverage and maintains the ratchet baseline.
pub const COVERAGE_ACTION: &str = "leynos/shared-actions/.github/actions/generate-coverage";
/// The secret a pull-request lane must not receive.
pub const ACCESS_TOKEN: &str = "CS_ACCESS_TOKEN";
/// The CLI a lane must not reach for directly either.
const COVERAGE_CLI: &str = "cs-coverage";
/// The service host, however it is reached.
const CODESCENE_HOST: &str = "codescene.io";

/// Returns a step's `with` input as written, if present.
pub fn input<'a>(step: &'a Mapping, key: &str) -> Option<&'a Value> {
    get(step, "with")
        .and_then(Value::as_mapping)
        .and_then(|with| get(with, key))
}

/// Reads a step input as a boolean, accepting the string and native forms.
pub fn input_is(step: &Mapping, key: &str, expected: bool) -> bool {
    input(step, key).is_some_and(|value| {
        value.as_bool() == Some(expected) || value.as_str() == Some(expected.to_string().as_str())
    })
}

/// Returns whether a step runs the shared coverage action.
pub fn is_coverage(step: &Mapping) -> bool {
    uses(step).is_some_and(|reference| reference.starts_with(COVERAGE_ACTION))
}

/// Returns whether a step runs the shared coverage action with the ratchet on.
pub fn is_ratcheted_coverage(step: &Mapping) -> bool {
    is_coverage(step) && input_is(step, "with-ratchet", true)
}

/// Returns whether a step runs the shared upload action, in any mode.
pub fn is_upload_action(step: &Mapping) -> bool {
    uses(step).is_some_and(|reference| reference.starts_with(UPLOAD_ACTION))
}

/// Returns whether a step uploads to `CodeScene`, through the action or the CLI.
///
/// `upload` is the action's default mode, so an absent mode is an upload,
/// and `check` is not one: it gates changed lines and publishes nothing.
pub fn is_upload(step: &Mapping) -> bool {
    let mode = input(step, "mode").and_then(Value::as_str);
    let action = is_upload_action(step) && matches!(mode, None | Some("upload"));
    let cli = get(step, "run")
        .and_then(Value::as_str)
        .is_some_and(runs_cli_upload);
    action || cli
}

/// Returns whether `word` names the coverage CLI, bare or by path.
fn is_cli(word: &str) -> bool {
    word == COVERAGE_CLI || word.ends_with(&format!("/{COVERAGE_CLI}"))
}

/// Returns whether a `run` body invokes `cs-coverage upload`.
///
/// Read as the shell reads it rather than as text: a backslash-newline
/// continuation joins `cs-coverage \` and `upload` into one command, and any
/// run of whitespace separates the words, so a contiguous-text search would
/// miss an upload the shell still performs. A path to the binary counts too.
fn runs_cli_upload(run: &str) -> bool {
    let joined = run.replace("\\\r\n", " ").replace("\\\n", " ");
    let words: Vec<&str> = joined.split_whitespace().collect();
    words
        .windows(2)
        .any(|pair| matches!(pair, [cli, "upload"] if is_cli(cli)))
}

/// Returns the reasons found by reading the whole workflow as text.
///
/// The token and host clauses read every scalar, case-folded for the host, so
/// a workflow-level `defaults.run.shell` or a callee's `workflow_call` secret
/// declaration cannot reach the service with no step naming it.
fn text_findings(workflow: &Value) -> Vec<String> {
    let text = rendered(workflow);
    let mut findings = Vec::new();
    if text.contains(ACCESS_TOKEN) {
        findings.push(format!("a pull-request lane receives {ACCESS_TOKEN}"));
    }
    if computes_a_secret(&text) {
        findings.push("a pull-request lane reaches a secret by a computed name".to_owned());
    }
    if text.to_ascii_lowercase().contains(CODESCENE_HOST) {
        findings.push(format!("a pull-request lane contacts {CODESCENE_HOST}"));
    }
    findings
}

/// Returns the reasons found in the workflow's job-level calls.
fn call_findings(workflow: &Value) -> Vec<String> {
    let inherits = reader::jobs(workflow)
        .into_iter()
        .filter(|(_, job)| get(job, "secrets").and_then(Value::as_str) == Some("inherit"))
        .map(|(id, _)| format!("job {id} forwards every secret with `secrets: inherit`"));
    let refused = reader::job_calls(workflow)
        .into_iter()
        .filter(|(_, reference)| reader::classify_call(reference) == reader::Call::Refused)
        .map(|(id, reference)| {
            format!("job {id} calls `{reference}`, which resolves to no workflow here")
        });
    inherits.chain(refused).collect()
}

/// Returns the reasons one pull-request step breaches CV-005.
fn step_findings(step: &Mapping) -> Vec<String> {
    let mut findings = Vec::new();
    if is_upload_action(step) {
        findings.push(format!("a pull-request lane invokes {UPLOAD_ACTION}"));
    }
    if get(step, "run")
        .and_then(Value::as_str)
        .is_some_and(|run| run.contains(COVERAGE_CLI))
    {
        findings.push(format!("a pull-request lane runs {COVERAGE_CLI} directly"));
    }
    if is_coverage(step) && !input_is(step, "with-ratchet", true) {
        findings.push("a pull-request coverage step does not set with-ratchet".to_owned());
    }
    if is_coverage(step) && !input_is(step, "publish-artefact", false) {
        findings.push("a pull-request coverage step publishes its report".to_owned());
    }
    findings
}

/// Returns the reasons a workflow a pull request can reach breaches CV-005.
pub fn pull_request_findings(workflow: &Value) -> Vec<String> {
    let mut findings = text_findings(workflow);
    findings.extend(call_findings(workflow));
    findings.extend(reader::steps(workflow).into_iter().flat_map(step_findings));
    findings
}

/// Returns whether a workflow is triggered by a push restricted to `main`.
///
/// A push with no branch filter is not a main publisher: it fires on every
/// branch, so the baseline it writes would be whichever branch pushed last.
/// A tag filter fails too, since it names no branch at all, and so does a
/// glob such as `'**'`, which is not the literal `main`.
pub fn publishes_from_main(workflow: &Value) -> bool {
    reader::trigger(workflow, "push")
        .and_then(Value::as_mapping)
        .and_then(|push| get(push, "branches"))
        .and_then(Value::as_sequence)
        .is_some_and(|branches| {
            !branches.is_empty()
                && branches
                    .iter()
                    .all(|branch| branch.as_str() == Some("main"))
        })
}

/// Returns the conjuncts of an `if:` condition, or `None` if it has a `||`.
///
/// Quoted literals are respected, so a `||` inside a string does not count
/// and an `&&` inside one does not split. A disjunction anywhere makes every
/// conjunct optional, which is why it is refused rather than parsed:
/// `... && ref == main && actor != 'x' || dispatch` keeps every required
/// conjunct whole and still uploads a dispatch from any branch.
pub fn conjuncts(condition: &str) -> Option<Vec<String>> {
    let trimmed = condition.trim();
    let body = trimmed
        .strip_prefix("${{")
        .and_then(|inner| inner.strip_suffix("}}"))
        .unwrap_or(trimmed);
    let mut parts = vec![String::new()];
    let mut in_quote = false;
    let mut characters = body.chars().peekable();
    while let Some(character) = characters.next() {
        let is_doubled = !in_quote && characters.peek() == Some(&character);
        match character {
            '\'' => in_quote = !in_quote,
            '|' if is_doubled => return None,
            '&' if is_doubled => {
                characters.next();
                parts.push(String::new());
                continue;
            }
            _ => {}
        }
        parts.last_mut()?.push(character);
    }
    Some(
        parts
            .iter()
            .map(|part| part.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect(),
    )
}
