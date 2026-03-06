//! Rust-based modeling contracts.
//!
//! This module exposes only generic system-side contracts. Concrete domain
//! models belong in user code, examples, or tests rather than inside `src/`.

use std::{
    collections::{BTreeMap, BTreeSet},
    collections::{HashSet, VecDeque},
    fmt::Debug,
    hash::Hash,
};

use crate::{
    api::{ExplainCandidateCause, ExplainResponse},
    coverage::CoverageReport,
    engine::{
        AssuranceLevel, BackendKind, CheckOutcome, ExplicitRunResult, PropertyResult, RunManifest,
        RunStatus,
    },
    evidence::{EvidenceKind, EvidenceTrace, TraceStep},
    ir::Value,
    support::hash::stable_hash_hex,
    testgen::{build_counterexample_vector, TestVector, VectorActionStep},
};

pub trait IntoModelValue {
    fn into_model_value(self) -> Value;
}

impl IntoModelValue for bool {
    fn into_model_value(self) -> Value {
        Value::Bool(self)
    }
}

impl IntoModelValue for u8 {
    fn into_model_value(self) -> Value {
        Value::UInt(self as u64)
    }
}

impl IntoModelValue for u16 {
    fn into_model_value(self) -> Value {
        Value::UInt(self as u64)
    }
}

impl IntoModelValue for u32 {
    fn into_model_value(self) -> Value {
        Value::UInt(self as u64)
    }
}

impl IntoModelValue for u64 {
    fn into_model_value(self) -> Value {
        Value::UInt(self)
    }
}

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

#[macro_export]
macro_rules! valid_state {
    (
        struct $state:ident {
            $($field:ident : $field_ty:ty),+ $(,)?
        }
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        struct $state {
            $( $field: $field_ty, )+
        }

        impl $crate::modeling::ModelingState for $state {
            fn snapshot(&self) -> std::collections::BTreeMap<String, $crate::ir::Value> {
                std::collections::BTreeMap::from([
                    $(
                        (
                            stringify!($field).to_string(),
                            $crate::modeling::IntoModelValue::into_model_value(self.$field.clone()),
                        )
                    ),+
                ])
            }
        }
    };
}

#[macro_export]
macro_rules! valid_actions {
    (
        enum $action:ident {
            $($variant:ident => $action_id:literal),+ $(,)?
        }
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        enum $action {
            $( $variant, )+
        }

        impl $crate::modeling::Finite for $action {
            fn all() -> Vec<Self> {
                vec![$(Self::$variant),+]
            }
        }

        impl $crate::modeling::ModelingAction for $action {
            fn action_id(&self) -> String {
                match self {
                    $( Self::$variant => $action_id.to_string(), )+
                }
            }
        }
    };
}

#[macro_export]
macro_rules! valid_model {
    (
        model $model:ident<$state_ty:ty, $action_ty:ty>;
        property $property:ident;
        init [$($init_state:expr),* $(,)?];
        step |$state:ident, $action:ident| $step_body:block
        invariant |$holds_state:ident| $holds_expr:expr;
    ) => {
        struct $model;

        impl $crate::modeling::VerifiedMachine for $model {
            type State = $state_ty;
            type Action = $action_ty;

            fn model_id() -> &'static str {
                stringify!($model)
            }

            fn property_id() -> &'static str {
                stringify!($property)
            }

            fn init_states() -> Vec<Self::State> {
                vec![$($init_state),*]
            }

            fn step($state: &Self::State, $action: &Self::Action) -> Vec<Self::State> $step_body

            fn holds($holds_state: &Self::State) -> bool {
                $holds_expr
            }
        }
    };
}

#[derive(Debug, Clone)]
struct ModelingNode<S, A> {
    state: S,
    parent: Option<usize>,
    via_action: Option<A>,
    depth: u32,
}

#[derive(Debug, Clone)]
struct ModelingEdge<S, A> {
    from_index: usize,
    to_index: usize,
    action: A,
    state_before: S,
    state_after: S,
}

pub fn check_machine<M: VerifiedMachine>() -> ModelingCheckResult<M::State, M::Action> {
    let exploration = explore_machine::<M>();
    if let Some(failure_index) = exploration.failure_index {
        return ModelingCheckResult {
            model_id: M::model_id(),
            property_id: M::property_id(),
            status: ModelingRunStatus::Fail,
            explored_states: exploration.visited_states,
            explored_transitions: exploration.explored_transitions,
            trace: build_trace::<M>(&exploration.nodes, failure_index),
        };
    }
    ModelingCheckResult {
        model_id: M::model_id(),
        property_id: M::property_id(),
        status: ModelingRunStatus::Pass,
        explored_states: exploration.visited_states,
        explored_transitions: exploration.explored_transitions,
        trace: Vec::new(),
    }
}

