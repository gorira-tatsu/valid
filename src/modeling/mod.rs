//! Rust-based modeling contracts.
//!
//! This module exposes only generic system-side contracts. Concrete domain
//! models belong in user code, examples, or tests rather than inside `src/`.

use std::{
    collections::BTreeMap,
    collections::{HashSet, VecDeque},
    fmt::Debug,
    hash::Hash,
};

use crate::{
    engine::{
        AssuranceLevel, BackendKind, CheckOutcome, ExplicitRunResult, PropertyResult, RunManifest,
        RunStatus,
    },
    evidence::{EvidenceKind, EvidenceTrace, TraceStep},
    ir::Value,
    support::hash::stable_hash_hex,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelingRunStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelingTraceStep<S, A> {
    pub index: usize,
    pub action: A,
    pub state_before: S,
    pub state_after: S,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelingCheckResult<S, A> {
    pub model_id: &'static str,
    pub property_id: &'static str,
    pub status: ModelingRunStatus,
    pub explored_states: usize,
    pub explored_transitions: usize,
    pub trace: Vec<ModelingTraceStep<S, A>>,
}

pub trait Finite: Sized {
    fn all() -> Vec<Self>;
}

pub trait ModelingState: Clone + Debug + Eq + Hash {
    fn snapshot(&self) -> BTreeMap<String, Value>;
}

pub trait ModelingAction: Clone + Debug + Eq + Hash + Finite {
    fn action_id(&self) -> String;

    fn action_label(&self) -> String {
        self.action_id()
    }
}

pub trait VerifiedMachine {
    type State: ModelingState;
    type Action: ModelingAction;

    fn model_id() -> &'static str;
    fn property_id() -> &'static str;
    fn init_states() -> Vec<Self::State>;
    fn step(state: &Self::State, action: &Self::Action) -> Vec<Self::State>;
    fn holds(state: &Self::State) -> bool;
}

#[derive(Debug, Clone)]
struct ModelingNode<S, A> {
    state: S,
    parent: Option<usize>,
    via_action: Option<A>,
}

pub fn check_machine<M: VerifiedMachine>() -> ModelingCheckResult<M::State, M::Action> {
    let actions = M::Action::all();
    let init_states = M::init_states();
    assert!(
        !init_states.is_empty(),
        "VerifiedMachine::init_states must return at least one state"
    );

    let mut nodes = Vec::new();
    let mut frontier = VecDeque::new();
    let mut visited = HashSet::new();
    let mut explored_transitions = 0usize;

    for state in init_states {
        if visited.insert(state.clone()) {
            let index = nodes.len();
            nodes.push(ModelingNode {
                state,
                parent: None,
                via_action: None,
            });
            frontier.push_back(index);
        }
    }

    while let Some(node_index) = frontier.pop_front() {
        let node = nodes[node_index].clone();
        if !M::holds(&node.state) {
            return ModelingCheckResult {
                model_id: M::model_id(),
                property_id: M::property_id(),
                status: ModelingRunStatus::Fail,
                explored_states: visited.len(),
                explored_transitions,
                trace: build_trace::<M>(&nodes, node_index),
            };
        }

        for action in &actions {
            let next_states = M::step(&node.state, action);
            explored_transitions += 1;
            for next_state in next_states {
                if visited.insert(next_state.clone()) {
                    let child_index = nodes.len();
                    nodes.push(ModelingNode {
                        state: next_state,
                        parent: Some(node_index),
                        via_action: Some(action.clone()),
                    });
                    frontier.push_back(child_index);
                }
            }
        }
    }

    ModelingCheckResult {
        model_id: M::model_id(),
        property_id: M::property_id(),
        status: ModelingRunStatus::Pass,
        explored_states: visited.len(),
        explored_transitions,
        trace: Vec::new(),
    }
}

pub fn check_machine_outcome<M: VerifiedMachine>(request_id: &str) -> CheckOutcome {
    let result = check_machine::<M>();
    let run_id = format!(
        "run-{}",
        stable_hash_hex(&(request_id.to_string() + M::model_id() + M::property_id()))
            .replace("sha256:", "")
    );
    let source_hash = stable_hash_hex(M::model_id());
    let contract_hash = stable_hash_hex(&(M::model_id().to_string() + M::property_id()));
    let manifest = RunManifest {
        request_id: request_id.to_string(),
        run_id: run_id.clone(),
        schema_version: "1.0.0".to_string(),
        source_hash,
        contract_hash,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        backend_name: BackendKind::Explicit,
        backend_version: env!("CARGO_PKG_VERSION").to_string(),
        seed: None,
    };

    let (status, reason_code, summary, trace) = match result.status {
        ModelingRunStatus::Pass => (
            RunStatus::Pass,
            Some("COMPLETE_SPACE_EXHAUSTED".to_string()),
            "no violating state found in the reachable state space".to_string(),
            None,
        ),
        ModelingRunStatus::Fail => {
            let evidence_id = format!("ev-{}", run_id);
            let steps = result
                .trace
                .iter()
                .enumerate()
                .map(|(index, step)| TraceStep {
                    index,
                    from_state_id: if index == 0 {
                        "s-init".to_string()
                    } else {
                        format!("s-{index}")
                    },
                    action_id: Some(step.action.action_id()),
                    action_label: Some(step.action.action_label()),
                    to_state_id: format!("s-{}", index + 1),
                    depth: (index + 1) as u32,
                    state_before: step.state_before.snapshot(),
                    state_after: step.state_after.snapshot(),
                    note: None,
                })
                .collect::<Vec<_>>();
            let trace_hash = stable_hash_hex(
                &steps
                    .iter()
                    .map(|step| format!("{:?}{:?}", step.action_id, step.state_after))
                    .collect::<String>(),
            );
            (
                RunStatus::Fail,
                Some("PROPERTY_VIOLATED".to_string()),
                "violating state discovered in reachable state space".to_string(),
                Some(EvidenceTrace {
                    schema_version: "1.0.0".to_string(),
                    evidence_id: evidence_id.clone(),
                    run_id: run_id.clone(),
                    property_id: M::property_id().to_string(),
                    evidence_kind: EvidenceKind::Trace,
                    assurance_level: AssuranceLevel::Complete,
                    trace_hash,
                    steps,
                }),
            )
        }
    };

    let evidence_id = trace.as_ref().map(|item| item.evidence_id.clone());
    CheckOutcome::Completed(ExplicitRunResult {
        manifest,
        status,
        assurance_level: AssuranceLevel::Complete,
        property_result: PropertyResult {
            property_id: M::property_id().to_string(),
            property_kind: crate::ir::PropertyKind::Invariant,
            status,
            assurance_level: AssuranceLevel::Complete,
            reason_code,
            unknown_reason: None,
            terminal_state_id: trace
                .as_ref()
                .and_then(|item| item.steps.last().map(|step| step.to_state_id.clone())),
            evidence_id,
            summary,
        },
        explored_states: result.explored_states,
        explored_transitions: result.explored_transitions,
        trace,
    })
}

fn build_trace<M: VerifiedMachine>(
    nodes: &[ModelingNode<M::State, M::Action>],
    end_index: usize,
) -> Vec<ModelingTraceStep<M::State, M::Action>> {
    let mut indices = Vec::new();
    let mut cursor = Some(end_index);
    while let Some(index) = cursor {
        indices.push(index);
        cursor = nodes[index].parent;
    }
    indices.reverse();

    let mut trace = Vec::new();
    for (step_index, pair) in indices.windows(2).enumerate() {
        let before = &nodes[pair[0]];
        let after = &nodes[pair[1]];
        trace.push(ModelingTraceStep {
            index: step_index,
            action: after
                .via_action
                .clone()
                .expect("non-root node must have an action"),
            state_before: before.state.clone(),
            state_after: after.state.clone(),
        });
    }
    trace
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        check_machine, check_machine_outcome, Finite, ModelingAction, ModelingRunStatus,
        ModelingState, VerifiedMachine,
    };
    use crate::{engine::CheckOutcome, ir::Value};

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    struct State {
        x: u8,
        locked: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    enum Action {
        Inc,
        Lock,
        Unlock,
    }

    impl Finite for Action {
        fn all() -> Vec<Self> {
            vec![Self::Inc, Self::Lock, Self::Unlock]
        }
    }

    impl ModelingAction for Action {
        fn action_id(&self) -> String {
            match self {
                Action::Inc => "INC".to_string(),
                Action::Lock => "LOCK".to_string(),
                Action::Unlock => "UNLOCK".to_string(),
            }
        }
    }

    impl ModelingState for State {
        fn snapshot(&self) -> BTreeMap<String, Value> {
            BTreeMap::from([
                ("x".to_string(), Value::UInt(self.x as u64)),
                ("locked".to_string(), Value::Bool(self.locked)),
            ])
        }
    }

    struct CounterModel;

    impl VerifiedMachine for CounterModel {
        type State = State;
        type Action = Action;

        fn model_id() -> &'static str {
            "CounterModel"
        }

        fn property_id() -> &'static str {
            "P_RANGE"
        }

        fn init_states() -> Vec<Self::State> {
            vec![State {
                x: 0,
                locked: false,
            }]
        }

        fn step(state: &Self::State, action: &Self::Action) -> Vec<Self::State> {
            match action {
                Action::Inc if !state.locked && state.x < 3 => vec![State {
                    x: state.x + 1,
                    locked: state.locked,
                }],
                Action::Lock => vec![State {
                    x: state.x,
                    locked: true,
                }],
                Action::Unlock => vec![State {
                    x: state.x,
                    locked: false,
                }],
                _ => Vec::new(),
            }
        }

        fn holds(state: &Self::State) -> bool {
            state.x <= 3
        }
    }

    struct FailingCounterModel;

    impl VerifiedMachine for FailingCounterModel {
        type State = State;
        type Action = Action;

        fn model_id() -> &'static str {
            "FailingCounterModel"
        }

        fn property_id() -> &'static str {
            "P_FAIL"
        }

        fn init_states() -> Vec<Self::State> {
            CounterModel::init_states()
        }

        fn step(state: &Self::State, action: &Self::Action) -> Vec<Self::State> {
            CounterModel::step(state, action)
        }

        fn holds(state: &Self::State) -> bool {
            state.x <= 1
        }
    }

    #[test]
    fn rust_native_model_can_pass() {
        let result = check_machine::<CounterModel>();
        assert_eq!(result.status, ModelingRunStatus::Pass);
        assert!(result.trace.is_empty());
    }

    #[test]
    fn rust_native_model_can_fail_with_shortest_trace() {
        let result = check_machine::<FailingCounterModel>();
        assert_eq!(result.status, ModelingRunStatus::Fail);
        assert_eq!(result.trace.len(), 2);
    }

    #[test]
    fn modeling_check_can_produce_common_outcome() {
        let outcome = check_machine_outcome::<FailingCounterModel>("req-modeling");
        match outcome {
            CheckOutcome::Completed(result) => {
                assert_eq!(result.status, crate::engine::RunStatus::Fail);
                assert!(result.trace.is_some());
                assert_eq!(result.property_result.property_id, "P_FAIL");
            }
            CheckOutcome::Errored(error) => panic!("unexpected error: {:?}", error.diagnostics),
        }
    }
}
