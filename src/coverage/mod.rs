//! Coverage collection and gate evaluation.

use std::collections::BTreeSet;

use crate::{evidence::EvidenceTrace, ir::ModelIr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageReport {
    pub transition_coverage_percent: u32,
    pub covered_actions: BTreeSet<String>,
    pub total_actions: BTreeSet<String>,
    pub visited_state_count: usize,
    pub max_depth_observed: u32,
    pub guard_true_actions: BTreeSet<String>,
    pub guard_false_actions: BTreeSet<String>,
    pub step_count: usize,
}

pub fn collect_coverage(model: &ModelIr, traces: &[EvidenceTrace]) -> CoverageReport {
    let total_actions = model
        .actions
        .iter()
        .map(|a| a.action_id.clone())
        .collect::<BTreeSet<_>>();
    let mut covered_actions = BTreeSet::new();
    let mut visited_states = BTreeSet::new();
    let mut max_depth_observed = 0u32;
    let mut guard_true_actions = BTreeSet::new();
    let mut guard_false_actions = total_actions.clone();
    let mut step_count = 0usize;
    for trace in traces {
        for step in &trace.steps {
            step_count += 1;
            max_depth_observed = max_depth_observed.max(step.depth);
            visited_states.insert(step.from_state_id.clone());
            visited_states.insert(step.to_state_id.clone());
            if let Some(action_id) = &step.action_id {
                covered_actions.insert(action_id.clone());
                guard_true_actions.insert(action_id.clone());
                guard_false_actions.remove(action_id);
            }
        }
    }
    let transition_coverage_percent = if total_actions.is_empty() {
        100
    } else {
        ((covered_actions.len() * 100) / total_actions.len()) as u32
    };
    CoverageReport {
        transition_coverage_percent,
        covered_actions,
        total_actions,
        visited_state_count: visited_states.len(),
        max_depth_observed,
        guard_true_actions,
        guard_false_actions,
        step_count,
    }
}

pub fn evaluate_coverage_gate(report: &CoverageReport, minimum_percent: u32) -> bool {
    report.transition_coverage_percent >= minimum_percent
}

pub fn render_coverage_json(report: &CoverageReport) -> String {
    let mut out = String::from("{");
    out.push_str(&format!(
        "\"transition_coverage_percent\":{}",
        report.transition_coverage_percent
    ));
    out.push_str(",\"covered_actions\":[");
    for (index, action) in report.covered_actions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\"", action));
    }
    out.push(']');
    out.push_str(",\"total_actions\":[");
    for (index, action) in report.total_actions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\"", action));
    }
    out.push(']');
    out.push_str(&format!(
        ",\"visited_state_count\":{}",
        report.visited_state_count
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
    out.push('}');
    out
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

    use super::{collect_coverage, evaluate_coverage_gate};

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
        assert_eq!(report.transition_coverage_percent, 50);
        assert_eq!(report.visited_state_count, 2);
        assert_eq!(report.max_depth_observed, 1);
        assert!(report.guard_true_actions.contains("A_INC"));
        assert!(report.guard_false_actions.contains("A_DEC"));
        assert!(!evaluate_coverage_gate(&report, 60));
    }
}
