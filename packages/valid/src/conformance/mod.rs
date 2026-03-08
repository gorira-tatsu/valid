use std::{
    collections::BTreeMap,
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};

use crate::{
    ir::{ModelIr, Path, PropertyKind, Value},
    kernel::{
        eval::eval_expr,
        transition::{apply_action_transition, build_initial_state},
    },
    testgen::TestVector,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceRequest {
    pub schema_version: String,
    pub vector: TestVector,
    pub actions: Vec<String>,
    pub initial_state: Option<BTreeMap<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConformanceResponse {
    pub schema_version: String,
    pub status: String,
    #[serde(default)]
    pub observations: Vec<BTreeMap<String, Value>>,
    #[serde(default)]
    pub property_holds: Option<bool>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationMismatch {
    pub index: usize,
    pub expected: BTreeMap<String, Value>,
    pub actual: Option<BTreeMap<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
pub struct ConformanceReport {
    pub schema_version: String,
    pub vector_id: String,
    pub status: String,
    pub mismatch_count: usize,
    pub mismatch_categories: Vec<String>,
    pub mismatches: Vec<ConformanceMismatch>,
    pub observation_mismatches: Vec<ObservationMismatch>,
    pub expected_property_holds: Option<bool>,
    pub actual_property_holds: Option<bool>,
    pub runner: String,
}

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
    Ok(TestVector {
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
        replay_target: None,
    })
}

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
    ConformanceReport {
        schema_version: "1.0.0".to_string(),
        vector_id: vector.vector_id.clone(),
        status: if mismatch_count == 0 { "PASS" } else { "FAIL" }.to_string(),
        mismatch_count,
        mismatch_categories,
        mismatches,
        observation_mismatches,
        expected_property_holds: vector.expected_property_holds,
        actual_property_holds: response.property_holds,
        runner: runner.to_string(),
    }
}

pub fn render_conformance_report_json(report: &ConformanceReport) -> Result<String, String> {
    serde_json::to_string(report)
        .map_err(|err| format!("failed to serialize conformance report: {err}"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        ir::{Path, Value},
        testgen::{TestVector, VectorActionStep},
    };

    use super::{compare_conformance, ConformanceMismatchKind, ConformanceResponse};

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
}
