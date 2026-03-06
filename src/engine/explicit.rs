use std::{collections::{HashSet, VecDeque}, time::Instant};

use crate::{
    evidence::{EvidenceKind, EvidenceTrace, TraceStep},
    ir::{ModelIr, PropertyIr, PropertyKind, Value},
    kernel::{eval::eval_expr, transition::apply_action, transition::build_initial_state, MachineState},
    support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode},
};

use super::{AssuranceLevel, RunPlan, RunStatus, SearchStrategy, UnknownReason};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitRunResult {
    pub status: RunStatus,
    pub assurance_level: AssuranceLevel,
    pub property_id: Option<String>,
    pub explored_states: usize,
    pub explored_transitions: usize,
    pub unknown_reason: Option<UnknownReason>,
    pub trace: Option<EvidenceTrace>,
}

#[derive(Debug, Clone)]
struct NodeRecord {
    state: MachineState,
    depth: usize,
    parent: Option<usize>,
    via_action: Option<String>,
    note: Option<String>,
}

pub fn run_explicit(model: &ModelIr, plan: &RunPlan) -> Result<ExplicitRunResult, Diagnostic> {
    let start = Instant::now();
    let initial = match build_initial_state(model) {
        Ok(state) => state,
        Err(_) => {
            return Ok(ExplicitRunResult {
                status: RunStatus::Unknown,
                assurance_level: AssuranceLevel::Incomplete,
                property_id: plan.property_id.clone(),
                explored_states: 0,
                explored_transitions: 0,
                unknown_reason: Some(UnknownReason::UnsatInit),
                trace: None,
            })
        }
    };

    let property = selected_property(model, plan)?;
    let mut nodes = vec![NodeRecord {
        state: initial.clone(),
        depth: 0,
        parent: None,
        via_action: None,
        note: Some("initial state".to_string()),
    }];
    let mut frontier = VecDeque::from([0usize]);
    let mut visited = HashSet::from([initial]);
    let mut explored_transitions = 0usize;

    while let Some(node_index) = pop_next(&mut frontier, plan.strategy) {
        if let Some(limit) = plan.time_limit_ms {
            if start.elapsed().as_millis() >= u128::from(limit) {
                return Ok(unknown_result(model, plan, &nodes, explored_transitions, UnknownReason::TimeLimitReached));
            }
        }

        let node = nodes[node_index].clone();
        if let Some(property) = property {
            if !property_holds(model, &node.state, property)? {
                return Ok(fail_result(model, plan, property.property_id.clone(), &nodes, node_index));
            }
        }

        if let Some(limit) = plan.max_depth {
            if node.depth >= limit {
                return Ok(unknown_result(model, plan, &nodes, explored_transitions, UnknownReason::DepthLimitReached));
            }
        }

        let mut enabled = 0usize;
        for action in &model.actions {
            match apply_action(model, &node.state, &action.action_id)? {
                Some(next_state) => {
                    enabled += 1;
                    explored_transitions += 1;
                    if visited.insert(next_state.clone()) {
                        if let Some(limit) = plan.max_states {
                            if visited.len() > limit {
                                return Ok(unknown_result(model, plan, &nodes, explored_transitions, UnknownReason::StateLimitReached));
                            }
                        }
                        let child_index = nodes.len();
                        nodes.push(NodeRecord {
                            state: next_state,
                            depth: node.depth + 1,
                            parent: Some(node_index),
                            via_action: Some(action.action_id.clone()),
                            note: None,
                        });
                        frontier.push_back(child_index);
                    }
                }
                None => continue,
            }
        }

        if plan.detect_deadlocks && enabled == 0 {
            return Ok(ExplicitRunResult {
                status: RunStatus::Fail,
                assurance_level: AssuranceLevel::Complete,
                property_id: property.map(|item| item.property_id.clone()),
                explored_states: visited.len(),
                explored_transitions,
                unknown_reason: None,
                trace: Some(build_trace(model, &nodes, node_index, Some("deadlock detected".to_string()), AssuranceLevel::Complete)),
            });
        }
    }

    Ok(ExplicitRunResult {
        status: RunStatus::Pass,
        assurance_level: AssuranceLevel::Complete,
        property_id: property.map(|item| item.property_id.clone()),
        explored_states: visited.len(),
        explored_transitions,
        unknown_reason: None,
        trace: None,
    })
}

fn pop_next(frontier: &mut VecDeque<usize>, strategy: SearchStrategy) -> Option<usize> {
    match strategy {
        SearchStrategy::Bfs => frontier.pop_front(),
        SearchStrategy::Dfs => frontier.pop_back(),
    }
}

