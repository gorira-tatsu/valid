//! Solver capability descriptions and adapter traits.

use crate::{
    engine::{
        check_explicit, AssuranceLevel, BackendKind, CheckErrorEnvelope, CheckOutcome, ErrorStatus,
        ExplicitRunResult, PropertyResult, RunPlan, RunStatus, UnknownReason,
    },
    evidence::{EvidenceKind, EvidenceTrace, TraceStep},
    ir::ModelIr,
    kernel::replay::replay_actions,
    support::{
        diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode},
        hash::stable_hash_hex,
        schema::require_non_empty,
    },
};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityMatrix {
    pub backend_name: String,
    pub supports_explicit: bool,
    pub supports_bmc: bool,
    pub supports_certificate: bool,
    pub supports_trace: bool,
    pub supports_witness: bool,
    pub selfcheck_compatible: bool,
}

pub trait SolverAdapter {
    fn backend_kind(&self) -> BackendKind;
    fn capabilities(&self) -> CapabilityMatrix;
    fn build_plan(&self, model: &ModelIr, run_plan: &RunPlan) -> Result<SolverRunPlan, String>;
    fn run(&self, model: &ModelIr, plan: &SolverRunPlan) -> Result<RawSolverResult, String>;
    fn normalize(
        &self,
        model: &ModelIr,
        run_plan: &RunPlan,
        raw: RawSolverResult,
    ) -> Result<NormalizedRunResult, String>;
}