pub fn collect_machine_coverage<M: VerifiedMachine>() -> CoverageReport {
    let exploration = explore_machine::<M>();
    let total_actions = M::Action::all()
        .into_iter()
        .map(|action| action.action_id())
        .collect::<BTreeSet<_>>();
    let mut covered_actions = BTreeSet::new();
    let mut action_execution_counts = BTreeMap::new();
    let mut guard_true_actions = BTreeSet::new();
    let mut guard_false_actions = BTreeSet::new();
    let mut guard_true_counts = BTreeMap::new();
    let mut guard_false_counts = BTreeMap::new();
    let mut depth_histogram = BTreeMap::new();
    let mut repeated_state_count = 0usize;

    for node in &exploration.nodes {
        *depth_histogram.entry(node.depth).or_insert(0) += 1;
        for action in M::Action::all() {
            let next_states = M::step(&node.state, &action);
            if next_states.is_empty() {
                guard_false_actions.insert(action.action_id());
                *guard_false_counts.entry(action.action_id()).or_insert(0) += 1;
            } else {
                guard_true_actions.insert(action.action_id());
                *guard_true_counts.entry(action.action_id()).or_insert(0) += 1;
            }
        }
    }

    for edge in &exploration.edges {
        let action_id = edge.action.action_id();
        covered_actions.insert(action_id.clone());
        *action_execution_counts.entry(action_id).or_insert(0) += 1;
        if edge.to_index <= edge.from_index {
            repeated_state_count += 1;
        }
    }

    let transition_coverage_percent = if total_actions.is_empty() {
        100
    } else {
        ((covered_actions.len() * 100) / total_actions.len()) as u32
    };
    let fully_covered_guards = total_actions
        .iter()
        .filter(|action_id| {
            guard_true_actions.contains(*action_id) && guard_false_actions.contains(*action_id)
        })
        .count();
    let guard_full_coverage_percent = if total_actions.is_empty() {
        100
    } else {
        ((fully_covered_guards * 100) / total_actions.len()) as u32
    };
    let uncovered_guards = total_actions
        .iter()
        .filter_map(|action_id| {
            if guard_true_actions.contains(action_id) && guard_false_actions.contains(action_id) {
                None
            } else if guard_true_actions.contains(action_id) {
                Some(format!("{action_id}:false"))
            } else if guard_false_actions.contains(action_id) {
                Some(format!("{action_id}:true"))
            } else {
                Some(format!("{action_id}:true,false"))
            }
        })
        .collect::<Vec<_>>();

    CoverageReport {
        schema_version: "1.0.0".to_string(),
        model_id: M::model_id().to_string(),
        transition_coverage_percent,
        guard_full_coverage_percent,
        covered_actions,
        total_actions,
        action_execution_counts,
        visited_state_count: exploration.nodes.len(),
        repeated_state_count,
        max_depth_observed: exploration.nodes.iter().map(|node| node.depth).max().unwrap_or(0),
        guard_true_actions,
        guard_false_actions,
        guard_true_counts,
        guard_false_counts,
        uncovered_guards,
        depth_histogram,
        step_count: exploration.edges.len(),
    }
}

