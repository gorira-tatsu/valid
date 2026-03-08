use std::{
    collections::{HashMap, VecDeque},
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
    pub scenario_id: Option<String>,
    pub vacuous: bool,
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
    via_action_index: Option<usize>,
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
    let scenario = selected_scenario(model, plan)?;
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
        via_action_index: None,
        via_action: None,
        note: Some("initial state".to_string()),
    }];
    let mut frontier = VecDeque::from([0usize]);
    let mut visited = HashMap::from([(initial, 0usize)]);
    let mut edges = vec![Vec::new()];
    let mut explored_transitions = 0usize;
    let mut bounded_frontier_cut = false;
    let mut matched_states = 0usize;
    let mut matched_transitions = 0usize;

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
        let state_matches = scoped_state_matches(model, &node.state, property, scenario)?;
        if state_matches {
            matched_states += 1;
            if property_triggered(model, &node.state, property)? {
                return Ok(match property.kind {
                    PropertyKind::Cover => {
                        cover_hit_result(model, plan, property, &nodes, node_index)
                    }
                    _ => fail_result(model, plan, property, &nodes, node_index, false),
                });
            }
        }

        let mut enabled = 0usize;
        for (action_index, action) in model.actions.iter().enumerate() {
            explored_transitions += 1;
            match apply_action_transition(model, &node.state, action)? {
                Some(next_state) => {
                    enabled += 1;
                    if matches!(property.kind, PropertyKind::Transition)
                        && transition_in_scope(
                            model,
                            property,
                            scenario,
                            &node.state,
                            &next_state,
                            &action.action_id,
                        )?
                    {
                        matched_transitions += 1;
                        if !transition_property_value(model, property, &node.state, &next_state)? {
                            let mut failing_nodes = nodes.clone();
                            let child_index = failing_nodes.len();
                            failing_nodes.push(NodeRecord {
                                state: next_state.clone(),
                                depth: node.depth + 1,
                                parent: Some(node_index),
                                via_action_index: Some(action_index),
                                via_action: Some(action.action_id.clone()),
                                note: None,
                            });
                            return Ok(fail_result(
                                model,
                                plan,
                                property,
                                &failing_nodes,
                                child_index,
                                false,
                            ));
                        }
                    }
                    if hit_depth_bound(&plan.search_bounds, node.depth) {
                        bounded_frontier_cut = true;
                        continue;
                    }
                    if let Some(existing_index) = visited.get(&next_state).copied() {
                        edges[node_index].push(existing_index);
                        continue;
                    }
                    let child_index = nodes.len();
                    visited.insert(next_state.clone(), child_index);
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
                    nodes.push(NodeRecord {
                        state: next_state,
                        depth: node.depth + 1,
                        parent: Some(node_index),
                        via_action_index: Some(action_index),
                        via_action: Some(action.action_id.clone()),
                        note: None,
                    });
                    edges.push(Vec::new());
                    edges[node_index].push(child_index);
                    frontier.push_back(child_index);
                }
                None => continue,
            }
        }

        if matches!(property.kind, PropertyKind::DeadlockFreedom) && enabled == 0 && state_matches {
            return Ok(deadlock_result(
                model, plan, property, &nodes, node_index, false,
            ));
        }
    }

    let assurance = if plan.search_bounds.max_depth.is_some() && bounded_frontier_cut {
        AssuranceLevel::Bounded
    } else {
        AssuranceLevel::Complete
    };

    if matches!(property.kind, PropertyKind::Temporal) {
        return temporal_result(
            model,
            plan,
            property,
            &nodes,
            &edges,
            explored_transitions,
            assurance,
        );
    }

    if matches!(property.kind, PropertyKind::Cover) {
        return Ok(cover_miss_result(
            plan,
            property,
            explored_transitions,
            visited.len(),
            assurance,
            matched_states == 0 && (plan.scenario_selection.is_some() || property.scope.is_some()),
        ));
    }

    Ok(ExplicitRunResult {
        manifest: plan.manifest.clone(),
        status: RunStatus::Pass,
        assurance_level: assurance,
        property_result: PropertyResult {
            property_id: property.property_id.clone(),
            property_kind: property.kind,
            status: RunStatus::Pass,
            assurance_level: assurance,
            scenario_id: plan.scenario_selection.clone(),
            vacuous: match property.kind {
                PropertyKind::Transition => matched_transitions == 0,
                _ => {
                    matched_states == 0
                        && (plan.scenario_selection.is_some() || property.scope.is_some())
                }
            },
            reason_code: Some(pass_reason_code(property.kind, assurance).to_string()),
            unknown_reason: None,
            terminal_state_id: None,
            evidence_id: None,
            summary: pass_summary(
                property,
                assurance,
                match property.kind {
                    PropertyKind::Transition => matched_transitions == 0,
                    _ => {
                        matched_states == 0
                            && (plan.scenario_selection.is_some() || property.scope.is_some())
                    }
                },
            ),
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

fn selected_scenario<'a>(
    model: &'a ModelIr,
    plan: &RunPlan,
) -> Result<Option<&'a crate::ir::ScenarioIr>, Diagnostic> {
    match &plan.scenario_selection {
        Some(id) => model
            .scenarios
            .iter()
            .find(|item| item.scenario_id == *id)
            .map(Some)
            .ok_or_else(|| {
                Diagnostic::new(
                    ErrorCode::SearchError,
                    DiagnosticSegment::EngineSearch,
                    format!("unknown scenario `{id}`"),
                )
                .with_help("select one scenario id emitted by inspect")
            }),
        None => Ok(None),
    }
}

fn scoped_state_matches(
    model: &ModelIr,
    state: &MachineState,
    property: &PropertyIr,
    scenario: Option<&crate::ir::ScenarioIr>,
) -> Result<bool, Diagnostic> {
    let scenario_ok = match scenario {
        Some(scenario) => eval_bool_expr(model, state, &scenario.expr)?,
        None => true,
    };
    if !scenario_ok {
        return Ok(false);
    }
    match &property.scope {
        Some(scope) => eval_bool_expr(model, state, scope),
        None => Ok(true),
    }
}

fn transition_in_scope(
    model: &ModelIr,
    property: &PropertyIr,
    scenario: Option<&crate::ir::ScenarioIr>,
    prev: &MachineState,
    next: &MachineState,
    action_id: &str,
) -> Result<bool, Diagnostic> {
    if property.action_filter.as_deref() != Some(action_id) {
        return Ok(false);
    }
    let scenario_ok = match scenario {
        Some(scenario) => eval_bool_expr(model, prev, &scenario.expr)?,
        None => true,
    };
    if !scenario_ok {
        return Ok(false);
    }
    match &property.scope {
        Some(scope) => transition_property_value_with_expr(model, scope, prev, next),
        None => Ok(true),
    }
}

fn transition_property_value(
    model: &ModelIr,
    property: &PropertyIr,
    prev: &MachineState,
    next: &MachineState,
) -> Result<bool, Diagnostic> {
    transition_property_value_with_expr(model, &property.expr, prev, next)
}

fn transition_property_value_with_expr(
    model: &ModelIr,
    expr: &crate::ir::ExprIr,
    prev: &MachineState,
    next: &MachineState,
) -> Result<bool, Diagnostic> {
    let transition_model = transition_eval_model(model);
    let transition_state = transition_eval_state(prev, next);
    match eval_expr(&transition_model, &transition_state, expr)? {
        Value::Bool(value) => Ok(value),
        _ => Err(Diagnostic::new(
            ErrorCode::SearchError,
            DiagnosticSegment::EngineSearch,
            "transition property did not evaluate to bool",
        )),
    }
}

fn property_triggered(
    model: &ModelIr,
    state: &MachineState,
    property: &PropertyIr,
) -> Result<bool, Diagnostic> {
    match property.kind {
        PropertyKind::Invariant => Ok(!property_value(model, state, property)?),
        PropertyKind::Reachability => Ok(property_value(model, state, property)?),
        PropertyKind::Cover => Ok(property_value(model, state, property)?),
        PropertyKind::DeadlockFreedom | PropertyKind::Temporal | PropertyKind::Transition => {
            Ok(false)
        }
    }
}

fn property_value(
    model: &ModelIr,
    state: &MachineState,
    property: &PropertyIr,
) -> Result<bool, Diagnostic> {
    match eval_expr(model, state, &property.expr)? {
        Value::Bool(value) => Ok(value),
        _ => Err(Diagnostic::new(
            ErrorCode::EvalError,
            DiagnosticSegment::EngineSearch,
            format!(
                "property `{}` did not evaluate to bool",
                property.property_id
            ),
        )
        .with_help("keep lowered properties boolean regardless of property kind")),
    }
}

fn eval_bool_expr(
    model: &ModelIr,
    state: &MachineState,
    expr: &crate::ir::ExprIr,
) -> Result<bool, Diagnostic> {
    match eval_expr(model, state, expr)? {
        Value::Bool(value) => Ok(value),
        _ => Err(Diagnostic::new(
            ErrorCode::EvalError,
            DiagnosticSegment::EngineSearch,
            "scope/scenario expression did not evaluate to bool",
        )),
    }
}

fn transition_eval_model(model: &ModelIr) -> ModelIr {
    let mut state_fields = Vec::with_capacity(model.state_fields.len() * 2);
    for prefix in ["prev", "next"] {
        for field in &model.state_fields {
            let mut cloned = field.clone();
            cloned.id = format!("{prefix}_{}", field.id);
            cloned.name = format!("{prefix}.{}", field.name);
            state_fields.push(cloned);
        }
    }
    ModelIr {
        model_id: format!("{}::transition_eval", model.model_id),
        state_fields,
        init: Vec::new(),
        actions: Vec::new(),
        predicates: Vec::new(),
        scenarios: Vec::new(),
        properties: Vec::new(),
    }
}

fn transition_eval_state(prev: &MachineState, next: &MachineState) -> MachineState {
    let mut values = prev.values.clone();
    values.extend(next.values.clone());
    MachineState::new(values)
}

fn temporal_result(
    model: &ModelIr,
    plan: &RunPlan,
    property: &PropertyIr,
    nodes: &[NodeRecord],
    edges: &[Vec<usize>],
    explored_transitions: usize,
    assurance: AssuranceLevel,
) -> Result<ExplicitRunResult, Diagnostic> {
    let truth = temporal_truth_set(model, nodes, edges, &property.expr)?;
    if truth.first().copied().unwrap_or(false) {
        return Ok(ExplicitRunResult {
            manifest: plan.manifest.clone(),
            status: RunStatus::Pass,
            assurance_level: assurance,
            property_result: PropertyResult {
                property_id: property.property_id.clone(),
                property_kind: property.kind,
                status: RunStatus::Pass,
                assurance_level: assurance,
                scenario_id: plan.scenario_selection.clone(),
                vacuous: false,
                reason_code: Some(pass_reason_code(property.kind, assurance).to_string()),
                unknown_reason: None,
                terminal_state_id: None,
                evidence_id: None,
                summary: pass_summary(property, assurance, false),
            },
            explored_states: nodes.len(),
            explored_transitions,
            trace: None,
        });
    }

    let failing_index = temporal_failure_index(model, nodes, edges, &property.expr, 0)?;
    Ok(fail_result(
        model,
        plan,
        property,
        nodes,
        failing_index,
        false,
    ))
}

fn temporal_truth_set(
    model: &ModelIr,
    nodes: &[NodeRecord],
    edges: &[Vec<usize>],
    expr: &crate::ir::ExprIr,
) -> Result<Vec<bool>, Diagnostic> {
    match expr {
        crate::ir::ExprIr::Unary {
            op: crate::ir::UnaryOp::TemporalAlways,
            expr,
        } => {
            let inner = temporal_truth_set(model, nodes, edges, expr)?;
            let mut set = inner.clone();
            loop {
                let mut changed = false;
                for index in 0..nodes.len() {
                    let holds = inner[index] && edges[index].iter().all(|child| set[*child]);
                    if set[index] != holds {
                        set[index] = holds;
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
            Ok(set)
        }
        crate::ir::ExprIr::Unary {
            op: crate::ir::UnaryOp::TemporalEventually,
            expr,
        } => {
            let target = temporal_truth_set(model, nodes, edges, expr)?;
            let mut set = target.clone();
            loop {
                let mut changed = false;
                for index in 0..nodes.len() {
                    let holds = target[index]
                        || (!edges[index].is_empty()
                            && edges[index].iter().all(|child| set[*child]));
                    if set[index] != holds {
                        set[index] = holds;
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
            Ok(set)
        }
        crate::ir::ExprIr::Unary {
            op: crate::ir::UnaryOp::TemporalNext,
            expr,
        } => {
            let inner = temporal_truth_set(model, nodes, edges, expr)?;
            Ok((0..nodes.len())
                .map(|index| {
                    !edges[index].is_empty() && edges[index].iter().all(|child| inner[*child])
                })
                .collect())
        }
        crate::ir::ExprIr::Binary {
            op: crate::ir::BinaryOp::TemporalUntil,
            left,
            right,
        } => {
            let left_set = temporal_truth_set(model, nodes, edges, left)?;
            let right_set = temporal_truth_set(model, nodes, edges, right)?;
            let mut set = right_set.clone();
            loop {
                let mut changed = false;
                for index in 0..nodes.len() {
                    let holds = right_set[index]
                        || (left_set[index]
                            && !edges[index].is_empty()
                            && edges[index].iter().all(|child| set[*child]));
                    if set[index] != holds {
                        set[index] = holds;
                        changed = true;
                    }
                }
                if !changed {
                    break;
                }
            }
            Ok(set)
        }
        _ => nodes
            .iter()
            .map(|node| match eval_expr(model, &node.state, expr)? {
                Value::Bool(value) => Ok(value),
                _ => Err(Diagnostic::new(
                    ErrorCode::EvalError,
                    DiagnosticSegment::EngineSearch,
                    "temporal state predicate did not evaluate to bool",
                )),
            })
            .collect(),
    }
}

fn temporal_failure_index(
    model: &ModelIr,
    nodes: &[NodeRecord],
    edges: &[Vec<usize>],
    expr: &crate::ir::ExprIr,
    start: usize,
) -> Result<usize, Diagnostic> {
    let truth = temporal_truth_set(model, nodes, edges, expr)?;
    match expr {
        crate::ir::ExprIr::Unary {
            op: crate::ir::UnaryOp::TemporalAlways,
            expr,
        } => {
            let inner = temporal_truth_set(model, nodes, edges, expr)?;
            if !inner[start] {
                return temporal_failure_index(model, nodes, edges, expr, start);
            }
            if let Some(child) = edges[start].iter().copied().find(|child| !truth[*child]) {
                if child == start {
                    return Ok(start);
                }
                return temporal_failure_index(model, nodes, edges, expr, child);
            }
            Ok(start)
        }
        crate::ir::ExprIr::Unary {
            op: crate::ir::UnaryOp::TemporalEventually,
            expr: inner,
        } => {
            let target = temporal_truth_set(model, nodes, edges, inner)?;
            if target[start] {
                return Ok(start);
            }
            if let Some(child) = edges[start].iter().copied().find(|child| !truth[*child]) {
                if child == start {
                    return Ok(start);
                }
                return temporal_failure_index(model, nodes, edges, expr, child);
            }
            Ok(start)
        }
        crate::ir::ExprIr::Unary {
            op: crate::ir::UnaryOp::TemporalNext,
            expr: inner,
        } => {
            let inner_truth = temporal_truth_set(model, nodes, edges, inner)?;
            if let Some(child) = edges[start]
                .iter()
                .copied()
                .find(|child| !inner_truth[*child])
            {
                if child == start {
                    return Ok(start);
                }
                return temporal_failure_index(model, nodes, edges, inner, child);
            }
            Ok(start)
        }
        crate::ir::ExprIr::Binary {
            op: crate::ir::BinaryOp::TemporalUntil,
            left,
            right,
        } => {
            let left_truth = temporal_truth_set(model, nodes, edges, left)?;
            let right_truth = temporal_truth_set(model, nodes, edges, right)?;
            if right_truth[start] {
                return Ok(start);
            }
            if !left_truth[start] {
                return temporal_failure_index(model, nodes, edges, left, start);
            }
            if let Some(child) = edges[start].iter().copied().find(|child| !truth[*child]) {
                if child == start {
                    return Ok(start);
                }
                return temporal_failure_index(model, nodes, edges, expr, child);
            }
            Ok(start)
        }
        _ => Ok(start),
    }
}

fn fail_result(
    model: &ModelIr,
    plan: &RunPlan,
    property: &PropertyIr,
    nodes: &[NodeRecord],
    failing_index: usize,
    vacuous: bool,
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
            property_kind: property.kind,
            status: RunStatus::Fail,
            assurance_level: assurance,
            scenario_id: plan.scenario_selection.clone(),
            vacuous,
            reason_code: Some(fail_reason_code(property.kind).to_string()),
            unknown_reason: None,
            terminal_state_id: Some(format!("s-{failing_index:06}")),
            evidence_id: Some(evidence_id.clone()),
            summary: fail_summary(property, assurance),
        },
        explored_states: nodes.len(),
        explored_transitions: nodes.len().saturating_sub(1),
        trace: Some(build_trace(
            model,
            &evidence_id,
            &plan.manifest.run_id,
            &property.property_id,
            property_evidence_kind(property.kind),
            nodes,
            failing_index,
            Some(fail_note(property.kind, assurance).to_string()),
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
    vacuous: bool,
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
            property_kind: property.kind,
            status: RunStatus::Fail,
            assurance_level: assurance,
            scenario_id: plan.scenario_selection.clone(),
            vacuous,
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
            EvidenceKind::Trace,
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
    let property = selected_property(model, plan).ok();
    ExplicitRunResult {
        manifest: plan.manifest.clone(),
        status: RunStatus::Unknown,
        assurance_level: AssuranceLevel::Incomplete,
        property_result: PropertyResult {
            property_id: property
                .as_ref()
                .map(|property| property.property_id.clone())
                .unwrap_or_else(|| "unknown".to_string()),
            property_kind: property
                .as_ref()
                .map(|property| property.kind)
                .unwrap_or(PropertyKind::Invariant),
            status: RunStatus::Unknown,
            assurance_level: AssuranceLevel::Incomplete,
            scenario_id: plan.scenario_selection.clone(),
            vacuous: false,
            reason_code: None,
            unknown_reason: Some(reason),
            terminal_state_id: Some(format!("s-{last_index:06}")),
            evidence_id: None,
            summary: unknown_summary(property.as_ref().map(|property| property.kind), reason),
        },
        explored_states: nodes.len(),
        explored_transitions,
        trace: Some(build_trace(
            model,
            &format!("dbg-{}", plan.manifest.run_id),
            &plan.manifest.run_id,
            property
                .as_ref()
                .map(|property| property.property_id.as_str())
                .unwrap_or("unknown"),
            EvidenceKind::Trace,
            nodes,
            last_index,
            Some(unknown_note(
                property.as_ref().map(|property| property.kind),
                reason,
            )),
            AssuranceLevel::Incomplete,
        )),
    }
}

fn cover_hit_result(
    model: &ModelIr,
    plan: &RunPlan,
    property: &PropertyIr,
    nodes: &[NodeRecord],
    hit_index: usize,
) -> ExplicitRunResult {
    let assurance = if plan.search_bounds.max_depth.is_some() {
        AssuranceLevel::Bounded
    } else {
        AssuranceLevel::Complete
    };
    let evidence_id = format!("ev-{}", plan.manifest.run_id);
    ExplicitRunResult {
        manifest: plan.manifest.clone(),
        status: RunStatus::Pass,
        assurance_level: assurance,
        property_result: PropertyResult {
            property_id: property.property_id.clone(),
            property_kind: property.kind,
            status: RunStatus::Pass,
            assurance_level: assurance,
            scenario_id: plan.scenario_selection.clone(),
            vacuous: false,
            reason_code: Some("COVER_REACHED".to_string()),
            unknown_reason: None,
            terminal_state_id: Some(format!("s-{hit_index:06}")),
            evidence_id: Some(evidence_id.clone()),
            summary: format!("cover target `{}` was reached", property.property_id),
        },
        explored_states: nodes.len(),
        explored_transitions: nodes.len().saturating_sub(1),
        trace: Some(build_trace(
            model,
            &evidence_id,
            &plan.manifest.run_id,
            &property.property_id,
            EvidenceKind::Witness,
            nodes,
            hit_index,
            Some("cover target reached".to_string()),
            assurance,
        )),
    }
}

fn cover_miss_result(
    plan: &RunPlan,
    property: &PropertyIr,
    explored_transitions: usize,
    explored_states: usize,
    assurance: AssuranceLevel,
    vacuous: bool,
) -> ExplicitRunResult {
    ExplicitRunResult {
        manifest: plan.manifest.clone(),
        status: RunStatus::Fail,
        assurance_level: assurance,
        property_result: PropertyResult {
            property_id: property.property_id.clone(),
            property_kind: property.kind,
            status: RunStatus::Fail,
            assurance_level: assurance,
            scenario_id: plan.scenario_selection.clone(),
            vacuous,
            reason_code: Some("COVER_UNREACHED".to_string()),
            unknown_reason: None,
            terminal_state_id: None,
            evidence_id: None,
            summary: if vacuous {
                "cover target was never evaluated because no scoped state was reachable".to_string()
            } else {
                "cover target was not reached in the explored state space".to_string()
            },
        },
        explored_states,
        explored_transitions,
        trace: None,
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
    evidence_kind: EvidenceKind,
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
            path: to
                .via_action_index
                .and_then(|action_index| model.actions.get(action_index))
                .map(|action| action.decision_path()),
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
        evidence_kind,
        assurance_level,
        trace_hash: format!("trace:{}:{}", evidence_id, steps.len()),
        steps,
    }
}

fn property_evidence_kind(kind: PropertyKind) -> EvidenceKind {
    match kind {
        PropertyKind::Invariant => EvidenceKind::Counterexample,
        PropertyKind::Reachability => EvidenceKind::Witness,
        PropertyKind::Cover => EvidenceKind::Witness,
        PropertyKind::Transition => EvidenceKind::Counterexample,
        PropertyKind::DeadlockFreedom => EvidenceKind::Counterexample,
        PropertyKind::Temporal => EvidenceKind::Counterexample,
    }
}

fn fail_reason_code(kind: PropertyKind) -> &'static str {
    match kind {
        PropertyKind::Invariant => "PROPERTY_FAILED",
        PropertyKind::Reachability => "TARGET_REACHED",
        PropertyKind::Cover => "COVER_UNREACHED",
        PropertyKind::Transition => "TRANSITION_PROPERTY_FAILED",
        PropertyKind::DeadlockFreedom => "DEADLOCK_REACHED",
        PropertyKind::Temporal => "TEMPORAL_PROPERTY_FAILED",
    }
}

fn pass_reason_code(kind: PropertyKind, assurance: AssuranceLevel) -> &'static str {
    match (kind, assurance) {
        (PropertyKind::Invariant, AssuranceLevel::Bounded) => "BOUNDED_SPACE_EXHAUSTED",
        (PropertyKind::Invariant, _) => "COMPLETE_SPACE_EXHAUSTED",
        (PropertyKind::Reachability, AssuranceLevel::Bounded) => "TARGET_NOT_REACHED_WITHIN_BOUND",
        (PropertyKind::Reachability, _) => "TARGET_UNREACHABLE",
        (PropertyKind::Cover, _) => "COVER_REACHED",
        (PropertyKind::Transition, AssuranceLevel::Bounded) => "BOUNDED_SPACE_EXHAUSTED",
        (PropertyKind::Transition, _) => "COMPLETE_SPACE_EXHAUSTED",
        (PropertyKind::DeadlockFreedom, AssuranceLevel::Bounded) => "BOUNDED_SPACE_EXHAUSTED",
        (PropertyKind::DeadlockFreedom, _) => "COMPLETE_SPACE_EXHAUSTED",
        (PropertyKind::Temporal, AssuranceLevel::Bounded) => "TEMPORAL_BOUND_EXHAUSTED",
        (PropertyKind::Temporal, _) => "TEMPORAL_PROPERTY_PROVED_ON_REACHABLE_GRAPH",
    }
}

fn fail_summary(property: &PropertyIr, assurance: AssuranceLevel) -> String {
    match (property.kind, assurance) {
        (PropertyKind::Invariant, _) => format!(
            "property `{}` failed during explicit exploration",
            property.property_id
        ),
        (PropertyKind::Reachability, _) => format!(
            "reachability target for `{}` was reached during explicit exploration",
            property.property_id
        ),
        (PropertyKind::Cover, _) => {
            format!("cover target `{}` was not reached", property.property_id)
        }
        (PropertyKind::Transition, _) => format!(
            "transition property `{}` failed during explicit exploration",
            property.property_id
        ),
        (PropertyKind::DeadlockFreedom, _) => format!(
            "deadlock found for `{}` during explicit exploration",
            property.property_id
        ),
        (PropertyKind::Temporal, AssuranceLevel::Bounded) => format!(
            "temporal property `{}` failed within the configured exploration bound",
            property.property_id
        ),
        (PropertyKind::Temporal, _) => format!(
            "temporal property `{}` failed on the explored reachable graph",
            property.property_id
        ),
    }
}

fn pass_summary(property: &PropertyIr, assurance: AssuranceLevel, vacuous: bool) -> String {
    if vacuous {
        return "property held vacuously because no scoped state/transition was reached"
            .to_string();
    }
    match (property.kind, assurance) {
        (PropertyKind::Invariant, AssuranceLevel::Bounded) => {
            "no violating state found within the configured depth bound".to_string()
        }
        (PropertyKind::Invariant, _) => {
            "no violating state found in the reachable state space".to_string()
        }
        (PropertyKind::Reachability, AssuranceLevel::Bounded) => {
            "reachability target was not reached within the configured depth bound".to_string()
        }
        (PropertyKind::Reachability, _) => {
            "reachability target was not found in the reachable state space".to_string()
        }
        (PropertyKind::Cover, _) => "cover target was reached".to_string(),
        (PropertyKind::Transition, AssuranceLevel::Bounded) => {
            "no violating scoped transition found within the configured depth bound".to_string()
        }
        (PropertyKind::Transition, _) => {
            "no violating scoped transition found in the reachable state space".to_string()
        }
        (PropertyKind::DeadlockFreedom, AssuranceLevel::Bounded) => {
            "no deadlock state found within the configured depth bound".to_string()
        }
        (PropertyKind::DeadlockFreedom, _) => {
            "no deadlock state found in the reachable state space".to_string()
        }
        (PropertyKind::Temporal, AssuranceLevel::Bounded) => {
            "temporal property held within the configured exploration bound".to_string()
        }
        (PropertyKind::Temporal, _) => {
            "temporal property held on the explored reachable graph".to_string()
        }
    }
}

fn fail_note(kind: PropertyKind, assurance: AssuranceLevel) -> &'static str {
    match (kind, assurance) {
        (PropertyKind::Invariant, _) => "property violated",
        (PropertyKind::Reachability, _) => "reachability target reached",
        (PropertyKind::Cover, _) => "cover target unreached",
        (PropertyKind::Transition, _) => "transition property violated",
        (PropertyKind::DeadlockFreedom, _) => "deadlock reached",
        (PropertyKind::Temporal, AssuranceLevel::Bounded) => {
            "temporal property violated within the configured exploration bound"
        }
        (PropertyKind::Temporal, _) => "temporal property violated on the explored reachable graph",
    }
}

fn unknown_summary(kind: Option<PropertyKind>, reason: UnknownReason) -> String {
    match kind {
        Some(PropertyKind::Temporal) => format!(
            "temporal search stopped before completing the requested semantics: {}",
            unknown_reason_label(reason)
        ),
        _ => format!(
            "search stopped before completion: {}",
            unknown_reason_label(reason)
        ),
    }
}

fn unknown_note(kind: Option<PropertyKind>, reason: UnknownReason) -> String {
    match kind {
        Some(PropertyKind::Temporal) => format!(
            "temporal exploration stopped before the reachable-graph evaluation completed: {}",
            unknown_reason_label(reason)
        ),
        _ => format!("search stopped: {}", unknown_reason_label(reason)),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        engine::{
            check_explicit, AssuranceLevel, CheckOutcome, PropertySelection, ResourceLimits,
            RunPlan, RunStatus, SearchBounds, SearchStrategy, UnknownReason,
        },
        evidence::EvidenceKind,
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
                    role: crate::ir::action::ActionRole::Business,
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
                    role: crate::ir::action::ActionRole::Business,
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
            predicates: vec![],
            scenarios: vec![],
            properties: vec![PropertyIr {
                property_id: "SAFE".to_string(),
                kind: PropertyKind::Invariant,
                expr: ExprIr::Binary {
                    op: BinaryOp::LessThanOrEqual,
                    left: Box::new(ExprIr::FieldRef("x".to_string())),
                    right: Box::new(ExprIr::Literal(Value::UInt(1))),
                },
                scope: None,
                action_filter: None,
            }],
        }
    }

    fn default_plan() -> RunPlan {
        RunPlan {
            property_selection: PropertySelection::ExactlyOne("SAFE".to_string()),
            ..RunPlan::default()
        }
    }

    fn reachability_model(target: u64) -> ModelIr {
        let mut model = counter_model();
        model.properties = vec![PropertyIr {
            property_id: "REACH".to_string(),
            kind: PropertyKind::Reachability,
            expr: ExprIr::Binary {
                op: BinaryOp::Equal,
                left: Box::new(ExprIr::FieldRef("x".to_string())),
                right: Box::new(ExprIr::Literal(Value::UInt(target))),
            },
            scope: None,
            action_filter: None,
        }];
        model
    }

    fn reachability_plan() -> RunPlan {
        RunPlan {
            property_selection: PropertySelection::ExactlyOne("REACH".to_string()),
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
    fn reachability_returns_witness_when_target_is_reachable() {
        let model = reachability_model(2);
        let outcome = check_explicit(&model, &reachability_plan());
        let CheckOutcome::Completed(result) = outcome else {
            panic!("expected completed")
        };
        assert_eq!(result.status, RunStatus::Fail);
        assert_eq!(
            result.property_result.reason_code.as_deref(),
            Some("TARGET_REACHED")
        );
        let trace = result.trace.expect("reachability should emit a witness");
        assert_eq!(trace.evidence_kind, EvidenceKind::Witness);
        assert_eq!(trace.steps.len(), 1);
        assert_eq!(trace.steps[0].action_id.as_deref(), Some("Jump"));
    }

    #[test]
    fn reachability_returns_pass_when_target_is_unreachable() {
        let model = reachability_model(4);
        let outcome = check_explicit(&model, &reachability_plan());
        let CheckOutcome::Completed(result) = outcome else {
            panic!("expected completed")
        };
        assert_eq!(result.status, RunStatus::Pass);
        assert_eq!(result.assurance_level, AssuranceLevel::Complete);
        assert_eq!(
            result.property_result.reason_code.as_deref(),
            Some("TARGET_UNREACHABLE")
        );
        assert!(result.trace.is_none());
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

    #[test]
    fn deadlock_freedom_fails_with_counterexample() {
        let mut model = counter_model();
        model.actions = vec![ActionIr {
            action_id: "OnlyOnce".to_string(),
            label: "OnlyOnce".to_string(),
            role: crate::ir::action::ActionRole::Business,
            reads: vec!["x".to_string()],
            writes: vec!["x".to_string()],
            path_tags: vec!["guard_path".to_string()],
            guard: ExprIr::Binary {
                op: BinaryOp::LessThanOrEqual,
                left: Box::new(ExprIr::FieldRef("x".to_string())),
                right: Box::new(ExprIr::Literal(Value::UInt(0))),
            },
            updates: vec![UpdateIr {
                field: "x".to_string(),
                value: ExprIr::Literal(Value::UInt(1)),
            }],
        }];
        model.properties = vec![PropertyIr {
            property_id: "P_LIVE".to_string(),
            kind: PropertyKind::DeadlockFreedom,
            expr: ExprIr::Literal(Value::Bool(true)),
            scope: None,
            action_filter: None,
        }];
        let outcome = check_explicit(
            &model,
            &RunPlan {
                property_selection: PropertySelection::ExactlyOne("P_LIVE".to_string()),
                ..RunPlan::default()
            },
        );
        let CheckOutcome::Completed(result) = outcome else {
            panic!("expected completed")
        };
        assert_eq!(result.status, RunStatus::Fail);
        assert_eq!(
            result.property_result.reason_code.as_deref(),
            Some("DEADLOCK_REACHED")
        );
        let trace = result.trace.expect("deadlock trace");
        assert_eq!(trace.steps.len(), 1);
        assert_eq!(trace.steps[0].action_id.as_deref(), Some("OnlyOnce"));
        assert_eq!(trace.steps[0].note.as_deref(), Some("deadlock detected"));
    }

    #[test]
    fn deadlock_freedom_passes_when_some_action_is_always_enabled() {
        let mut model = counter_model();
        model.properties = vec![PropertyIr {
            property_id: "P_LIVE".to_string(),
            kind: PropertyKind::DeadlockFreedom,
            expr: ExprIr::Literal(Value::Bool(true)),
            scope: None,
            action_filter: None,
        }];
        let outcome = check_explicit(
            &model,
            &RunPlan {
                property_selection: PropertySelection::ExactlyOne("P_LIVE".to_string()),
                ..RunPlan::default()
            },
        );
        let CheckOutcome::Completed(result) = outcome else {
            panic!("expected completed")
        };
        assert_eq!(result.status, RunStatus::Pass);
    }

    #[test]
    fn invariant_property_does_not_fail_just_because_a_state_is_deadlocked() {
        let model = ModelIr {
            model_id: "Terminal".to_string(),
            state_fields: vec![StateField {
                id: "x".to_string(),
                name: "x".to_string(),
                ty: FieldType::BoundedU8 { min: 0, max: 1 },
                span: SourceSpan { line: 1, column: 1 },
            }],
            init: vec![InitAssignment {
                field: "x".to_string(),
                value: Value::UInt(0),
                span: SourceSpan { line: 1, column: 1 },
            }],
            actions: vec![],
            predicates: vec![],
            scenarios: vec![],
            properties: vec![PropertyIr {
                property_id: "SAFE".to_string(),
                kind: PropertyKind::Invariant,
                expr: ExprIr::Literal(Value::Bool(true)),
                scope: None,
                action_filter: None,
            }],
        };
        let outcome = check_explicit(&model, &default_plan());
        let CheckOutcome::Completed(result) = outcome else {
            panic!("expected completed")
        };
        assert_eq!(result.status, RunStatus::Pass);
    }

    #[test]
    fn temporal_eventually_fails_when_goal_is_not_forced() {
        let model = ModelIr {
            model_id: "TemporalEventually".to_string(),
            state_fields: vec![
                StateField {
                    id: "x".to_string(),
                    name: "x".to_string(),
                    ty: FieldType::BoundedU8 { min: 0, max: 1 },
                    span: SourceSpan { line: 1, column: 1 },
                },
                StateField {
                    id: "stuck".to_string(),
                    name: "stuck".to_string(),
                    ty: FieldType::Bool,
                    span: SourceSpan { line: 1, column: 1 },
                },
            ],
            init: vec![
                InitAssignment {
                    field: "x".to_string(),
                    value: Value::UInt(0),
                    span: SourceSpan { line: 1, column: 1 },
                },
                InitAssignment {
                    field: "stuck".to_string(),
                    value: Value::Bool(false),
                    span: SourceSpan { line: 1, column: 1 },
                },
            ],
            actions: vec![
                ActionIr {
                    action_id: "Reach".to_string(),
                    label: "Reach".to_string(),
                    role: crate::ir::action::ActionRole::Business,
                    reads: vec!["x".to_string()],
                    writes: vec!["x".to_string()],
                    path_tags: vec!["write_path".to_string()],
                    guard: ExprIr::Unary {
                        op: crate::ir::UnaryOp::Not,
                        expr: Box::new(ExprIr::FieldRef("stuck".to_string())),
                    },
                    updates: vec![UpdateIr {
                        field: "x".to_string(),
                        value: ExprIr::Literal(Value::UInt(1)),
                    }],
                },
                ActionIr {
                    action_id: "Loop".to_string(),
                    label: "Loop".to_string(),
                    role: crate::ir::action::ActionRole::Business,
                    reads: vec!["x".to_string()],
                    writes: vec!["x".to_string()],
                    path_tags: vec!["write_path".to_string()],
                    guard: ExprIr::Literal(Value::Bool(true)),
                    updates: vec![UpdateIr {
                        field: "x".to_string(),
                        value: ExprIr::FieldRef("x".to_string()),
                    }],
                },
            ],
            predicates: vec![],
            scenarios: vec![],
            properties: vec![PropertyIr {
                property_id: "P_EVENTUAL".to_string(),
                kind: PropertyKind::Temporal,
                expr: ExprIr::Unary {
                    op: crate::ir::UnaryOp::TemporalEventually,
                    expr: Box::new(ExprIr::Binary {
                        op: BinaryOp::Equal,
                        left: Box::new(ExprIr::FieldRef("x".to_string())),
                        right: Box::new(ExprIr::Literal(Value::UInt(1))),
                    }),
                },
                scope: None,
                action_filter: None,
            }],
        };
        let outcome = check_explicit(
            &model,
            &RunPlan {
                property_selection: PropertySelection::ExactlyOne("P_EVENTUAL".to_string()),
                ..RunPlan::default()
            },
        );
        let CheckOutcome::Completed(result) = outcome else {
            panic!("expected completed")
        };
        assert_eq!(result.status, RunStatus::Fail);
        assert_eq!(
            result.property_result.reason_code.as_deref(),
            Some("TEMPORAL_PROPERTY_FAILED")
        );
    }
}
