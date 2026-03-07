//! Test vector generation and rendering.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use crate::{
    evidence::EvidenceTrace,
    ir::{ModelIr, PropertyKind, Value},
    kernel::{
        eval::eval_expr,
        replay::replay_actions,
        transition::{apply_action, build_initial_state},
        MachineState,
    },
    support::{artifact::generated_test_path, hash::stable_hash_hex, io::write_text_file},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayTarget {
    pub runner: String,
    pub args: Vec<String>,
}

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
    pub focus_action_id: Option<String>,
    pub focus_field: Option<String>,
    pub expected_guard_enabled: Option<bool>,
    pub notes: Vec<String>,
    pub replay_target: Option<ReplayTarget>,
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
        focus_action_id: None,
        focus_field: None,
        expected_guard_enabled: None,
        notes: Vec::new(),
        replay_target: None,
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
            &[(
                &initial,
                first_action.action_id.as_str(),
                first_action.label.as_str(),
                &first_state,
            )],
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
                    (
                        &initial,
                        first_action.action_id.as_str(),
                        first_action.label.as_str(),
                        &first_state,
                    ),
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

pub fn build_model_test_vectors_for_strategy(
    model: &ModelIr,
    property_id: &str,
    strategy: &str,
) -> Result<Vec<TestVector>, String> {
    match strategy {
        "transition" | "witness" => {
            let vectors = build_synthetic_witness_vectors(model, property_id);
            if vectors.is_empty() {
                Ok(build_model_random_vectors(model, property_id, 3)?)
            } else {
                Ok(vectors)
            }
        }
        "path" => build_model_path_vectors(model, property_id),
        "guard" => build_model_guard_vectors(model, property_id),
        "boundary" => build_model_boundary_vectors(model, property_id),
        "random" => build_model_random_vectors(model, property_id, 5),
        _ => Ok(Vec::new()),
    }
}

fn model_transition_tags(model: &ModelIr, action_id: &str) -> Vec<String> {
    model
        .actions
        .iter()
        .find(|action| action.action_id == action_id)
        .map(|action| action.path_tags.clone())
        .unwrap_or_else(|| vec!["transition_path".to_string()])
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
        .map(
            |(index, (before, action_id, action_label, after))| crate::evidence::TraceStep {
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
                    .map(|(field_index, field)| {
                        (field.name.clone(), before.values[field_index].clone())
                    })
                    .collect::<BTreeMap<_, _>>(),
                state_after: model
                    .state_fields
                    .iter()
                    .enumerate()
                    .map(|(field_index, field)| {
                        (field.name.clone(), after.values[field_index].clone())
                    })
                    .collect::<BTreeMap<_, _>>(),
                note: Some(if transitions.len() == 1 {
                    "synthetic witness from initial state".to_string()
                } else {
                    "synthetic witness sequence".to_string()
                }),
            },
        )
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
    out.push_str(&format!(
        "    let source_kind = \"{}\";\n",
        vector.source_kind
    ));
    out.push_str(&format!("    let strategy = \"{}\";\n", vector.strategy));
    if let Some(action_id) = &vector.focus_action_id {
        out.push_str(&format!(
            "    let focus_action_id = Some(\"{}\");\n",
            action_id
        ));
    } else {
        out.push_str("    let focus_action_id: Option<&str> = None;\n");
    }
    if let Some(field) = &vector.focus_field {
        out.push_str(&format!("    let focus_field = Some(\"{}\");\n", field));
    } else {
        out.push_str("    let focus_field: Option<&str> = None;\n");
    }
    if let Some(enabled) = vector.expected_guard_enabled {
        out.push_str(&format!(
            "    let expected_guard_enabled = Some({enabled});\n"
        ));
    } else {
        out.push_str("    let expected_guard_enabled: Option<bool> = None;\n");
    }
    out.push_str("    let mut actions = Vec::new();\n");
    for action in &vector.actions {
        out.push_str(&format!("    actions.push(\"{}\");\n", action.action_id));
    }
    out.push_str("    let mut expected_states = Vec::new();\n");
    for state in &vector.expected_states {
        out.push_str(&format!("    expected_states.push({state:?});\n"));
    }
    out.push_str("    let mut notes = Vec::new();\n");
    for note in &vector.notes {
        out.push_str(&format!("    notes.push({note:?});\n"));
    }
    if let Some(target) = &vector.replay_target {
        out.push_str(&format!("    let replay_runner = {:?};\n", target.runner));
        out.push_str("    let mut replay_args = Vec::new();\n");
        for arg in &target.args {
            out.push_str(&format!("    replay_args.push({arg:?});\n"));
        }
        out.push_str("    let runner_path = match replay_runner {\n");
        out.push_str("        \"valid\" => env!(\"CARGO_BIN_EXE_valid\"),\n");
        out.push_str("        \"cargo-valid\" => env!(\"CARGO_BIN_EXE_cargo-valid\"),\n");
        out.push_str("        other => panic!(\"unknown replay runner: {other}\"),\n");
        out.push_str("    };\n");
        out.push_str("    let output = std::process::Command::new(runner_path)\n");
        out.push_str("        .args(&replay_args)\n");
        out.push_str("        .output()\n");
        out.push_str("        .expect(\"generated replay command should execute\");\n");
        out.push_str("    assert!(output.status.success(), \"replay failed: {}\", String::from_utf8_lossy(&output.stderr));\n");
        out.push_str("    let stdout = String::from_utf8(output.stdout).expect(\"replay output must be utf-8\");\n");
        out.push_str("    valid::testgen::assert_replay_output_json(&stdout, &actions, &expected_states, property_id, focus_action_id, expected_guard_enabled);\n");
    }
    out.push_str("    assert!(!vector_id.is_empty());\n");
    out.push_str("    assert!(!property_id.is_empty());\n");
    out.push_str("    assert!(!source_kind.is_empty());\n");
    out.push_str("    assert!(!strategy.is_empty());\n");
    out.push_str("    let _ = &notes;\n");
    out.push_str("    assert!(focus_action_id.is_some() || focus_field.is_some() || !actions.is_empty() || expected_guard_enabled.is_some() || !expected_states.is_empty());\n");
    out.push_str("}\n");
    out
}

