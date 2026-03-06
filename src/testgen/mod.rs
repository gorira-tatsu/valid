//! Test vector generation and rendering.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    evidence::EvidenceTrace,
    ir::{ModelIr, PropertyKind, Value},
    kernel::{
        eval::eval_expr,
        replay::replay_actions,
        transition::{apply_action, build_initial_state},
    },
    support::{artifact::generated_test_path, hash::stable_hash_hex},
};

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
    pub initial_state: Option<BTreeMap<String, Value>>,
    pub expected_states: Vec<String>,
    pub property_id: String,
    pub minimized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorActionStep {
    pub index: usize,
    pub action_id: String,
    pub action_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimizeResult {
    pub original_steps: usize,
    pub minimized_steps: usize,
    pub vector: TestVector,
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
                action_label: step
                    .action_label
                    .clone()
                    .unwrap_or_else(|| action_id.clone()),
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
        initial_state: trace.steps.first().map(|step| step.state_before.clone()),
        actions,
        expected_states,
        property_id: trace.property_id.clone(),
        minimized: false,
    })
}

pub fn build_transition_coverage_vectors(
    traces: &[EvidenceTrace],
    all_action_ids: &[String],
) -> Vec<TestVector> {
    let mut uncovered = all_action_ids.iter().cloned().collect::<BTreeSet<_>>();
    let mut ordered = traces.to_vec();
    ordered.sort_by_key(|trace| trace.steps.len());
    let mut vectors = Vec::new();

    for trace in ordered {
        let covered = trace
            .steps
            .iter()
            .filter_map(|step| step.action_id.clone())
            .filter(|action_id| uncovered.contains(action_id))
            .collect::<BTreeSet<_>>();
        if covered.is_empty() {
            continue;
        }
        if let Ok(vector) = build_witness_vector(&trace) {
            for action_id in covered {
                uncovered.remove(&action_id);
            }
            vectors.push(vector);
        }
        if uncovered.is_empty() {
            break;
        }
    }

    vectors
}

pub fn build_witness_vector(trace: &EvidenceTrace) -> Result<TestVector, String> {
    if trace.steps.is_empty() {
        return Err("cannot build a witness vector from an empty trace".to_string());
    }
    let mut vector = build_counterexample_vector(trace)?;
    vector.source_kind = "witness".to_string();
    vector.strategy = "transition_coverage".to_string();
    vector.minimized = false;
    Ok(vector)
}

pub fn build_synthetic_witness_vectors(model: &ModelIr, property_id: &str) -> Vec<TestVector> {
    let initial = match build_initial_state(model) {
        Ok(state) => state,
        Err(_) => return Vec::new(),
    };
    let mut vectors = Vec::new();
    let mut seen_sequences = BTreeSet::new();

    for first_action in &model.actions {
        let Some(first_state) = apply_action(model, &initial, &first_action.action_id)
            .ok()
            .flatten()
        else {
            continue;
        };

        let single = synthetic_trace_from_states(
            model,
            property_id,
            &[(&initial, first_action.action_id.as_str(), first_action.label.as_str(), &first_state)],
        );
        if let Some(vector) = single
            .as_ref()
            .and_then(|trace| build_witness_vector(trace).ok())
        {
            let signature = vector
                .actions
                .iter()
                .map(|step| step.action_id.clone())
                .collect::<Vec<_>>();
            if seen_sequences.insert(signature) {
                vectors.push(vector);
            }
        }

        for second_action in &model.actions {
            let Some(second_state) = apply_action(model, &first_state, &second_action.action_id)
                .ok()
                .flatten()
            else {
                continue;
            };
            let Some(trace) = synthetic_trace_from_states(
                model,
                property_id,
                &[
                    (&initial, first_action.action_id.as_str(), first_action.label.as_str(), &first_state),
                    (
                        &first_state,
                        second_action.action_id.as_str(),
                        second_action.label.as_str(),
                        &second_state,
                    ),
                ],
            ) else {
                continue;
            };
            let Ok(vector) = build_witness_vector(&trace) else {
                continue;
            };
            let signature = vector
                .actions
                .iter()
                .map(|step| step.action_id.clone())
                .collect::<Vec<_>>();
            if seen_sequences.insert(signature) {
                vectors.push(vector);
            }
        }
    }

    vectors
}

