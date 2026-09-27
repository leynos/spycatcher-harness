//! Reads workflows for the CV-005 contract.
//!
//! Every reader here errs towards seeing more. A workflow the contract cannot
//! see is a workflow it passes, so each place GitHub accepts more than one
//! spelling is read in all of them: both file extensions in either case, the
//! `on` key as a string or as the boolean YAML 1.1 makes of it, a trigger
//! written as a scalar, a sequence or a mapping, and a reusable-workflow call
//! however its local path is prefixed.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, ensure};
use camino::{Utf8Path, Utf8PathBuf};
use cap_std::{ambient_authority, fs_utf8::Dir};
use serde_yaml::{Mapping, Value};

/// The directory GitHub reads workflows from, relative to the repository.
const WORKFLOW_PREFIX: &str = ".github/workflows/";

/// Extensions GitHub accepts for a workflow file, compared case-insensitively.
const WORKFLOW_EXTENSIONS: [&str; 2] = ["yml", "yaml"];

/// Events that run a workflow on behalf of a pull request.
///
/// The review events and `merge_group` run pull-request code as surely as
/// `pull_request` does, and `workflow_run` chains a workflow onto another
/// run, which may itself be a pull request's. Every one seeds the closure.
const PULL_REQUEST_EVENTS: [&str; 6] = [
    "pull_request",
    "pull_request_target",
    "pull_request_review",
    "pull_request_review_comment",
    "merge_group",
    "workflow_run",
];

/// Every workflow in the repository, keyed by file name.
pub type Workflows = BTreeMap<String, Value>;

/// Returns the repository's workflow directory.
fn workflow_dir() -> Utf8PathBuf {
    Utf8Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows")
}

/// Parses one workflow, refusing a repeated key or a doubled trigger block.
///
/// `serde_yaml` refuses duplicate keys itself, and this is the one place the
/// contract parses, so the refusal cannot be bypassed by a second reader. A
/// parser that kept the last duplicate would let a `runs-on` or an `if:`
/// carry one value in the file and another in the parse. A root declaring
/// both `on` and the boolean `true` is refused for the same reason: GitHub
/// merges the two blocks, and a reader that picks one is blind to the other.
///
/// # Errors
///
/// Returns an error naming the file when it is not valid YAML, repeats a key
/// within one mapping, or declares its triggers under both keys.
pub fn parse(name: &str, text: &str) -> Result<Value> {
    let parsed: Value =
        serde_yaml::from_str(text).with_context(|| format!("parse {name} as YAML"))?;
    let doubled = parsed
        .as_mapping()
        .is_some_and(|root| get(root, "on").is_some() && root.get(Value::Bool(true)).is_some());
    ensure!(
        !doubled,
        "{name} declares its triggers under both `on` and `true`"
    );
    Ok(parsed)
}

/// Returns whether `name` is a workflow file, in either extension or case.
pub fn is_workflow(name: &str) -> bool {
    Utf8Path::new(name).extension().is_some_and(|extension| {
        WORKFLOW_EXTENSIONS
            .iter()
            .any(|accepted| extension.eq_ignore_ascii_case(accepted))
    })
}

/// Reads every workflow in `.github/workflows`.
///
/// # Errors
///
/// Returns an error when the directory cannot be read, when any workflow
/// fails [`parse`], or when no workflow is found at all, since every contract
/// ranging over an empty set would pass.
pub fn workflows() -> Result<Workflows> {
    let path = workflow_dir();
    let directory = Dir::open_ambient_dir(&path, ambient_authority())
        .with_context(|| format!("open {path}"))?;
    let mut found = Workflows::new();
    for entry in directory
        .entries()
        .with_context(|| format!("read {path}"))?
    {
        let name = entry
            .context("read a workflow directory entry")?
            .file_name()
            .context("workflow file name should be UTF-8")?;
        if is_workflow(&name) {
            let text = directory
                .read_to_string(&name)
                .with_context(|| format!("read {name}"))?;
            let parsed = parse(&name, &text)?;
            found.insert(name, parsed);
        }
    }
    ensure!(!found.is_empty(), "no workflows found under {path}");
    Ok(found)
}

/// Returns the value under `key` in a mapping, if the value is present.
pub fn get<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a Value> {
    mapping.get(Value::String(key.to_owned()))
}

/// Returns a workflow's trigger block, under the string or the boolean key.
fn trigger_block(workflow: &Value) -> Option<&Value> {
    let root = workflow.as_mapping()?;
    get(root, "on").or_else(|| root.get(Value::Bool(true)))
}

/// Returns the event names a workflow declares, in any of the three forms.
///
/// GitHub accepts `on: push`, `on: [push, pull_request]` and the mapping
/// form. A mapping-only reader stringifies the sequence into one key named
/// after the whole list, and the workflow then escapes every pull-request
/// clause.
pub fn trigger_names(workflow: &Value) -> Vec<String> {
    match trigger_block(workflow) {
        Some(Value::String(name)) => vec![name.clone()],
        Some(Value::Sequence(names)) => names
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Some(Value::Mapping(events)) => events
            .keys()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

/// Returns the configuration of one trigger, when it is written as a mapping.
pub fn trigger<'a>(workflow: &'a Value, event: &str) -> Option<&'a Value> {
    trigger_block(workflow)?
        .as_mapping()
        .and_then(|events| get(events, event))
}

/// Returns whether a workflow starts on behalf of a pull request.
pub fn starts_on_pull_request(workflow: &Value) -> bool {
    trigger_names(workflow)
        .iter()
        .any(|name| PULL_REQUEST_EVENTS.contains(&name.as_str()))
}