pub fn assert_replay_output_json(
    body: &str,
    expected_actions: &[&str],
    expected_states: &[&str],
    expected_property_id: &str,
    expected_focus_action_id: Option<&str>,
    expected_guard_enabled: Option<bool>,
) {
    let normalized = body.trim();
    assert!(
        normalized.contains("\"status\":\"ok\""),
        "replay must return ok: {normalized}"
    );
    assert!(
        normalized.contains(&format!("\"property_id\":\"{expected_property_id}\"")),
        "replay property_id mismatch: {normalized}"
    );
    for action in expected_actions {
        assert!(
            normalized.contains(&format!("\"{action}\"")),
            "replay missing action `{action}`: {normalized}"
        );
    }
    if let Some(last_state) = expected_states.last() {
        assert!(
            normalized.contains(&format!("\"terminal_state\":{last_state:?}")),
            "replay terminal state mismatch: expected {last_state}, got {normalized}"
        );
    }
    if let Some(action_id) = expected_focus_action_id {
        assert!(
            normalized.contains(&format!("\"focus_action_id\":\"{action_id}\"")),
            "replay focus_action_id mismatch: {normalized}"
        );
    }
    if let Some(enabled) = expected_guard_enabled {
        assert!(
            normalized.contains(&format!("\"focus_action_enabled\":{enabled}")),
            "replay focus_action_enabled mismatch: {normalized}"
        );
    }
}

pub fn render_replay_json(
    property_id: &str,
    action_ids: &[String],
    terminal_state: &BTreeMap<String, Value>,
    focus_action_id: Option<&str>,
    focus_action_enabled: Option<bool>,
) -> String {
    let actions = action_ids
        .iter()
        .map(|action| format!("\"{action}\""))
        .collect::<Vec<_>>()
        .join(",");
    let focus_action_id = focus_action_id
        .map(|action| format!("\"{action}\""))
        .unwrap_or_else(|| "null".to_string());
    let focus_action_enabled = focus_action_enabled
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string());
    let terminal_state = format!("{terminal_state:?}");
    format!(
        "{{\"schema_version\":\"1.0.0\",\"status\":\"ok\",\"property_id\":\"{}\",\"replayed_actions\":[{}],\"terminal_state\":{:?},\"focus_action_id\":{},\"focus_action_enabled\":{}}}",
        property_id,
        actions,
        terminal_state,
        focus_action_id,
        focus_action_enabled
    )
}