fn synthetic_trace_from_states(
    model: &ModelIr,
    property_id: &str,
    transitions: &[(
        &crate::kernel::MachineState,
        &str,
        &str,
        &crate::kernel::MachineState,
    )],
) -> Option<EvidenceTrace> {
    if transitions.is_empty() {
        return None;
    }
    let steps = transitions
        .iter()
        .enumerate()
        .map(|(index, (before, action_id, action_label, after))| crate::evidence::TraceStep {
            index,
            from_state_id: if index == 0 {
                "s-init".to_string()
            } else {
                format!("s-{index}")
            },
            action_id: Some((*action_id).to_string()),
            action_label: Some((*action_label).to_string()),
            to_state_id: format!("s-{}", index + 1),
            depth: (index + 1) as u32,
            state_before: model
                .state_fields
                .iter()
                .enumerate()
                .map(|(field_index, field)| (field.name.clone(), before.values[field_index].clone()))
                .collect::<BTreeMap<_, _>>(),
            state_after: model
                .state_fields
                .iter()
                .enumerate()
                .map(|(field_index, field)| (field.name.clone(), after.values[field_index].clone()))
                .collect::<BTreeMap<_, _>>(),
            note: Some(if transitions.len() == 1 {
                "synthetic witness from initial state".to_string()
            } else {
                "synthetic witness sequence".to_string()
            }),
        })
        .collect::<Vec<_>>();

    let action_signature = transitions
        .iter()
        .map(|(_, action_id, _, _)| *action_id)
        .collect::<Vec<_>>()
        .join("->");

    Some(EvidenceTrace {
        schema_version: "1.0.0".to_string(),
        evidence_id: format!(
            "ev-witness-{}",
            stable_hash_hex(&(model.model_id.clone() + &action_signature)).replace("sha256:", "")
        ),
        run_id: format!("run-witness-{}", action_signature.replace("->", "-")),
        property_id: property_id.to_string(),
        evidence_kind: crate::evidence::EvidenceKind::Trace,
        assurance_level: crate::engine::AssuranceLevel::Complete,
        trace_hash: format!("trace:witness:{action_signature}"),
        steps,
    })
}

pub fn minimize_counterexample_vector(
    model: &ModelIr,
    vector: &TestVector,
    property_id: &str,
) -> Result<MinimizeResult, String> {
    let property = model
        .properties
        .iter()
        .find(|item| item.property_id == property_id)
        .ok_or_else(|| format!("unknown property `{property_id}`"))?;
    if property.kind != PropertyKind::Invariant {
        return Err("only invariant minimization is supported in the MVP".to_string());
    }

    let original_steps = vector.actions.len();
    let mut current = vector.clone();
    let mut changed = true;
    while changed {
        changed = false;
        let len = current.actions.len();
        for start in 0..len {
            let mut candidate = current.clone();
            candidate.actions.remove(start);
            if candidate.actions.is_empty() {
                continue;
            }
            if reproduces_failure(model, property_id, &candidate)? {
                candidate.vector_id = format!("{}-min", vector.vector_id);
                candidate.minimized = true;
                current = candidate;
                changed = true;
                break;
            }
        }
    }

    Ok(MinimizeResult {
        original_steps,
        minimized_steps: current.actions.len(),
        vector: current,
    })
}

