//! Holds the `CodeScene` token's environment to the uploading job (CV-005).
//!
//! The token lives in the `codescene` environment, whose deployment policy
//! admits `main` alone. So every job that invokes the uploader declares that
//! environment, no other job does, and no workflow a pull request can start
//! declares it in any job: a declaration there would let branch code ask for
//! the token. Each test below mutates a copy of this repository's workflows
//! the way a later edit could, and asserts the clause meant to catch it does.

use std::collections::{BTreeMap, BTreeSet};

use cap_std::{
    ambient_authority,
    fs_utf8::{Dir, camino::Utf8Path},
};

use serde_yaml::{Mapping, Value};

const ENVIRONMENT: &str = "codescene";
const UPLOAD_ACTION: &str = "upload-codescene-coverage";
const LOCAL_WORKFLOW: &str = "./.github/workflows/";
const PUBLISHER: &str = "coverage-main.yml";
const LANE: &str = "ci.yml";
const PULL_REQUEST_EVENTS: [&str; 6] = [
    "pull_request",
    "pull_request_target",
    "pull_request_review",
    "pull_request_review_comment",
    "issue_comment",
    "merge_group",
];
const MISSING: &str = "the uploading job must declare `environment: codescene`";
const STRAY: &str = "declares `codescene` but uploads nothing";
const REACHABLE: &str = "is reachable from a pull request and declares `codescene`";

type Workflows = BTreeMap<String, Value>;

/// Reads every workflow in the repository, by file name.
fn read_workflows() -> Result<Workflows, String> {
    let root = Dir::open_ambient_dir(env!("CARGO_MANIFEST_DIR"), ambient_authority())
        .map_err(|error| error.to_string())?;
    let directory = root
        .open_dir(".github/workflows")
        .map_err(|error| error.to_string())?;
    let mut workflows = Workflows::new();
    for entry in directory.entries().map_err(|error| error.to_string())? {
        let name = entry
            .map_err(|error| error.to_string())?
            .file_name()
            .map_err(|error| error.to_string())?;
        let is_yaml = Utf8Path::new(&name).extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("yml") || extension.eq_ignore_ascii_case("yaml")
        });
        if !is_yaml {
            continue;
        }
        let text = directory
            .read_to_string(&name)
            .map_err(|error| error.to_string())?;
        let document: Value = serde_yaml::from_str(&text).map_err(|error| error.to_string())?;
        workflows.insert(name, document);
    }
    if workflows.is_empty() {
        return Err("no workflows under .github/workflows".to_owned());
    }
    Ok(workflows)
}