pub fn generated_test_output_path(vector: &TestVector) -> String {
    generated_test_path(&vector.vector_id)
}

pub fn write_generated_test_files(vectors: &[TestVector]) -> Result<Vec<String>, String> {
    let mut generated_files = Vec::with_capacity(vectors.len());
    for vector in vectors {
        let rendered = render_rust_test(vector);
        let path = generated_test_output_path(vector);
        write_text_file(&path, &rendered)?;
        generated_files.push(path);
    }
    Ok(generated_files)
}

#[derive(Debug, Clone)]
struct ModelNode {
    state: MachineState,
    parent: Option<usize>,
    via_action_id: Option<String>,
    via_action_label: Option<String>,
}

#[derive(Debug, Clone)]
struct ModelEdge {
    action_id: String,
    to_index: usize,
}

#[derive(Debug, Clone)]
struct ModelExploration {
    nodes: Vec<ModelNode>,
    edges_by_node: Vec<Vec<ModelEdge>>,
}

fn explore_model(model: &ModelIr) -> Result<ModelExploration, String> {
    let initial = build_initial_state(model).map_err(|diagnostic| diagnostic.message.clone())?;
    let mut nodes = vec![ModelNode {
        state: initial.clone(),
        parent: None,
        via_action_id: None,
        via_action_label: None,
    }];
    let mut edges_by_node = vec![Vec::new()];
    let mut frontier = VecDeque::from([0usize]);
    let mut visited = HashSet::from([initial]);

    while let Some(node_index) = frontier.pop_front() {
        let state = nodes[node_index].state.clone();
        for action in &model.actions {
            let next = apply_action(model, &state, &action.action_id)
                .map_err(|diagnostic| diagnostic.message.clone())?;
            let Some(next_state) = next else {
                continue;
            };
            let to_index = if visited.insert(next_state.clone()) {
                let index = nodes.len();
                nodes.push(ModelNode {
                    state: next_state,
                    parent: Some(node_index),
                    via_action_id: Some(action.action_id.clone()),
                    via_action_label: Some(action.label.clone()),
                });
                edges_by_node.push(Vec::new());
                frontier.push_back(index);
                index
            } else {
                nodes
                    .iter()
                    .position(|node| node.state == next_state)
                    .expect("visited state must be present")
            };
            edges_by_node[node_index].push(ModelEdge {
                action_id: action.action_id.clone(),
                to_index,
            });
        }
    }

    Ok(ModelExploration {
        nodes,
        edges_by_node,
    })
}

