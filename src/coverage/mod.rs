//! Coverage collection and gate evaluation.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    evidence::EvidenceTrace,
    ir::ModelIr,
    kernel::{guard::evaluate_guard, MachineState},
    support::{
        json::{
            parse_json, require_array_field, require_bool_field, require_number_field,
            require_object, require_string_field,
        },
        schema::{require_non_empty, require_schema_version},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageReport {
    pub schema_version: String,
    pub model_id: String,
    pub transition_coverage_percent: u32,
    pub guard_full_coverage_percent: u32,
    pub covered_actions: BTreeSet<String>,
    pub total_actions: BTreeSet<String>,
    pub action_execution_counts: BTreeMap<String, usize>,
    pub visited_state_count: usize,
    pub repeated_state_count: usize,
    pub max_depth_observed: u32,
    pub guard_true_actions: BTreeSet<String>,
    pub guard_false_actions: BTreeSet<String>,
    pub guard_true_counts: BTreeMap<String, usize>,
    pub guard_false_counts: BTreeMap<String, usize>,
    pub uncovered_guards: Vec<String>,
    pub depth_histogram: BTreeMap<u32, usize>,
    pub step_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverageGateStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageGateResult {
    pub schema_version: String,
    pub status: CoverageGateStatus,
    pub policy_id: String,
    pub reasons: Vec<String>,
}

pub fn collect_coverage(model: &ModelIr, traces: &[EvidenceTrace]) -> CoverageReport {
    let total_actions = model
        .actions
        .iter()
        .map(|a| a.action_id.clone())
        .collect::<BTreeSet<_>>();
    let mut covered_actions = BTreeSet::new();
    let mut visited_states = BTreeSet::new();
    let mut all_seen_states = 0usize;
    let mut max_depth_observed = 0u32;
    let mut guard_true_actions = BTreeSet::new();
    let mut guard_false_actions = BTreeSet::new();
    let mut guard_true_counts = BTreeMap::new();
    let mut guard_false_counts = BTreeMap::new();
    let mut action_execution_counts = BTreeMap::new();
    let mut depth_histogram = BTreeMap::new();
    let mut step_count = 0usize;
    for trace in traces {
        let mut state_depths: BTreeMap<String, u32> = BTreeMap::new();
        for step in &trace.steps {
            step_count += 1;
            max_depth_observed = max_depth_observed.max(step.depth);
            all_seen_states += 2;
            visited_states.insert(step.from_state_id.clone());
            visited_states.insert(step.to_state_id.clone());
            state_depths
                .entry(step.from_state_id.clone())
                .and_modify(|depth| *depth = (*depth).min(step.depth.saturating_sub(1)))
                .or_insert(step.depth.saturating_sub(1));
            state_depths
                .entry(step.to_state_id.clone())
                .and_modify(|depth| *depth = (*depth).min(step.depth))
                .or_insert(step.depth);
            if let Some(action_id) = &step.action_id {
                covered_actions.insert(action_id.clone());
                *action_execution_counts
                    .entry(action_id.clone())
                    .or_insert(0) += 1;
            }

            if let Some(state) = machine_state_from_snapshot(model, &step.state_before) {
                for action in &model.actions {
                    match evaluate_guard(model, &state, action) {
                        Ok(true) => {
                            guard_true_actions.insert(action.action_id.clone());
                            *guard_true_counts
                                .entry(action.action_id.clone())
                                .or_insert(0) += 1;
                        }
                        Ok(false) => {
                            guard_false_actions.insert(action.action_id.clone());
                            *guard_false_counts
                                .entry(action.action_id.clone())
                                .or_insert(0) += 1;
                        }
                        Err(_) => {}
                    }
                }
            }
        }
        for depth in state_depths.into_values() {
            *depth_histogram.entry(depth).or_insert(0) += 1;
        }
    }
    let transition_coverage_percent = if total_actions.is_empty() {
        100
    } else {
        ((covered_actions.len() * 100) / total_actions.len()) as u32
    };
    let fully_covered_guards = total_actions
        .iter()
        .filter(|action| {
            guard_true_actions.contains(*action) && guard_false_actions.contains(*action)
        })
        .count();
    let guard_full_coverage_percent = if total_actions.is_empty() {
        100
    } else {
        ((fully_covered_guards * 100) / total_actions.len()) as u32
    };
    let uncovered_guards = total_actions
        .iter()
        .filter_map(|action| {
            if guard_true_actions.contains(action) && guard_false_actions.contains(action) {
                None
            } else if guard_true_actions.contains(action) {
                Some(format!("{action}:false"))
            } else if guard_false_actions.contains(action) {
                Some(format!("{action}:true"))
            } else {
                Some(format!("{action}:true,false"))
            }
        })
        .collect::<Vec<_>>();
    CoverageReport {
        schema_version: "1.0.0".to_string(),
        model_id: model.model_id.clone(),
        transition_coverage_percent,
        guard_full_coverage_percent,
        covered_actions,
        total_actions,
        action_execution_counts,
        visited_state_count: visited_states.len(),
        repeated_state_count: all_seen_states.saturating_sub(visited_states.len()),
        max_depth_observed,
        guard_true_actions,
        guard_false_actions,
        guard_true_counts,
        guard_false_counts,
        uncovered_guards,
        depth_histogram,
        step_count,
    }
}

pub fn evaluate_coverage_gate(report: &CoverageReport, minimum_percent: u32) -> CoverageGateResult {
    let mut reasons = Vec::new();
    let status = if report.transition_coverage_percent >= minimum_percent
        && report.guard_full_coverage_percent >= 80
    {
        CoverageGateStatus::Pass
    } else if report.transition_coverage_percent >= minimum_percent {
        reasons.push("guard_full_coverage below threshold".to_string());
        CoverageGateStatus::Warn
    } else {
        reasons.push("transition_coverage below threshold".to_string());
        if report.guard_full_coverage_percent < 80 {
            reasons.push("guard_full_coverage below threshold".to_string());
        }
        CoverageGateStatus::Fail
    };
    CoverageGateResult {
        schema_version: "1.0.0".to_string(),
        status,
        policy_id: "default-mvp-policy".to_string(),
        reasons,
    }
}

pub fn render_coverage_json(report: &CoverageReport) -> String {
    let gate = evaluate_coverage_gate(report, 80);
    let mut out = String::from("{");
    out.push_str(&format!("\"schema_version\":\"{}\"", report.schema_version));
    out.push_str(&format!(",\"model_id\":\"{}\"", report.model_id));
    out.push_str(&format!(
        ",\"summary\":{{\"transition_coverage_percent\":{},\"guard_full_coverage_percent\":{},\"visited_state_count\":{},\"repeated_state_count\":{},\"step_count\":{},\"max_depth_observed\":{}}}",
        report.transition_coverage_percent,
        report.guard_full_coverage_percent,
        report.visited_state_count,
        report.repeated_state_count,
        report.step_count,
        report.max_depth_observed
    ));
    out.push_str(",\"actions\":[");
    for (index, action) in report.total_actions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"action_id\":\"{}\",\"covered\":{},\"count\":{}}}",
            action,
            report.covered_actions.contains(action),
            report
                .action_execution_counts
                .get(action)
                .copied()
                .unwrap_or(0)
        ));
    }
    out.push(']');
    out.push_str(",\"guards\":[");
    for (index, action) in report.total_actions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"action_id\":\"{}\",\"true_seen\":{},\"false_seen\":{},\"true_count\":{},\"false_count\":{},\"covered_both\":{}}}",
            action,
            report.guard_true_actions.contains(action),
            report.guard_false_actions.contains(action),
            report.guard_true_counts.get(action).copied().unwrap_or(0),
            report.guard_false_counts.get(action).copied().unwrap_or(0),
            report.guard_true_actions.contains(action) && report.guard_false_actions.contains(action)
        ));
    }
    out.push(']');
    out.push_str(",\"depth_histogram\":{");
    for (index, (depth, count)) in report.depth_histogram.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\":{}", depth, count));
    }
    out.push('}');
    out.push_str(&format!(
        ",\"visited_state_count\":{}",
        report.visited_state_count
    ));
    out.push_str(&format!(
        ",\"repeated_state_count\":{}",
        report.repeated_state_count
    ));
    out.push_str(&format!(
        ",\"max_depth_observed\":{}",
        report.max_depth_observed
    ));
    out.push_str(",\"guard_true_actions\":[");
    for (index, action) in report.guard_true_actions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\"", action));
    }
    out.push(']');
    out.push_str(",\"guard_false_actions\":[");
    for (index, action) in report.guard_false_actions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\"", action));
    }
    out.push(']');
    out.push_str(&format!(",\"step_count\":{}", report.step_count));
    out.push_str(&format!(
        ",\"gate\":{{\"schema_version\":\"{}\",\"status\":\"{}\",\"policy_id\":\"{}\",\"reasons\":[",
        gate.schema_version,
        coverage_gate_status_label(&gate.status),
        gate.policy_id
    ));
    for (index, reason) in gate.reasons.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\"", reason));
    }
    out.push_str("]}");
    out.push('}');
    out
}

