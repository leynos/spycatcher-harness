//! The CV-005 judgements about the one workflow that publishes from `main`.
//!
//! The publisher uploads behind two conjuncts: a ref guard confining it to
//! `main`, and the output of a check step reporting whether the token is set.
//! The token itself travels only as the upload action's `access-token` input
//! and inside the check step's one exact command. It sits in no `env` at any
//! scope, because the upload action is composite and hands its step's `env`
//! to the nested artefact and cache steps it runs, while binding the token
//! itself from `access-token`.

use serde_yaml::{Mapping, Value};

use super::{
    reader::{self, get},
    rules::{ACCESS_TOKEN, conjuncts, input, is_ratcheted_coverage, is_upload, is_upload_action},
    text::{computes_a_secret, rendered, rendered_mapping},
};

/// The conjunct that restricts the publisher's upload to the trunk.
pub const MAIN_REF_GUARD: &str = "github.ref == 'refs/heads/main'";
/// The token check's sole command.
///
/// The expression evaluates to `true` or `false` before the shell runs, so
/// the command binds nothing and holds no shell conditional that could skip
/// the write.
pub const CHECK_COMMAND: &str =
    r#"echo "available=${{ secrets.CS_ACCESS_TOKEN != '' }}" >> "$GITHUB_OUTPUT""#;
/// The upload's `access-token` input.
pub const TOKEN_INPUT: &str = "${{ secrets.CS_ACCESS_TOKEN }}";
/// The publisher's concurrency group: one group per ref, never per event.
///
/// Keyed on the event, an earlier dispatch could finish after a newer push
/// and upload older coverage last.
pub const CONCURRENCY_GROUP: &str = "${{ github.workflow }}-${{ github.ref }}";
/// The expression that hands a step the secret itself.
const SECRET_REFERENCE: &str = "secrets.CS_ACCESS_TOKEN";
/// The only keys the token check may carry: no `if:`, no `env`, no `shell`.
const CHECK_KEYS: [&str; 3] = ["name", "id", "run"];

/// Returns the check step's id when `step` is the token check exactly.
///
/// The command must be the step's sole command, compared whole, because a
/// step that merely contains it (`false && ...`) never writes the output.
fn token_check_id(step: &Mapping) -> Option<&str> {
    let is_exact = get(step, "run").and_then(Value::as_str).map(str::trim) == Some(CHECK_COMMAND);
    let only_known_keys = step
        .keys()
        .all(|key| key.as_str().is_some_and(|name| CHECK_KEYS.contains(&name)));
    if !(is_exact && only_known_keys) {
        return None;
    }
    get(step, "id").and_then(Value::as_str)
}

/// Returns the reasons an upload's condition does not confine it as required.
///
/// Both required conjuncts must be present whole; extra conjuncts only
/// narrow the condition and are allowed. The check must be an earlier step
/// of the same job, since a step's outputs are visible to later steps only.
fn guard_findings(upload: &Mapping, earlier: &[&Mapping]) -> Vec<String> {
    let Some(parts) = get(upload, "if")
        .and_then(Value::as_str)
        .and_then(conjuncts)
    else {
        return vec!["an upload step has no condition, or one with `||`".to_owned()];
    };
    let mut findings = Vec::new();
    if !parts.iter().any(|part| part == MAIN_REF_GUARD) {
        findings.push(format!(
            "an upload step is not guarded by `{MAIN_REF_GUARD}`"
        ));
    }
    let checked = earlier
        .iter()
        .filter_map(|step| token_check_id(step))
        .any(|id| {
            let wanted = format!("steps.{id}.outputs.available == 'true'");
            parts.contains(&wanted)
        });
    if !checked {
        findings.push("an upload step is not guarded by an earlier token check".to_owned());
    }
    findings
}

/// Returns the reasons any upload in the publisher is unguarded.
fn every_guard_finding(workflow: &Value) -> Vec<String> {
    let mut findings = Vec::new();
    for (_, job) in reader::jobs(workflow) {
        let steps = reader::job_steps(job);
        for (position, step) in steps.iter().enumerate() {
            if is_upload(step) {
                findings.extend(guard_findings(
                    step,
                    steps.get(..position).unwrap_or_default(),
                ));
            }
        }
    }
    findings
}

/// Renders a step without the one place it may name the token, if any.
fn rendered_outside_allowance(step: &Mapping) -> String {
    let mut remainder = step.clone();
    if token_check_id(step).is_some() {
        remainder.remove("run");
    }
    if is_upload_action(step)
        && let Some(Value::Mapping(with)) = remainder.get_mut("with")
    {
        with.remove("access-token");
    }
    rendered_mapping(&remainder)
}

/// Returns each `env` block in the workflow, named by its scope.
fn env_blocks(workflow: &Value) -> Vec<(String, &Value)> {
    let root = workflow
        .as_mapping()
        .and_then(|mapping| get(mapping, "env"))
        .map(|env| ("the workflow".to_owned(), env));
    let jobs = reader::jobs(workflow).into_iter().flat_map(|(id, job)| {
        let own = get(job, "env").map(|env| (format!("job {id}"), env));
        let steps = reader::job_steps(job)
            .into_iter()
            .filter_map(move |step| Some((format!("a step of job {id}"), get(step, "env")?)));
        own.into_iter().chain(steps)
    });
    root.into_iter().chain(jobs).collect()
}

/// Returns whether a job calling a reusable workflow hands it the token.
///
/// Such a job has no steps, so the step clauses never see it: the token can
/// travel through its `with:` inputs, a named `secrets:` entry, or
/// `secrets: inherit`.
fn forwards_the_token(job: &Mapping) -> bool {
    if get(job, "uses").is_none() {
        return false;
    }
    let inherits = get(job, "secrets").and_then(Value::as_str) == Some("inherit");
    let names_it = ["with", "secrets"]
        .iter()
        .filter_map(|key| get(job, key))
        .any(|value| rendered(value).contains(SECRET_REFERENCE));
    inherits || names_it
}

