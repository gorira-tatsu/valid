//! Rust-based modeling contracts.
//!
//! This module exposes only generic system-side contracts. Concrete domain
//! models belong in user code, examples, or tests rather than inside `src/`.

use std::{
    collections::{HashSet, VecDeque},
    fmt::Debug,
    hash::Hash,
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

pub trait VerifiedMachine {
    type State: Clone + Debug + Eq + Hash;
    type Action: Clone + Debug + Eq + Hash + Finite;

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
    use super::{check_machine, Finite, ModelingRunStatus, VerifiedMachine};

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
}