fn reproduces_failure(
    model: &ModelIr,
    property_id: &str,
    vector: &TestVector,
) -> Result<bool, String> {
    let property = model
        .properties
        .iter()
        .find(|item| item.property_id == property_id)
        .ok_or_else(|| format!("unknown property `{property_id}`"))?;
    let mut state = build_initial_state(model).map_err(|diagnostic| diagnostic.message.clone())?;
    let initial_eval = eval_expr(model, &state, &property.expr)
        .map_err(|diagnostic| diagnostic.message.clone())?;
    if matches!(initial_eval, Value::Bool(false)) {
        return Ok(true);
    }
    let action_ids = vector
        .actions
        .iter()
        .map(|step| step.action_id.clone())
        .collect::<Vec<_>>();
    for end in 1..=action_ids.len() {
        state = replay_actions(model, &action_ids[..end])
            .map_err(|diagnostic| diagnostic.message.clone())?;
        let value = eval_expr(model, &state, &property.expr)
            .map_err(|diagnostic| diagnostic.message.clone())?;
        if matches!(value, Value::Bool(false)) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn render_rust_test(vector: &TestVector) -> String {
    let mut out = String::new();
    out.push_str("#[test]\n");
    out.push_str(&format!(
        "fn generated_{}() {{\n",
        vector.vector_id.replace('-', "_")
    ));
    out.push_str(&format!("    let vector_id = \"{}\";\n", vector.vector_id));
    out.push_str(&format!(
        "    let property_id = \"{}\";\n",
        vector.property_id
    ));
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

    use crate::{
        engine::AssuranceLevel,
        evidence::{EvidenceKind, EvidenceTrace, TraceStep},
        ir::{
            ActionIr, BinaryOp, ExprIr, FieldType, InitAssignment, ModelIr, PropertyIr,
            PropertyKind, SourceSpan, StateField, UpdateIr, Value,
        },
    };

    use super::{
        build_counterexample_vector, build_synthetic_witness_vectors,
        build_transition_coverage_vectors, build_witness_vector, minimize_counterexample_vector,
        render_rust_test,
    };

    fn trace_with_actions(actions: &[&str]) -> EvidenceTrace {
        let mut x = 0u64;
        let steps = actions
            .iter()
            .enumerate()
            .map(|(index, action)| {
                let before = BTreeMap::from([("x".to_string(), Value::UInt(x))]);
                x += 1;
                let after = BTreeMap::from([("x".to_string(), Value::UInt(x))]);
                TraceStep {
                    index,
                    from_state_id: format!("s-{index:06}"),
                    action_id: Some((*action).to_string()),
                    action_label: Some((*action).to_string()),
                    to_state_id: format!("s-{:06}", index + 1),
                    depth: (index + 1) as u32,
                    state_before: before,
                    state_after: after,
                    note: None,
                }
            })
            .collect();
        EvidenceTrace {
            schema_version: "1.0.0".to_string(),
            evidence_id: format!("ev-{}", actions.join("-")),
            run_id: "run-1".to_string(),
            property_id: "SAFE".to_string(),
            evidence_kind: EvidenceKind::Trace,
            assurance_level: AssuranceLevel::Complete,
            trace_hash: "sha256:trace".to_string(),
            steps,
        }
    }

    fn minimization_model() -> ModelIr {
        ModelIr {
            model_id: "Mini".to_string(),
            state_fields: vec![StateField {
                id: "x".to_string(),
                name: "x".to_string(),
                ty: FieldType::BoundedU8 { min: 0, max: 7 },
                span: SourceSpan { line: 1, column: 1 },
            }],
            init: vec![InitAssignment {
                field: "x".to_string(),
                value: Value::UInt(0),
                span: SourceSpan { line: 2, column: 1 },
            }],
            actions: vec![
                ActionIr {
                    action_id: "Inc".to_string(),
                    label: "Inc".to_string(),
                    reads: vec!["x".to_string()],
                    writes: vec!["x".to_string()],
                    guard: ExprIr::Literal(Value::Bool(true)),
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
        assert_eq!(
            vector.initial_state,
            Some(BTreeMap::from([("x".to_string(), Value::UInt(0))]))
        );
        assert!(render_rust_test(&vector).contains("generated_vec_000001"));
    }

    #[test]
    fn builds_witness_vectors_for_uncovered_actions() {
        let traces = vec![trace_with_actions(&["Inc"]), trace_with_actions(&["Jump"])];
        let vectors =
            build_transition_coverage_vectors(&traces, &["Inc".to_string(), "Jump".to_string()]);
        assert_eq!(vectors.len(), 2);
        assert!(vectors.iter().all(|vector| vector.source_kind == "witness"));
    }

    #[test]
    fn builds_synthetic_witness_vectors_from_model() {
        let vectors = build_synthetic_witness_vectors(&minimization_model(), "SAFE");
        assert_eq!(vectors.len(), 6);
        assert!(vectors.iter().all(|vector| vector.source_kind == "witness"));
        assert!(vectors.iter().any(|vector| vector.actions.len() == 2));
    }

    #[test]
    fn minimizes_counterexample_vector_while_preserving_failure() {
        let trace = trace_with_actions(&["Inc", "Jump"]);
        let vector = build_witness_vector(&trace).unwrap();
        let result =
            minimize_counterexample_vector(&minimization_model(), &vector, "SAFE").unwrap();
        assert_eq!(result.original_steps, 2);
        assert_eq!(result.minimized_steps, 1);
        assert_eq!(result.vector.actions[0].action_id, "Jump");
        assert!(result.vector.minimized);
    }
}