/// Returns whether a workflow starts on a push, to any branch or tag.
pub fn starts_on_push(workflow: &Value) -> bool {
    trigger_names(workflow).iter().any(|name| name == "push")
}

/// Returns every job of a workflow as `(job id, job mapping)`.
pub fn jobs(workflow: &Value) -> Vec<(&str, &Mapping)> {
    workflow
        .as_mapping()
        .and_then(|root| get(root, "jobs"))
        .and_then(Value::as_mapping)
        .map(|all| {
            all.iter()
                .filter_map(|(id, job)| Some((id.as_str()?, job.as_mapping()?)))
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the steps of one job, in order.
pub fn job_steps(job: &Mapping) -> Vec<&Mapping> {
    get(job, "steps")
        .and_then(Value::as_sequence)
        .map(|listed| listed.iter().filter_map(Value::as_mapping).collect())
        .unwrap_or_default()
}

/// Returns every step of every job in a workflow.
pub fn steps(workflow: &Value) -> Vec<&Mapping> {
    jobs(workflow)
        .into_iter()
        .flat_map(|(_, job)| job_steps(job))
        .collect()
}

/// Returns a step's `uses` reference, when it has one.
pub fn uses(step: &Mapping) -> Option<&str> {
    get(step, "uses").and_then(Value::as_str)
}

/// What a job-level `uses:` reference names.
#[derive(Debug, PartialEq, Eq)]
pub enum Call<'a> {
    /// A workflow file directly under this repository's workflow directory.
    Local(&'a str),
    /// A local-shaped reference the contract cannot resolve to one file.
    Refused,
    /// A reusable workflow in another repository, or not a workflow call.
    Remote,
}

/// Classifies a job-level `uses` reference.
///
/// Matched by shape rather than by an enumerated list of spellings: a leading
/// `./` or GitHub's documented `$/` is stripped, and what remains is local
/// when it is a path under `.github/workflows/`. A call into another
/// repository carries an owner first and so never matches. A local-shaped
/// reference carrying an `@ref`, or naming a subdirectory, is refused rather
/// than read as remote: it resolves to no file here, so reading it as "not
/// local" would let whatever it runs escape the closure in silence.
pub fn classify_call(reference: &str) -> Call<'_> {
    let path = reference
        .strip_prefix("./")
        .or_else(|| reference.strip_prefix("$/"))
        .unwrap_or(reference);
    let Some(file) = path.strip_prefix(WORKFLOW_PREFIX) else {
        return Call::Remote;
    };
    if names_one_file(file) {
        Call::Local(file)
    } else {
        Call::Refused
    }
}

/// Returns whether `file` names one file directly in the workflow directory,
/// with no subdirectory and no `@ref`.
fn names_one_file(file: &str) -> bool {
    !file.is_empty() && !file.contains(['/', '@'])
}

/// Returns each job's `uses` reference in a workflow, with the job's id.
pub fn job_calls(workflow: &Value) -> Vec<(&str, &str)> {
    jobs(workflow)
        .into_iter()
        .filter_map(|(id, job)| Some((id, get(job, "uses")?.as_str()?)))
        .collect()
}

/// Returns the local workflows `workflow` calls, by file name.
fn local_callees(workflow: &Value) -> Vec<&str> {
    job_calls(workflow)
        .into_iter()
        .filter_map(|(_, reference)| match classify_call(reference) {
            Call::Local(callee) => Some(callee),
            Call::Refused | Call::Remote => None,
        })
        .collect()
}

/// Returns the workflows reachable from those `seeds` selects, by file name.
///
/// A workflow declaring only `workflow_call` names no event of its own, yet
/// it runs with whatever the caller hands it, including the caller's secrets
/// under `secrets: inherit`. So every clause about what an event can reach
/// runs over this closure, never over the trigger list alone.
pub fn closure(all: &Workflows, seeds: fn(&Value) -> bool) -> BTreeSet<String> {
    let mut reached: BTreeSet<String> = all
        .iter()
        .filter(|(_, workflow)| seeds(workflow))
        .map(|(name, _)| name.clone())
        .collect();
    let mut pending: Vec<String> = reached.iter().cloned().collect();
    while let Some(name) = pending.pop() {
        let callees = all.get(&name).map(local_callees).unwrap_or_default();
        for callee in callees {
            if all.contains_key(callee) && reached.insert(callee.to_owned()) {
                pending.push(callee.to_owned());
            }
        }
    }
    reached
}

/// Returns the workflows a pull request can reach, by file name.
pub fn pull_request_closure(all: &Workflows) -> BTreeSet<String> {
    closure(all, starts_on_pull_request)
}

/// Returns the workflows a push can reach, by file name.
pub fn push_closure(all: &Workflows) -> BTreeSet<String> {
    closure(all, starts_on_push)
}

/// Returns each local call, from the named workflows, to a file that is not there.
///
/// The closure can only follow a call to a workflow it has read, so a call
/// to a missing file would otherwise drop out of it in silence, and with it
/// whatever that file would run once it exists. Each entry names the caller
/// and the reference as written.
pub fn missing_callees(all: &Workflows, names: &BTreeSet<String>) -> Vec<String> {
    names
        .iter()
        .filter_map(|name| Some((name, all.get(name)?)))
        .flat_map(|(name, workflow)| {
            job_calls(workflow)
                .into_iter()
                .filter(|(_, reference)| {
                    matches!(classify_call(reference), Call::Local(file) if !all.contains_key(file))
                })
                .map(move |(_, reference)| format!("{name} calls `{reference}`, which is not there"))
        })
        .collect()
}