fn build_model_guard_vectors(
    model: &ModelIr,
    property_id: &str,
) -> Result<Vec<TestVector>, String> {
    let exploration = explore_model(model)?;
    let mut vectors = Vec::new();
    let mut seen = BTreeSet::new();

    for action in &model.actions {
        if let Some((node_index, edge)) =
            exploration
                .edges_by_node
                .iter()
                .enumerate()
                .find_map(|(node_index, edges)| {
                    edges
                        .iter()
                        .find(|edge| edge.action_id == action.action_id)
                        .map(|edge| (node_index, edge))
                })
        {
            if let Some(vector) = build_model_vector_for_node(
                model,
                &exploration.nodes,
                edge.to_index,
                property_id,
                "guard",
                "guard",
                Some(action.action_id.clone()),
                None,
                Some(true),
                {
                    let mut notes = vec![format!("guard_true:{:?}", action.guard)];
                    notes.extend(
                        model_transition_tags(model, &action.action_id)
                            .into_iter()
                            .map(|tag| format!("path_tag:{tag}")),
                    );
                    notes
                },
            ) {
                let signature = (
                    vector.focus_action_id.clone(),
                    vector.expected_guard_enabled,
                    vector
                        .actions
                        .iter()
                        .map(|step| step.action_id.clone())
                        .collect::<Vec<_>>(),
                );
                if seen.insert(signature) {
                    vectors.push(vector);
                }
            }
            let _ = node_index;
        }

        if let Some((node_index, _)) = exploration.nodes.iter().enumerate().find(|(_, node)| {
            apply_action(model, &node.state, &action.action_id)
                .ok()
                .flatten()
                .is_none()
        }) {
            if let Some(vector) = build_model_vector_for_node(
                model,
                &exploration.nodes,
                node_index,
                property_id,
                "guard",
                "guard",
                Some(action.action_id.clone()),
                None,
                Some(false),
                {
                    let mut notes = vec![format!("guard_false:{:?}", action.guard)];
                    notes.extend(
                        model_transition_tags(model, &action.action_id)
                            .into_iter()
                            .map(|tag| format!("path_tag:{tag}")),
                    );
                    notes
                },
            ) {
                let signature = (
                    vector.focus_action_id.clone(),
                    vector.expected_guard_enabled,
                    vector
                        .actions
                        .iter()
                        .map(|step| step.action_id.clone())
                        .collect::<Vec<_>>(),
                );
                if seen.insert(signature) {
                    vectors.push(vector);
                }
            }
        }
    }

    Ok(vectors)
}

fn build_model_path_vectors(model: &ModelIr, property_id: &str) -> Result<Vec<TestVector>, String> {
    let exploration = explore_model(model)?;
    let mut vectors = Vec::new();
    let mut seen = BTreeSet::new();

    for action in &model.actions {
        let tags = model_transition_tags(model, &action.action_id);
        let Some((_, edge)) =
            exploration
                .edges_by_node
                .iter()
                .enumerate()
                .find_map(|(node_index, edges)| {
                    edges
                        .iter()
                        .find(|edge| edge.action_id == action.action_id)
                        .map(|edge| (node_index, edge))
                })
        else {
            continue;
        };
        for tag in tags {
            if let Some(vector) = build_model_vector_for_node(
                model,
                &exploration.nodes,
                edge.to_index,
                property_id,
                "path",
                "path",
                Some(action.action_id.clone()),
                None,
                Some(true),
                vec![format!("path_tag:{tag}")],
            ) {
                let signature = (
                    tag,
                    vector.focus_action_id.clone(),
                    vector
                        .actions
                        .iter()
                        .map(|step| step.action_id.clone())
                        .collect::<Vec<_>>(),
                );
                if seen.insert(signature) {
                    vectors.push(vector);
                }
            }
        }
    }

    Ok(vectors)
}

fn build_model_boundary_vectors(
    model: &ModelIr,
    property_id: &str,
) -> Result<Vec<TestVector>, String> {
    let exploration = explore_model(model)?;
    let mut vectors = Vec::new();
    let mut seen = BTreeSet::new();

    for (field_index, field) in model.state_fields.iter().enumerate() {
        let (min, max) = match field.ty {
            crate::ir::FieldType::BoundedU8 { min, max } => (min as u64, max as u64),
            crate::ir::FieldType::BoundedU16 { min, max } => (min as u64, max as u64),
            crate::ir::FieldType::Bool => continue,
        };
        for target in [min, max] {
            if let Some((node_index, _)) = exploration.nodes.iter().enumerate().find(|(_, node)| {
                matches!(node.state.values.get(field_index), Some(Value::UInt(value)) if *value == target)
            }) {
                if let Some(vector) = build_model_vector_for_node(
                    model,
                    &exploration.nodes,
                    node_index,
                    property_id,
                    "boundary",
                    "boundary",
                    None,
                    Some(field.name.clone()),
                    None,
                    vec![format!("boundary_target:{target}")],
                ) {
                    let signature = (
                        vector.focus_field.clone(),
                        vector.notes.clone(),
                        vector.actions.iter().map(|step| step.action_id.clone()).collect::<Vec<_>>(),
                    );
                    if seen.insert(signature) {
                        vectors.push(vector);
                    }
                }
            }
        }
    }

    Ok(vectors)
}

