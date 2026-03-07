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
        build_counterexample_vector, build_model_test_vectors_for_strategy,
        build_synthetic_witness_vectors, minimize_counterexample_vector,
        write_generated_test_files, MinimizeResult, ReplayTarget,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectResponse {
    pub schema_version: String,
    pub request_id: String,
    pub status: String,
    pub model_id: String,
    pub machine_ir_ready: bool,
    pub machine_ir_error: Option<String>,
    pub capabilities: InspectCapabilities,
    pub state_fields: Vec<String>,
    pub actions: Vec<String>,
    pub properties: Vec<String>,
    pub state_field_details: Vec<InspectStateField>,
    pub action_details: Vec<InspectAction>,
    pub transition_details: Vec<InspectTransition>,
    pub property_details: Vec<InspectProperty>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectCapabilities {
    pub parse_ready: bool,
    pub explicit_ready: bool,
    pub ir_ready: bool,
    pub solver_ready: bool,
    pub coverage_ready: bool,
    pub explain_ready: bool,
    pub testgen_ready: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectStateField {
    pub name: String,
    pub rust_type: String,
    pub range: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectAction {
    pub action_id: String,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectTransition {
    pub action_id: String,
    pub guard: Option<String>,
    pub effect: Option<String>,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub path_tags: Vec<String>,
    pub updates: Vec<InspectTransitionUpdate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectTransitionUpdate {
    pub field: String,
    pub expr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectProperty {
    pub property_id: String,
    pub kind: String,
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
pub struct LintFinding {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintResponse {
    pub schema_version: String,
    pub request_id: String,
    pub status: String,
    pub model_id: String,
    pub capabilities: InspectCapabilities,
    pub findings: Vec<LintFinding>,
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
    pub property_id: Option<String>,
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
        return inspect_bundled_model(&request.request_id, &request.source_name).map_err(
            |message| {
                vec![Diagnostic::new(
                    crate::support::diagnostics::ErrorCode::SearchError,
                    crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                    message,
                )]
            },
        );
    }
    let model = frontend::compile_model(&request.source)?;
    Ok(InspectResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request.request_id.clone(),
        status: "ok".to_string(),
        model_id: model.model_id.clone(),
        machine_ir_ready: true,
        machine_ir_error: None,
        capabilities: InspectCapabilities {
            parse_ready: true,
            explicit_ready: true,
            ir_ready: true,
            solver_ready: true,
            coverage_ready: true,
            explain_ready: true,
            testgen_ready: true,
            reasons: Vec::new(),
        },
        state_fields: model.state_fields.iter().map(|f| f.name.clone()).collect(),
        actions: model.actions.iter().map(|a| a.action_id.clone()).collect(),
        properties: model
            .properties
            .iter()
            .map(|p| p.property_id.clone())
            .collect(),
        state_field_details: model
            .state_fields
            .iter()
            .map(|field| InspectStateField {
                name: field.name.clone(),
                rust_type: match field.ty {
                    crate::ir::FieldType::Bool => "bool".to_string(),
                    crate::ir::FieldType::BoundedU8 { .. } => "u8".to_string(),
                },
                range: match field.ty {
                    crate::ir::FieldType::Bool => None,
                    crate::ir::FieldType::BoundedU8 { min, max } => Some(format!("{min}..={max}")),
                },
            })
            .collect(),
        action_details: model
            .actions
            .iter()
            .map(|action| InspectAction {
                action_id: action.action_id.clone(),
                reads: action.reads.clone(),
                writes: action.writes.clone(),
            })
            .collect(),
        transition_details: model
            .actions
            .iter()
            .map(|action| InspectTransition {
                action_id: action.action_id.clone(),
                guard: Some(render_expr_ir(&action.guard)),
                effect: Some(render_update_effect(&action.updates)),
                reads: action.reads.clone(),
                writes: action.writes.clone(),
                path_tags: action.path_tags.clone(),
                updates: action
                    .updates
                    .iter()
                    .map(|update| InspectTransitionUpdate {
                        field: update.field.clone(),
                        expr: render_expr_ir(&update.value),
                    })
                    .collect(),
            })
            .collect(),
        property_details: model
            .properties
            .iter()
            .map(|property| InspectProperty {
                property_id: property.property_id.clone(),
                kind: format!("{:?}", property.kind),
            })
            .collect(),
    })
}

pub fn compile_source(source: &str) -> Result<ModelIr, Vec<Diagnostic>> {
    frontend::compile_model(source)
}

pub fn capabilities_response(
    request: &CapabilitiesRequest,
) -> Result<CapabilitiesResponse, String> {
    let config =
        match request.backend.as_deref() {
            None | Some("explicit") => AdapterConfig::Explicit,
            Some("mock-bmc") => AdapterConfig::MockBmc,
            Some("smt-cvc5") => AdapterConfig::SmtCvc5 {
                executable: request.solver_executable.clone().ok_or_else(|| {
                    "solver_executable is required when backend=smt-cvc5".to_string()
                })?,
                args: request.solver_args.clone(),
            },
            Some("command") => AdapterConfig::Command {
                backend_name: "command".to_string(),
                executable: request.solver_executable.clone().ok_or_else(|| {
                    "solver_executable is required when backend=command".to_string()
                })?,
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
        let backend = backend_config_from_orchestrate_request(request).map_err(|message| {
            CheckErrorEnvelope {
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
            }
        })?;
        return orchestrate_bundled_model(
            &request.request_id,
            &request.source_name,
            Some(&backend),
        )
        .map_err(|message| CheckErrorEnvelope {
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
        });
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
        let adapter = match backend_config_from_request(request) {
            Ok(adapter) => adapter,
            Err(message) => {
                return CheckOutcome::Errored(CheckErrorEnvelope {
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
            }
        };
        return check_bundled_model(
            &request.request_id,
            &request.source_name,
            request.property_id.as_deref(),
            Some(&adapter),
        )
        .unwrap_or_else(|message| {
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
        });
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
                                action.path_tags.clone(),
                            )
                        })
                })
            });
            let coverage_report = compiled_model
                .as_ref()
                .map(|model| collect_coverage(model, std::slice::from_ref(&trace)));
            let write_overlap = action_metadata
                .as_ref()
                .map(|(_, _, writes, _)| {
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
                if let Some((action_id, reads, writes, path_tags)) = &action_metadata {
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
                    if !path_tags.is_empty() {
                        causes.push(ExplainCandidateCause {
                            kind: "decision_path_tags".to_string(),
                            message: format!(
                                "action {action_id} participates in path tags [{}]",
                                path_tags.join(", ")
                            ),
                        });
                    }
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
        let bundled_adapter_request = CheckRequest {
            request_id: request.request_id.clone(),
            source_name: request.source_name.clone(),
            source: request.source.clone(),
            property_id: request.property_id.clone(),
            backend: request.backend.clone(),
            solver_executable: request.solver_executable.clone(),
            solver_args: request.solver_args.clone(),
        };
        let bundled_adapter = backend_config_from_request(&bundled_adapter_request).ok();
        return testgen_bundled_model(
            &request.request_id,
            &request.source_name,
            request.property_id.as_deref(),
            &request.strategy,
            bundled_adapter.as_ref(),
        )
        .map_err(|message| CheckErrorEnvelope {
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
        });
    }
    let outcome = check_source(&CheckRequest {
        request_id: request.request_id.clone(),
        source_name: request.source_name.clone(),
        source: request.source.clone(),
        property_id: request.property_id.clone(),
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
    let target_property_id = request
        .property_id
        .as_deref()
        .or_else(|| {
            model
                .properties
                .first()
                .map(|property| property.property_id.as_str())
        })
        .unwrap_or("P_SAFE");
    let mut vectors = if request.strategy == "counterexample" {
        traces
            .iter()
            .filter_map(|trace| build_counterexample_vector(trace).ok())
            .collect::<Vec<_>>()
    } else {
        let mut vectors =
            build_model_test_vectors_for_strategy(&model, target_property_id, &request.strategy)
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
        if vectors.is_empty() && matches!(request.strategy.as_str(), "transition" | "witness") {
            vectors = build_synthetic_witness_vectors(&model, target_property_id);
        }
        vectors
    };
    annotate_model_replay_targets(&request.source_name, target_property_id, &mut vectors);
    let generated_files =
        write_generated_test_files(&vectors).map_err(|message| CheckErrorEnvelope {
            manifest: result.manifest.clone(),
            status: crate::engine::ErrorStatus::Error,
            assurance_level: crate::engine::AssuranceLevel::Incomplete,
            diagnostics: vec![Diagnostic::new(
                crate::support::diagnostics::ErrorCode::SearchError,
                crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                message,
            )],
        })?;
    Ok(TestgenResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request.request_id.clone(),
        status: "ok".to_string(),
        vector_ids: vectors
            .iter()
            .map(|vector| vector.vector_id.clone())
            .collect(),
        generated_files,
    })
}

fn annotate_model_replay_targets(
    source_name: &str,
    property_id: &str,
    vectors: &mut [crate::testgen::TestVector],
) {
    for vector in vectors {
        let mut args = vec![
            "replay".to_string(),
            source_name.to_string(),
            format!("--property={}", vector.property_id.as_str()),
        ];
        if let Some(action_id) = &vector.focus_action_id {
            args.push(format!("--focus-action={action_id}"));
        }
        if !vector.actions.is_empty() {
            args.push(format!(
                "--actions={}",
                vector
                    .actions
                    .iter()
                    .map(|step| step.action_id.clone())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        args.push("--json".to_string());
        vector.replay_target = Some(ReplayTarget {
            runner: "valid".to_string(),
            args,
        });
        if vector.property_id.is_empty() {
            vector.property_id = property_id.to_string();
        }
    }
}

pub fn validate_check_request(request: &CheckRequest) -> Result<(), String> {
    require_non_empty(&request.request_id, "request_id")?;
    require_non_empty(&request.source_name, "source_name")?;
    if !is_bundled_model_ref(&request.source_name) {
        require_non_empty(&request.source, "source")?;
    }
    if let Some(property_id) = request.property_id.as_deref() {
        require_non_empty(property_id, "property_id")?;
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
    if response.machine_ir_ready != response.capabilities.ir_ready {
        return Err("machine_ir_ready must match capabilities.ir_ready".to_string());
    }
    require_len_match(
        response.state_fields.len(),
        response.state_field_details.len(),
        "state_fields",
        "state_field_details",
    )?;
    require_len_match(
        response.actions.len(),
        response.action_details.len(),
        "actions",
        "action_details",
    )?;
    require_len_match(
        response.properties.len(),
        response.property_details.len(),
        "properties",
        "property_details",
    )?;
    Ok(())
}

pub fn render_inspect_json(response: &InspectResponse) -> String {
    let mut out = String::from("{");
    out.push_str(&format!(
        "\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"model_id\":\"{}\",\"machine_ir_ready\":{},\"machine_ir_error\":{}",
        escape_json(&response.schema_version),
        escape_json(&response.request_id),
        escape_json(&response.status),
        escape_json(&response.model_id),
        response.machine_ir_ready,
        response
            .machine_ir_error
            .as_ref()
            .map(|error| format!("\"{}\"", escape_json(error)))
            .unwrap_or_else(|| "null".to_string())
    ));
    out.push_str(&format!(
        ",\"capabilities\":{{\"parse_ready\":{},\"explicit_ready\":{},\"ir_ready\":{},\"solver_ready\":{},\"coverage_ready\":{},\"explain_ready\":{},\"testgen_ready\":{},\"reasons\":{}}}",
        response.capabilities.parse_ready,
        response.capabilities.explicit_ready,
        response.capabilities.ir_ready,
        response.capabilities.solver_ready,
        response.capabilities.coverage_ready,
        response.capabilities.explain_ready,
        response.capabilities.testgen_ready,
        render_string_array(&response.capabilities.reasons),
    ));
    out.push_str(",\"state_fields\":[");
    for (index, field) in response.state_fields.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\"", escape_json(field)));
    }
    out.push(']');
    out.push_str(",\"actions\":[");
    for (index, action) in response.actions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\"", escape_json(action)));
    }
    out.push(']');
    out.push_str(",\"properties\":[");
    for (index, property) in response.properties.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\"", escape_json(property)));
    }
    out.push(']');
    out.push_str(",\"state_field_details\":[");
    for (index, field) in response.state_field_details.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"name\":\"{}\",\"rust_type\":\"{}\",\"range\":{}}}",
            escape_json(&field.name),
            escape_json(&field.rust_type),
            field
                .range
                .as_ref()
                .map(|range| format!("\"{}\"", escape_json(range)))
                .unwrap_or_else(|| "null".to_string())
        ));
    }
    out.push(']');
    out.push_str(",\"action_details\":[");
    for (index, action) in response.action_details.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"action_id\":\"{}\",\"reads\":{},\"writes\":{}}}",
            escape_json(&action.action_id),
            render_string_array(&action.reads),
            render_string_array(&action.writes)
        ));
    }
    out.push(']');
    out.push_str(",\"transition_details\":[");
    for (index, transition) in response.transition_details.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"action_id\":\"{}\",\"guard\":{},\"effect\":{},\"reads\":{},\"writes\":{},\"path_tags\":{},\"updates\":[{}]}}",
            escape_json(&transition.action_id),
            transition
                .guard
                .as_ref()
                .map(|guard| format!("\"{}\"", escape_json(guard)))
                .unwrap_or_else(|| "null".to_string()),
            transition
                .effect
                .as_ref()
                .map(|effect| format!("\"{}\"", escape_json(effect)))
                .unwrap_or_else(|| "null".to_string()),
            render_string_array(&transition.reads),
            render_string_array(&transition.writes),
            render_string_array(&transition.path_tags),
            transition
                .updates
                .iter()
                .map(|update| format!(
                    "{{\"field\":\"{}\",\"expr\":\"{}\"}}",
                    escape_json(&update.field),
                    escape_json(&update.expr)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    out.push(']');
    out.push_str(",\"property_details\":[");
    for (index, property) in response.property_details.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"property_id\":\"{}\",\"kind\":\"{}\"}}",
            escape_json(&property.property_id),
            escape_json(&property.kind)
        ));
    }
    out.push_str("]}");
    out
}

