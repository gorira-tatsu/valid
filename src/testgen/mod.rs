//! Test vector generation and rendering.

use crate::{evidence::EvidenceTrace, support::artifact::generated_test_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestVector {
    pub schema_version: String,
    pub vector_id: String,
    pub source_kind: String,
    pub evidence_id: Option<String>,
    pub strategy: String,
    pub generator_version: String,
    pub seed: Option<u64>,
    pub actions: Vec<VectorActionStep>,
    pub expected_states: Vec<String>,
    pub property_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorActionStep {
    pub index: usize,
    pub action_id: String,
    pub action_label: String,
}

pub fn build_counterexample_vector(trace: &EvidenceTrace) -> Result<TestVector, String> {
    if trace.steps.is_empty() {
        return Err("cannot build a counterexample vector from an empty trace".to_string());
    }
    let actions = trace
        .steps
        .iter()
        .filter_map(|step| {
            step.action_id.as_ref().map(|action_id| VectorActionStep {
                index: step.index,
                action_id: action_id.clone(),
                action_label: step.action_label.clone().unwrap_or_else(|| action_id.clone()),
            })
        })
        .collect::<Vec<_>>();
    let expected_states = trace
        .steps
        .iter()
        .map(|step| format!("{:?}", step.state_after))
        .collect::<Vec<_>>();
    Ok(TestVector {
        schema_version: "1.0.0".to_string(),
        vector_id: trace.evidence_id.replace("ev-", "vec-"),
        source_kind: "counterexample".to_string(),
        evidence_id: Some(trace.evidence_id.clone()),
        strategy: "counterexample".to_string(),
        generator_version: env!("CARGO_PKG_VERSION").to_string(),
        seed: None,
        actions,
        expected_states,
        property_id: trace.property_id.clone(),
    })
}

pub fn render_rust_test(vector: &TestVector) -> String {
    let mut out = String::new();
    out.push_str("#[test]\n");
    out.push_str(&format!("fn generated_{}() {{\n", vector.vector_id.replace('-', "_")));
    out.push_str(&format!("    let vector_id = \"{}\";\n", vector.vector_id));
    out.push_str(&format!("    let property_id = \"{}\";\n", vector.property_id));
    out.push_str("    let mut actions = Vec::new();\n");
    for action in &vector.actions {
        out.push_str(&format!("    actions.push(\"{}\");\n", action.action_id));
    }
    out.push_str("    assert!(!actions.is_empty(), \"vector_id={} property_id={}\", vector_id, property_id);\n");
    out.push_str("}\n");
    out
}

pub fn generated_test_output_path(vector: &TestVector) -> String {
    generated_test_path(&vector.vector_id)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{engine::AssuranceLevel, evidence::{EvidenceKind, EvidenceTrace, TraceStep}, ir::Value};

    use super::{build_counterexample_vector, render_rust_test};

    #[test]
    fn builds_vector_from_trace() {
        let trace = EvidenceTrace {
            schema_version: "1.0.0".to_string(),
            evidence_id: "ev-000001".to_string(),
            run_id: "run-1".to_string(),
            property_id: "P_SAFE".to_string(),
            evidence_kind: EvidenceKind::Trace,
            assurance_level: AssuranceLevel::Complete,
            trace_hash: "sha256:x".to_string(),
            steps: vec![TraceStep {
                index: 0,
                from_state_id: "s-0".to_string(),
                action_id: Some("A_INC".to_string()),
                action_label: Some("Inc".to_string()),
                to_state_id: "s-1".to_string(),
                depth: 1,
                state_before: BTreeMap::from([("x".to_string(), Value::UInt(0))]),
                state_after: BTreeMap::from([("x".to_string(), Value::UInt(1))]),
                note: None,
            }],
        };
        let vector = build_counterexample_vector(&trace).unwrap();
        assert_eq!(vector.vector_id, "vec-000001");
        assert!(render_rust_test(&vector).contains("generated_vec_000001"));
    }
}