fn selected_property<'a>(model: &'a ModelIr, plan: &RunPlan) -> Result<Option<&'a PropertyIr>, Diagnostic> {
    match &plan.property_id {
        Some(id) => model.properties.iter().find(|item| item.property_id == *id).map(Some).ok_or_else(|| {
            Diagnostic::new(
                ErrorCode::SearchError,
                DiagnosticSegment::EngineSearch,
                format!("unknown property `{id}`"),
            )
            .with_help("select a property id emitted by the frontend")
        }),
        None => Ok(model.properties.first()),
    }
}

fn property_holds(model: &ModelIr, state: &MachineState, property: &PropertyIr) -> Result<bool, Diagnostic> {
    match property.kind {
        PropertyKind::Invariant => match eval_expr(model, state, &property.expr)? {
            Value::Bool(value) => Ok(value),
            _ => Err(Diagnostic::new(
                ErrorCode::EvalError,
                DiagnosticSegment::EngineSearch,
                format!("property `{}` did not evaluate to bool", property.property_id),
            )
            .with_help("keep invariant properties boolean after lowering")),
        },
    }
}

fn fail_result(
    model: &ModelIr,
    plan: &RunPlan,
    property_id: String,
    nodes: &[NodeRecord],
    failing_index: usize,
) -> ExplicitRunResult {
    ExplicitRunResult {
        status: RunStatus::Fail,
        assurance_level: AssuranceLevel::Complete,
        property_id: Some(property_id),
        explored_states: nodes.len(),
        explored_transitions: nodes.len().saturating_sub(1),
        unknown_reason: None,
        trace: Some(build_trace(model, nodes, failing_index, None, plan_assurance(plan))),
    }
}

fn unknown_result(
    model: &ModelIr,
    plan: &RunPlan,
    nodes: &[NodeRecord],
    explored_transitions: usize,
    reason: UnknownReason,
) -> ExplicitRunResult {
    let last_index = nodes.len().saturating_sub(1);
    ExplicitRunResult {
        status: RunStatus::Unknown,
        assurance_level: AssuranceLevel::Bounded,
        property_id: plan.property_id.clone(),
        explored_states: nodes.len(),
        explored_transitions,
        unknown_reason: Some(reason),
        trace: Some(build_trace(
            model,
            nodes,
            last_index,
            Some(format!("search stopped: {}", unknown_reason_label(reason))),
            plan_assurance(plan),
        )),
    }
}

fn plan_assurance(plan: &RunPlan) -> AssuranceLevel {
    if plan.max_depth.is_some() || plan.max_states.is_some() || plan.time_limit_ms.is_some() {
        AssuranceLevel::Bounded
    } else {
        AssuranceLevel::Complete
    }
}

fn unknown_reason_label(reason: UnknownReason) -> &'static str {
    match reason {
        UnknownReason::UnsatInit => "unsat init",
        UnknownReason::StateLimitReached => "state limit reached",
        UnknownReason::DepthLimitReached => "depth limit reached",
        UnknownReason::TimeLimitReached => "time limit reached",
    }
}