pub fn render_coverage_text(report: &CoverageReport) -> String {
    let gate = evaluate_coverage_gate(report, 80);
    format!(
        "COVERAGE model={}\ntransition_coverage_percent={}\nguard_full_coverage_percent={}\nvisited_state_count={}\nrepeated_state_count={}\nstep_count={}\nmax_depth_observed={}\ngate_status={}\n{}",
        report.model_id,
        report.transition_coverage_percent,
        report.guard_full_coverage_percent,
        report.visited_state_count,
        report.repeated_state_count,
        report.step_count,
        report.max_depth_observed,
        coverage_gate_status_label(&gate.status),
        if report.uncovered_guards.is_empty() {
            "uncovered_guards=".to_string()
        } else {
            format!("uncovered_guards={}", report.uncovered_guards.join(", "))
        }
    )
}

pub fn validate_coverage_report(report: &CoverageReport) -> Result<(), String> {
    require_schema_version(&report.schema_version)?;
    require_non_empty(&report.model_id, "model_id")?;
    if report.transition_coverage_percent > 100 {
        return Err("transition_coverage_percent must not exceed 100".to_string());
    }
    if report.guard_full_coverage_percent > 100 {
        return Err("guard_full_coverage_percent must not exceed 100".to_string());
    }
    if report.covered_actions.len() > report.total_actions.len() {
        return Err("covered_actions must be a subset of total_actions".to_string());
    }
    for action in &report.covered_actions {
        if !report.total_actions.contains(action) {
            return Err("covered_actions must reference declared actions only".to_string());
        }
    }
    for action in report.action_execution_counts.keys() {
        if !report.total_actions.contains(action) {
            return Err("action_execution_counts must reference declared actions only".to_string());
        }
    }
    for action in report.guard_true_counts.keys() {
        if !report.total_actions.contains(action) {
            return Err("guard_true_counts must reference declared actions only".to_string());
        }
    }
    for action in report.guard_false_counts.keys() {
        if !report.total_actions.contains(action) {
            return Err("guard_false_counts must reference declared actions only".to_string());
        }
    }
    Ok(())
}