pub struct ExplicitAdapter;
pub struct MockBmcAdapter;
pub struct CommandSolverAdapter {
    pub backend_name: String,
    pub executable: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolverRunPlan {
    pub run_id: String,
    pub backend: BackendKind,
    pub target_property_ids: Vec<String>,
    pub horizon: Option<u32>,
    pub encoded_model_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawSolverResult {
    Explicit(CheckOutcome),
    Protocol(CommandProtocolResult),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandProtocolResult {
    pub status: String,
    pub actions: Vec<String>,
    pub raw_output: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedRunResult {
    pub outcome: CheckOutcome,
    pub trace: Option<EvidenceTrace>,
}

pub fn render_capability_matrix_json(matrix: &CapabilityMatrix) -> String {
    format!(
        "{{\"backend\":\"{}\",\"capabilities\":{{\"supports_explicit\":{},\"supports_bmc\":{},\"supports_certificate\":{},\"supports_trace\":{},\"supports_witness\":{},\"selfcheck_compatible\":{}}}}}",
        matrix.backend_name,
        matrix.supports_explicit,
        matrix.supports_bmc,
        matrix.supports_certificate,
        matrix.supports_trace,
        matrix.supports_witness,
        matrix.selfcheck_compatible
    )
}

pub fn validate_capability_matrix(matrix: &CapabilityMatrix) -> Result<(), String> {
    require_non_empty(&matrix.backend_name, "backend_name")
}

impl SolverAdapter for ExplicitAdapter {
    fn backend_kind(&self) -> BackendKind {
        BackendKind::Explicit
    }

    fn capabilities(&self) -> CapabilityMatrix {
        CapabilityMatrix {
            backend_name: "explicit".to_string(),
            supports_explicit: true,
            supports_bmc: false,
            supports_certificate: false,
            supports_trace: true,
            supports_witness: false,
            selfcheck_compatible: true,
        }
    }

    fn build_plan(&self, _model: &ModelIr, run_plan: &RunPlan) -> Result<SolverRunPlan, String> {
        let target_property_ids = match &run_plan.property_selection {
            crate::engine::PropertySelection::ExactlyOne(id) => vec![id.clone()],
        };
        Ok(SolverRunPlan {
            run_id: run_plan.manifest.run_id.clone(),
            backend: BackendKind::Explicit,
            target_property_ids,
            horizon: run_plan.search_bounds.max_depth.map(|value| value as u32),
            encoded_model_hash: format!("encoded:{}", run_plan.manifest.source_hash),
        })
    }

    fn run(&self, model: &ModelIr, plan: &SolverRunPlan) -> Result<RawSolverResult, String> {
        let mut run_plan = RunPlan::default();
        run_plan.manifest.run_id = plan.run_id.clone();
        if let Some(property_id) = plan.target_property_ids.first() {
            run_plan.property_selection =
                crate::engine::PropertySelection::ExactlyOne(property_id.clone());
        }
        run_plan.search_bounds.max_depth = plan.horizon.map(|value| value as usize);
        Ok(RawSolverResult::Explicit(check_explicit(model, &run_plan)))
    }

    fn normalize(
        &self,
        _model: &ModelIr,
        _run_plan: &RunPlan,
        raw: RawSolverResult,
    ) -> Result<NormalizedRunResult, String> {
        match raw {
            RawSolverResult::Explicit(outcome) => {
                let trace = match &outcome {
                    CheckOutcome::Completed(result) => result.trace.clone(),
                    CheckOutcome::Errored(_) => None,
                };
                Ok(NormalizedRunResult { outcome, trace })
            }
            RawSolverResult::Protocol(_) => {
                Err("explicit adapter cannot normalize protocol results".to_string())
            }
        }
    }
}

impl SolverAdapter for MockBmcAdapter {
    fn backend_kind(&self) -> BackendKind {
        BackendKind::MockBmc
    }

    fn capabilities(&self) -> CapabilityMatrix {
        CapabilityMatrix {
            backend_name: "mock-bmc".to_string(),
            supports_explicit: false,
            supports_bmc: true,
            supports_certificate: false,
            supports_trace: true,
            supports_witness: true,
            selfcheck_compatible: false,
        }
    }

    fn build_plan(&self, _model: &ModelIr, run_plan: &RunPlan) -> Result<SolverRunPlan, String> {
        let target_property_ids = match &run_plan.property_selection {
            crate::engine::PropertySelection::ExactlyOne(id) => vec![id.clone()],
        };
        Ok(SolverRunPlan {
            run_id: format!("{}-bmc", run_plan.manifest.run_id),
            backend: BackendKind::MockBmc,
            target_property_ids,
            horizon: run_plan
                .search_bounds
                .max_depth
                .map(|value| value as u32)
                .or(Some(8)),
            encoded_model_hash: format!("bmc:{}", run_plan.manifest.source_hash),
        })
    }

    fn run(&self, model: &ModelIr, plan: &SolverRunPlan) -> Result<RawSolverResult, String> {
        let mut run_plan = RunPlan::default();
        run_plan.manifest.run_id = plan.run_id.clone();
        if let Some(property_id) = plan.target_property_ids.first() {
            run_plan.property_selection =
                crate::engine::PropertySelection::ExactlyOne(property_id.clone());
        }
        run_plan.search_bounds.max_depth = plan.horizon.map(|value| value as usize);
        Ok(RawSolverResult::Explicit(check_explicit(model, &run_plan)))
    }

    fn normalize(
        &self,
        _model: &ModelIr,
        _run_plan: &RunPlan,
        raw: RawSolverResult,
    ) -> Result<NormalizedRunResult, String> {
        match raw {
            RawSolverResult::Explicit(outcome) => {
                let trace = match &outcome {
                    CheckOutcome::Completed(result) => result.trace.clone(),
                    CheckOutcome::Errored(_) => None,
                };
                Ok(NormalizedRunResult { outcome, trace })
            }
            RawSolverResult::Protocol(_) => {
                Err("mock-bmc adapter cannot normalize protocol results".to_string())
            }
        }
    }
}

impl SolverAdapter for CommandSolverAdapter {
    fn backend_kind(&self) -> BackendKind {
        BackendKind::MockBmc
    }

    fn capabilities(&self) -> CapabilityMatrix {
        CapabilityMatrix {
            backend_name: self.backend_name.clone(),
            supports_explicit: false,
            supports_bmc: true,
            supports_certificate: false,
            supports_trace: true,
            supports_witness: true,
            selfcheck_compatible: false,
        }
    }

    fn build_plan(&self, _model: &ModelIr, run_plan: &RunPlan) -> Result<SolverRunPlan, String> {
        let target_property_ids = match &run_plan.property_selection {
            crate::engine::PropertySelection::ExactlyOne(id) => vec![id.clone()],
        };
        Ok(SolverRunPlan {
            run_id: format!("{}-cmd", run_plan.manifest.run_id),
            backend: BackendKind::MockBmc,
            target_property_ids,
            horizon: run_plan.search_bounds.max_depth.map(|value| value as u32),
            encoded_model_hash: format!("cmd:{}", run_plan.manifest.source_hash),
        })
    }

    fn run(&self, _model: &ModelIr, plan: &SolverRunPlan) -> Result<RawSolverResult, String> {
        let output = Command::new(&self.executable)
            .args(&self.args)
            .env("VALID_RUN_ID", &plan.run_id)
            .output()
            .map_err(|err| format!("failed to execute solver command: {err}"))?;
        if !output.status.success() {
            return Err(format!(
                "solver command failed with status {}",
                output.status
            ));
        }
        let body = String::from_utf8(output.stdout)
            .map_err(|err| format!("solver output was not utf8: {err}"))?;
        let status = parse_protocol_value(&body, "STATUS")
            .ok_or_else(|| "missing STATUS in solver output".to_string())?;
        Ok(RawSolverResult::Protocol(CommandProtocolResult {
            status,
            actions: parse_protocol_actions(&body),
            raw_output: body,
        }))
    }

    fn normalize(
        &self,
        model: &ModelIr,
        run_plan: &RunPlan,
        raw: RawSolverResult,
    ) -> Result<NormalizedRunResult, String> {
        match raw {
            RawSolverResult::Protocol(protocol) => {
                normalize_protocol_result(model, run_plan, protocol)
            }
            RawSolverResult::Explicit(CheckOutcome::Errored(error)) => Ok(NormalizedRunResult {
                outcome: CheckOutcome::Errored(error),
                trace: None,
            }),
            RawSolverResult::Explicit(outcome) => {
                let action_ids = Vec::<String>::new();
                let trace = if action_ids.is_empty() {
                    match &outcome {
                        CheckOutcome::Completed(result) => result.trace.clone(),
                        CheckOutcome::Errored(_) => None,
                    }
                } else {
                    let terminal = replay_actions(model, &action_ids)
                        .map_err(|diagnostic| diagnostic.message)?;
                    let initial = crate::kernel::transition::build_initial_state(model)
                        .map_err(|diagnostic| diagnostic.message)?;
                    Some(EvidenceTrace {
                        schema_version: "1.0.0".to_string(),
                        evidence_id: format!("ev-{}", run_plan.manifest.run_id),
                        run_id: run_plan.manifest.run_id.clone(),
                        property_id: match &run_plan.property_selection {
                            crate::engine::PropertySelection::ExactlyOne(id) => id.clone(),
                        },
                        evidence_kind: EvidenceKind::Trace,
                        assurance_level: crate::engine::AssuranceLevel::Incomplete,
                        trace_hash: format!(
                            "cmd:{}:{}",
                            run_plan.manifest.run_id,
                            action_ids.len()
                        ),
                        steps: vec![TraceStep {
                            index: 0,
                            from_state_id: "s-000000".to_string(),
                            action_id: Some(action_ids.join(",")),
                            action_label: Some("external-sequence".to_string()),
                            to_state_id: "s-000001".to_string(),
                            depth: action_ids.len() as u32,
                            state_before: initial.as_named_map(model),
                            state_after: terminal.as_named_map(model),
                            note: Some("normalized from command adapter".to_string()),
                        }],
                    })
                };
                Ok(NormalizedRunResult { outcome, trace })
            }
        }
    }
}

fn normalize_protocol_result(
    model: &ModelIr,
    run_plan: &RunPlan,
    protocol: CommandProtocolResult,
) -> Result<NormalizedRunResult, String> {
    let property_id = match &run_plan.property_selection {
        crate::engine::PropertySelection::ExactlyOne(id) => id.clone(),
    };
    let property_kind = model
        .properties
        .iter()
        .find(|property| property.property_id == property_id)
        .map(|property| property.kind.clone())
        .ok_or_else(|| format!("unknown property `{}`", property_id))?;
    let assurance_level = if run_plan.search_bounds.max_depth.is_some() {
        AssuranceLevel::Bounded
    } else {
        AssuranceLevel::Incomplete
    };
    let trace = if protocol.actions.is_empty() {
        None
    } else {
        let terminal =
            replay_actions(model, &protocol.actions).map_err(|diagnostic| diagnostic.message)?;
        let initial = crate::kernel::transition::build_initial_state(model)
            .map_err(|diagnostic| diagnostic.message)?;
        Some(EvidenceTrace {
            schema_version: "1.0.0".to_string(),
            evidence_id: format!("ev-{}", run_plan.manifest.run_id),
            run_id: run_plan.manifest.run_id.clone(),
            property_id: property_id.clone(),
            evidence_kind: EvidenceKind::Trace,
            assurance_level,
            trace_hash: stable_hash_hex(&protocol.actions.join("\u{1f}")),
            steps: vec![TraceStep {
                index: 0,
                from_state_id: "s-000000".to_string(),
                action_id: Some(protocol.actions.join(",")),
                action_label: Some("external-sequence".to_string()),
                to_state_id: "s-000001".to_string(),
                depth: protocol.actions.len() as u32,
                state_before: initial.as_named_map(model),
                state_after: terminal.as_named_map(model),
                note: Some("normalized from command adapter".to_string()),
            }],
        })
    };

    let outcome = match protocol.status.as_str() {
        "PASS" => CheckOutcome::Completed(ExplicitRunResult {
            manifest: run_plan.manifest.clone(),
            status: RunStatus::Pass,
            assurance_level,
            property_result: PropertyResult {
                property_id: property_id.clone(),
                property_kind: property_kind.clone(),
                status: RunStatus::Pass,
                assurance_level,
                reason_code: Some("SOLVER_REPORTED_PASS".to_string()),
                unknown_reason: None,
                terminal_state_id: None,
                evidence_id: trace.as_ref().map(|item| item.evidence_id.clone()),
                summary: "external solver reported pass".to_string(),
            },
            explored_states: 0,
            explored_transitions: trace.as_ref().map(|item| item.steps.len()).unwrap_or(0),
            trace: trace.clone(),
        }),
        "FAIL" => {
            if trace.is_none() {
                return Ok(NormalizedRunResult {
                    outcome: CheckOutcome::Errored(CheckErrorEnvelope {
                        manifest: run_plan.manifest.clone(),
                        status: ErrorStatus::Error,
                        assurance_level: AssuranceLevel::Incomplete,
                        diagnostics: vec![Diagnostic::new(
                            ErrorCode::SearchError,
                            DiagnosticSegment::EngineSearch,
                            "external solver reported FAIL without replayable actions",
                        )
                        .with_help(
                            "configure the solver adapter to emit ACTIONS for failing runs",
                        )],
                    }),
                    trace: None,
                });
            }
            CheckOutcome::Completed(ExplicitRunResult {
                manifest: run_plan.manifest.clone(),
                status: RunStatus::Fail,
                assurance_level,
                property_result: PropertyResult {
                    property_id: property_id.clone(),
                    property_kind: property_kind.clone(),
                    status: RunStatus::Fail,
                    assurance_level,
                    reason_code: Some("SOLVER_REPORTED_FAIL".to_string()),
                    unknown_reason: None,
                    terminal_state_id: Some("s-000001".to_string()),
                    evidence_id: trace.as_ref().map(|item| item.evidence_id.clone()),
                    summary: "external solver reported fail".to_string(),
                },
                explored_states: 0,
                explored_transitions: trace.as_ref().map(|item| item.steps.len()).unwrap_or(0),
                trace: trace.clone(),
            })
        }
        "UNKNOWN" => CheckOutcome::Completed(ExplicitRunResult {
            manifest: run_plan.manifest.clone(),
            status: RunStatus::Unknown,
            assurance_level: AssuranceLevel::Incomplete,
            property_result: PropertyResult {
                property_id,
                property_kind,
                status: RunStatus::Unknown,
                assurance_level: AssuranceLevel::Incomplete,
                reason_code: Some("SOLVER_REPORTED_UNKNOWN".to_string()),
                unknown_reason: Some(UnknownReason::EngineAborted),
                terminal_state_id: trace.as_ref().map(|_| "s-000001".to_string()),
                evidence_id: trace.as_ref().map(|item| item.evidence_id.clone()),
                summary: "external solver reported unknown".to_string(),
            },
            explored_states: 0,
            explored_transitions: trace.as_ref().map(|item| item.steps.len()).unwrap_or(0),
            trace: trace.clone(),
        }),
        other => CheckOutcome::Errored(CheckErrorEnvelope {
            manifest: run_plan.manifest.clone(),
            status: ErrorStatus::Error,
            assurance_level: AssuranceLevel::Incomplete,
            diagnostics: vec![Diagnostic::new(
                ErrorCode::SearchError,
                DiagnosticSegment::EngineSearch,
                format!("external solver protocol unsupported status `{other}`"),
            )
            .with_help("supported statuses are PASS, FAIL, and UNKNOWN")],
        }),
    };

    Ok(NormalizedRunResult { outcome, trace })
}

fn parse_protocol_value(body: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    body.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .map(|value| value.trim().to_string())
    })
}

fn parse_protocol_actions(body: &str) -> Vec<String> {
    parse_protocol_value(body, "ACTIONS")
        .unwrap_or_default()
        .split(',')
        .filter(|item| !item.trim().is_empty())
        .map(|item| item.trim().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{
        engine::{PropertySelection, RunPlan},
        frontend::compile_model,
    };

    use super::{
        render_capability_matrix_json, validate_capability_matrix, CommandSolverAdapter,
        ExplicitAdapter, MockBmcAdapter, SolverAdapter,
    };

    #[test]
    fn explicit_adapter_normalizes_completed_outcome() {
        let model = compile_model(
            "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_SAFE:\n  invariant: x <= 1\n",
        )
        .unwrap();
        let mut run_plan = RunPlan::default();
        run_plan.property_selection = PropertySelection::ExactlyOne("P_SAFE".to_string());
        let adapter = ExplicitAdapter;
        let plan = adapter.build_plan(&model, &run_plan).unwrap();
        let raw = adapter.run(&model, &plan).unwrap();
        let normalized = adapter.normalize(&model, &run_plan, raw).unwrap();
        assert!(normalized.trace.is_some());
    }

    #[test]
    fn mock_bmc_adapter_reports_bmc_capabilities() {
        let adapter = MockBmcAdapter;
        let caps = adapter.capabilities();
        assert!(caps.supports_bmc);
        assert!(caps.supports_witness);
        validate_capability_matrix(&caps).unwrap();
        assert!(render_capability_matrix_json(&caps).contains("\"backend\":\"mock-bmc\""));
    }

    #[test]
    fn command_adapter_executes_process() {
        let adapter = CommandSolverAdapter {
            backend_name: "cmd".to_string(),
            executable: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'STATUS=UNKNOWN\\nACTIONS=Jump'".to_string(),
            ],
        };
        let model = compile_model(
            "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_SAFE:\n  invariant: x <= 7\n",
        )
        .unwrap();
        let mut run_plan = RunPlan::default();
        run_plan.property_selection = PropertySelection::ExactlyOne("P_SAFE".to_string());
        let plan = adapter.build_plan(&model, &run_plan).unwrap();
        let raw = adapter.run(&model, &plan).unwrap();
        let normalized = adapter.normalize(&model, &run_plan, raw).unwrap();
        assert!(normalized.trace.is_some());
    }
}