pub fn render_inspect_text(response: &InspectResponse) -> String {
    let mut out = String::new();
    out.push_str(&format!("model_id: {}\n", response.model_id));
    out.push_str(&format!(
        "machine_ir_ready: {}\n",
        response.machine_ir_ready
    ));
    if let Some(error) = &response.machine_ir_error {
        out.push_str(&format!("machine_ir_error: {}\n", error));
    }
    out.push_str(&format!(
        "capabilities: parse={} explicit={} ir={} solver={} coverage={} explain={} testgen={}\n",
        response.capabilities.parse_ready,
        response.capabilities.explicit_ready,
        response.capabilities.ir_ready,
        response.capabilities.solver_ready,
        response.capabilities.coverage_ready,
        response.capabilities.explain_ready,
        response.capabilities.testgen_ready,
    ));
    if !response.capabilities.reasons.is_empty() {
        out.push_str(&format!(
            "capability_reasons: {}\n",
            response.capabilities.reasons.join(", ")
        ));
    }
    out.push_str(&format!(
        "state_fields: {}\n",
        response.state_fields.join(", ")
    ));
    out.push_str(&format!("actions: {}\n", response.actions.join(", ")));
    out.push_str(&format!("properties: {}\n", response.properties.join(", ")));
    if !response.state_field_details.is_empty() {
        out.push_str("state_field_details:\n");
        for field in &response.state_field_details {
            out.push_str(&format!(
                "- {}: {}{}\n",
                field.name,
                field.rust_type,
                field
                    .range
                    .as_ref()
                    .map(|range| format!(" range={range}"))
                    .unwrap_or_default()
            ));
        }
    }
    if !response.action_details.is_empty() {
        out.push_str("action_details:\n");
        for action in &response.action_details {
            out.push_str(&format!(
                "- {} reads=[{}] writes=[{}]\n",
                action.action_id,
                action.reads.join(", "),
                action.writes.join(", ")
            ));
        }
    }
    if !response.transition_details.is_empty() {
        out.push_str("transition_details:\n");
        for transition in &response.transition_details {
            out.push_str(&format!(
                "- {} guard={} effect={} path_tags=[{}]\n",
                transition.action_id,
                transition.guard.as_deref().unwrap_or("n/a"),
                transition.effect.as_deref().unwrap_or("n/a"),
                transition.path_tags.join(", ")
            ));
            for update in &transition.updates {
                out.push_str(&format!("  update {} := {}\n", update.field, update.expr));
            }
        }
    }
    out
}