pub fn validate_rendered_coverage_json(body: &str) -> Result<(), String> {
    let root = parse_json(body)?;
    let object = require_object(&root, "coverage")?;
    require_string_field(object, "schema_version")?;
    require_string_field(object, "model_id")?;
    let summary = require_object(
        object
            .get("summary")
            .ok_or_else(|| "summary must be present".to_string())?,
        "summary",
    )?;
    require_number_field(summary, "transition_coverage_percent")?;
    require_number_field(summary, "guard_full_coverage_percent")?;
    require_number_field(summary, "visited_state_count")?;
    require_number_field(summary, "repeated_state_count")?;
    require_number_field(summary, "step_count")?;
    require_number_field(summary, "max_depth_observed")?;
    for action in require_array_field(object, "actions")? {
        let action_object = require_object(action, "actions[]")?;
        require_string_field(action_object, "action_id")?;
        require_bool_field(action_object, "covered")?;
        require_number_field(action_object, "count")?;
    }
    for guard in require_array_field(object, "guards")? {
        let guard_object = require_object(guard, "guards[]")?;
        require_string_field(guard_object, "action_id")?;
        require_bool_field(guard_object, "true_seen")?;
        require_bool_field(guard_object, "false_seen")?;
        require_number_field(guard_object, "true_count")?;
        require_number_field(guard_object, "false_count")?;
        require_bool_field(guard_object, "covered_both")?;
    }
    require_object(
        object
            .get("depth_histogram")
            .ok_or_else(|| "depth_histogram must be present".to_string())?,
        "depth_histogram",
    )?;
    let gate = require_object(
        object
            .get("gate")
            .ok_or_else(|| "gate must be present".to_string())?,
        "gate",
    )?;
    require_string_field(gate, "schema_version")?;
    require_string_field(gate, "status")?;
    require_string_field(gate, "policy_id")?;
    require_array_field(gate, "reasons")?;
    Ok(())
}