fn build_trace(
    model: &ModelIr,
    nodes: &[NodeRecord],
    end_index: usize,
    final_note: Option<String>,
    assurance_level: AssuranceLevel,
) -> EvidenceTrace {
    let mut indices = Vec::new();
    let mut cursor = Some(end_index);
    while let Some(index) = cursor {
        indices.push(index);
        cursor = nodes[index].parent;
    }
    indices.reverse();

    let mut steps = Vec::new();
    for (position, index) in indices.into_iter().enumerate() {
        let node = &nodes[index];
        let note = if index == end_index { final_note.clone().or_else(|| node.note.clone()) } else { node.note.clone() };
        steps.push(TraceStep {
            index: position,
            action_id: node.via_action.clone(),
            state: node.state.as_named_map(model),
            note,
        });
    }

    EvidenceTrace {
        evidence_kind: EvidenceKind::Trace,
        assurance_level,
        steps,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        engine::{RunPlan, RunStatus, SearchStrategy, UnknownReason},
        ir::{ActionIr, BinaryOp, ExprIr, FieldType, InitAssignment, ModelIr, PropertyIr, PropertyKind, SourceSpan, StateField, UpdateIr, Value},
    };

    use super::run_explicit;

    fn counter_model() -> ModelIr {
        ModelIr {
            model_id: "Counter".to_string(),
            state_fields: vec![StateField {
                id: "x".to_string(),
                name: "x".to_string(),
                ty: FieldType::BoundedU8 { min: 0, max: 3 },
                span: SourceSpan { line: 1, column: 1 },
            }],
            init: vec![InitAssignment {
                field: "x".to_string(),
                value: Value::UInt(0),
                span: SourceSpan { line: 2, column: 1 },
            }],
            actions: vec![
                ActionIr {
                    action_id: "Inc1".to_string(),
                    label: "Inc1".to_string(),
                    reads: vec!["x".to_string()],
                    writes: vec!["x".to_string()],
                    guard: ExprIr::Binary {
                        op: BinaryOp::LessThanOrEqual,
                        left: Box::new(ExprIr::FieldRef("x".to_string())),
                        right: Box::new(ExprIr::Literal(Value::UInt(2))),
                    },
                    updates: vec![UpdateIr {
                        field: "x".to_string(),
                        value: ExprIr::Binary {
                            op: BinaryOp::Add,
                            left: Box::new(ExprIr::FieldRef("x".to_string())),
                            right: Box::new(ExprIr::Literal(Value::UInt(1))),
                        },
                    }],
                },
                ActionIr {
                    action_id: "Jump".to_string(),
                    label: "Jump".to_string(),
                    reads: vec!["x".to_string()],
                    writes: vec!["x".to_string()],
                    guard: ExprIr::Literal(Value::Bool(true)),
                    updates: vec![UpdateIr {
                        field: "x".to_string(),
                        value: ExprIr::Literal(Value::UInt(2)),
                    }],
                },
            ],
            properties: vec![PropertyIr {
                property_id: "SAFE".to_string(),
                kind: PropertyKind::Invariant,
                expr: ExprIr::Binary {
                    op: BinaryOp::LessThanOrEqual,
                    left: Box::new(ExprIr::FieldRef("x".to_string())),
                    right: Box::new(ExprIr::Literal(Value::UInt(1))),
                },
            }],
        }
    }

    #[test]
    fn bfs_returns_shortest_counterexample() {
        let model = counter_model();
        let result = run_explicit(&model, &RunPlan::default()).unwrap();
        assert_eq!(result.status, RunStatus::Fail);
        let trace = result.trace.unwrap();
        assert_eq!(trace.steps.len(), 2);
        assert_eq!(trace.steps[1].action_id.as_deref(), Some("Jump"));
    }

    #[test]
    fn state_limit_returns_unknown() {
        let model = counter_model();
        let result = run_explicit(
            &model,
            &RunPlan {
                max_states: Some(1),
                ..RunPlan::default()
            },
        )
        .unwrap();
        assert_eq!(result.status, RunStatus::Unknown);
        assert_eq!(result.unknown_reason, Some(UnknownReason::StateLimitReached));
    }

    #[test]
    fn depth_limit_returns_unknown() {
        let model = counter_model();
        let result = run_explicit(
            &model,
            &RunPlan {
                strategy: SearchStrategy::Dfs,
                max_depth: Some(0),
                ..RunPlan::default()
            },
        )
        .unwrap();
        assert_eq!(result.status, RunStatus::Unknown);
        assert_eq!(result.unknown_reason, Some(UnknownReason::DepthLimitReached));
    }

    #[test]
    fn time_limit_returns_unknown() {
        let model = counter_model();
        let result = run_explicit(
            &model,
            &RunPlan {
                time_limit_ms: Some(0),
                ..RunPlan::default()
            },
        )
        .unwrap();
        assert_eq!(result.status, RunStatus::Unknown);
        assert_eq!(result.unknown_reason, Some(UnknownReason::TimeLimitReached));
    }

    #[test]
    fn missing_init_is_unsat_init() {
        let mut model = counter_model();
        model.init.clear();
        let result = run_explicit(&model, &RunPlan::default()).unwrap();
        assert_eq!(result.status, RunStatus::Unknown);
        assert_eq!(result.unknown_reason, Some(UnknownReason::UnsatInit));
    }

    #[test]
    fn deadlock_is_fail() {
        let model = ModelIr {
            model_id: "Deadlock".to_string(),
            state_fields: vec![StateField {
                id: "locked".to_string(),
                name: "locked".to_string(),
                ty: FieldType::Bool,
                span: SourceSpan { line: 1, column: 1 },
            }],
            init: vec![InitAssignment {
                field: "locked".to_string(),
                value: Value::Bool(true),
                span: SourceSpan { line: 2, column: 1 },
            }],
            actions: vec![ActionIr {
                action_id: "Unlock".to_string(),
                label: "Unlock".to_string(),
                reads: vec!["locked".to_string()],
                writes: vec!["locked".to_string()],
                guard: ExprIr::Unary {
                    op: crate::ir::UnaryOp::Not,
                    expr: Box::new(ExprIr::FieldRef("locked".to_string())),
                },
                updates: vec![UpdateIr {
                    field: "locked".to_string(),
                    value: ExprIr::Literal(Value::Bool(false)),
                }],
            }],
            properties: vec![],
        };
        let result = run_explicit(&model, &RunPlan::default()).unwrap();
        assert_eq!(result.status, RunStatus::Fail);
        assert_eq!(result.trace.unwrap().steps.last().unwrap().note.as_deref(), Some("deadlock detected"));
    }
}
