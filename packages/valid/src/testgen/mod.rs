//! Test vector generation and rendering.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::{
    evidence::{write_vector_artifact, EvidenceKind, EvidenceTrace},
    ir::{DecisionKind, DecisionOutcome, ModelIr, Path, PropertyKind, Value},
    kernel::{
        eval::eval_expr,
        replay::replay_actions,
        transition::{apply_action_transition, build_initial_state},
        MachineState,
    },
    support::{
        artifact::generated_test_path,
        artifact_index::{record_artifact, ArtifactRecord},
        hash::stable_hash_hex,
        io::write_text_file,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayTarget {
    pub runner: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestVector {
    pub schema_version: String,
    pub vector_id: String,
    pub run_id: String,
    pub source_kind: String,
    pub strictness: String,
    pub derivation: String,
    pub evidence_id: Option<String>,
    pub strategy: String,
    pub generator_version: String,
    pub seed: Option<u64>,
    #[serde(default)]
    pub actions: Vec<VectorActionStep>,
    #[serde(default)]
    pub initial_state: Option<BTreeMap<String, Value>>,
    #[serde(default)]
    pub expected_observations: Vec<BTreeMap<String, Value>>,
    #[serde(default)]
    pub expected_states: Vec<String>,
    pub property_id: String,
    pub minimized: bool,
    pub focus_action_id: Option<String>,
    pub focus_field: Option<String>,
    pub expected_guard_enabled: Option<bool>,
    pub expected_property_holds: Option<bool>,
    #[serde(default)]
    pub expected_path: Path,
    #[serde(default)]
    pub expected_path_tags: Vec<String>,
    #[serde(default)]
    pub setup_action_ids: Vec<String>,
    #[serde(default)]
    pub business_action_ids: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub grouping: VectorGrouping,
    #[serde(default)]
    pub replay_target: Option<ReplayTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorActionStep {
    pub index: usize,
    pub action_id: String,
    pub action_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VectorGrouping {
    #[serde(default)]
    pub requirement_clusters: Vec<String>,
    #[serde(default)]
    pub risk_clusters: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimizeResult {
    pub original_steps: usize,
    pub minimized_steps: usize,
    pub vector: TestVector,
}

fn vector_provenance(source_kind: &str, strategy: &str) -> (&'static str, &'static str) {
    match (source_kind, strategy) {
        ("counterexample", _) => ("strict", "counterexample_trace"),
        ("witness", "transition_coverage") => ("strict", "witness_trace"),
        ("witness", _) => ("strict", "witness_trace"),
        (_, "transition") => ("heuristic", "transition_search"),
        (_, "guard") => ("heuristic", "guard_search"),
        (_, "boundary") => ("heuristic", "boundary_search"),
        (_, "path") => ("heuristic", "path_tag_search"),
        (_, "random") => ("heuristic", "deterministic_random_search"),
        _ => ("heuristic", "model_exploration"),
    }
}

fn infer_vector_grouping(path_tags: &[String]) -> VectorGrouping {
    let requirement_clusters = path_tags
        .iter()
        .filter(|tag| {
            !matches!(
                tag.as_str(),
                "guard_path" | "read_path" | "write_path" | "transition_path"
            )
        })
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let risk_clusters = requirement_clusters
        .iter()
        .filter(|tag| is_risk_cluster_tag(tag))
        .cloned()
        .collect::<Vec<_>>();
    VectorGrouping {
        requirement_clusters,
        risk_clusters,
    }
}

pub fn vector_grouping_from_path_tags(path_tags: &[String]) -> VectorGrouping {
    infer_vector_grouping(path_tags)
}

fn is_risk_cluster_tag(tag: &str) -> bool {
    matches!(
        tag,
        "risk_path"
            | "audit_path"
            | "compliance_path"
            | "exception_path"
            | "regression_path"
            | "recovery_path"
            | "security_path"
            | "privacy_path"
            | "fraud_path"
            | "governance_path"
            | "boundary_path"
            | "deny_path"
            | "tenant_isolation_path"
    ) || tag.contains("risk")
        || tag.contains("audit")
        || tag.contains("compliance")
        || tag.contains("security")
        || tag.contains("privacy")
        || tag.contains("fraud")
        || tag.contains("isolation")
        || tag.contains("governance")
        || tag.contains("regression")
        || tag.contains("recovery")
}

pub fn build_counterexample_vector(trace: &EvidenceTrace) -> Result<TestVector, String> {
    if trace.steps.is_empty() {
        return Err("cannot build a counterexample vector from an empty trace".to_string());
    }
    if trace.evidence_kind == EvidenceKind::Witness {
        return Err("cannot build a counterexample vector from a witness trace".to_string());
    }
    build_base_vector_from_trace(trace)
}

fn build_base_vector_from_trace(trace: &EvidenceTrace) -> Result<TestVector, String> {
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
    let expected_observations = trace
        .steps
        .iter()
        .map(|step| step.state_after.clone())
        .collect::<Vec<_>>();
    let expected_path = trace_path(trace);
    let expected_path_tags = path_tags_or_empty(&expected_path);
    let grouping = infer_vector_grouping(&expected_path_tags);
    Ok(TestVector {
        schema_version: "1.0.0".to_string(),
        vector_id: trace.evidence_id.replace("ev-", "vec-"),
        run_id: trace.run_id.clone(),
        source_kind: "counterexample".to_string(),
        strictness: "strict".to_string(),
        derivation: "counterexample_trace".to_string(),
        evidence_id: Some(trace.evidence_id.clone()),
        strategy: "counterexample".to_string(),
        generator_version: env!("CARGO_PKG_VERSION").to_string(),
        seed: None,
        initial_state: trace.steps.first().map(|step| step.state_before.clone()),
        expected_observations,
        actions,
        expected_states,
        property_id: trace.property_id.clone(),
        minimized: false,
        focus_action_id: None,
        focus_field: None,
        expected_guard_enabled: None,
        expected_property_holds: Some(false),
        expected_path,
        expected_path_tags,
        setup_action_ids: Vec::new(),
        business_action_ids: Vec::new(),
        notes: Vec::new(),
        grouping,
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
    if trace.evidence_kind == EvidenceKind::Counterexample {
        return Err("cannot build a witness vector from a counterexample trace".to_string());
    }
    let mut vector = build_base_vector_from_trace(trace)?;
    vector.source_kind = "witness".to_string();
    vector.strictness = "strict".to_string();
    vector.derivation = "witness_trace".to_string();
    vector.strategy = "transition_coverage".to_string();
    vector.minimized = false;
    vector.expected_property_holds = Some(true);
    Ok(vector)
}

pub fn build_synthetic_witness_vectors(model: &ModelIr, property_id: &str) -> Vec<TestVector> {
    let Some(property) = model
        .properties
        .iter()
        .find(|property| property.property_id == property_id)
    else {
        return Vec::new();
    };
    let initial = match build_initial_state(model) {
        Ok(state) => state,
        Err(_) => return Vec::new(),
    };
    let mut vectors = Vec::new();
    let mut seen_sequences = BTreeSet::new();

    for first_action in &model.actions {
        let Some(first_state) = apply_action_transition(model, &initial, first_action)
            .ok()
            .flatten()
        else {
            continue;
        };
        if !state_satisfies_property(model, property, &first_state) {
            continue;
        }

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
            let mut vector = vector;
            vector.strictness = "synthetic".to_string();
            vector.derivation = "synthetic_witness".to_string();
            vector.expected_property_holds = Some(true);
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
            let Some(second_state) = apply_action_transition(model, &first_state, second_action)
                .ok()
                .flatten()
            else {
                continue;
            };
            if !state_satisfies_property(model, property, &second_state) {
                continue;
            }
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
            let mut vector = vector;
            vector.strictness = "synthetic".to_string();
            vector.derivation = "synthetic_witness".to_string();
            vector.expected_property_holds = Some(true);
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
                path: model
                    .actions
                    .iter()
                    .find(|action| {
                        action.action_id == *action_id
                            && apply_action_transition(model, before, action)
                                .ok()
                                .flatten()
                                .map(|next| next == **after)
                                .unwrap_or(false)
                    })
                    .map(|action| action.decision_path()),
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
        evidence_kind: crate::evidence::EvidenceKind::Witness,
        assurance_level: crate::engine::AssuranceLevel::Complete,
        trace_hash: format!("trace:witness:{action_signature}"),
        steps,
    })
}

fn state_satisfies_property(
    model: &ModelIr,
    property: &crate::ir::PropertyIr,
    state: &MachineState,
) -> bool {
    matches!(
        eval_expr(model, state, &property.expr),
        Ok(Value::Bool(true))
    )
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
    out.push_str(&format!("// valid-run-id: {}\n", vector.run_id));
    if !vector.grouping.requirement_clusters.is_empty() {
        out.push_str(&format!(
            "// valid-requirement-clusters: {}\n",
            vector.grouping.requirement_clusters.join(",")
        ));
    }
    if !vector.grouping.risk_clusters.is_empty() {
        out.push_str(&format!(
            "// valid-risk-clusters: {}\n",
            vector.grouping.risk_clusters.join(",")
        ));
    }
    out.push_str("#[test]\n");
    out.push_str(&format!(
        "fn generated_{}() {{\n",
        vector.vector_id.replace('-', "_")
    ));
    out.push_str(&format!("    let vector_id = \"{}\";\n", vector.vector_id));
    out.push_str(&format!("    let run_id = \"{}\";\n", vector.run_id));
    out.push_str(&format!(
        "    let property_id = \"{}\";\n",
        vector.property_id
    ));
    out.push_str(&format!(
        "    let source_kind = \"{}\";\n",
        vector.source_kind
    ));
    out.push_str(&format!(
        "    let strictness = \"{}\";\n",
        vector.strictness
    ));
    out.push_str(&format!(
        "    let derivation = \"{}\";\n",
        vector.derivation
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
    if let Some(property_holds) = vector.expected_property_holds {
        out.push_str(&format!(
            "    let expected_property_holds = Some({property_holds});\n"
        ));
    } else {
        out.push_str("    let expected_property_holds: Option<bool> = None;\n");
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
    out.push_str("    let mut requirement_clusters = Vec::new();\n");
    for cluster in &vector.grouping.requirement_clusters {
        out.push_str(&format!("    requirement_clusters.push({cluster:?});\n"));
    }
    out.push_str("    let mut risk_clusters = Vec::new();\n");
    for cluster in &vector.grouping.risk_clusters {
        out.push_str(&format!("    risk_clusters.push({cluster:?});\n"));
    }
    out.push_str("    let mut expected_path_tags = Vec::new();\n");
    for tag in &vector.expected_path_tags {
        out.push_str(&format!("    expected_path_tags.push({tag:?});\n"));
    }
    out.push_str("    let mut expected_decision_ids = Vec::new();\n");
    for decision_id in vector.expected_path.decision_ids() {
        out.push_str(&format!(
            "    expected_decision_ids.push({decision_id:?});\n"
        ));
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
        out.push_str("    valid::testgen::assert_replay_output_json(&stdout, &actions, &expected_states, property_id, focus_action_id, expected_guard_enabled, expected_property_holds, &expected_path_tags, &expected_decision_ids);\n");
    }
    out.push_str("    assert!(!vector_id.is_empty());\n");
    out.push_str("    assert!(!run_id.is_empty());\n");
    out.push_str("    assert!(!property_id.is_empty());\n");
    out.push_str("    assert!(!source_kind.is_empty());\n");
    out.push_str("    assert!(!strictness.is_empty());\n");
    out.push_str("    assert!(!derivation.is_empty());\n");
    out.push_str("    assert!(!strategy.is_empty());\n");
    out.push_str("    if let Some(expected) = expected_property_holds {\n");
    out.push_str("        if strategy == \"counterexample\" {\n");
    out.push_str("            assert!(!expected, \"counterexample vector should expect property violation\");\n");
    out.push_str("        }\n");
    out.push_str("        if strategy == \"witness\" {\n");
    out.push_str(
        "            assert!(expected, \"witness vector should expect property to hold\");\n",
    );
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("    for tag in &expected_path_tags {\n");
    out.push_str("        assert!(!tag.is_empty(), \"path tag must be non-empty\");\n");
    out.push_str("    }\n");
    out.push_str("    for note in &notes {\n");
    out.push_str("        assert!(!note.is_empty(), \"note must be non-empty\");\n");
    out.push_str("    }\n");
    out.push_str("    for cluster in &requirement_clusters {\n");
    out.push_str(
        "        assert!(!cluster.is_empty(), \"requirement cluster must be non-empty\");\n",
    );
    out.push_str("    }\n");
    out.push_str("    for cluster in &risk_clusters {\n");
    out.push_str("        assert!(requirement_clusters.contains(cluster), \"risk cluster must also be a requirement cluster\");\n");
    out.push_str("    }\n");
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
    expected_property_holds: Option<bool>,
    expected_path_tags: &[&str],
    expected_decision_ids: &[&str],
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
    if let Some(property_holds) = expected_property_holds {
        assert!(
            normalized.contains(&format!("\"property_holds\":{property_holds}")),
            "replay property_holds mismatch: {normalized}"
        );
    }
    for tag in expected_path_tags {
        assert!(
            normalized.contains(&format!("\"{tag}\"")),
            "replay missing path tag `{tag}`: {normalized}"
        );
    }
    for decision_id in expected_decision_ids {
        assert!(
            normalized.contains(&format!("\"decision_id\":\"{decision_id}\"")),
            "replay missing decision `{decision_id}`: {normalized}"
        );
    }
}

pub fn render_test_vector_json(vector: &TestVector) -> Result<String, String> {
    serde_json::to_string(vector).map_err(|err| format!("failed to serialize test vector: {err}"))
}

pub fn parse_test_vector_json(body: &str) -> Result<TestVector, String> {
    serde_json::from_str(body).map_err(|err| format!("failed to parse test vector json: {err}"))
}

pub fn render_replay_json(
    property_id: &str,
    action_ids: &[String],
    terminal_state: &BTreeMap<String, Value>,
    focus_action_id: Option<&str>,
    focus_action_enabled: Option<bool>,
    property_holds: Option<bool>,
    path: &Path,
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
    let property_holds = property_holds
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string());
    let path_tags = path
        .legacy_path_tags()
        .iter()
        .map(|tag| format!("\"{tag}\""))
        .collect::<Vec<_>>()
        .join(",");
    let terminal_state = format!("{terminal_state:?}");
    format!(
        "{{\"schema_version\":\"1.0.0\",\"status\":\"ok\",\"property_id\":\"{}\",\"replayed_actions\":[{}],\"terminal_state\":{:?},\"focus_action_id\":{},\"focus_action_enabled\":{},\"property_holds\":{},\"path\":{},\"path_tags\":[{}]}}",
        property_id,
        actions,
        terminal_state,
        focus_action_id,
        focus_action_enabled,
        property_holds,
        render_path_json(path),
        path_tags
    )
}

pub fn replay_path_for_model(
    model: &ModelIr,
    action_ids: &[String],
    focus_action_id: Option<&str>,
    focus_action_enabled: Option<bool>,
) -> Path {
    let mut path = Path::default();
    for action_id in action_ids {
        if let Some(action) = model
            .actions
            .iter()
            .find(|action| action.action_id == *action_id)
        {
            path.extend(action.decision_path());
        }
    }
    if let Some(action_id) = focus_action_id {
        let focus_is_already_last = action_ids.last().map(String::as_str) == Some(action_id);
        if !focus_is_already_last {
            if let Some(action) = model
                .actions
                .iter()
                .find(|action| action.action_id == action_id)
            {
                let guard_enabled = focus_action_enabled.unwrap_or(false);
                path.extend(Path::new(
                    action
                        .decision_path_for_guard(guard_enabled)
                        .decisions
                        .into_iter()
                        .take(1)
                        .collect(),
                ));
            }
        }
    }
    path
}

pub fn generated_test_output_path(vector: &TestVector) -> String {
    generated_test_path(&vector.vector_id)
}

pub fn write_generated_test_files(vectors: &[TestVector]) -> Result<Vec<String>, String> {
    let mut generated_files = Vec::with_capacity(vectors.len());
    for vector in vectors {
        let vector_json = render_test_vector_json(vector)?;
        let _ = write_vector_artifact(&vector.run_id, &vector.vector_id, &vector_json)?;
        let rendered = render_rust_test(vector);
        let path = generated_test_output_path(vector);
        write_text_file(&path, &rendered)?;
        record_artifact(ArtifactRecord {
            artifact_kind: "generated_test".to_string(),
            path: path.clone(),
            run_id: vector.run_id.clone(),
            model_id: None,
            property_id: Some(vector.property_id.clone()),
            evidence_id: vector.evidence_id.clone(),
            vector_id: Some(vector.vector_id.clone()),
            suite_id: None,
        })?;
        generated_files.push(path);
    }
    Ok(generated_files)
}

#[derive(Debug, Clone)]
struct ModelNode {
    state: MachineState,
    parent: Option<usize>,
    via_action_index: Option<usize>,
    via_action_id: Option<String>,
    via_action_label: Option<String>,
}

#[derive(Debug, Clone)]
struct ModelEdge {
    action_index: usize,
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
        via_action_index: None,
        via_action_id: None,
        via_action_label: None,
    }];
    let mut edges_by_node = vec![Vec::new()];
    let mut frontier = VecDeque::from([0usize]);
    let mut visited = HashSet::from([initial]);

    while let Some(node_index) = frontier.pop_front() {
        let state = nodes[node_index].state.clone();
        for (action_index, action) in model.actions.iter().enumerate() {
            let next = apply_action_transition(model, &state, action)
                .map_err(|diagnostic| diagnostic.message.clone())?;
            let Some(next_state) = next else {
                continue;
            };
            let to_index = if visited.insert(next_state.clone()) {
                let index = nodes.len();
                nodes.push(ModelNode {
                    state: next_state,
                    parent: Some(node_index),
                    via_action_index: Some(action_index),
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
                action_index,
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

    for (action_index, action) in model.actions.iter().enumerate() {
        if let Some((node_index, edge)) =
            exploration
                .edges_by_node
                .iter()
                .enumerate()
                .find_map(|(node_index, edges)| {
                    edges
                        .iter()
                        .find(|edge| edge.action_index == action_index)
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
                action.decision_path(),
                {
                    let mut notes = vec![format!("guard_true:{:?}", action.guard)];
                    notes.extend(
                        action
                            .decision_path()
                            .legacy_path_tags()
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
            apply_action_transition(model, &node.state, action)
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
                action.decision_path_for_guard(false),
                {
                    let mut notes = vec![format!("guard_false:{:?}", action.guard)];
                    notes.extend(
                        action
                            .decision_path_for_guard(false)
                            .legacy_path_tags()
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

    for (action_index, action) in model.actions.iter().enumerate() {
        let action_path = action.decision_path();
        let tags = action_path.legacy_path_tags();
        let Some((_, edge)) =
            exploration
                .edges_by_node
                .iter()
                .enumerate()
                .find_map(|(node_index, edges)| {
                    edges
                        .iter()
                        .find(|edge| edge.action_index == action_index)
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
                Path::from_legacy_tags(vec![tag.clone()]),
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
            crate::ir::FieldType::BoundedU32 { min, max } => (min as u64, max as u64),
            crate::ir::FieldType::Bool => continue,
            crate::ir::FieldType::String { .. } => continue,
            crate::ir::FieldType::Enum { .. }
            | crate::ir::FieldType::EnumSet { .. }
            | crate::ir::FieldType::EnumRelation { .. }
            | crate::ir::FieldType::EnumMap { .. } => continue,
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
                    Path::default(),
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
                Path::default(),
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
    expected_path: Path,
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
    let expected_observations = if steps.is_empty() {
        vec![nodes.get(end_index)?.state.as_named_map(model)]
    } else {
        steps
            .iter()
            .map(|step| step.state_after.clone())
            .collect::<Vec<_>>()
    };
    let signature = actions
        .iter()
        .map(|step| step.action_id.clone())
        .collect::<Vec<_>>()
        .join(",");
    let (strictness, derivation) = vector_provenance(source_kind, strategy);
    let property = model
        .properties
        .iter()
        .find(|property| property.property_id == property_id)?;
    let mut expected_path = expected_path;
    if expected_path.decisions.is_empty() {
        for step in &steps {
            expected_path.extend(step.path.clone());
        }
    }
    let property_holds = matches!(
        eval_expr(model, &nodes.get(end_index)?.state, &property.expr).ok(),
        Some(Value::Bool(true))
    );
    let setup_action_ids = actions
        .iter()
        .filter_map(|step| {
            model
                .actions
                .iter()
                .find(|action| action.action_id == step.action_id)
                .and_then(|action| {
                    (action.role == crate::ir::action::ActionRole::Setup)
                        .then(|| step.action_id.clone())
                })
        })
        .collect::<Vec<_>>();
    let business_action_ids = actions
        .iter()
        .filter_map(|step| {
            model
                .actions
                .iter()
                .find(|action| action.action_id == step.action_id)
                .and_then(|action| {
                    (action.role == crate::ir::action::ActionRole::Business)
                        .then(|| step.action_id.clone())
                })
        })
        .collect::<Vec<_>>();
    let expected_path_tags = path_tags_or_empty(&expected_path);
    let grouping = infer_vector_grouping(&expected_path_tags);
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
        run_id: format!(
            "run-vector-{}",
            stable_hash_hex(&(model.model_id.clone() + property_id + &signature))
                .replace("sha256:", "")
        ),
        source_kind: source_kind.to_string(),
        strictness: strictness.to_string(),
        derivation: derivation.to_string(),
        evidence_id: None,
        strategy: strategy.to_string(),
        generator_version: env!("CARGO_PKG_VERSION").to_string(),
        seed: None,
        actions,
        initial_state: Some(nodes.first()?.state.as_named_map(model)),
        expected_observations,
        expected_states,
        property_id: property_id.to_string(),
        minimized: false,
        focus_action_id,
        focus_field,
        expected_guard_enabled,
        expected_property_holds: Some(property_holds),
        expected_path_tags,
        expected_path,
        setup_action_ids,
        business_action_ids,
        notes,
        grouping,
        replay_target: None,
    })
}

struct ModelPathStep {
    action_id: String,
    action_label: String,
    state_after: BTreeMap<String, Value>,
    path: Path,
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
            let action = after
                .via_action_index
                .and_then(|action_index| model.actions.get(action_index))
                .expect("non-root node must have an action index");
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
                path: action.decision_path(),
            }
        })
        .collect()
}

fn trace_path(trace: &EvidenceTrace) -> Path {
    let mut path = Path::default();
    for step in &trace.steps {
        if let Some(step_path) = &step.path {
            path.extend(step_path.clone());
        }
    }
    if path.decisions.is_empty() {
        let fallback = trace
            .steps
            .iter()
            .filter_map(|step| step.note.as_ref())
            .filter_map(|note| note.strip_prefix("path_tag:"))
            .map(str::to_string)
            .collect::<Vec<_>>();
        if !fallback.is_empty() {
            return Path::from_legacy_tags(fallback);
        }
    }
    path
}

fn path_tags_or_empty(path: &Path) -> Vec<String> {
    if path.decisions.is_empty() {
        Vec::new()
    } else {
        path.legacy_path_tags()
    }
}

fn render_path_json(path: &Path) -> String {
    let mut out = String::from("{\"decisions\":[");
    for (index, decision) in path.decisions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str(&format!("\"decision_id\":\"{}\"", decision.decision_id()));
        out.push_str(&format!(",\"action_id\":\"{}\"", decision.point.action_id));
        out.push_str(&format!(
            ",\"kind\":\"{}\"",
            match decision.point.kind {
                DecisionKind::Guard => "guard",
                DecisionKind::StateUpdate => "state_update",
            }
        ));
        out.push_str(&format!(",\"label\":{:?}", decision.point.label));
        if let Some(field) = &decision.point.field {
            out.push_str(&format!(",\"field\":{:?}", field));
        } else {
            out.push_str(",\"field\":null");
        }
        out.push_str(&format!(
            ",\"reads\":[{}]",
            decision
                .point
                .reads
                .iter()
                .map(|value| format!("{value:?}"))
                .collect::<Vec<_>>()
                .join(",")
        ));
        out.push_str(&format!(
            ",\"writes\":[{}]",
            decision
                .point
                .writes
                .iter()
                .map(|value| format!("{value:?}"))
                .collect::<Vec<_>>()
                .join(",")
        ));
        out.push_str(&format!(
            ",\"path_tags\":[{}]",
            decision
                .point
                .path_tags
                .iter()
                .map(|value| format!("{value:?}"))
                .collect::<Vec<_>>()
                .join(",")
        ));
        out.push_str(&format!(
            ",\"outcome\":\"{}\"",
            match decision.outcome {
                DecisionOutcome::GuardTrue => "guard_true",
                DecisionOutcome::GuardFalse => "guard_false",
                DecisionOutcome::UpdateApplied => "update_applied",
            }
        ));
        out.push('}');
    }
    out.push_str("]}");
    out
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
                    path: None,
                    note: None,
                }
            })
            .collect();
        EvidenceTrace {
            schema_version: "1.0.0".to_string(),
            evidence_id: format!("ev-{}", actions.join("-")),
            run_id: "run-1".to_string(),
            property_id: "SAFE".to_string(),
            evidence_kind: EvidenceKind::Witness,
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
                    role: crate::ir::action::ActionRole::Business,
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
                layer: crate::ir::PropertyLayer::Assert,
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

    #[test]
    fn builds_vector_from_trace() {
        let trace = EvidenceTrace {
            schema_version: "1.0.0".to_string(),
            evidence_id: "ev-000001".to_string(),
            run_id: "run-1".to_string(),
            property_id: "P_SAFE".to_string(),
            evidence_kind: EvidenceKind::Counterexample,
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
                path: None,
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
        assert_eq!(vectors.len(), 1);
        assert!(vectors.iter().all(|vector| vector.source_kind == "witness"));
        assert!(vectors
            .iter()
            .all(|vector| vector.expected_property_holds == Some(true)));
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
