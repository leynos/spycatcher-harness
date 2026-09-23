//! Property cover for the pull-request closure.
//!
//! The named cases pin one chain and one fan-out. The closure's claim is
//! wider: over any graph of local calls, including branches, cycles and
//! self-calls, it returns exactly the workflows reachable from one that a
//! pull request starts. These properties generate small call graphs, write
//! each as real workflow YAML with a randomly chosen call spelling, and
//! compare the rule's own query with an independent reachability computed by
//! repeated relaxation over the edge list.

use std::collections::BTreeSet;

use proptest::prelude::*;

use super::{pull_request_cases::parse, reader};

/// The largest graph generated. Small enough to keep a case cheap, large
/// enough for chains, branches and cycles several calls long.
const MAX_WORKFLOWS: usize = 6;

/// The spellings a local call may take, all of which must reach the file.
const CALL_PREFIXES: [&str; 3] = ["./", "$/", ""];

/// One generated call: caller, callee and the spelling it uses.
#[derive(Debug, Clone, Copy)]
struct Edge {
    /// The calling workflow's index.
    caller: usize,
    /// The called workflow's index.
    callee: usize,
    /// The spelling of the call.
    prefix: &'static str,
}

/// A generated call graph: which workflows a pull request starts, and the
/// calls between them.
#[derive(Debug, Clone)]
struct CallGraph {
    /// Whether workflow `i` is triggered by a pull request.
    starts_on_pull_request: Vec<bool>,
    /// The calls between workflows.
    edges: Vec<Edge>,
}

impl CallGraph {
    /// Renders workflow `index` as YAML.
    fn workflow(&self, index: usize) -> String {
        let is_seed = self
            .starts_on_pull_request
            .get(index)
            .copied()
            .unwrap_or(false);
        let trigger = if is_seed {
            "pull_request"
        } else {
            "workflow_call"
        };
        let jobs = self
            .edges
            .iter()
            .filter(|edge| edge.caller == index)
            .enumerate()
            .map(|(position, edge)| {
                [
                    "  call_",
                    &position.to_string(),
                    ":\n    uses: ",
                    edge.prefix,
                    ".github/workflows/w",
                    &edge.callee.to_string(),
                    ".yml\n",
                ]
                .concat()
            })
            .collect::<String>();
        if jobs.is_empty() {
            format!("on: {trigger}\njobs: {{}}\n")
        } else {
            format!("on: {trigger}\njobs:\n{jobs}")
        }
    }

    /// Returns the workflows reachable from a pull-request trigger, computed
    /// by relaxing every edge until the set stops growing, not by search.
    fn expected_closure(&self) -> BTreeSet<String> {
        let mut reached: BTreeSet<usize> = self
            .starts_on_pull_request
            .iter()
            .enumerate()
            .filter_map(|(index, is_seed)| is_seed.then_some(index))
            .collect();
        loop {
            let callees: Vec<usize> = self
                .edges
                .iter()
                .filter(|edge| reached.contains(&edge.caller))
                .map(|edge| edge.callee)
                .collect();
            let before = reached.len();
            reached.extend(callees);
            if reached.len() == before {
                break;
            }
        }
        reached
            .into_iter()
            .map(|index| format!("w{index}.yml"))
            .collect()
    }
}

/// Generates a call graph of one to [`MAX_WORKFLOWS`] workflows.
fn call_graph() -> impl Strategy<Value = CallGraph> {
    (1..=MAX_WORKFLOWS).prop_flat_map(|size| {
        let edge = (
            0..size,
            0..size,
            prop::sample::select(CALL_PREFIXES.to_vec()),
        )
            .prop_map(|(caller, callee, prefix)| Edge {
                caller,
                callee,
                prefix,
            });
        (
            prop::collection::vec(any::<bool>(), size),
            prop::collection::vec(edge, 0..=size * 2),
        )
            .prop_map(|(starts_on_pull_request, edges)| CallGraph {
                starts_on_pull_request,
                edges,
            })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Scenario: an arbitrary graph of local calls, in any spelling.
    ///
    /// Invariant: the closure is exactly the set of workflows reachable from
    /// one a pull request starts, cycles and self-calls included.
    #[test]
    fn the_closure_is_exactly_what_a_pull_request_can_reach(graph in call_graph()) {
        let mut all = reader::Workflows::new();
        for index in 0..graph.starts_on_pull_request.len() {
            let parsed = parse(&graph.workflow(index));
            prop_assert!(parsed.is_ok(), "workflow {index} did not parse: {:?}", parsed.err());
            if let Ok(workflow) = parsed {
                all.insert(format!("w{index}.yml"), workflow);
            }
        }
        prop_assert_eq!(reader::pull_request_closure(&all), graph.expected_closure());
    }
}