/// Returns a workflow's jobs that are mappings, by name.
fn jobs(workflow: &Value) -> Vec<(String, &Mapping)> {
    workflow
        .get("jobs")
        .and_then(Value::as_mapping)
        .map(|declared| {
            declared
                .iter()
                .filter_map(|(key, job)| Some((key.as_str()?.to_owned(), job.as_mapping()?)))
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the environment a job declares, from either accepted form.
fn environment_name(job: &Mapping) -> Option<&str> {
    match job.get("environment")? {
        Value::String(name) => Some(name),
        Value::Mapping(declared) => declared.get("name")?.as_str(),
        _ => None,
    }
}

/// Returns whether a job has a step invoking the uploader.
fn uploads(job: &Mapping) -> bool {
    job.get("steps")
        .and_then(Value::as_sequence)
        .is_some_and(|steps| {
            steps.iter().any(|step| {
                step.get("uses")
                    .and_then(Value::as_str)
                    .is_some_and(|uses| uses.contains(UPLOAD_ACTION))
            })
        })
}

/// Returns a workflow's trigger names and filters, from any of their forms.
fn triggers(workflow: &Value) -> Vec<(String, Option<&Value>)> {
    let declared = workflow
        .get("on")
        .or_else(|| workflow.as_mapping()?.get(Value::Bool(true)));
    match declared {
        Some(Value::String(event)) => vec![(event.clone(), None)],
        Some(Value::Sequence(events)) => events
            .iter()
            .filter_map(|event| Some((event.as_str()?.to_owned(), None)))
            .collect(),
        Some(Value::Mapping(events)) => events
            .iter()
            .filter_map(|(event, filter)| Some((event.as_str()?.to_owned(), Some(filter))))
            .collect(),
        _ => Vec::new(),
    }
}

/// Returns whether a push trigger is confined to `main` or to tags.
fn push_is_trunk_only(filter: Option<&Value>) -> bool {
    let Some(Value::Mapping(push)) = filter else {
        return false;
    };
    match push.get("branches") {
        None => push.contains_key("tags") && !push.contains_key("branches-ignore"),
        Some(Value::Sequence(branches)) => branches
            .iter()
            .all(|branch| branch.as_str() == Some("main")),
        Some(_) => false,
    }
}

/// Returns whether a pull request can start a workflow by its own triggers.
fn starts_on_pull_request(workflow: &Value) -> bool {
    triggers(workflow).iter().any(|(event, filter)| {
        PULL_REQUEST_EVENTS.contains(&event.as_str())
            || (event == "push" && !push_is_trunk_only(*filter))
    })
}

/// Returns every workflow a pull request can start, directly or through calls.
fn pull_request_closure(workflows: &Workflows) -> BTreeSet<String> {
    let mut reached: BTreeSet<String> = workflows
        .iter()
        .filter(|(_, workflow)| starts_on_pull_request(workflow))
        .map(|(name, _)| name.clone())
        .collect();
    let mut pending: Vec<String> = reached.iter().cloned().collect();
    while let Some(name) = pending.pop() {
        let Some(workflow) = workflows.get(&name) else {
            continue;
        };
        let callees: Vec<String> = jobs(workflow)
            .into_iter()
            .filter_map(|(_, job)| {
                let uses = job.get("uses")?.as_str()?;
                uses.strip_prefix(LOCAL_WORKFLOW).map(str::to_owned)
            })
            .filter(|callee| workflows.contains_key(callee))
            .collect();
        for callee in callees {
            if reached.insert(callee.clone()) {
                pending.push(callee);
            }
        }
    }
    reached
}

/// Reports every departure from the `codescene` environment placement.
fn environment_violations(workflows: &Workflows) -> Vec<String> {
    let placed: Vec<(String, &Mapping)> = workflows
        .iter()
        .flat_map(|(name, workflow)| {
            jobs(workflow)
                .into_iter()
                .map(move |(id, job)| (format!("{name}:{id}"), job))
        })
        .collect();
    if !placed.iter().any(|(_, job)| uploads(job)) {
        return vec!["no workflow job invokes the CodeScene uploader".to_owned()];
    }
    let mut problems: Vec<String> = placed
        .iter()
        .filter(|(_, job)| uploads(job) && environment_name(job) != Some(ENVIRONMENT))
        .map(|(where_, _)| format!("{where_}: {MISSING}"))
        .collect();
    problems.extend(
        placed
            .iter()
            .filter(|(_, job)| !uploads(job) && environment_name(job) == Some(ENVIRONMENT))
            .map(|(where_, _)| format!("{where_} {STRAY}")),
    );
    let reachable = pull_request_closure(workflows);
    problems.extend(
        placed
            .iter()
            .filter(|(where_, job)| {
                where_
                    .split_once(':')
                    .is_some_and(|(name, _)| reachable.contains(name))
                    && environment_name(job) == Some(ENVIRONMENT)
            })
            .map(|(where_, _)| format!("{where_} {REACHABLE}")),
    );
    problems
}

/// Returns the first job of one workflow, for mutation in place.
fn first_job<'a>(workflows: &'a mut Workflows, name: &str) -> Option<&'a mut Mapping> {
    workflows
        .get_mut(name)?
        .get_mut("jobs")?
        .as_mapping_mut()?
        .iter_mut()
        .next()?
        .1
        .as_mapping_mut()
}

/// Returns the violations after applying one mutation to a fresh copy.
fn violations_after(mutation: impl FnOnce(&mut Workflows)) -> Result<Vec<String>, String> {
    let mut workflows = read_workflows()?;
    mutation(&mut workflows);
    Ok(environment_violations(&workflows))
}

/// Fails unless some violation contains `fragment`.
fn assert_reports(found: &[String], fragment: &str) {
    assert!(
        found.iter().any(|problem| problem.contains(fragment)),
        "expected a violation naming {fragment:?}, got {found:?}"
    );
}

#[test]
fn repository_places_the_environment() {
    let found = violations_after(|_| {}).expect("the workflows should be readable");
    assert!(found.is_empty(), "expected no violations, got {found:?}");
}

#[test]
fn the_pull_request_lane_is_read() {
    let workflows = read_workflows().expect("the workflows should be readable");
    let reached = pull_request_closure(&workflows);
    assert!(
        reached.contains(LANE),
        "{LANE} must be pull-request reachable: {reached:?}"
    );
}

#[test]
fn publisher_cannot_drop_the_environment() {
    let found = violations_after(|workflows| {
        first_job(workflows, PUBLISHER)
            .expect("the publisher should have a job")
            .remove("environment");
    })
    .expect("the workflows should be readable");
    assert_reports(&found, MISSING);
}

#[test]
fn publisher_cannot_name_another_environment() {
    let found = violations_after(|workflows| {
        first_job(workflows, PUBLISHER)
            .expect("the publisher should have a job")
            .insert("environment".into(), "production".into());
    })
    .expect("the workflows should be readable");
    assert_reports(&found, MISSING);
}

#[test]
fn mapping_form_is_accepted() {
    let found = violations_after(|workflows| {
        let mut declared = Mapping::new();
        declared.insert("name".into(), ENVIRONMENT.into());
        first_job(workflows, PUBLISHER)
            .expect("the publisher should have a job")
            .insert("environment".into(), Value::Mapping(declared));
    })
    .expect("the workflows should be readable");
    assert!(
        found.is_empty(),
        "the mapping form must be accepted, got {found:?}"
    );
}

#[test]
fn no_other_job_may_declare_it() {
    let found = violations_after(|workflows| {
        let job: Value = serde_yaml::from_str(
            "runs-on: ubuntu-latest\nenvironment: codescene\nsteps:\n  - run: 'true'\n",
        )
        .expect("the job should parse");
        workflows
            .get_mut(PUBLISHER)
            .and_then(|workflow| workflow.get_mut("jobs"))
            .and_then(Value::as_mapping_mut)
            .expect("the publisher should have jobs")
            .insert("other".into(), job);
    })
    .expect("the workflows should be readable");
    assert_reports(&found, STRAY);
}

#[test]
fn no_pull_request_job_may_declare_it() {
    let found = violations_after(|workflows| {
        first_job(workflows, LANE)
            .expect("the lane should have a job")
            .insert("environment".into(), ENVIRONMENT.into());
    })
    .expect("the workflows should be readable");
    assert_reports(&found, REACHABLE);
}

#[test]
fn a_called_workflow_is_read_too() {
    let found = violations_after(|workflows| {
        let called: Value = serde_yaml::from_str(
            "on:\n  workflow_call:\njobs:\n  inner:\n    environment: codescene\n    steps:\n      - run: 'true'\n",
        )
        .expect("the called workflow should parse");
        workflows.insert("called.yml".to_owned(), called);
        let forward: Value = serde_yaml::from_str("uses: ./.github/workflows/called.yml\n")
            .expect("the calling job should parse");
        workflows
            .get_mut(LANE)
            .and_then(|workflow| workflow.get_mut("jobs"))
            .and_then(Value::as_mapping_mut)
            .expect("the lane should have jobs")
            .insert("forward".into(), forward);
    })
    .expect("the workflows should be readable");
    assert_reports(&found, &format!("called.yml:inner {REACHABLE}"));
}

#[test]
fn an_empty_reading_is_refused() {
    let found = violations_after(|workflows| {
        let job = first_job(workflows, PUBLISHER).expect("the publisher should have a job");
        if let Some(Value::Sequence(steps)) = job.get_mut("steps") {
            steps.retain(|step| {
                !step
                    .get("uses")
                    .and_then(Value::as_str)
                    .is_some_and(|uses| uses.contains(UPLOAD_ACTION))
            });
        }
    })
    .expect("the workflows should be readable");
    assert_reports(&found, "no workflow job invokes");
}