pub fn explain_machine<M: VerifiedMachine>(request_id: &str) -> Result<ExplainResponse, String> {
    let outcome = check_machine_outcome::<M>(request_id);
    let CheckOutcome::Completed(result) = outcome else {
        return Err("modeling adapter returned an error outcome".to_string());
    };
    let trace = result
        .trace
        .ok_or_else(|| "no evidence trace available for explain".to_string())?;
    let failure_step = trace
        .steps
        .last()
        .ok_or_else(|| "empty trace cannot be explained".to_string())?;
    let involved_fields = failure_step
        .state_before
        .iter()
        .filter_map(|(field, before)| {
            let after = failure_step.state_after.get(field)?;
            if before != after {
                Some(field.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let coverage = collect_machine_coverage::<M>();
    let action_id = failure_step.action_id.clone().unwrap_or_else(|| "INITIAL".to_string());
    let mut candidate_causes = Vec::new();
    if involved_fields.is_empty() {
        candidate_causes.push(ExplainCandidateCause {
            kind: "terminal_violation".to_string(),
            message: format!(
                "property {} failed without a visible field diff at the terminal state",
                trace.property_id
            ),
        });
    } else {
        candidate_causes.extend(involved_fields.iter().map(|field| ExplainCandidateCause {
            kind: "field_flip".to_string(),
            message: format!("{field} changed at step {}", failure_step.index),
        }));
    }
    let execution_count = coverage
        .action_execution_counts
        .get(&action_id)
        .copied()
        .unwrap_or(0);
    if execution_count <= 1 {
        candidate_causes.push(ExplainCandidateCause {
            kind: "rare_action_path".to_string(),
            message: format!(
                "action {action_id} was executed only {} time across the reachable state space",
                execution_count
            ),
        });
    }
    if let Some(uncovered) = coverage
        .uncovered_guards
        .iter()
        .find(|entry| entry.starts_with(&format!("{action_id}:")))
    {
        candidate_causes.push(ExplainCandidateCause {
            kind: "guard_polarity_gap".to_string(),
            message: format!("guard coverage for action {action_id} is incomplete: {uncovered}"),
        });
    }
    let mut repair_hints = vec![
        "review the action semantics that lead into the violating state".to_string(),
        format!("verify invariant {} is intended", trace.property_id),
    ];
    if !involved_fields.is_empty() {
        repair_hints.push(format!(
            "focus on fields [{}] when reviewing the failing transition",
            involved_fields.join(", ")
        ));
    }
    let confidence = (0.45_f32
        + if !involved_fields.is_empty() { 0.2_f32 } else { 0.0_f32 }
        + if execution_count <= 1 { 0.15_f32 } else { 0.0_f32 }
        + if coverage
            .uncovered_guards
            .iter()
            .any(|entry| entry.starts_with(&format!("{action_id}:")))
        {
            0.1_f32
        } else {
            0.0_f32
        })
    .min(0.95_f32);

    Ok(ExplainResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        status: "ok".to_string(),
        evidence_id: trace.evidence_id,
        property_id: trace.property_id,
        failure_step_index: failure_step.index,
        involved_fields,
        candidate_causes,
        repair_hints,
        confidence,
        best_practices: vec![
            "keep actions small so violating transitions stay explainable".to_string(),
            "cover both enabled and disabled outcomes of critical actions".to_string(),
        ],
    })
}

pub fn build_machine_test_vectors<M: VerifiedMachine>() -> Vec<TestVector> {
    let exploration = explore_machine::<M>();
    if let Some(failure_index) = exploration.failure_index {
        let trace = build_evidence_trace::<M>(
            "req-modeling",
            &modeling_result_from_failure::<M>(&exploration, failure_index),
        );
        return build_counterexample_vector(&trace)
            .map(|vector| vec![vector])
            .unwrap_or_default();
    }

    let mut seen_sequences = BTreeSet::new();
    let mut vectors = Vec::new();
    for edge in &exploration.edges {
        let first_sequence = vec![edge.action.action_id()];
        if seen_sequences.insert(first_sequence.clone()) {
            vectors.push(TestVector {
                schema_version: "1.0.0".to_string(),
                vector_id: format!(
                    "vec-{}",
                    stable_hash_hex(&(M::model_id().to_string() + &first_sequence.join(",")))
                        .replace("sha256:", "")
                ),
                source_kind: "witness".to_string(),
                evidence_id: None,
                strategy: "witness".to_string(),
                generator_version: env!("CARGO_PKG_VERSION").to_string(),
                seed: None,
                actions: vec![VectorActionStep {
                    index: 0,
                    action_id: edge.action.action_id(),
                    action_label: edge.action.action_label(),
                }],
                initial_state: Some(edge.state_before.snapshot()),
                expected_states: vec![format!("{:?}", edge.state_after.snapshot())],
                property_id: M::property_id().to_string(),
                minimized: false,
            });
        }
    }
    vectors
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
            let trace = build_evidence_trace::<M>(request_id, &result);
            (
                RunStatus::Fail,
                Some("PROPERTY_VIOLATED".to_string()),
                "violating state discovered in reachable state space".to_string(),
                Some(trace),
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

#[derive(Debug, Clone)]
struct ModelingExploration<S, A> {
    nodes: Vec<ModelingNode<S, A>>,
    edges: Vec<ModelingEdge<S, A>>,
    explored_transitions: usize,
    visited_states: usize,
    failure_index: Option<usize>,
}

fn explore_machine<M: VerifiedMachine>() -> ModelingExploration<M::State, M::Action> {
    let actions = M::Action::all();
    let init_states = M::init_states();
    assert!(
        !init_states.is_empty(),
        "VerifiedMachine::init_states must return at least one state"
    );

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
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
                depth: 0,
            });
            frontier.push_back(index);
        }
    }

    let mut failure_index = None;
    while let Some(node_index) = frontier.pop_front() {
        let node = nodes[node_index].clone();
        if !M::holds(&node.state) {
            failure_index = Some(node_index);
            break;
        }

        for action in &actions {
            let next_states = M::step(&node.state, action);
            explored_transitions += 1;
            for next_state in next_states {
                let prior_state = next_state.clone();
                let to_index = if visited.insert(next_state.clone()) {
                    let child_index = nodes.len();
                    nodes.push(ModelingNode {
                        state: next_state,
                        parent: Some(node_index),
                        via_action: Some(action.clone()),
                        depth: node.depth + 1,
                    });
                    frontier.push_back(child_index);
                    child_index
                } else {
                    nodes.iter()
                        .position(|item| item.state == prior_state)
                        .expect("visited state must exist in node list")
                };
                edges.push(ModelingEdge {
                    from_index: node_index,
                    to_index,
                    action: action.clone(),
                    state_before: node.state.clone(),
                    state_after: nodes[to_index].state.clone(),
                });
            }
        }
    }

    ModelingExploration {
        nodes,
        edges,
        explored_transitions,
        visited_states: visited.len(),
        failure_index,
    }
}

fn modeling_result_from_failure<M: VerifiedMachine>(
    exploration: &ModelingExploration<M::State, M::Action>,
    failure_index: usize,
) -> ModelingCheckResult<M::State, M::Action> {
    ModelingCheckResult {
        model_id: M::model_id(),
        property_id: M::property_id(),
        status: ModelingRunStatus::Fail,
        explored_states: exploration.visited_states,
        explored_transitions: exploration.explored_transitions,
        trace: build_trace::<M>(&exploration.nodes, failure_index),
    }
}

fn build_evidence_trace<M: VerifiedMachine>(
    request_id: &str,
    result: &ModelingCheckResult<M::State, M::Action>,
) -> EvidenceTrace {
    let run_id = format!(
        "run-{}",
        stable_hash_hex(&(request_id.to_string() + M::model_id() + M::property_id()))
            .replace("sha256:", "")
    );
    let evidence_id = format!("ev-{run_id}");
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
    EvidenceTrace {
        schema_version: "1.0.0".to_string(),
        evidence_id,
        run_id,
        property_id: M::property_id().to_string(),
        evidence_kind: EvidenceKind::Trace,
        assurance_level: AssuranceLevel::Complete,
        trace_hash,
        steps,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_machine_test_vectors, check_machine, check_machine_outcome, collect_machine_coverage,
        explain_machine, ModelingRunStatus,
    };
    use crate::{engine::CheckOutcome, valid_actions, valid_state};

    valid_state! {
        struct State {
            x: u8,
            locked: bool,
        }
    }

    valid_actions! {
        enum Action {
            Inc => "INC",
            Lock => "LOCK",
            Unlock => "UNLOCK",
        }
    }

    crate::valid_model! {
        model CounterModel<State, Action>;
        property P_RANGE;
        init [State {
            x: 0,
            locked: false,
        }];
        step |state, action| {
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
        invariant |state| state.x <= 3;
    }

    crate::valid_model! {
        model FailingCounterModel<State, Action>;
        property P_FAIL;
        init [State {
            x: 0,
            locked: false,
        }];
        step |state, action| {
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
        invariant |state| state.x <= 1;
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

    #[test]
    fn modeling_check_can_produce_coverage() {
        let report = collect_machine_coverage::<CounterModel>();
        assert_eq!(report.model_id, "CounterModel");
        assert!(report.transition_coverage_percent >= 66);
        assert!(report.visited_state_count >= 4);
        assert!(report.guard_true_counts.contains_key("INC"));
    }

    #[test]
    fn modeling_check_can_produce_explain() {
        let explain =
            explain_machine::<FailingCounterModel>("req-explain").expect("explain should exist");
        assert_eq!(explain.property_id, "P_FAIL");
        assert!(!explain.candidate_causes.is_empty());
        assert!(explain.confidence > 0.4);
    }

    #[test]
    fn modeling_check_can_produce_test_vectors() {
        let counterexample_vectors = build_machine_test_vectors::<FailingCounterModel>();
        assert_eq!(counterexample_vectors.len(), 1);
        assert_eq!(counterexample_vectors[0].strategy, "counterexample");

        let witness_vectors = build_machine_test_vectors::<CounterModel>();
        assert!(!witness_vectors.is_empty());
        assert!(witness_vectors.iter().all(|vector| vector.strategy == "witness"));
    }
}
