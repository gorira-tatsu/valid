//! Machine-readable API layer for AI and CLI integration.

use crate::{
    bundled_models::{
        check_bundled_model, explain_bundled_model, inspect_bundled_model, is_bundled_model_ref,
        orchestrate_bundled_model, testgen_bundled_model,
    },
    contract::snapshot_model,
    coverage::{collect_coverage, validate_coverage_report, CoverageReport},
    engine::{CheckErrorEnvelope, CheckOutcome, PropertySelection, RunManifest, RunPlan},
    frontend,
    ir::ModelIr,
    orchestrator::run_all_properties_with_backend,
    solver::{capabilities_for_config, run_with_adapter, AdapterConfig, CapabilityMatrix},
    support::{
        diagnostics::Diagnostic,
        hash::stable_hash_hex,
        schema::{require_len_match, require_non_empty, require_schema_version},
    },
    testgen::{
        build_counterexample_vector, build_synthetic_witness_vectors,
        minimize_counterexample_vector, MinimizeResult,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectResponse {
    pub schema_version: String,
    pub request_id: String,
    pub status: String,
    pub model_id: String,
    pub state_fields: Vec<String>,
    pub actions: Vec<String>,
    pub properties: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectRequest {
    pub request_id: String,
    pub source_name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRequest {
    pub request_id: String,
    pub source_name: String,
    pub source: String,
    pub property_id: Option<String>,
    pub backend: Option<String>,
    pub solver_executable: Option<String>,
    pub solver_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExplainCandidateCause {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExplainResponse {
    pub schema_version: String,
    pub request_id: String,
    pub status: String,
    pub evidence_id: String,
    pub property_id: String,
    pub failure_step_index: usize,
    pub involved_fields: Vec<String>,
    pub candidate_causes: Vec<ExplainCandidateCause>,
    pub repair_hints: Vec<String>,
    pub confidence: f32,
    pub best_practices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimizeRequest {
    pub request_id: String,
    pub source_name: String,
    pub source: String,
    pub property_id: Option<String>,
    pub backend: Option<String>,
    pub solver_executable: Option<String>,
    pub solver_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimizeResponse {
    pub schema_version: String,
    pub request_id: String,
    pub status: String,
    pub original_steps: usize,
    pub minimized_steps: usize,
    pub vector_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestgenRequest {
    pub request_id: String,
    pub source_name: String,
    pub source: String,
    pub strategy: String,
    pub backend: Option<String>,
    pub solver_executable: Option<String>,
    pub solver_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestgenResponse {
    pub schema_version: String,
    pub request_id: String,
    pub status: String,
    pub vector_ids: Vec<String>,
    pub generated_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilitiesResponse {
    pub schema_version: String,
    pub request_id: String,
    pub backend: String,
    pub capabilities: CapabilityMatrix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilitiesRequest {
    pub request_id: String,
    pub backend: Option<String>,
    pub solver_executable: Option<String>,
    pub solver_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestrateRequest {
    pub request_id: String,
    pub source_name: String,
    pub source: String,
    pub backend: Option<String>,
    pub solver_executable: Option<String>,
    pub solver_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestratedRunSummary {
    pub property_id: String,
    pub status: String,
    pub assurance_level: String,
    pub run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestrateResponse {
    pub schema_version: String,
    pub request_id: String,
    pub runs: Vec<OrchestratedRunSummary>,
    pub aggregate_coverage: Option<CoverageReport>,
}

pub fn inspect_source(request: &InspectRequest) -> Result<InspectResponse, Vec<Diagnostic>> {
    if is_bundled_model_ref(&request.source_name) {
        return inspect_bundled_model(&request.request_id, &request.source_name).map_err(|message| {
            vec![Diagnostic::new(
                crate::support::diagnostics::ErrorCode::SearchError,
                crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                message,
            )]
        });
    }
    let model = frontend::compile_model(&request.source)?;
    Ok(InspectResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request.request_id.clone(),
        status: "ok".to_string(),
        model_id: model.model_id.clone(),
        state_fields: model.state_fields.iter().map(|f| f.name.clone()).collect(),
        actions: model.actions.iter().map(|a| a.action_id.clone()).collect(),
        properties: model
            .properties
            .iter()
            .map(|p| p.property_id.clone())
            .collect(),
    })
}

pub fn compile_source(source: &str) -> Result<ModelIr, Vec<Diagnostic>> {
    frontend::compile_model(source)
}

pub fn capabilities_response(
    request: &CapabilitiesRequest,
) -> Result<CapabilitiesResponse, String> {
    let config = match request.backend.as_deref() {
        None | Some("explicit") => AdapterConfig::Explicit,
        Some("mock-bmc") => AdapterConfig::MockBmc,
        Some("smt-cvc5") => AdapterConfig::SmtCvc5 {
            executable: request
                .solver_executable
                .clone()
                .ok_or_else(|| "solver_executable is required when backend=smt-cvc5".to_string())?,
            args: request.solver_args.clone(),
        },
        Some("command") => AdapterConfig::Command {
            backend_name: "command".to_string(),
            executable: request
                .solver_executable
                .clone()
                .ok_or_else(|| "solver_executable is required when backend=command".to_string())?,
            args: request.solver_args.clone(),
        },
        Some(other) => return Err(format!("unsupported backend `{other}`")),
    };
    let capabilities = capabilities_for_config(&config);
    Ok(CapabilitiesResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request.request_id.clone(),
        backend: capabilities.backend_name.clone(),
        capabilities,
    })
}

pub fn orchestrate_source(
    request: &OrchestrateRequest,
) -> Result<OrchestrateResponse, CheckErrorEnvelope> {
    if is_bundled_model_ref(&request.source_name) {
        return orchestrate_bundled_model(&request.request_id, &request.source_name).map_err(
            |message| CheckErrorEnvelope {
                manifest: RunManifest {
                    request_id: request.request_id.clone(),
                    run_id: format!(
                        "run-{}",
                        stable_hash_hex(&request.request_id).replace("sha256:", "")
                    ),
                    schema_version: "1.0.0".to_string(),
                    source_hash: stable_hash_hex(&request.source_name),
                    contract_hash: stable_hash_hex(&request.source_name),
                    engine_version: env!("CARGO_PKG_VERSION").to_string(),
                    backend_name: crate::engine::BackendKind::Explicit,
                    backend_version: env!("CARGO_PKG_VERSION").to_string(),
                    seed: None,
                },
                status: crate::engine::ErrorStatus::Error,
                assurance_level: crate::engine::AssuranceLevel::Incomplete,
                diagnostics: vec![Diagnostic::new(
                    crate::support::diagnostics::ErrorCode::SearchError,
                    crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                    message,
                )],
            },
        );
    }
    let backend_fallback =
        backend_config_from_orchestrate_request(request).unwrap_or(AdapterConfig::Explicit);
    let model =
        frontend::compile_model(&request.source).map_err(|diagnostics| CheckErrorEnvelope {
            manifest: RunManifest {
                request_id: request.request_id.clone(),
                run_id: format!(
                    "run-{}",
                    stable_hash_hex(&request.request_id).replace("sha256:", "")
                ),
                schema_version: "1.0.0".to_string(),
                source_hash: stable_hash_hex(&request.source),
                contract_hash: "sha256:unknown".to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                backend_name: backend_kind_for_config(&backend_fallback),
                backend_version: backend_version_for_config(&backend_fallback),
                seed: None,
            },
            status: crate::engine::ErrorStatus::Error,
            assurance_level: crate::engine::AssuranceLevel::Incomplete,
            diagnostics,
        })?;
    let snapshot = snapshot_model(&model);
    let backend =
        backend_config_from_orchestrate_request(request).map_err(|message| CheckErrorEnvelope {
            manifest: RunManifest {
                request_id: request.request_id.clone(),
                run_id: format!(
                    "run-{}",
                    stable_hash_hex(&request.request_id).replace("sha256:", "")
                ),
                schema_version: "1.0.0".to_string(),
                source_hash: stable_hash_hex(&request.source),
                contract_hash: snapshot.contract_hash.clone(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                backend_name: crate::engine::BackendKind::Explicit,
                backend_version: env!("CARGO_PKG_VERSION").to_string(),
                seed: None,
            },
            status: crate::engine::ErrorStatus::Error,
            assurance_level: crate::engine::AssuranceLevel::Incomplete,
            diagnostics: vec![Diagnostic::new(
                crate::support::diagnostics::ErrorCode::SearchError,
                crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                message,
            )],
        })?;
    let base_plan = RunPlan::default();
    let mut traces = Vec::new();
    let runs = run_all_properties_with_backend(&model, &base_plan, &backend)
        .into_iter()
        .map(|run| match run.outcome {
            CheckOutcome::Completed(result) => {
                if let Some(trace) = result.trace.clone() {
                    traces.push(trace);
                }
                OrchestratedRunSummary {
                    property_id: run.property_id,
                    status: format!("{:?}", result.status),
                    assurance_level: format!("{:?}", result.assurance_level),
                    run_id: result.manifest.run_id,
                }
            }
            CheckOutcome::Errored(error) => OrchestratedRunSummary {
                property_id: run.property_id,
                status: "ERROR".to_string(),
                assurance_level: format!("{:?}", error.assurance_level),
                run_id: error.manifest.run_id,
            },
        })
        .collect();
    let aggregate_coverage = if traces.is_empty() {
        None
    } else {
        Some(collect_coverage(&model, &traces))
    };
    Ok(OrchestrateResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request.request_id.clone(),
        runs,
        aggregate_coverage,
    })
}

pub fn check_source(request: &CheckRequest) -> CheckOutcome {
    if is_bundled_model_ref(&request.source_name) {
        return check_bundled_model(&request.request_id, &request.source_name).unwrap_or_else(
            |message| {
                CheckOutcome::Errored(CheckErrorEnvelope {
                    manifest: RunManifest {
                        request_id: request.request_id.clone(),
                        run_id: format!(
                            "run-{}",
                            stable_hash_hex(&request.request_id).replace("sha256:", "")
                        ),
                        schema_version: "1.0.0".to_string(),
                        source_hash: stable_hash_hex(&request.source_name),
                        contract_hash: stable_hash_hex(&request.source_name),
                        engine_version: env!("CARGO_PKG_VERSION").to_string(),
                        backend_name: crate::engine::BackendKind::Explicit,
                        backend_version: env!("CARGO_PKG_VERSION").to_string(),
                        seed: None,
                    },
                    status: crate::engine::ErrorStatus::Error,
                    assurance_level: crate::engine::AssuranceLevel::Incomplete,
                    diagnostics: vec![Diagnostic::new(
                        crate::support::diagnostics::ErrorCode::SearchError,
                        crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                        message,
                    )],
                })
            },
        );
    }
    let source_hash = stable_hash_hex(&request.source);
    let adapter = backend_config_from_request(request).unwrap_or(AdapterConfig::Explicit);
    match frontend::compile_model(&request.source) {
        Ok(model) => {
            let snapshot = snapshot_model(&model);
            let property_id = request
                .property_id
                .clone()
                .or_else(|| {
                    model
                        .properties
                        .first()
                        .map(|property| property.property_id.clone())
                })
                .unwrap_or_else(|| "P_SAFE".to_string());
            let mut plan = RunPlan::default();
            plan.manifest = RunManifest {
                request_id: request.request_id.clone(),
                run_id: format!(
                    "run-{}",
                    stable_hash_hex(&(request.request_id.clone() + &property_id))
                        .replace("sha256:", "")
                ),
                schema_version: "1.0.0".to_string(),
                source_hash,
                contract_hash: snapshot.contract_hash,
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                backend_name: backend_kind_for_config(&adapter),
                backend_version: backend_version_for_config(&adapter),
                seed: None,
            };
            plan.property_selection = PropertySelection::ExactlyOne(property_id);
            match run_with_adapter(&model, &plan, &adapter) {
                Ok(normalized) => normalized.outcome,
                Err(message) => CheckOutcome::Errored(CheckErrorEnvelope {
                    manifest: plan.manifest.clone(),
                    status: crate::engine::ErrorStatus::Error,
                    assurance_level: crate::engine::AssuranceLevel::Incomplete,
                    diagnostics: vec![Diagnostic::new(
                        crate::support::diagnostics::ErrorCode::SearchError,
                        crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                        message,
                    )],
                }),
            }
        }
        Err(diagnostics) => CheckOutcome::Errored(CheckErrorEnvelope {
            manifest: RunManifest {
                request_id: request.request_id.clone(),
                run_id: format!(
                    "run-{}",
                    stable_hash_hex(&request.request_id).replace("sha256:", "")
                ),
                schema_version: "1.0.0".to_string(),
                source_hash,
                contract_hash: "sha256:unknown".to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                backend_name: backend_kind_for_config(&adapter),
                backend_version: backend_version_for_config(&adapter),
                seed: None,
            },
            status: crate::engine::ErrorStatus::Error,
            assurance_level: crate::engine::AssuranceLevel::Incomplete,
            diagnostics,
        }),
    }
}

pub fn explain_source(request: &CheckRequest) -> Result<ExplainResponse, CheckErrorEnvelope> {
    if is_bundled_model_ref(&request.source_name) {
        return explain_bundled_model(&request.request_id, &request.source_name).map_err(
            |message| CheckErrorEnvelope {
                manifest: RunManifest {
                    request_id: request.request_id.clone(),
                    run_id: format!(
                        "run-{}",
                        stable_hash_hex(&request.request_id).replace("sha256:", "")
                    ),
                    schema_version: "1.0.0".to_string(),
                    source_hash: stable_hash_hex(&request.source_name),
                    contract_hash: stable_hash_hex(&request.source_name),
                    engine_version: env!("CARGO_PKG_VERSION").to_string(),
                    backend_name: crate::engine::BackendKind::Explicit,
                    backend_version: env!("CARGO_PKG_VERSION").to_string(),
                    seed: None,
                },
                status: crate::engine::ErrorStatus::Error,
                assurance_level: crate::engine::AssuranceLevel::Incomplete,
                diagnostics: vec![Diagnostic::new(
                    crate::support::diagnostics::ErrorCode::SearchError,
                    crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                    message,
                )],
            },
        );
    }
    let compiled_model = frontend::compile_model(&request.source).ok();
    match check_source(request) {
        CheckOutcome::Completed(result) => {
            let trace = result.trace.ok_or_else(|| CheckErrorEnvelope {
                manifest: result.manifest.clone(),
                status: crate::engine::ErrorStatus::Error,
                assurance_level: crate::engine::AssuranceLevel::Incomplete,
                diagnostics: vec![Diagnostic::new(
                    crate::support::diagnostics::ErrorCode::SearchError,
                    crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                    "no evidence trace available for explain",
                )],
            })?;
            let failure_step = trace.steps.last().ok_or_else(|| CheckErrorEnvelope {
                manifest: result.manifest.clone(),
                status: crate::engine::ErrorStatus::Error,
                assurance_level: crate::engine::AssuranceLevel::Incomplete,
                diagnostics: vec![Diagnostic::new(
                    crate::support::diagnostics::ErrorCode::SearchError,
                    crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                    "empty trace cannot be explained",
                )],
            })?;
            let involved_fields = failure_step
                .state_before
                .iter()
                .filter_map(|(field, before)| {
                    let after = failure_step.state_after.get(field)?;
                    if before != after {
                        Some(field.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            let action_metadata = compiled_model.as_ref().and_then(|model| {
                failure_step.action_id.as_ref().and_then(|action_id| {
                    model
                        .actions
                        .iter()
                        .find(|action| &action.action_id == action_id)
                        .map(|action| {
                            (
                                action.action_id.clone(),
                                action.reads.clone(),
                                action.writes.clone(),
                            )
                        })
                })
            });
            let coverage_report = compiled_model
                .as_ref()
                .map(|model| collect_coverage(model, std::slice::from_ref(&trace)));
            let write_overlap = action_metadata
                .as_ref()
                .map(|(_, _, writes)| {
                    involved_fields
                        .iter()
                        .filter(|field| writes.contains(*field))
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let candidate_causes = if involved_fields.is_empty() {
                vec![
                    ExplainCandidateCause {
                        kind: "terminal_violation".to_string(),
                        message: format!(
                            "property {} failed without a field diff at terminal step",
                            trace.property_id
                        ),
                    },
                    ExplainCandidateCause {
                        kind: "action_semantics".to_string(),
                        message: failure_step
                            .action_id
                            .as_ref()
                            .map(|action| format!("{action} reached a violating state without a visible field delta"))
                            .unwrap_or_else(|| "initial or terminal condition violated the property".to_string()),
                    },
                ]
            } else {
                let mut causes = Vec::new();
                if let Some((action_id, reads, writes)) = &action_metadata {
                    if !write_overlap.is_empty() {
                        causes.push(ExplainCandidateCause {
                            kind: "write_set_overlap".to_string(),
                            message: format!(
                                "action {action_id} writes {} and overlaps with failing fields {}",
                                writes.join(", "),
                                write_overlap.join(", ")
                            ),
                        });
                    }
                    causes.push(ExplainCandidateCause {
                        kind: "action_write_set".to_string(),
                        message: format!(
                            "review writes [{}] and reads [{}] of action {action_id} at failing step {}",
                            writes.join(", "),
                            reads.join(", "),
                            failure_step.index,
                        ),
                    });
                    if let Some(report) = &coverage_report {
                        let execution_count = report
                            .action_execution_counts
                            .get(action_id)
                            .copied()
                            .unwrap_or(0);
                        if execution_count <= 1 {
                            causes.push(ExplainCandidateCause {
                                kind: "rare_action_path".to_string(),
                                message: format!(
                                    "action {action_id} was executed only {} time in the available witness/trace set",
                                    execution_count
                                ),
                            });
                        }
                        if let Some(uncovered) = report
                            .uncovered_guards
                            .iter()
                            .find(|entry| entry.starts_with(&format!("{action_id}:")))
                        {
                            causes.push(ExplainCandidateCause {
                                kind: "guard_polarity_gap".to_string(),
                                message: format!(
                                    "guard coverage for action {action_id} is incomplete: {uncovered}"
                                ),
                            });
                        }
                    }
                }
                causes.extend(involved_fields.iter().map(|field| ExplainCandidateCause {
                    kind: "field_flip".to_string(),
                    message: format!("{field} changed at step {}", failure_step.index),
                }));
                causes
            };
            let mut repair_hints = vec![
                "review the guard and update set of the failing action".to_string(),
                format!("verify invariant {} is intended", trace.property_id),
            ];
            if let Some(action_id) = &failure_step.action_id {
                repair_hints.push(format!(
                    "inspect the postcondition or implementation of action {action_id}"
                ));
            }
            if !write_overlap.is_empty() {
                repair_hints.push(format!(
                    "check whether writes [{}] should be narrowed or guarded",
                    write_overlap.join(", ")
                ));
            }
            if let (Some(report), Some(action_id)) = (&coverage_report, &failure_step.action_id) {
                if report
                    .uncovered_guards
                    .iter()
                    .any(|entry| entry.starts_with(&format!("{action_id}:")))
                {
                    repair_hints.push(format!(
                        "add witness coverage for both guard outcomes of action {action_id}"
                    ));
                }
            }
            let mut confidence = 0.45f32;
            if failure_step.action_id.is_some() {
                confidence += 0.1;
            }
            if !involved_fields.is_empty() {
                confidence += 0.1;
            }
            if !write_overlap.is_empty() {
                confidence += 0.2;
            }
            if let (Some(report), Some(action_id)) = (&coverage_report, &failure_step.action_id) {
                if report
                    .uncovered_guards
                    .iter()
                    .any(|entry| entry.starts_with(&format!("{action_id}:")))
                {
                    confidence += 0.1;
                }
            }
            if trace.steps.len() == 1 {
                confidence += 0.1;
            }
            confidence = confidence.min(0.95);
            Ok(ExplainResponse {
                schema_version: "1.0.0".to_string(),
                request_id: request.request_id.clone(),
                status: "ok".to_string(),
                evidence_id: trace.evidence_id.clone(),
                property_id: trace.property_id.clone(),
                failure_step_index: failure_step.index,
                involved_fields,
                candidate_causes,
                repair_hints,
                confidence,
                best_practices: vec![
                    "keep write sets explicit so involved fields stay explainable".to_string(),
                    "add witness vectors for critical actions so explain results stay reproducible"
                        .to_string(),
                ],
            })
        }
        CheckOutcome::Errored(error) => Err(error),
    }
}

pub fn minimize_source(request: &MinimizeRequest) -> Result<MinimizeResponse, CheckErrorEnvelope> {
    let property_id = request.property_id.clone();
    let compiled =
        frontend::compile_model(&request.source).map_err(|diagnostics| CheckErrorEnvelope {
            manifest: RunManifest {
                request_id: request.request_id.clone(),
                run_id: format!(
                    "run-{}",
                    stable_hash_hex(&request.request_id).replace("sha256:", "")
                ),
                schema_version: "1.0.0".to_string(),
                source_hash: stable_hash_hex(&request.source),
                contract_hash: "sha256:unknown".to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                backend_name: crate::engine::BackendKind::Explicit,
                backend_version: env!("CARGO_PKG_VERSION").to_string(),
                seed: None,
            },
            status: crate::engine::ErrorStatus::Error,
            assurance_level: crate::engine::AssuranceLevel::Incomplete,
            diagnostics,
        })?;
    let check = check_source(&CheckRequest {
        request_id: request.request_id.clone(),
        source_name: request.source_name.clone(),
        source: request.source.clone(),
        property_id,
        backend: request.backend.clone(),
        solver_executable: request.solver_executable.clone(),
        solver_args: request.solver_args.clone(),
    });
    let result = match check {
        CheckOutcome::Completed(result) => result,
        CheckOutcome::Errored(error) => return Err(error),
    };
    let trace = result.trace.clone().ok_or_else(|| CheckErrorEnvelope {
        manifest: result.manifest.clone(),
        status: crate::engine::ErrorStatus::Error,
        assurance_level: crate::engine::AssuranceLevel::Incomplete,
        diagnostics: vec![Diagnostic::new(
            crate::support::diagnostics::ErrorCode::SearchError,
            crate::support::diagnostics::DiagnosticSegment::EngineSearch,
            "no evidence trace available for minimization",
        )],
    })?;
    let vector = build_counterexample_vector(&trace).map_err(|message| CheckErrorEnvelope {
        manifest: result.manifest.clone(),
        status: crate::engine::ErrorStatus::Error,
        assurance_level: crate::engine::AssuranceLevel::Incomplete,
        diagnostics: vec![Diagnostic::new(
            crate::support::diagnostics::ErrorCode::SearchError,
            crate::support::diagnostics::DiagnosticSegment::EngineSearch,
            message,
        )],
    })?;
    let minimized = minimize_counterexample_vector(&compiled, &vector, &trace.property_id)
        .map_err(|message| CheckErrorEnvelope {
            manifest: result.manifest.clone(),
            status: crate::engine::ErrorStatus::Error,
            assurance_level: crate::engine::AssuranceLevel::Incomplete,
            diagnostics: vec![Diagnostic::new(
                crate::support::diagnostics::ErrorCode::SearchError,
                crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                message,
            )],
        })?;
    build_minimize_response(&request.request_id, minimized)
}

fn build_minimize_response(
    request_id: &str,
    minimized: MinimizeResult,
) -> Result<MinimizeResponse, CheckErrorEnvelope> {
    Ok(MinimizeResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        status: "ok".to_string(),
        original_steps: minimized.original_steps,
        minimized_steps: minimized.minimized_steps,
        vector_id: minimized.vector.vector_id,
    })
}

pub fn testgen_source(request: &TestgenRequest) -> Result<TestgenResponse, CheckErrorEnvelope> {
    if is_bundled_model_ref(&request.source_name) {
        return testgen_bundled_model(&request.request_id, &request.source_name, &request.strategy).map_err(
            |message| CheckErrorEnvelope {
                manifest: RunManifest {
                    request_id: request.request_id.clone(),
                    run_id: format!(
                        "run-{}",
                        stable_hash_hex(&request.request_id).replace("sha256:", "")
                    ),
                    schema_version: "1.0.0".to_string(),
                    source_hash: stable_hash_hex(&request.source_name),
                    contract_hash: stable_hash_hex(&request.source_name),
                    engine_version: env!("CARGO_PKG_VERSION").to_string(),
                    backend_name: crate::engine::BackendKind::Explicit,
                    backend_version: env!("CARGO_PKG_VERSION").to_string(),
                    seed: None,
                },
                status: crate::engine::ErrorStatus::Error,
                assurance_level: crate::engine::AssuranceLevel::Incomplete,
                diagnostics: vec![Diagnostic::new(
                    crate::support::diagnostics::ErrorCode::SearchError,
                    crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                    message,
                )],
            },
        );
    }
    let outcome = check_source(&CheckRequest {
        request_id: request.request_id.clone(),
        source_name: request.source_name.clone(),
        source: request.source.clone(),
        property_id: None,
        backend: request.backend.clone(),
        solver_executable: request.solver_executable.clone(),
        solver_args: request.solver_args.clone(),
    });
    let model =
        frontend::compile_model(&request.source).map_err(|diagnostics| CheckErrorEnvelope {
            manifest: RunManifest {
                request_id: request.request_id.clone(),
                run_id: format!(
                    "run-{}",
                    stable_hash_hex(&request.request_id).replace("sha256:", "")
                ),
                schema_version: "1.0.0".to_string(),
                source_hash: stable_hash_hex(&request.source),
                contract_hash: "sha256:unknown".to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                backend_name: crate::engine::BackendKind::Explicit,
                backend_version: env!("CARGO_PKG_VERSION").to_string(),
                seed: None,
            },
            status: crate::engine::ErrorStatus::Error,
            assurance_level: crate::engine::AssuranceLevel::Incomplete,
            diagnostics,
        })?;
    let result = match outcome {
        CheckOutcome::Completed(result) => result,
        CheckOutcome::Errored(error) => return Err(error),
    };
    let traces = result.trace.into_iter().collect::<Vec<_>>();
    let vectors = if request.strategy == "transition" || request.strategy == "witness" {
        let action_ids = model
            .actions
            .iter()
            .map(|action| action.action_id.clone())
            .collect::<Vec<_>>();
        let mut vectors = crate::testgen::build_transition_coverage_vectors(&traces, &action_ids);
        if vectors.is_empty() {
            let fallback_property_id = model
                .properties
                .first()
                .map(|property| property.property_id.as_str())
                .unwrap_or("P_SAFE");
            vectors = build_synthetic_witness_vectors(&model, fallback_property_id);
        }
        vectors
    } else {
        traces
            .iter()
            .filter_map(|trace| build_counterexample_vector(trace).ok())
            .collect::<Vec<_>>()
    };
    Ok(TestgenResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request.request_id.clone(),
        status: "ok".to_string(),
        vector_ids: vectors
            .iter()
            .map(|vector| vector.vector_id.clone())
            .collect(),
        generated_files: vectors
            .iter()
            .map(crate::testgen::generated_test_output_path)
            .collect(),
    })
}

pub fn validate_check_request(request: &CheckRequest) -> Result<(), String> {
    require_non_empty(&request.request_id, "request_id")?;
    require_non_empty(&request.source_name, "source_name")?;
    if !is_bundled_model_ref(&request.source_name) {
        require_non_empty(&request.source, "source")?;
    }
    if let Some(backend) = &request.backend {
        require_non_empty(backend, "backend")?;
    }
    Ok(())
}

pub fn validate_inspect_request(request: &InspectRequest) -> Result<(), String> {
    require_non_empty(&request.request_id, "request_id")?;
    require_non_empty(&request.source_name, "source_name")?;
    if !is_bundled_model_ref(&request.source_name) {
        require_non_empty(&request.source, "source")?;
    }
    Ok(())
}

pub fn validate_inspect_response(response: &InspectResponse) -> Result<(), String> {
    require_schema_version(&response.schema_version)?;
    require_non_empty(&response.request_id, "request_id")?;
    require_non_empty(&response.model_id, "model_id")?;
    Ok(())
}

pub fn validate_explain_request(request: &CheckRequest) -> Result<(), String> {
    validate_check_request(request)
}

pub fn validate_explain_response(response: &ExplainResponse) -> Result<(), String> {
    require_schema_version(&response.schema_version)?;
    require_non_empty(&response.request_id, "request_id")?;
    require_non_empty(&response.evidence_id, "evidence_id")?;
    require_non_empty(&response.property_id, "property_id")?;
    if !(0.0..=1.0).contains(&response.confidence) {
        return Err("confidence must be between 0.0 and 1.0".to_string());
    }
    Ok(())
}

pub fn validate_minimize_request(request: &MinimizeRequest) -> Result<(), String> {
    require_non_empty(&request.request_id, "request_id")?;
    require_non_empty(&request.source_name, "source_name")?;
    if !is_bundled_model_ref(&request.source_name) {
        require_non_empty(&request.source, "source")?;
    }
    Ok(())
}

pub fn validate_minimize_response(response: &MinimizeResponse) -> Result<(), String> {
    require_schema_version(&response.schema_version)?;
    require_non_empty(&response.request_id, "request_id")?;
    require_non_empty(&response.vector_id, "vector_id")?;
    if response.minimized_steps > response.original_steps {
        return Err("minimized_steps must not exceed original_steps".to_string());
    }
    Ok(())
}

pub fn validate_testgen_response(response: &TestgenResponse) -> Result<(), String> {
    require_schema_version(&response.schema_version)?;
    require_non_empty(&response.request_id, "request_id")?;
    require_len_match(
        response.vector_ids.len(),
        response.generated_files.len(),
        "vector_ids",
        "generated_files",
    )?;
    Ok(())
}

pub fn validate_capabilities_request(request: &CapabilitiesRequest) -> Result<(), String> {
    require_non_empty(&request.request_id, "request_id")?;
    if let Some(backend) = request.backend.as_deref() {
        require_non_empty(backend, "backend")?;
        if matches!(backend, "command" | "smt-cvc5") && request.solver_executable.is_none() {
            return Err(format!(
                "solver_executable is required when backend={backend}"
            ));
        }
    }
    Ok(())
}

pub fn validate_capabilities_response(response: &CapabilitiesResponse) -> Result<(), String> {
    require_schema_version(&response.schema_version)?;
    require_non_empty(&response.request_id, "request_id")?;
    require_non_empty(&response.backend, "backend")?;
    require_non_empty(
        &response.capabilities.backend_name,
        "capabilities.backend_name",
    )?;
    Ok(())
}

pub fn validate_testgen_request(request: &TestgenRequest) -> Result<(), String> {
    require_non_empty(&request.request_id, "request_id")?;
    require_non_empty(&request.source_name, "source_name")?;
    if !is_bundled_model_ref(&request.source_name) {
        require_non_empty(&request.source, "source")?;
    }
    match request.strategy.as_str() {
        "counterexample" | "transition" | "witness" => Ok(()),
        other => Err(format!(
            "strategy must be one of counterexample, transition, witness, got `{other}`"
        )),
    }
}

pub fn validate_orchestrate_request(request: &OrchestrateRequest) -> Result<(), String> {
    require_non_empty(&request.request_id, "request_id")?;
    require_non_empty(&request.source_name, "source_name")?;
    if !is_bundled_model_ref(&request.source_name) {
        require_non_empty(&request.source, "source")?;
    }
    Ok(())
}

pub fn validate_orchestrate_response(response: &OrchestrateResponse) -> Result<(), String> {
    require_schema_version(&response.schema_version)?;
    require_non_empty(&response.request_id, "request_id")?;
    for run in &response.runs {
        require_non_empty(&run.property_id, "runs[].property_id")?;
        require_non_empty(&run.status, "runs[].status")?;
        require_non_empty(&run.assurance_level, "runs[].assurance_level")?;
        require_non_empty(&run.run_id, "runs[].run_id")?;
    }
    if let Some(report) = &response.aggregate_coverage {
        validate_coverage_report(report)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        capabilities_response, check_source, explain_source, inspect_source, minimize_source,
        orchestrate_source, testgen_source, validate_capabilities_request,
        validate_capabilities_response, validate_check_request, validate_explain_request,
        validate_explain_response, validate_inspect_request, validate_inspect_response,
        validate_minimize_request, validate_minimize_response, validate_orchestrate_response,
        validate_testgen_request, validate_testgen_response, CapabilitiesRequest, CheckRequest,
        InspectRequest, MinimizeRequest, OrchestrateRequest, TestgenRequest,
    };
    use crate::engine::CheckOutcome;

    #[test]
    fn inspect_returns_model_outline() {
        let source = "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\nproperty P_SAFE:\n  invariant: x <= 7\n";
        let request = InspectRequest {
            request_id: "req-1".to_string(),
            source_name: "a.valid".to_string(),
            source: source.to_string(),
        };
        validate_inspect_request(&request).unwrap();
        let response = inspect_source(&request).unwrap();
        assert_eq!(response.model_id, "A");
        assert_eq!(response.properties, vec!["P_SAFE"]);
        validate_inspect_response(&response).unwrap();
    }

    #[test]
    fn check_wraps_frontend_errors_in_error_outcome() {
        let outcome = check_source(&CheckRequest {
            request_id: "req-1".to_string(),
            source_name: "broken.valid".to_string(),
            source: "model A\nstate:\n  x: u8[0..7]\ninit:\n  y = 0\n".to_string(),
            property_id: None,
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        });
        assert!(matches!(outcome, CheckOutcome::Errored(_)));
    }

    #[test]
    fn explain_returns_structured_failure_causes() {
        let source = "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_SAFE:\n  invariant: x <= 1\n";
        let request = CheckRequest {
            request_id: "req-explain".to_string(),
            source_name: "a.valid".to_string(),
            source: source.to_string(),
            property_id: Some("P_SAFE".to_string()),
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        };
        validate_explain_request(&request).unwrap();
        let response = explain_source(&request).unwrap();
        assert_eq!(response.property_id, "P_SAFE");
        assert_eq!(response.failure_step_index, 0);
        assert_eq!(response.involved_fields, vec!["x".to_string()]);
        assert!(response
            .candidate_causes
            .iter()
            .any(|cause| cause.kind == "write_set_overlap"));
        assert!(response
            .candidate_causes
            .iter()
            .any(|cause| cause.kind == "rare_action_path"));
        assert!(response
            .candidate_causes
            .iter()
            .any(|cause| cause.kind == "guard_polarity_gap"));
        assert!(response
            .candidate_causes
            .iter()
            .any(|cause| cause.kind == "action_write_set"));
        assert!(response.confidence >= 0.8);
        assert!(response
            .repair_hints
            .iter()
            .any(|hint| hint.contains("both guard outcomes")));
        validate_explain_response(&response).unwrap();
    }

    #[test]
    fn minimize_returns_shorter_vector_when_failure_is_preserved() {
        let source = "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Inc:\n  pre: true\n  post:\n    x = x + 1\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_SAFE:\n  invariant: x <= 1\n";
        let request = MinimizeRequest {
            request_id: "req-min".to_string(),
            source_name: "a.valid".to_string(),
            source: source.to_string(),
            property_id: Some("P_SAFE".to_string()),
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        };
        validate_minimize_request(&request).unwrap();
        let response = minimize_source(&request).unwrap();
        assert_eq!(response.original_steps, 1);
        assert_eq!(response.minimized_steps, 1);
        assert!(response.vector_id.contains("vec-"));
        validate_minimize_response(&response).unwrap();
    }

    #[test]
    fn testgen_returns_generated_file_targets() {
        let source = "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_SAFE:\n  invariant: x <= 1\n";
        let response = testgen_source(&TestgenRequest {
            request_id: "req-testgen".to_string(),
            source_name: "a.valid".to_string(),
            source: source.to_string(),
            strategy: "counterexample".to_string(),
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        })
        .unwrap();
        assert_eq!(response.vector_ids.len(), 1);
        validate_testgen_response(&response).unwrap();
    }

    #[test]
    fn witness_testgen_can_fallback_to_synthetic_vectors() {
        let source = "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_SAFE:\n  invariant: x <= 7\n";
        let response = testgen_source(&TestgenRequest {
            request_id: "req-witness".to_string(),
            source_name: "a.valid".to_string(),
            source: source.to_string(),
            strategy: "witness".to_string(),
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        })
        .unwrap();
        assert!(response.vector_ids.len() >= 1);
        validate_testgen_response(&response).unwrap();
    }

    #[test]
    fn capabilities_can_be_reported_for_command_backend() {
        let request = CapabilitiesRequest {
            request_id: "req-cap".to_string(),
            backend: Some("command".to_string()),
            solver_executable: Some("sh".to_string()),
            solver_args: vec!["-c".to_string(), "true".to_string()],
        };
        validate_capabilities_request(&request).unwrap();
        let response = capabilities_response(&request).unwrap();
        validate_capabilities_response(&response).unwrap();
        assert_eq!(response.backend, "command");
        assert!(response.capabilities.supports_bmc);
    }

    #[test]
    fn capabilities_can_be_reported_for_cvc5_backend() {
        let request = CapabilitiesRequest {
            request_id: "req-cap-cvc5".to_string(),
            backend: Some("smt-cvc5".to_string()),
            solver_executable: Some("sh".to_string()),
            solver_args: vec!["-c".to_string(), "true".to_string()],
        };
        validate_capabilities_request(&request).unwrap();
        let response = capabilities_response(&request).unwrap();
        validate_capabilities_response(&response).unwrap();
        assert_eq!(response.backend, "smt-cvc5");
        assert!(response.capabilities.supports_bmc);
        assert!(response.capabilities.supports_witness);
    }

    #[test]
    fn request_validation_rejects_empty_source() {
        let error = validate_check_request(&CheckRequest {
            request_id: "req".to_string(),
            source_name: "a.valid".to_string(),
            source: "".to_string(),
            property_id: None,
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        })
        .unwrap_err();
        assert!(error.contains("source"));
    }

    #[test]
    fn testgen_request_validation_rejects_unknown_strategy() {
        let error = validate_testgen_request(&TestgenRequest {
            request_id: "req".to_string(),
            source_name: "a.valid".to_string(),
            source: "model A".to_string(),
            strategy: "weird".to_string(),
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        })
        .unwrap_err();
        assert!(error.contains("strategy"));
    }

    #[test]
    fn check_can_use_command_backend() {
        let outcome = check_source(&CheckRequest {
            request_id: "req-cmd".to_string(),
            source_name: "cmd.valid".to_string(),
            source: "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_SAFE:\n  invariant: x <= 7\n".to_string(),
            property_id: Some("P_SAFE".to_string()),
            backend: Some("command".to_string()),
            solver_executable: Some("sh".to_string()),
            solver_args: vec![
                "-c".to_string(),
                "printf 'STATUS=UNKNOWN\\nACTIONS=Jump'".to_string(),
            ],
        });
        match outcome {
            CheckOutcome::Completed(result) => {
                assert_eq!(result.status, crate::engine::RunStatus::Unknown);
            }
            CheckOutcome::Errored(error) => panic!("unexpected error: {:?}", error),
        }
    }

    #[test]
    fn check_preserves_source_and_contract_hashes_in_manifest() {
        let outcome = check_source(&CheckRequest {
            request_id: "req-hash".to_string(),
            source_name: "hash.valid".to_string(),
            source: "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_SAFE:\n  invariant: x <= 7\n".to_string(),
            property_id: Some("P_SAFE".to_string()),
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        });
        match outcome {
            CheckOutcome::Completed(result) => {
                assert_ne!(result.manifest.source_hash, "sha256:unknown");
                assert_ne!(result.manifest.contract_hash, "sha256:unknown");
            }
            CheckOutcome::Errored(error) => panic!("unexpected error: {:?}", error),
        }
    }

    #[test]
    fn orchestrate_returns_one_entry_per_property() {
        let response = orchestrate_source(&OrchestrateRequest {
            request_id: "req-orch".to_string(),
            source_name: "a.valid".to_string(),
            source: "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P1:\n  invariant: x <= 1\nproperty P2:\n  invariant: x <= 7\n".to_string(),
            backend: Some("mock-bmc".to_string()),
            solver_executable: None,
            solver_args: vec![],
        })
        .unwrap();
        validate_orchestrate_response(&response).unwrap();
        assert_eq!(response.runs.len(), 2);
        assert!(response.aggregate_coverage.is_some());
    }
}

fn backend_config_from_request(request: &CheckRequest) -> Result<AdapterConfig, String> {
    match request.backend.as_deref() {
        None | Some("explicit") => Ok(AdapterConfig::Explicit),
        Some("mock-bmc") => Ok(AdapterConfig::MockBmc),
        Some("smt-cvc5") => {
            let executable = request
                .solver_executable
                .clone()
                .ok_or_else(|| "solver_executable is required when backend=smt-cvc5".to_string())?;
            Ok(AdapterConfig::SmtCvc5 {
                executable,
                args: request.solver_args.clone(),
            })
        }
        Some("command") => {
            let executable = request
                .solver_executable
                .clone()
                .ok_or_else(|| "solver_executable is required when backend=command".to_string())?;
            Ok(AdapterConfig::Command {
                backend_name: "command".to_string(),
                executable,
                args: request.solver_args.clone(),
            })
        }
        Some(other) => Err(format!("unsupported backend `{other}`")),
    }
}

fn backend_config_from_orchestrate_request(
    request: &OrchestrateRequest,
) -> Result<AdapterConfig, String> {
    match request.backend.as_deref() {
        None | Some("explicit") => Ok(AdapterConfig::Explicit),
        Some("mock-bmc") => Ok(AdapterConfig::MockBmc),
        Some("smt-cvc5") => {
            let executable = request
                .solver_executable
                .clone()
                .ok_or_else(|| "solver_executable is required when backend=smt-cvc5".to_string())?;
            Ok(AdapterConfig::SmtCvc5 {
                executable,
                args: request.solver_args.clone(),
            })
        }
        Some("command") => {
            let executable = request
                .solver_executable
                .clone()
                .ok_or_else(|| "solver_executable is required when backend=command".to_string())?;
            Ok(AdapterConfig::Command {
                backend_name: "command".to_string(),
                executable,
                args: request.solver_args.clone(),
            })
        }
        Some(other) => Err(format!("unsupported backend `{other}`")),
    }
}

fn backend_kind_for_config(config: &AdapterConfig) -> crate::engine::BackendKind {
    match config {
        AdapterConfig::Explicit => crate::engine::BackendKind::Explicit,
        AdapterConfig::MockBmc | AdapterConfig::Command { .. } => crate::engine::BackendKind::MockBmc,
        AdapterConfig::SmtCvc5 { .. } => crate::engine::BackendKind::SmtCvc5,
    }
}

fn backend_version_for_config(config: &AdapterConfig) -> String {
    match config {
        AdapterConfig::Command { .. } | AdapterConfig::SmtCvc5 { .. } => "external".to_string(),
        _ => env!("CARGO_PKG_VERSION").to_string(),
    }
}