fn render_expr_ir(expr: &crate::ir::ExprIr) -> String {
    match expr {
        crate::ir::ExprIr::Literal(value) => match value {
            crate::ir::Value::Bool(value) => value.to_string(),
            crate::ir::Value::UInt(value) => value.to_string(),
        },
        crate::ir::ExprIr::FieldRef(field) => field.clone(),
        crate::ir::ExprIr::Unary { op, expr } => match op {
            crate::ir::UnaryOp::Not => format!("!({})", render_expr_ir(expr)),
        },
        crate::ir::ExprIr::Binary { op, left, right } => {
            let operator = match op {
                crate::ir::BinaryOp::Add => "+",
                crate::ir::BinaryOp::LessThanOrEqual => "<=",
                crate::ir::BinaryOp::Equal => "==",
                crate::ir::BinaryOp::And => "&&",
                crate::ir::BinaryOp::Or => "||",
            };
            format!(
                "({} {} {})",
                render_expr_ir(left),
                operator,
                render_expr_ir(right)
            )
        }
    }
}

fn render_update_effect(updates: &[crate::ir::UpdateIr]) -> String {
    if updates.is_empty() {
        return "[]".to_string();
    }
    updates
        .iter()
        .map(|update| format!("{} := {}", update.field, render_expr_ir(&update.value)))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn lint_source(request: &InspectRequest) -> Result<LintResponse, Vec<Diagnostic>> {
    let inspect = inspect_source(request)?;
    Ok(lint_from_inspect(&inspect))
}

pub fn lint_from_inspect(inspect: &InspectResponse) -> LintResponse {
    let mut findings = Vec::new();
    for reason in &inspect.capabilities.reasons {
        match reason.as_str() {
            "opaque_step_closure" => findings.push(LintFinding {
                severity: "warn".to_string(),
                code: "opaque_step_closure".to_string(),
                message: "model uses a free-form step closure, so solver lowering is not available".to_string(),
                suggestion: Some(
                    "rewrite critical actions with declarative transitions { ... }".to_string(),
                ),
            }),
            "missing_declarative_transitions" => findings.push(LintFinding {
                severity: "warn".to_string(),
                code: "missing_declarative_transitions".to_string(),
                message: "model does not expose declarative transition descriptors".to_string(),
                suggestion: Some(
                    "add transitions { transition ... } so guard/effect metadata becomes first-class".to_string(),
                ),
            }),
            "unsupported_machine_guard_expr" => findings.push(LintFinding {
                severity: "warn".to_string(),
                code: "unsupported_machine_guard_expr".to_string(),
                message: "one or more guard expressions are outside the current solver-neutral subset".to_string(),
                suggestion: Some(
                    "simplify guards to the current IR subset or extend lowering support".to_string(),
                ),
            }),
            "unsupported_machine_update_expr" => findings.push(LintFinding {
                severity: "warn".to_string(),
                code: "unsupported_machine_update_expr".to_string(),
                message: "one or more transition updates are outside the current solver-neutral subset".to_string(),
                suggestion: Some(
                    "rewrite updates with supported expressions or extend lowering support".to_string(),
                ),
            }),
            "unsupported_machine_property_expr" => findings.push(LintFinding {
                severity: "warn".to_string(),
                code: "unsupported_machine_property_expr".to_string(),
                message: "one or more properties cannot be lowered into the current machine IR".to_string(),
                suggestion: Some(
                    "keep properties within the supported boolean/arithmetic subset for solver runs".to_string(),
                ),
            }),
            other => findings.push(LintFinding {
                severity: "warn".to_string(),
                code: other.to_string(),
                message: format!("model is not fully analysis-ready: {other}"),
                suggestion: None,
            }),
        }
    }
    if inspect
        .action_details
        .iter()
        .any(|action| action.reads.is_empty() && action.writes.is_empty())
    {
        findings.push(LintFinding {
            severity: "info".to_string(),
            code: "missing_action_metadata".to_string(),
            message: "some actions do not declare reads/writes metadata".to_string(),
            suggestion: Some(
                "add reads=[...] and writes=[...] to improve explain, coverage, and testgen"
                    .to_string(),
            ),
        });
    }
    if inspect
        .capabilities
        .reasons
        .iter()
        .any(|reason| reason == "opaque_step_closure")
    {
        for action in &inspect.action_details {
            findings.push(LintFinding {
                severity: "info".to_string(),
                code: "transition_candidate".to_string(),
                message: format!(
                    "action {} is a candidate for declarative transition extraction",
                    action.action_id
                ),
                suggestion: Some(format!(
                    "start with `transition {} when |state| <guard> => [NextState {{ ... }}];` and carry reads=[{}], writes=[{}]",
                    action.action_id,
                    action.reads.join(", "),
                    action.writes.join(", ")
                )),
            });
        }
    }
    if inspect
        .transition_details
        .iter()
        .all(|transition| transition.path_tags == ["transition_path".to_string()])
    {
        findings.push(LintFinding {
            severity: "info".to_string(),
            code: "generic_decision_paths".to_string(),
            message: "decision/path tags are still generic for all transitions".to_string(),
            suggestion: Some(
                "use descriptive action ids and metadata so allow/deny/boundary paths become visible".to_string(),
            ),
        });
    }
    let status = if findings
        .iter()
        .any(|finding| finding.severity == "warn" || finding.severity == "error")
    {
        "warn"
    } else {
        "ok"
    };
    LintResponse {
        schema_version: inspect.schema_version.clone(),
        request_id: inspect.request_id.clone(),
        status: status.to_string(),
        model_id: inspect.model_id.clone(),
        capabilities: inspect.capabilities.clone(),
        findings,
    }
}

pub fn render_lint_json(response: &LintResponse) -> String {
    let findings = response
        .findings
        .iter()
        .map(|finding| {
            format!(
                "{{\"severity\":\"{}\",\"code\":\"{}\",\"message\":\"{}\",\"suggestion\":{}}}",
                escape_json(&finding.severity),
                escape_json(&finding.code),
                escape_json(&finding.message),
                finding
                    .suggestion
                    .as_ref()
                    .map(|value| format!("\"{}\"", escape_json(value)))
                    .unwrap_or_else(|| "null".to_string())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"model_id\":\"{}\",\"capabilities\":{{\"parse_ready\":{},\"explicit_ready\":{},\"ir_ready\":{},\"solver_ready\":{},\"coverage_ready\":{},\"explain_ready\":{},\"testgen_ready\":{},\"reasons\":{}}},\"findings\":[{}]}}",
        escape_json(&response.schema_version),
        escape_json(&response.request_id),
        escape_json(&response.status),
        escape_json(&response.model_id),
        response.capabilities.parse_ready,
        response.capabilities.explicit_ready,
        response.capabilities.ir_ready,
        response.capabilities.solver_ready,
        response.capabilities.coverage_ready,
        response.capabilities.explain_ready,
        response.capabilities.testgen_ready,
        render_string_array(&response.capabilities.reasons),
        findings
    )
}

pub fn render_lint_text(response: &LintResponse) -> String {
    let mut out = String::new();
    out.push_str(&format!("model_id: {}\n", response.model_id));
    out.push_str(&format!("status: {}\n", response.status));
    out.push_str(&format!(
        "capabilities: parse={} explicit={} ir={} solver={} coverage={} explain={} testgen={}\n",
        response.capabilities.parse_ready,
        response.capabilities.explicit_ready,
        response.capabilities.ir_ready,
        response.capabilities.solver_ready,
        response.capabilities.coverage_ready,
        response.capabilities.explain_ready,
        response.capabilities.testgen_ready,
    ));
    if !response.capabilities.reasons.is_empty() {
        out.push_str(&format!(
            "capability_reasons: {}\n",
            response.capabilities.reasons.join(", ")
        ));
    }
    if response.findings.is_empty() {
        out.push_str("findings: none\n");
    } else {
        out.push_str("findings:\n");
        for finding in &response.findings {
            out.push_str(&format!(
                "- [{}] {}: {}{}\n",
                finding.severity,
                finding.code,
                finding.message,
                finding
                    .suggestion
                    .as_ref()
                    .map(|suggestion| format!(" suggestion={suggestion}"))
                    .unwrap_or_default()
            ));
        }
    }
    out
}

fn render_string_array(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!("\"{}\"", escape_json(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn escape_json(input: &str) -> String {
    input
        .chars()
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
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
        "counterexample" | "transition" | "witness" | "guard" | "boundary" | "random" => Ok(()),
        other => Err(format!(
            "strategy must be one of counterexample, transition, witness, guard, boundary, random, got `{other}`"
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
    use std::fs;

    use super::{
        capabilities_response, check_source, explain_source, inspect_source, lint_source,
        minimize_source, orchestrate_source, testgen_source, validate_capabilities_request,
        validate_capabilities_response, validate_check_request, validate_explain_request,
        validate_explain_response, validate_inspect_request, validate_inspect_response,
        validate_minimize_request, validate_minimize_response, validate_orchestrate_response,
        validate_testgen_request, validate_testgen_response, CapabilitiesRequest, CheckRequest,
        InspectRequest, MinimizeRequest, OrchestrateRequest, TestgenRequest,
    };

    fn cleanup_generated_files(paths: &[String]) {
        for path in paths {
            let _ = fs::remove_file(path);
        }
    }
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
        assert!(response.machine_ir_ready);
        assert!(response.capabilities.solver_ready);
        assert!(response.capabilities.reasons.is_empty());
        assert_eq!(response.properties, vec!["P_SAFE"]);
        assert_eq!(response.state_field_details[0].name, "x");
        assert_eq!(
            response.state_field_details[0].range.as_deref(),
            Some("0..=7")
        );
        assert_eq!(response.property_details[0].kind, "Invariant");
        assert!(response.transition_details.is_empty());
        validate_inspect_response(&response).unwrap();
    }

    #[test]
    fn lint_surfaces_step_to_declarative_migration_hints() {
        let request = InspectRequest {
            request_id: "req-lint".to_string(),
            source_name: "rust:counter".to_string(),
            source: String::new(),
        };
        let response = lint_source(&request).unwrap();
        assert_eq!(response.status, "warn");
        assert!(response
            .findings
            .iter()
            .any(|finding| finding.code == "opaque_step_closure"));
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
            property_id: None,
            strategy: "counterexample".to_string(),
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        })
        .unwrap();
        assert_eq!(response.vector_ids.len(), 1);
        validate_testgen_response(&response).unwrap();
        cleanup_generated_files(&response.generated_files);
    }

    #[test]
    fn witness_testgen_can_fallback_to_synthetic_vectors() {
        let source = "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_SAFE:\n  invariant: x <= 7\n";
        let response = testgen_source(&TestgenRequest {
            request_id: "req-witness".to_string(),
            source_name: "a.valid".to_string(),
            source: source.to_string(),
            property_id: None,
            strategy: "witness".to_string(),
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        })
        .unwrap();
        assert!(response.vector_ids.len() >= 1);
        validate_testgen_response(&response).unwrap();
        cleanup_generated_files(&response.generated_files);
    }

    #[test]
    fn guard_testgen_can_emit_vectors() {
        let source = "model A\nstate:\n  x: u8[0..2]\ninit:\n  x = 0\naction Inc:\n  pre: x <= 1\n  post:\n    x = x + 1\nproperty P_SAFE:\n  invariant: x <= 2\n";
        let response = testgen_source(&TestgenRequest {
            request_id: "req-guard".to_string(),
            source_name: "a.valid".to_string(),
            source: source.to_string(),
            property_id: None,
            strategy: "guard".to_string(),
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        })
        .unwrap();
        assert!(!response.vector_ids.is_empty());
        validate_testgen_response(&response).unwrap();
        cleanup_generated_files(&response.generated_files);
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
            property_id: None,
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
        AdapterConfig::MockBmc | AdapterConfig::Command { .. } => {
            crate::engine::BackendKind::MockBmc
        }
        AdapterConfig::SmtCvc5 { .. } => crate::engine::BackendKind::SmtCvc5,
    }
}

fn backend_version_for_config(config: &AdapterConfig) -> String {
    match config {
        AdapterConfig::Command { .. } | AdapterConfig::SmtCvc5 { .. } => "external".to_string(),
        _ => env!("CARGO_PKG_VERSION").to_string(),
    }
}
