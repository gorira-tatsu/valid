use std::{
    collections::{HashSet, VecDeque},
    time::Instant,
};

use crate::{
    evidence::{EvidenceKind, EvidenceTrace, TraceStep},
    ir::{ModelIr, PropertyIr, PropertyKind, Value},
    kernel::{
        eval::eval_expr,
        transition::{apply_action_transition, build_initial_state},
        MachineState,
    },
    support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode},
};

use super::{
    AssuranceLevel, ErrorStatus, PropertySelection, ResourceLimits, RunManifest, RunPlan,
    RunStatus, SearchBounds, SearchStrategy, UnknownReason,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitRunResult {
    pub manifest: RunManifest,
    pub status: RunStatus,
    pub assurance_level: AssuranceLevel,
    pub property_result: PropertyResult,
    pub explored_states: usize,
    pub explored_transitions: usize,
    pub trace: Option<EvidenceTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyResult {
    pub property_id: String,
    pub property_kind: PropertyKind,
    pub status: RunStatus,
    pub assurance_level: AssuranceLevel,
    pub reason_code: Option<String>,
    pub unknown_reason: Option<UnknownReason>,
    pub terminal_state_id: Option<String>,
    pub evidence_id: Option<String>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckErrorEnvelope {
    pub manifest: RunManifest,
    pub status: ErrorStatus,
    pub assurance_level: AssuranceLevel,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    Completed(ExplicitRunResult),
    Errored(CheckErrorEnvelope),
}

#[derive(Debug, Clone)]
struct NodeRecord {
    state: MachineState,
    depth: usize,
    parent: Option<usize>,
    via_action: Option<String>,
    note: Option<String>,
}

pub fn check_explicit(model: &ModelIr, plan: &RunPlan) -> CheckOutcome {
    match run_explicit(model, plan) {
        Ok(result) => CheckOutcome::Completed(result),
        Err(diagnostic) => CheckOutcome::Errored(CheckErrorEnvelope {
            manifest: plan.manifest.clone(),
            status: ErrorStatus::Error,
            assurance_level: AssuranceLevel::Incomplete,
            diagnostics: vec![diagnostic],
        }),
    }
}

fn run_explicit(model: &ModelIr, plan: &RunPlan) -> Result<ExplicitRunResult, Diagnostic> {
    let start = Instant::now();
    let property = selected_property(model, plan)?;
    let initial = match build_initial_state(model) {
        Ok(state) => state,
        Err(_) => {
            return Err(Diagnostic::new(
                ErrorCode::InvalidState,
                DiagnosticSegment::EngineSearch,
                "init does not produce any well-typed initial state",
            )
            .with_help("make sure every field receives a valid init assignment")
            .with_best_practice(
                "treat empty or ill-typed init sections as model errors, not unknown outcomes",
            ))
        }
    };

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
    let mut bounded_frontier_cut = false;

    while let Some(node_index) = pop_next(&mut frontier, plan.strategy) {
        if resource_limits_hit(&plan.resource_limits, visited.len(), start)
            == Some(UnknownReason::TimeLimitReached)
        {
            return Ok(unknown_result(
                model,
                plan,
                &nodes,
                explored_transitions,
                UnknownReason::TimeLimitReached,
            ));
        }

        let node = nodes[node_index].clone();
        if !property_holds(model, &node.state, property)? {
            return Ok(fail_result(model, plan, property, &nodes, node_index));
        }

        let mut enabled = 0usize;
        for action in &model.actions {
            explored_transitions += 1;
            match apply_action_transition(model, &node.state, action)? {
                Some(next_state) => {
                    enabled += 1;
                    if hit_depth_bound(&plan.search_bounds, node.depth) {
                        bounded_frontier_cut = true;
                        continue;
                    }
                    if visited.insert(next_state.clone()) {
                        if let Some(UnknownReason::StateLimitReached) =
                            resource_limits_hit(&plan.resource_limits, visited.len(), start)
                        {
                            return Ok(unknown_result(
                                model,
                                plan,
                                &nodes,
                                explored_transitions,
                                UnknownReason::StateLimitReached,
                            ));
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
            return Ok(deadlock_result(model, plan, property, &nodes, node_index));
        }
    }

    let assurance = if plan.search_bounds.max_depth.is_some() && bounded_frontier_cut {
        AssuranceLevel::Bounded
    } else {
        AssuranceLevel::Complete
    };

    Ok(ExplicitRunResult {
        manifest: plan.manifest.clone(),
        status: RunStatus::Pass,
        assurance_level: assurance,
        property_result: PropertyResult {
            property_id: property.property_id.clone(),
            property_kind: property.kind.clone(),
            status: RunStatus::Pass,
            assurance_level: assurance,
            reason_code: Some(if assurance == AssuranceLevel::Bounded {
                "BOUNDED_SPACE_EXHAUSTED".to_string()
            } else {
                "COMPLETE_SPACE_EXHAUSTED".to_string()
            }),
            unknown_reason: None,
            terminal_state_id: None,
            evidence_id: None,
            summary: if assurance == AssuranceLevel::Bounded {
                "no violating state found within the configured depth bound".to_string()
            } else {
                "no violating state found in the reachable state space".to_string()
            },
        },
        explored_states: visited.len(),
        explored_transitions,
        trace: None,
    })
}

fn pop_next(frontier: &mut VecDeque<usize>, strategy: SearchStrategy) -> Option<usize> {
    match strategy {
        SearchStrategy::Bfs => frontier.pop_front(),
        SearchStrategy::Dfs => frontier.pop_back(),
    }
}

fn selected_property<'a>(model: &'a ModelIr, plan: &RunPlan) -> Result<&'a PropertyIr, Diagnostic> {
    let PropertySelection::ExactlyOne(id) = &plan.property_selection;
    model
        .properties
        .iter()
        .find(|item| item.property_id == *id)
        .ok_or_else(|| {
            Diagnostic::new(
                ErrorCode::SearchError,
                DiagnosticSegment::EngineSearch,
                format!("unknown property `{id}`"),
            )
            .with_help("select one property id emitted by the frontend")
        })
}

fn property_holds(
    model: &ModelIr,
    state: &MachineState,
    property: &PropertyIr,
) -> Result<bool, Diagnostic> {
    match property.kind {
        PropertyKind::Invariant => match eval_expr(model, state, &property.expr)? {
            Value::Bool(value) => Ok(value),
            _ => Err(Diagnostic::new(
                ErrorCode::EvalError,
                DiagnosticSegment::EngineSearch,
                format!(
                    "property `{}` did not evaluate to bool",
                    property.property_id
                ),
            )
            .with_help("keep invariant properties boolean after lowering")),
        },
    }
}

fn fail_result(
    model: &ModelIr,
    plan: &RunPlan,
    property: &PropertyIr,
    nodes: &[NodeRecord],
    failing_index: usize,
) -> ExplicitRunResult {
    let assurance = if plan.search_bounds.max_depth.is_some() {
        AssuranceLevel::Bounded
    } else {
        AssuranceLevel::Complete
    };
    let evidence_id = format!("ev-{}", plan.manifest.run_id);
    ExplicitRunResult {
        manifest: plan.manifest.clone(),
        status: RunStatus::Fail,
        assurance_level: assurance,
        property_result: PropertyResult {
            property_id: property.property_id.clone(),
            property_kind: property.kind.clone(),
            status: RunStatus::Fail,
            assurance_level: assurance,
            reason_code: Some("PROPERTY_FAILED".to_string()),
            unknown_reason: None,
            terminal_state_id: Some(format!("s-{failing_index:06}")),
            evidence_id: Some(evidence_id.clone()),
            summary: format!(
                "property `{}` failed during explicit exploration",
                property.property_id
            ),
        },
        explored_states: nodes.len(),
        explored_transitions: nodes.len().saturating_sub(1),
        trace: Some(build_trace(
            model,
            &evidence_id,
            &plan.manifest.run_id,
            &property.property_id,
            nodes,
            failing_index,
            None,
            assurance,
        )),
    }
}

fn deadlock_result(
    model: &ModelIr,
    plan: &RunPlan,
    property: &PropertyIr,
    nodes: &[NodeRecord],
    deadlock_index: usize,
) -> ExplicitRunResult {
    let assurance = if plan.search_bounds.max_depth.is_some() {
        AssuranceLevel::Bounded
    } else {
        AssuranceLevel::Complete
    };
    let evidence_id = format!("ev-{}", plan.manifest.run_id);
    ExplicitRunResult {
        manifest: plan.manifest.clone(),
        status: RunStatus::Fail,
        assurance_level: assurance,
        property_result: PropertyResult {
            property_id: property.property_id.clone(),
            property_kind: property.kind.clone(),
            status: RunStatus::Fail,
            assurance_level: assurance,
            reason_code: Some("DEADLOCK_REACHED".to_string()),
            unknown_reason: None,
            terminal_state_id: Some(format!("s-{deadlock_index:06}")),
            evidence_id: Some(evidence_id.clone()),
            summary: "deadlock detected during explicit exploration".to_string(),
        },
        explored_states: nodes.len(),
        explored_transitions: nodes.len().saturating_sub(1),
        trace: Some(build_trace(
            model,
            &evidence_id,
            &plan.manifest.run_id,
            &property.property_id,
            nodes,
            deadlock_index,
            Some("deadlock detected".to_string()),
            assurance,
        )),
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
        manifest: plan.manifest.clone(),
        status: RunStatus::Unknown,
        assurance_level: AssuranceLevel::Incomplete,
        property_result: PropertyResult {
            property_id: selected_property(model, plan)
                .map(|p| p.property_id.clone())
                .unwrap_or_else(|_| "unknown".to_string()),
            property_kind: PropertyKind::Invariant,
            status: RunStatus::Unknown,
            assurance_level: AssuranceLevel::Incomplete,
            reason_code: None,
            unknown_reason: Some(reason),
            terminal_state_id: Some(format!("s-{last_index:06}")),
            evidence_id: None,
            summary: format!(
                "search stopped before completion: {}",
                unknown_reason_label(reason)
            ),
        },
        explored_states: nodes.len(),
        explored_transitions,
        trace: Some(build_trace(
            model,
            &format!("dbg-{}", plan.manifest.run_id),
            &plan.manifest.run_id,
            &selected_property(model, plan)
                .map(|p| p.property_id.clone())
                .unwrap_or_else(|_| "unknown".to_string()),
            nodes,
            last_index,
            Some(format!("search stopped: {}", unknown_reason_label(reason))),
            AssuranceLevel::Incomplete,
        )),
    }
}

fn hit_depth_bound(bounds: &SearchBounds, depth: usize) -> bool {
    match bounds.max_depth {
        Some(limit) => depth >= limit as usize,
        None => false,
    }
}

fn resource_limits_hit(
    limits: &ResourceLimits,
    states_seen: usize,
    start: Instant,
) -> Option<UnknownReason> {
    if let Some(limit) = limits.time_limit_ms {
        if start.elapsed().as_millis() >= u128::from(limit) {
            return Some(UnknownReason::TimeLimitReached);
        }
    }
    if let Some(limit) = limits.max_states {
        if states_seen > limit {
            return Some(UnknownReason::StateLimitReached);
        }
    }
    None
}

fn unknown_reason_label(reason: UnknownReason) -> &'static str {
    match reason {
        UnknownReason::StateLimitReached => "state limit reached",
        UnknownReason::TimeLimitReached => "time limit reached",
        UnknownReason::EngineAborted => "engine aborted",
    }
}

fn build_trace(
    model: &ModelIr,
    evidence_id: &str,
    run_id: &str,
    property_id: &str,
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
    for (position, window) in indices.windows(2).enumerate() {
        let from = &nodes[window[0]];
        let to = &nodes[window[1]];
        steps.push(TraceStep {
            index: position,
            from_state_id: format!("s-{:06}", window[0]),
            action_id: to.via_action.clone(),
            action_label: to.via_action.clone(),
            to_state_id: format!("s-{:06}", window[1]),
            depth: to.depth as u32,
            state_before: from.state.as_named_map(model),
            state_after: to.state.as_named_map(model),
            note: if window[1] == end_index {
                final_note.clone().or_else(|| to.note.clone())
            } else {
                to.note.clone()
            },
        });
    }

    EvidenceTrace {
        schema_version: "1.0.0".to_string(),
        evidence_id: evidence_id.to_string(),
        run_id: run_id.to_string(),
        property_id: property_id.to_string(),
        evidence_kind: EvidenceKind::Trace,
        assurance_level,
        trace_hash: format!("trace:{}:{}", evidence_id, steps.len()),
        steps,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        engine::{
            check_explicit, AssuranceLevel, CheckOutcome, PropertySelection, ResourceLimits,
            RunPlan, RunStatus, SearchBounds, SearchStrategy, UnknownReason,
        },
        ir::{
            ActionIr, BinaryOp, ExprIr, FieldType, InitAssignment, ModelIr, PropertyIr,
            PropertyKind, SourceSpan, StateField, UpdateIr, Value,
        },
    };

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
                    path_tags: vec!["guard_path".to_string(), "write_path".to_string()],
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
                    path_tags: vec!["write_path".to_string()],
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

    fn default_plan() -> RunPlan {
        RunPlan {
            property_selection: PropertySelection::ExactlyOne("SAFE".to_string()),
            ..RunPlan::default()
        }
    }

    #[test]
    fn bfs_returns_shortest_counterexample() {
        let model = counter_model();
        let outcome = check_explicit(&model, &default_plan());
        let CheckOutcome::Completed(result) = outcome else {
            panic!("expected completed")
        };
        assert_eq!(result.status, RunStatus::Fail);
        let trace = result.trace.unwrap();
        assert_eq!(trace.steps.len(), 1);
        assert_eq!(trace.steps[0].action_id.as_deref(), Some("Jump"));
    }

    #[test]
    fn state_limit_returns_unknown() {
        let model = counter_model();
        let outcome = check_explicit(
            &model,
            &RunPlan {
                resource_limits: ResourceLimits {
                    max_states: Some(1),
                    time_limit_ms: None,
                    memory_limit_mb: None,
                },
                ..default_plan()
            },
        );
        let CheckOutcome::Completed(result) = outcome else {
            panic!("expected completed")
        };
        assert_eq!(result.status, RunStatus::Unknown);
        assert_eq!(
            result.property_result.unknown_reason,
            Some(UnknownReason::StateLimitReached)
        );
    }

    #[test]
    fn depth_bound_returns_bounded_pass_when_no_fail_within_bound() {
        let model = counter_model();
        let outcome = check_explicit(
            &model,
            &RunPlan {
                strategy: SearchStrategy::Dfs,
                search_bounds: SearchBounds { max_depth: Some(0) },
                ..default_plan()
            },
        );
        let CheckOutcome::Completed(result) = outcome else {
            panic!("expected completed")
        };
        assert_eq!(result.status, RunStatus::Pass);
        assert_eq!(result.assurance_level, AssuranceLevel::Bounded);
    }

    #[test]
    fn time_limit_returns_unknown() {
        let model = counter_model();
        let outcome = check_explicit(
            &model,
            &RunPlan {
                resource_limits: ResourceLimits {
                    max_states: None,
                    time_limit_ms: Some(0),
                    memory_limit_mb: None,
                },
                ..default_plan()
            },
        );
        let CheckOutcome::Completed(result) = outcome else {
            panic!("expected completed")
        };
        assert_eq!(result.status, RunStatus::Unknown);
        assert_eq!(
            result.property_result.unknown_reason,
            Some(UnknownReason::TimeLimitReached)
        );
    }

    #[test]
    fn missing_init_is_error() {
        let mut model = counter_model();
        model.init.clear();
        let outcome = check_explicit(&model, &default_plan());
        let CheckOutcome::Errored(error) = outcome else {
            panic!("expected error")
        };
        assert_eq!(error.assurance_level, AssuranceLevel::Incomplete);
    }
}