/// Returns the reasons the token reaches anywhere but its two allowed places.
fn token_findings(workflow: &Value) -> Vec<String> {
    let mut findings: Vec<String> = env_blocks(workflow)
        .into_iter()
        .filter(|(_, env)| rendered(env).contains(ACCESS_TOKEN))
        .map(|(scope, _)| format!("{scope} binds {ACCESS_TOKEN} in its env"))
        .collect();
    for (id, job) in reader::jobs(workflow) {
        if forwards_the_token(job) {
            findings.push(format!(
                "job {id} forwards {ACCESS_TOKEN} to a reusable workflow"
            ));
        }
    }
    if computes_a_secret(&rendered(workflow)) {
        findings.push("the publisher reaches a secret by a computed name".to_owned());
    }
    if reader::steps(workflow)
        .into_iter()
        .any(|step| rendered_outside_allowance(step).contains(SECRET_REFERENCE))
    {
        findings.push(format!(
            "{ACCESS_TOKEN} is referenced outside the token check and the upload's access-token"
        ));
    }
    findings
}

/// Returns whether a concurrency block could cancel a run in progress.
///
/// Only an absent key or a literal `false` answers that without evaluation,
/// so an expression is refused too.
fn may_cancel(concurrency: &Value) -> bool {
    concurrency
        .as_mapping()
        .and_then(|mapping| get(mapping, "cancel-in-progress"))
        .is_some_and(|value| value.as_bool() != Some(false))
}

/// Returns the reasons the publisher's runs could cancel or reorder uploads.
///
/// A cancelled publisher abandons both its upload and its baseline write. A
/// group that never cancels keeps one pending run, which a newer trigger
/// replaces, so the newest baseline wins.
fn concurrency_findings(workflow: &Value) -> Vec<String> {
    let mut findings = Vec::new();
    let root = workflow
        .as_mapping()
        .and_then(|mapping| get(mapping, "concurrency"));
    let group = root
        .and_then(Value::as_mapping)
        .and_then(|mapping| get(mapping, "group"))
        .and_then(Value::as_str);
    if group != Some(CONCURRENCY_GROUP) {
        findings.push(format!(
            "the publisher's concurrency group is {group:?}, not `{CONCURRENCY_GROUP}`"
        ));
    }
    let job_blocks = reader::jobs(workflow)
        .into_iter()
        .filter_map(|(_, job)| get(job, "concurrency"));
    if root.into_iter().chain(job_blocks).any(may_cancel) {
        findings.push("the publisher may cancel a run in progress".to_owned());
    }
    findings
}

/// Returns the reasons a main publisher fails to publish what CV-005 requires.
pub fn publisher_findings(workflow: &Value) -> Vec<String> {
    let mut findings = Vec::new();
    let steps = reader::steps(workflow);
    if !steps.iter().any(|step| is_ratcheted_coverage(step)) {
        findings.push("the main publisher generates no ratcheted coverage".to_owned());
    }
    if !steps.iter().any(|step| is_upload(step)) {
        findings.push("the main publisher uploads nothing to CodeScene".to_owned());
    }
    findings.extend(every_guard_finding(workflow));
    findings.extend(token_findings(workflow));
    findings.extend(concurrency_findings(workflow));
    findings
}

/// Returns a step input as a string with its whitespace normalized.
fn normalized_input(step: &Mapping, key: &str) -> Option<String> {
    input(step, key)
        .and_then(Value::as_str)
        .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Returns the reasons the publisher's upload would not send what it measured.
///
/// Kept apart from [`publisher_findings`] because it compares two steps of a
/// complete publisher: each upload must read the file, in the format, that a
/// coverage step writes, or it uploads nothing useful while every other
/// clause passes; and it must pass the token as its `access-token`, or its
/// guard holds while the action runs unauthenticated.
pub fn wiring_findings(workflow: &Value) -> Vec<String> {
    let steps = reader::steps(workflow);
    let written: Vec<_> = steps
        .iter()
        .filter(|step| is_ratcheted_coverage(step))
        .map(|step| {
            (
                normalized_input(step, "output-path"),
                normalized_input(step, "format"),
            )
        })
        .collect();
    let mut findings = Vec::new();
    for upload in steps.iter().filter(|step| is_upload_action(step)) {
        let read = (
            normalized_input(upload, "path"),
            normalized_input(upload, "format"),
        );
        if !written.contains(&read) {
            findings.push(format!(
                "the upload reads {read:?}, which no coverage step writes; written: {written:?}"
            ));
        }
        let token = normalized_input(upload, "access-token");
        if token.as_deref() != Some(TOKEN_INPUT) {
            findings.push(format!(
                "the upload's access-token is {token:?}, not `{TOKEN_INPUT}`"
            ));
        }
    }
    findings
}

/// Returns the workflow of every ratcheted coverage step a push can reach.
///
/// `generate-coverage` saves the baseline on a push to `main`, so each entry
/// is a baseline writer; one name per step, so a workflow with two such
/// steps appears twice. Reached through [`reader::push_closure`], a called
/// workflow that ratchets is a writer as surely as its caller.
pub fn baseline_writers(all: &reader::Workflows) -> Vec<String> {
    reader::push_closure(all)
        .into_iter()
        .filter_map(|name| Some((all.get(&name)?, name)))
        .flat_map(|(workflow, name)| {
            let count = reader::steps(workflow)
                .into_iter()
                .filter(|step| is_ratcheted_coverage(step))
                .count();
            std::iter::repeat_n(name, count)
        })
        .collect()
}