fn machine_state_from_snapshot(
    model: &ModelIr,
    snapshot: &BTreeMap<String, crate::ir::Value>,
) -> Option<MachineState> {
    let mut values = Vec::with_capacity(model.state_fields.len());
    for field in &model.state_fields {
        values.push(snapshot.get(&field.name)?.clone());
    }
    Some(MachineState::new(values))
}

fn coverage_gate_status_label(status: &CoverageGateStatus) -> &'static str {
    match status {
        CoverageGateStatus::Pass => "pass",
        CoverageGateStatus::Warn => "warn",
        CoverageGateStatus::Fail => "fail",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        engine::AssuranceLevel,
        evidence::{EvidenceKind, EvidenceTrace, TraceStep},
        frontend::compile_model,
        ir::Value,
    };

    use super::{
        collect_coverage, evaluate_coverage_gate, render_coverage_json, render_coverage_text,
        validate_coverage_report, validate_rendered_coverage_json, CoverageGateStatus,
    };

    #[test]
    fn computes_transition_coverage() {
        let model = compile_model("model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction A_INC:\n  pre: true\n  post:\n    x = 1\naction A_DEC:\n  pre: true\n  post:\n    x = 0\nproperty P_SAFE:\n  invariant: x <= 7\n").unwrap();
        let trace = EvidenceTrace {
            schema_version: "1.0.0".to_string(),
            evidence_id: "ev-1".to_string(),
            run_id: "run-1".to_string(),
            property_id: "P_SAFE".to_string(),
            evidence_kind: EvidenceKind::Trace,
            assurance_level: AssuranceLevel::Complete,
            trace_hash: "sha256:x".to_string(),
            steps: vec![TraceStep {
                index: 0,
                from_state_id: "s-0".to_string(),
                action_id: Some("A_INC".to_string()),
                action_label: Some("A_INC".to_string()),
                to_state_id: "s-1".to_string(),
                depth: 1,
                state_before: BTreeMap::from([("x".to_string(), Value::UInt(0))]),
                state_after: BTreeMap::from([("x".to_string(), Value::UInt(1))]),
                note: None,
            }],
        };
        let report = collect_coverage(&model, &[trace]);
        assert_eq!(report.schema_version, "1.0.0");
        assert_eq!(report.model_id, "A");
        assert_eq!(report.transition_coverage_percent, 50);
        assert_eq!(report.guard_full_coverage_percent, 0);
        assert_eq!(report.visited_state_count, 2);
        assert_eq!(report.repeated_state_count, 0);
        assert_eq!(report.max_depth_observed, 1);
        assert_eq!(report.action_execution_counts.get("A_INC"), Some(&1));
        assert!(report.guard_true_actions.contains("A_INC"));
        assert!(report.guard_true_actions.contains("A_DEC"));
        assert!(!report.guard_false_actions.contains("A_DEC"));
        assert_eq!(report.guard_true_counts.get("A_INC"), Some(&1));
        assert_eq!(report.depth_histogram.get(&0), Some(&1));
        assert_eq!(report.depth_histogram.get(&1), Some(&1));
        assert_eq!(
            evaluate_coverage_gate(&report, 60).status,
            CoverageGateStatus::Fail
        );
        let json = render_coverage_json(&report);
        assert!(json.contains("\"summary\""));
        validate_rendered_coverage_json(&json).unwrap();
        validate_coverage_report(&report).unwrap();
        let text = render_coverage_text(&report);
        assert!(text.contains("gate_status=fail"));
        assert!(text.contains("transition_coverage_percent=50"));
        assert!(text.contains("repeated_state_count=0"));
    }

    #[test]
    fn fails_gate_when_transition_coverage_is_below_threshold() {
        let model = compile_model("model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction A_INC:\n  pre: true\n  post:\n    x = 1\naction A_DEC:\n  pre: true\n  post:\n    x = 0\nproperty P_SAFE:\n  invariant: x <= 7\n").unwrap();
        let report = collect_coverage(&model, &[]);
        let gate = evaluate_coverage_gate(&report, 80);
        assert_eq!(gate.status, CoverageGateStatus::Fail);
        assert!(gate
            .reasons
            .iter()
            .any(|reason| reason == "transition_coverage below threshold"));
    }
}