fn build_model_random_vectors(
    model: &ModelIr,
    property_id: &str,
    limit: usize,
) -> Result<Vec<TestVector>, String> {
    let exploration = explore_model(model)?;
    let mut candidates = exploration
        .nodes
        .iter()
        .enumerate()
        .filter(|(index, _)| *index > 0)
        .map(|(index, _)| {
            let signature = build_model_path(model, &exploration.nodes, index)
                .into_iter()
                .map(|step| step.action_id)
                .collect::<Vec<_>>()
                .join(",");
            (
                stable_hash_hex(&(model.model_id.clone() + &signature)),
                index,
            )
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.0.cmp(&right.0));

    let seed = stable_hash_hex(&model.model_id);
    Ok(candidates
        .into_iter()
        .take(limit)
        .filter_map(|(_, index)| {
            let mut vector = build_model_vector_for_node(
                model,
                &exploration.nodes,
                index,
                property_id,
                "random",
                "random",
                None,
                None,
                None,
                vec!["deterministic_randomized_sample".to_string()],
            )?;
            vector.seed = Some(seed.bytes().fold(0u64, |acc, byte| {
                acc.wrapping_mul(131).wrapping_add(byte as u64)
            }));
            Some(vector)
        })
        .collect())
}

fn build_model_vector_for_node(
    model: &ModelIr,
    nodes: &[ModelNode],
    end_index: usize,
    property_id: &str,
    source_kind: &str,
    strategy: &str,
    focus_action_id: Option<String>,
    focus_field: Option<String>,
    expected_guard_enabled: Option<bool>,
    notes: Vec<String>,
) -> Option<TestVector> {
    let steps = build_model_path(model, nodes, end_index);
    let actions = steps
        .iter()
        .enumerate()
        .map(|(index, step)| VectorActionStep {
            index,
            action_id: step.action_id.clone(),
            action_label: step.action_label.clone(),
        })
        .collect::<Vec<_>>();
    let expected_states = if steps.is_empty() {
        vec![format!(
            "{:?}",
            nodes.get(end_index)?.state.as_named_map(model)
        )]
    } else {
        steps
            .iter()
            .map(|step| format!("{:?}", step.state_after))
            .collect()
    };
    let signature = actions
        .iter()
        .map(|step| step.action_id.clone())
        .collect::<Vec<_>>()
        .join(",");
    Some(TestVector {
        schema_version: "1.0.0".to_string(),
        vector_id: format!(
            "vec-{}",
            stable_hash_hex(
                &(model.model_id.clone()
                    + property_id
                    + source_kind
                    + strategy
                    + &signature
                    + &format!(
                        "{focus_action_id:?}{focus_field:?}{expected_guard_enabled:?}{notes:?}"
                    ))
            )
            .replace("sha256:", "")
        ),
        source_kind: source_kind.to_string(),
        evidence_id: None,
        strategy: strategy.to_string(),
        generator_version: env!("CARGO_PKG_VERSION").to_string(),
        seed: None,
        actions,
        initial_state: Some(nodes.first()?.state.as_named_map(model)),
        expected_states,
        property_id: property_id.to_string(),
        minimized: false,
        focus_action_id,
        focus_field,
        expected_guard_enabled,
        notes,
        replay_target: None,
    })
}

struct ModelPathStep {
    action_id: String,
    action_label: String,
    state_after: BTreeMap<String, Value>,
}

fn build_model_path(model: &ModelIr, nodes: &[ModelNode], end_index: usize) -> Vec<ModelPathStep> {
    let mut indices = Vec::new();
    let mut cursor = Some(end_index);
    while let Some(index) = cursor {
        indices.push(index);
        cursor = nodes[index].parent;
    }
    indices.reverse();

    indices
        .windows(2)
        .map(|pair| {
            let after = &nodes[pair[1]];
            ModelPathStep {
                action_id: after
                    .via_action_id
                    .clone()
                    .expect("non-root node must have an action id"),
                action_label: after
                    .via_action_label
                    .clone()
                    .expect("non-root node must have an action label"),
                state_after: after.state.as_named_map(model),
            }
        })
        .collect()
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
                    path_tags: vec!["write_path".to_string()],
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
