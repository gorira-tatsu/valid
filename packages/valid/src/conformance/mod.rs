use std::{
    collections::BTreeMap,
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};

use crate::{
    api::{ExplainCandidateCause, ExplainFieldDiff, ExplainRepairTargetHint, TracebackSummary},
    ir::{ModelIr, Path, PropertyKind, Value},
    kernel::{
        eval::eval_expr,
        transition::{apply_action_transition, build_initial_state},
    },
    testgen::{
        ImplementationHints, ObservationContract, RequiredInput, SetupContract, TestVector,
        VectorActionStep, VectorGrouping,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Request payload for comparing a generated test vector with an implementation
/// surface.
pub struct ConformanceRequest {
    pub schema_version: String,
    pub vector: TestVector,
    pub actions: Vec<String>,
    pub initial_state: Option<BTreeMap<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Raw implementation response consumed by the conformance comparer.
pub struct ConformanceResponse {
    pub schema_version: String,
    pub status: String,
    #[serde(default)]
    pub observations: Vec<BTreeMap<String, Value>>,
    #[serde(default)]
    pub side_effects: Vec<BTreeMap<String, Value>>,
    #[serde(default)]
    pub property_holds: Option<bool>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// One observation-level mismatch between a generated vector and the SUT.
pub struct ObservationMismatch {
    pub index: usize,
    pub expected: BTreeMap<String, Value>,
    pub actual: Option<BTreeMap<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Normalized mismatch categories returned by conformance flows.
pub enum ConformanceMismatchKind {
    State,
    Property,
    Output,
    HarnessRuntime,
}

impl ConformanceMismatchKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::State => "state",
            Self::Property => "property",
            Self::Output => "output",
            Self::HarnessRuntime => "harness_runtime",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Structured mismatch item with fix-surface guidance.
pub struct ConformanceMismatch {
    pub kind: ConformanceMismatchKind,
    pub likely_fix_surface: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<BTreeMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<BTreeMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_property_holds: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_property_holds: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Reviewer-oriented summary of the conformance run.
pub struct ConformanceReviewSummary {
    pub headline: String,
    pub trace_steps: usize,
    pub failing_action_id: Option<String>,
    pub action_sequence: Vec<String>,
    pub next_steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Top-level structured conformance report returned by CLI, MCP, and library
/// helpers.
pub struct ConformanceReport {
    pub schema_version: String,
    pub vector_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_id: Option<String>,
    pub status: String,
    pub mismatch_count: usize,
    pub mismatch_categories: Vec<String>,
    pub mismatches: Vec<ConformanceMismatch>,
    pub observation_mismatches: Vec<ObservationMismatch>,
    pub expected_property_holds: Option<bool>,
    pub actual_property_holds: Option<bool>,
    pub runner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traceback: Option<TracebackSummary>,
    #[serde(default)]
    pub candidate_causes: Vec<ExplainCandidateCause>,
    #[serde(default)]
    pub repair_targets: Vec<ExplainRepairTargetHint>,
    #[serde(default)]
    pub next_steps: Vec<String>,
    pub review_summary: ConformanceReviewSummary,
}

/// Trait-based integration point for Rust implementations that can be driven
/// directly from generated test vectors.
pub trait RustConformanceHarness {
    fn harness_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    fn apply_action(&mut self, step: &VectorActionStep) -> Result<BTreeMap<String, Value>, String>;

    fn property_holds(&self, _property_id: &str) -> Result<Option<bool>, String> {
        Ok(None)
    }
}

/// Build a conformance-oriented test vector from a model and an explicit action
/// sequence.
pub fn build_vector_from_actions(
    model: &ModelIr,
    property_id: Option<&str>,
    action_ids: &[String],
) -> Result<TestVector, String> {
    let initial = build_initial_state(model).map_err(|err| err.message)?;
    let mut state = initial.clone();
    let mut expected_observations = Vec::new();
    let mut expected_states = Vec::new();
    let mut actions = Vec::new();

    for (index, action_id) in action_ids.iter().enumerate() {
        let action = model
            .actions
            .iter()
            .find(|candidate| candidate.action_id == *action_id)
            .ok_or_else(|| format!("unknown action `{action_id}`"))?;
        let next = apply_action_transition(model, &state, action)
            .map_err(|err| err.message)?
            .ok_or_else(|| {
                format!("action `{action_id}` was not enabled during conformance replay")
            })?;
        let observation = next.as_named_map(model);
        expected_states.push(format!("{observation:?}"));
        expected_observations.push(observation);
        actions.push(crate::testgen::VectorActionStep {
            index,
            action_id: action.action_id.clone(),
            action_label: action.label.clone(),
        });
        state = next;
    }

    let expected_property_holds = property_id.and_then(|target_property_id| {
        let property = model
            .properties
            .iter()
            .find(|candidate| candidate.property_id == target_property_id)?;
        match property.kind {
            PropertyKind::Invariant | PropertyKind::Reachability | PropertyKind::Cover => {
                match eval_expr(model, &state, &property.expr).ok() {
                    Some(Value::Bool(value)) => Some(value),
                    _ => None,
                }
            }
            PropertyKind::DeadlockFreedom | PropertyKind::Temporal | PropertyKind::Transition => {
                None
            }
        }
    });

    let business_action_ids = actions.iter().map(|step| step.action_id.clone()).collect();
    let mut vector = TestVector {
        schema_version: "1.0.0".to_string(),
        vector_id: format!("vec-conformance-{}", model.model_id),
        run_id: format!("run-conformance-{}", model.model_id),
        source_kind: "spec_conformance".to_string(),
        strictness: "strict".to_string(),
        derivation: "spec_replay".to_string(),
        evidence_id: None,
        strategy: "conformance".to_string(),
        generator_version: env!("CARGO_PKG_VERSION").to_string(),
        seed: None,
        actions,
        initial_state: Some(initial.as_named_map(model)),
        expected_observations,
        expected_states,
        property_id: property_id.unwrap_or("").to_string(),
        minimized: false,
        focus_action_id: None,
        focus_field: None,
        expected_guard_enabled: None,
        expected_property_holds,
        expected_path: Path::default(),
        expected_path_tags: Vec::new(),
        setup_action_ids: Vec::new(),
        business_action_ids,
        notes: vec!["generated from spec replay for implementation conformance".to_string()],
        grouping: VectorGrouping::default(),
        observation_contract: ObservationContract::default(),
        observation_layers: Vec::new(),
        oracle_targets: Vec::new(),
        required_inputs: Vec::<RequiredInput>::new(),
        setup_contract: SetupContract::default(),
        implementation_hints: ImplementationHints::default(),
        replay_target: None,
    };
    vector.normalize_language_agnostic_contract();
    Ok(vector)
}

/// Run conformance against an external JSON-speaking runner process.
pub fn run_conformance(
    vector: &TestVector,
    runner: &str,
    runner_args: &[String],
) -> Result<ConformanceReport, String> {
    let request = ConformanceRequest {
        schema_version: "1.0.0".to_string(),
        vector: vector.clone(),
        actions: vector
            .actions
            .iter()
            .map(|step| step.action_id.clone())
            .collect(),
        initial_state: vector.initial_state.clone(),
    };
    let input = serde_json::to_vec(&request)
        .map_err(|err| format!("failed to serialize conformance request: {err}"))?;
    let mut child = Command::new(runner)
        .args(runner_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to execute conformance runner `{runner}`: {err}"))?;
    {
        let Some(mut stdin) = child.stdin.take() else {
            return Err("failed to open conformance runner stdin".to_string());
        };
        use std::io::Write;
        stdin
            .write_all(&input)
            .map_err(|err| format!("failed to write conformance request: {err}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("failed to wait for conformance runner: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("conformance runner failed: {stderr}"));
    }
    let body = String::from_utf8(output.stdout)
        .map_err(|err| format!("conformance runner output must be utf-8: {err}"))?;
    let response: ConformanceResponse = serde_json::from_str(&body)
        .map_err(|err| format!("failed to parse conformance response: {err}"))?;
    if response.status != "ok" {
        return Err(response
            .message
            .unwrap_or_else(|| "conformance runner returned non-ok status".to_string()));
    }
    Ok(compare_conformance(vector, runner, &response))
}

/// Run conformance directly against an in-process Rust harness.
pub fn run_rust_conformance<H: RustConformanceHarness>(
    vector: &TestVector,
    harness: &mut H,
) -> ConformanceReport {
    let mut observations = Vec::new();
    for step in &vector.actions {
        match harness.apply_action(step) {
            Ok(observation) => observations.push(observation),
            Err(message) => {
                return harness_runtime_report(
                    vector,
                    harness.harness_name(),
                    format!(
                        "rust conformance harness failed at step {} (`{}`): {}",
                        step.index, step.action_id, message
                    ),
                );
            }
        }
    }
    let property_holds = if vector.property_id.is_empty() {
        None
    } else {
        match harness.property_holds(&vector.property_id) {
            Ok(value) => value,
            Err(message) => {
                return harness_runtime_report(
                    vector,
                    harness.harness_name(),
                    format!(
                        "rust conformance harness failed while checking property `{}`: {}",
                        vector.property_id, message
                    ),
                );
            }
        }
    };
    compare_conformance(
        vector,
        harness.harness_name(),
        &ConformanceResponse {
            schema_version: "1.0.0".to_string(),
            status: "ok".to_string(),
            observations,
            side_effects: Vec::new(),
            property_holds,
            message: None,
        },
    )
}

/// Compare a generated vector with an implementation response and classify the
/// resulting mismatches.
pub fn compare_conformance(
    vector: &TestVector,
    runner: &str,
    response: &ConformanceResponse,
) -> ConformanceReport {
    let mut observation_mismatches = Vec::new();
    let mut mismatches = Vec::new();
    for (index, expected) in vector.expected_observations.iter().enumerate() {
        let actual = response.observations.get(index).cloned();
        if actual.as_ref() != Some(expected) {
            let (kind, likely_fix_surface, summary) = match actual.as_ref() {
                Some(_) => (
                    ConformanceMismatchKind::State,
                    "implementation_state".to_string(),
                    format!("state observation mismatch at step {index}"),
                ),
                None => (
                    ConformanceMismatchKind::Output,
                    "implementation_output".to_string(),
                    format!("missing observation output at step {index}"),
                ),
            };
            observation_mismatches.push(ObservationMismatch {
                index,
                expected: expected.clone(),
                actual: actual.clone(),
            });
            mismatches.push(ConformanceMismatch {
                kind,
                likely_fix_surface,
                summary,
                index: Some(index),
                expected: Some(expected.clone()),
                actual,
                expected_property_holds: None,
                actual_property_holds: None,
            });
        }
    }
    for index in vector.expected_observations.len()..response.observations.len() {
        let actual = response.observations.get(index).cloned();
        observation_mismatches.push(ObservationMismatch {
            index,
            expected: BTreeMap::new(),
            actual: actual.clone(),
        });
        mismatches.push(ConformanceMismatch {
            kind: ConformanceMismatchKind::Output,
            likely_fix_surface: "implementation_output".to_string(),
            summary: format!("unexpected observation output at step {index}"),
            index: Some(index),
            expected: Some(BTreeMap::new()),
            actual,
            expected_property_holds: None,
            actual_property_holds: None,
        });
    }
    let property_mismatch = vector.expected_property_holds != response.property_holds;
    if property_mismatch {
        mismatches.push(ConformanceMismatch {
            kind: ConformanceMismatchKind::Property,
            likely_fix_surface: "implementation_or_model".to_string(),
            summary: "property result mismatch".to_string(),
            index: None,
            expected: None,
            actual: None,
            expected_property_holds: vector.expected_property_holds,
            actual_property_holds: response.property_holds,
        });
    }
    let mismatch_categories = {
        let mut categories = Vec::new();
        for mismatch in &mismatches {
            let category = mismatch.kind.as_str().to_string();
            if !categories.contains(&category) {
                categories.push(category);
            }
        }
        categories
    };
    let mismatch_count = mismatches.len();
    let traceback = mismatches
        .first()
        .map(|mismatch| build_traceback(vector, mismatch));
    let candidate_causes = mismatches
        .first()
        .map(|mismatch| build_candidate_causes(mismatch, traceback.as_ref()))
        .unwrap_or_default();
    let repair_targets = mismatches
        .first()
        .map(|mismatch| build_repair_targets(mismatch, traceback.as_ref()))
        .unwrap_or_default();
    let next_steps = build_next_steps(vector, mismatches.first(), &traceback);
    ConformanceReport {
        schema_version: "1.0.0".to_string(),
        vector_id: vector.vector_id.clone(),
        evidence_id: vector.evidence_id.clone(),
        property_id: property_id(vector),
        status: if mismatch_count == 0 { "PASS" } else { "FAIL" }.to_string(),
        mismatch_count,
        mismatch_categories,
        mismatches,
        observation_mismatches,
        expected_property_holds: vector.expected_property_holds,
        actual_property_holds: response.property_holds,
        runner: runner.to_string(),
        traceback: traceback.clone(),
        candidate_causes,
        repair_targets,
        next_steps: next_steps.clone(),
        review_summary: build_review_summary(
            vector,
            runner,
            mismatch_count,
            &traceback,
            next_steps,
        ),
    }
}

fn harness_runtime_report(vector: &TestVector, runner: &str, summary: String) -> ConformanceReport {
    let mismatch = ConformanceMismatch {
        kind: ConformanceMismatchKind::HarnessRuntime,
        likely_fix_surface: "conformance_harness".to_string(),
        summary,
        index: None,
        expected: None,
        actual: None,
        expected_property_holds: vector.expected_property_holds,
        actual_property_holds: None,
    };
    let traceback = Some(build_traceback(vector, &mismatch));
    let candidate_causes = build_candidate_causes(&mismatch, traceback.as_ref());
    let repair_targets = build_repair_targets(&mismatch, traceback.as_ref());
    let next_steps = build_next_steps(vector, Some(&mismatch), &traceback);
    ConformanceReport {
        schema_version: "1.0.0".to_string(),
        vector_id: vector.vector_id.clone(),
        evidence_id: vector.evidence_id.clone(),
        property_id: property_id(vector),
        status: "FAIL".to_string(),
        mismatch_count: 1,
        mismatch_categories: vec![ConformanceMismatchKind::HarnessRuntime.as_str().to_string()],
        mismatches: vec![mismatch],
        observation_mismatches: Vec::new(),
        expected_property_holds: vector.expected_property_holds,
        actual_property_holds: None,
        runner: runner.to_string(),
        traceback: traceback.clone(),
        candidate_causes,
        repair_targets,
        next_steps: next_steps.clone(),
        review_summary: build_review_summary(vector, runner, 1, &traceback, next_steps),
    }
}

fn property_id(vector: &TestVector) -> Option<String> {
    (!vector.property_id.is_empty()).then(|| vector.property_id.clone())
}

fn build_traceback(vector: &TestVector, mismatch: &ConformanceMismatch) -> TracebackSummary {
    let failure_step_index = mismatch
        .index
        .unwrap_or_else(|| vector.actions.len().saturating_sub(1));
    let failing_action_id = mismatch
        .index
        .and_then(|index| vector.actions.get(index))
        .map(|step| step.action_id.clone());
    let (changed_fields, field_diffs, involved_fields) = diff_fields(mismatch);
    TracebackSummary {
        breakpoint_kind: match mismatch.kind {
            ConformanceMismatchKind::State | ConformanceMismatchKind::Output => {
                "action_boundary".to_string()
            }
            ConformanceMismatchKind::Property => "terminal_boundary".to_string(),
            ConformanceMismatchKind::HarnessRuntime => "runner_boundary".to_string(),
        },
        breakpoint_note: Some(mismatch.summary.clone()),
        failure_step_index,
        failing_action_id,
        changed_fields,
        field_diffs,
        involved_fields,
    }
}

fn diff_fields(
    mismatch: &ConformanceMismatch,
) -> (Vec<String>, Vec<ExplainFieldDiff>, Vec<String>) {
    let mut involved_fields = Vec::new();
    let mut changed_fields = Vec::new();
    let mut field_diffs = Vec::new();
    if let Some(expected) = &mismatch.expected {
        for field in expected.keys() {
            push_unique(&mut involved_fields, field.clone());
        }
    }
    if let Some(actual) = &mismatch.actual {
        for field in actual.keys() {
            push_unique(&mut involved_fields, field.clone());
        }
    }
    if let (Some(expected), Some(actual)) = (&mismatch.expected, &mismatch.actual) {
        for field in &involved_fields {
            if expected.get(field) != actual.get(field) {
                changed_fields.push(field.clone());
                if let (Some(before), Some(after)) = (expected.get(field), actual.get(field)) {
                    field_diffs.push(ExplainFieldDiff {
                        field: field.clone(),
                        before: before.clone(),
                        after: after.clone(),
                    });
                }
            }
        }
    }
    (changed_fields, field_diffs, involved_fields)
}

fn build_candidate_causes(
    mismatch: &ConformanceMismatch,
    traceback: Option<&TracebackSummary>,
) -> Vec<ExplainCandidateCause> {
    match mismatch.kind {
        ConformanceMismatchKind::State => {
            let Some(traceback) = traceback else {
                return vec![ExplainCandidateCause {
                    kind: "state_mismatch".to_string(),
                    message: mismatch.summary.clone(),
                }];
            };
            if traceback.changed_fields.is_empty() {
                vec![ExplainCandidateCause {
                    kind: "state_mismatch".to_string(),
                    message: mismatch.summary.clone(),
                }]
            } else {
                traceback
                    .changed_fields
                    .iter()
                    .map(|field| ExplainCandidateCause {
                        kind: "state_mismatch".to_string(),
                        message: format!(
                            "state diverged on field `{field}` during conformance replay"
                        ),
                    })
                    .collect()
            }
        }
        ConformanceMismatchKind::Property => vec![ExplainCandidateCause {
            kind: "property_result_mismatch".to_string(),
            message: "the implementation and model disagreed on the property result".to_string(),
        }],
        ConformanceMismatchKind::Output => vec![ExplainCandidateCause {
            kind: "observation_mismatch".to_string(),
            message: mismatch.summary.clone(),
        }],
        ConformanceMismatchKind::HarnessRuntime => vec![ExplainCandidateCause {
            kind: "harness_runtime".to_string(),
            message: mismatch.summary.clone(),
        }],
    }
}

fn build_repair_targets(
    mismatch: &ConformanceMismatch,
    traceback: Option<&TracebackSummary>,
) -> Vec<ExplainRepairTargetHint> {
    vec![ExplainRepairTargetHint {
        target: mismatch.likely_fix_surface.clone(),
        reason: mismatch.summary.clone(),
        priority: match mismatch.kind {
            ConformanceMismatchKind::HarnessRuntime | ConformanceMismatchKind::State => {
                "high".to_string()
            }
            ConformanceMismatchKind::Property | ConformanceMismatchKind::Output => {
                "medium".to_string()
            }
        },
        action_id: traceback.and_then(|item| item.failing_action_id.clone()),
        fields: traceback
            .map(|item| item.involved_fields.clone())
            .unwrap_or_default(),
    }]
}

fn build_next_steps(
    vector: &TestVector,
    mismatch: Option<&ConformanceMismatch>,
    traceback: &Option<TracebackSummary>,
) -> Vec<String> {
    let mut steps = Vec::new();
    match mismatch {
        Some(ConformanceMismatch {
            kind: ConformanceMismatchKind::HarnessRuntime,
            ..
        }) => steps.push(
            "fix the conformance harness boundary before classifying requirement, model, or implementation drift"
                .to_string(),
        ),
        Some(_) => {
            if let Some(property_id) = property_id(vector) {
                steps.push(format!(
                    "run `cargo valid explain <model> --property={property_id}` to compare the model-side traceback"
                ));
            }
            if let Some(traceback) = traceback {
                steps.push(format!(
                    "rerun vector `{}` and inspect step {} in the implementation trace",
                    vector.vector_id, traceback.failure_step_index
                ));
            }
        }
        None => steps.push(
            "record this vector in CI so future conformance drift is caught on the same contract"
                .to_string(),
        ),
    }
    steps
}

fn build_review_summary(
    vector: &TestVector,
    runner: &str,
    mismatch_count: usize,
    traceback: &Option<TracebackSummary>,
    next_steps: Vec<String>,
) -> ConformanceReviewSummary {
    ConformanceReviewSummary {
        headline: if mismatch_count == 0 {
            format!("PASS conformance for {} via {}", vector.vector_id, runner)
        } else if let Some(property_id) = property_id(vector) {
            format!(
                "FAIL conformance for property {} via {}",
                property_id, runner
            )
        } else {
            format!("FAIL conformance for {} via {}", vector.vector_id, runner)
        },
        trace_steps: vector.actions.len(),
        failing_action_id: traceback
            .as_ref()
            .and_then(|item| item.failing_action_id.clone()),
        action_sequence: vector
            .actions
            .iter()
            .map(|step| step.action_id.clone())
            .collect(),
        next_steps,
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

/// Render a [`ConformanceReport`] to JSON for CLI or artifact output.
pub fn render_conformance_report_json(report: &ConformanceReport) -> Result<String, String> {
    serde_json::to_string(report)
        .map_err(|err| format!("failed to serialize conformance report: {err}"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        frontend::compile_model,
        ir::{Path, Value},
        testgen::{
            ImplementationHints, ObservationContract, SetupContract, TestVector, VectorActionStep,
            VectorGrouping,
        },
    };

    use super::{
        compare_conformance, run_rust_conformance, ConformanceMismatchKind, ConformanceResponse,
        RustConformanceHarness,
    };

    fn sample_vector() -> TestVector {
        TestVector {
            schema_version: "1.0.0".to_string(),
            vector_id: "vec-1".to_string(),
            run_id: "run-1".to_string(),
            source_kind: "counterexample".to_string(),
            strictness: "strict".to_string(),
            derivation: "counterexample_trace".to_string(),
            evidence_id: None,
            strategy: "counterexample".to_string(),
            generator_version: "0.1.0".to_string(),
            seed: None,
            actions: vec![VectorActionStep {
                index: 0,
                action_id: "Jump".to_string(),
                action_label: "Jump".to_string(),
            }],
            initial_state: Some(BTreeMap::from([("x".to_string(), Value::UInt(0))])),
            expected_observations: vec![BTreeMap::from([("x".to_string(), Value::UInt(2))])],
            expected_states: vec![r#"{"x":2}"#.to_string()],
            property_id: "P_SAFE".to_string(),
            minimized: false,
            focus_action_id: None,
            focus_field: None,
            expected_guard_enabled: None,
            expected_property_holds: Some(false),
            expected_path: Path::default(),
            expected_path_tags: Vec::new(),
            setup_action_ids: Vec::new(),
            business_action_ids: vec!["Jump".to_string()],
            notes: Vec::new(),
            grouping: VectorGrouping::default(),
            observation_contract: ObservationContract::default(),
            observation_layers: Vec::new(),
            oracle_targets: Vec::new(),
            required_inputs: Vec::new(),
            setup_contract: SetupContract::default(),
            implementation_hints: ImplementationHints::default(),
            replay_target: None,
        }
    }

    #[test]
    fn compare_conformance_passes_for_matching_observations() {
        let report = compare_conformance(
            &sample_vector(),
            "fixture-runner",
            &ConformanceResponse {
                schema_version: "1.0.0".to_string(),
                status: "ok".to_string(),
                observations: vec![BTreeMap::from([("x".to_string(), Value::UInt(2))])],
                side_effects: vec![],
                property_holds: Some(false),
                message: None,
            },
        );
        assert_eq!(report.status, "PASS");
        assert_eq!(report.mismatch_count, 0);
        assert!(report.mismatch_categories.is_empty());
        assert!(report.mismatches.is_empty());
    }

    #[test]
    fn compare_conformance_reports_state_and_property_mismatches() {
        let report = compare_conformance(
            &sample_vector(),
            "fixture-runner",
            &ConformanceResponse {
                schema_version: "1.0.0".to_string(),
                status: "ok".to_string(),
                observations: vec![BTreeMap::from([("x".to_string(), Value::UInt(1))])],
                side_effects: vec![],
                property_holds: Some(true),
                message: None,
            },
        );
        assert_eq!(report.status, "FAIL");
        assert_eq!(report.mismatch_count, 2);
        assert_eq!(report.observation_mismatches.len(), 1);
        assert_eq!(
            report.mismatch_categories,
            vec!["state".to_string(), "property".to_string()]
        );
        assert_eq!(report.mismatches[0].kind, ConformanceMismatchKind::State);
        assert_eq!(report.mismatches[1].kind, ConformanceMismatchKind::Property);
    }

    #[test]
    fn compare_conformance_classifies_missing_and_extra_outputs() {
        let missing_output = compare_conformance(
            &sample_vector(),
            "fixture-runner",
            &ConformanceResponse {
                schema_version: "1.0.0".to_string(),
                status: "ok".to_string(),
                observations: vec![],
                side_effects: vec![],
                property_holds: Some(false),
                message: None,
            },
        );
        assert_eq!(
            missing_output.mismatch_categories,
            vec!["output".to_string()]
        );
        assert_eq!(
            missing_output.mismatches[0].kind,
            ConformanceMismatchKind::Output
        );

        let extra_output = compare_conformance(
            &sample_vector(),
            "fixture-runner",
            &ConformanceResponse {
                schema_version: "1.0.0".to_string(),
                status: "ok".to_string(),
                observations: vec![
                    BTreeMap::from([("x".to_string(), Value::UInt(2))]),
                    BTreeMap::from([("x".to_string(), Value::UInt(3))]),
                ],
                side_effects: vec![],
                property_holds: Some(false),
                message: None,
            },
        );
        assert_eq!(extra_output.mismatch_categories, vec!["output".to_string()]);
        assert_eq!(
            extra_output.mismatches[0].kind,
            ConformanceMismatchKind::Output
        );
    }

    struct CounterHarness {
        x: u64,
    }

    impl RustConformanceHarness for CounterHarness {
        fn harness_name(&self) -> &'static str {
            "counter-harness"
        }

        fn apply_action(
            &mut self,
            step: &VectorActionStep,
        ) -> Result<BTreeMap<String, Value>, String> {
            match step.action_id.as_str() {
                "Inc" => {
                    self.x += 1;
                    Ok(BTreeMap::from([("x".to_string(), Value::UInt(self.x))]))
                }
                "Reset" => {
                    self.x = 0;
                    Ok(BTreeMap::from([("x".to_string(), Value::UInt(self.x))]))
                }
                other => Err(format!("unknown action `{other}`")),
            }
        }

        fn property_holds(&self, property_id: &str) -> Result<Option<bool>, String> {
            match property_id {
                "P_SAFE" => Ok(Some(self.x <= 2)),
                _ => Ok(None),
            }
        }
    }

    #[test]
    fn run_rust_conformance_executes_sample_sut_end_to_end() {
        let model = compile_model(include_str!(
            "../../../../tests/fixtures/models/safe_counter.valid"
        ))
        .expect("fixture should compile");
        let vector = super::build_vector_from_actions(&model, Some("P_SAFE"), &["Inc".to_string()])
            .expect("vector should build");
        let mut harness = CounterHarness { x: 0 };
        let report = run_rust_conformance(&vector, &mut harness);
        assert_eq!(report.status, "PASS");
        assert_eq!(report.runner, "counter-harness");
        assert!(report.mismatch_categories.is_empty());
    }

    #[test]
    fn run_rust_conformance_reports_runtime_harness_failures() {
        struct BrokenHarness;

        impl RustConformanceHarness for BrokenHarness {
            fn harness_name(&self) -> &'static str {
                "broken-harness"
            }

            fn apply_action(
                &mut self,
                step: &VectorActionStep,
            ) -> Result<BTreeMap<String, Value>, String> {
                Err(format!("refused action `{}`", step.action_id))
            }
        }

        let report = run_rust_conformance(&sample_vector(), &mut BrokenHarness);
        assert_eq!(report.status, "FAIL");
        assert_eq!(
            report.mismatch_categories,
            vec!["harness_runtime".to_string()]
        );
        assert_eq!(
            report.mismatches[0].kind,
            ConformanceMismatchKind::HarnessRuntime
        );
        assert!(report.mismatches[0]
            .summary
            .contains("rust conformance harness failed"));
    }
}
