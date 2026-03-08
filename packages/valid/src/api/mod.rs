//! Machine-readable API layer for AI and CLI integration.

use std::collections::{BTreeMap, BTreeSet};
use tabled::{
    builder::Builder,
    settings::{style::Style, Alignment, Modify, Padding},
};

use crate::kernel::{eval::eval_expr, guard::evaluate_guard};
use crate::{
    bundled_models::{
        check_bundled_model, explain_bundled_model, inspect_bundled_model, is_bundled_model_ref,
        orchestrate_bundled_model, testgen_bundled_model,
    },
    contract::snapshot_model,
    coverage::{
        collect_coverage, machine_state_from_snapshot, validate_coverage_report, CoverageReport,
    },
    engine::{build_run_manifest, CheckErrorEnvelope, CheckOutcome, PropertySelection, RunPlan},
    frontend,
    ir::{DecisionKind, DecisionOutcome, ModelIr, Path, PropertyKind},
    modeling::CapabilityDetail,
    orchestrator::run_all_properties_with_backend,
    solver::{
        backend_version_for_config as solver_backend_version_for_config, capabilities_for_config,
        run_with_adapter, AdapterConfig, CapabilityMatrix,
    },
    support::{
        diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode},
        hash::stable_hash_hex,
        schema::{require_len_match, require_non_empty, require_schema_version},
    },
    testgen::{
        build_counterexample_vector, build_model_test_vectors_for_strategy,
        build_synthetic_witness_vectors, build_witness_vector, minimize_counterexample_vector,
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
    pub predicates: Vec<String>,
    pub scenarios: Vec<String>,
    pub properties: Vec<String>,
    pub state_field_details: Vec<InspectStateField>,
    pub action_details: Vec<InspectAction>,
    pub predicate_details: Vec<InspectNamedExpr>,
    pub scenario_details: Vec<InspectNamedExpr>,
    pub transition_details: Vec<InspectTransition>,
    pub property_details: Vec<InspectProperty>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectCapabilities {
    pub parse_ready: bool,
    pub parse: CapabilityDetail,
    pub explicit_ready: bool,
    pub explicit: CapabilityDetail,
    pub ir_ready: bool,
    pub ir: CapabilityDetail,
    pub solver_ready: bool,
    pub solver: CapabilityDetail,
    pub coverage_ready: bool,
    pub coverage: CapabilityDetail,
    pub explain_ready: bool,
    pub explain: CapabilityDetail,
    pub testgen_ready: bool,
    pub testgen: CapabilityDetail,
    pub temporal: InspectTemporalCapabilities,
    pub reasons: Vec<String>,
}

impl InspectCapabilities {
    pub fn fully_ready() -> Self {
        Self {
            parse_ready: true,
            parse: CapabilityDetail::ready(),
            explicit_ready: true,
            explicit: CapabilityDetail::ready(),
            ir_ready: true,
            ir: CapabilityDetail::ready(),
            solver_ready: true,
            solver: CapabilityDetail::ready(),
            coverage_ready: true,
            coverage: CapabilityDetail::ready(),
            explain_ready: true,
            explain: CapabilityDetail::ready(),
            testgen_ready: true,
            testgen: CapabilityDetail::ready(),
            temporal: InspectTemporalCapabilities::not_applicable(),
            reasons: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectTemporalCapabilities {
    pub property_ids: Vec<String>,
    pub operators: Vec<String>,
    pub support_level: String,
    pub explicit_status: String,
    pub solver_status: String,
    pub reason: String,
}

impl InspectTemporalCapabilities {
    fn not_applicable() -> Self {
        Self {
            property_ids: Vec::new(),
            operators: Vec::new(),
            support_level: "not_applicable".to_string(),
            explicit_status: "not_applicable".to_string(),
            solver_status: "not_applicable".to_string(),
            reason: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectStateField {
    pub name: String,
    pub rust_type: String,
    pub range: Option<String>,
    pub variants: Vec<String>,
    pub is_set: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectAction {
    pub action_id: String,
    pub role: String,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectTransition {
    pub action_id: String,
    pub role: String,
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
pub struct InspectNamedExpr {
    pub id: String,
    pub expr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectProperty {
    pub property_id: String,
    pub kind: String,
    pub expr: Option<String>,
    pub scope_expr: Option<String>,
    pub action_filter: Option<String>,
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
    pub scenario_id: Option<String>,
    pub seed: Option<u64>,
    pub backend: Option<String>,
    pub solver_executable: Option<String>,
    pub solver_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExplainCandidateCause {
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainFieldDiff {
    pub field: String,
    pub before: crate::ir::Value,
    pub after: crate::ir::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainGuardReview {
    pub decision_id: String,
    pub label: String,
    pub outcome: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainReviewContext {
    pub scenario_id: Option<String>,
    pub scenario_expr: Option<String>,
    pub scenario_match_before: Option<bool>,
    pub scenario_match_after: Option<bool>,
    pub property_scope_expr: Option<String>,
    pub property_scope_match_before: Option<bool>,
    pub property_scope_match_after: Option<bool>,
    pub vacuous: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainRepairTargetHint {
    pub target: String,
    pub reason: String,
    pub priority: String,
    pub action_id: Option<String>,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExplainResponse {
    pub schema_version: String,
    pub request_id: String,
    pub status: String,
    pub evidence_id: String,
    pub property_id: String,
    pub breakpoint_kind: String,
    pub breakpoint_note: Option<String>,
    pub failure_step_index: usize,
    pub failing_action_id: Option<String>,
    pub failing_action_role: Option<String>,
    pub decision_path: Path,
    pub failing_action_reads: Vec<String>,
    pub failing_action_writes: Vec<String>,
    pub failing_action_path_tags: Vec<String>,
    pub changed_fields: Vec<String>,
    pub field_diffs: Vec<ExplainFieldDiff>,
    pub guard_reviews: Vec<ExplainGuardReview>,
    pub write_overlap_fields: Vec<String>,
    pub involved_fields: Vec<String>,
    pub review_context: ExplainReviewContext,
    pub candidate_causes: Vec<ExplainCandidateCause>,
    pub repair_targets: Vec<ExplainRepairTargetHint>,
    pub repair_hints: Vec<String>,
    pub next_steps: Vec<String>,
    pub confidence: f32,
    pub best_practices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintFinding {
    pub category: String,
    pub severity: String,
    pub code: String,
    pub message: String,
    pub suggestion: Option<String>,
    pub snippet: Option<String>,
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
pub struct MigrationSnippet {
    pub code: String,
    pub action: Option<String>,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationCheckResponse {
    pub status: String,
    pub mode: String,
    pub verified_equivalence: bool,
    pub total_action_count: usize,
    pub snippet_action_count: usize,
    pub covered_actions: Vec<String>,
    pub missing_actions: Vec<String>,
    pub reasons: Vec<String>,
    pub next_steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationResponse {
    pub schema_version: String,
    pub request_id: String,
    pub status: String,
    pub model_id: String,
    pub snippets: Vec<MigrationSnippet>,
    pub check: Option<MigrationCheckResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimizeRequest {
    pub request_id: String,
    pub source_name: String,
    pub source: String,
    pub property_id: Option<String>,
    pub seed: Option<u64>,
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
    pub seed: Option<u64>,
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
    pub vectors: Vec<TestgenVectorSummary>,
    pub generated_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestgenVectorSummary {
    pub vector_id: String,
    pub strictness: String,
    pub derivation: String,
    pub source_kind: String,
    pub strategy: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilitiesResponse {
    pub schema_version: String,
    pub request_id: String,
    pub backend: String,
    pub capabilities: CapabilityMatrix,
}

fn collect_temporal_operators(expr: &crate::ir::ExprIr, out: &mut BTreeSet<String>) {
    match expr {
        crate::ir::ExprIr::Literal(_) | crate::ir::ExprIr::FieldRef(_) => {}
        crate::ir::ExprIr::Unary { op, expr } => {
            match op {
                crate::ir::UnaryOp::TemporalAlways => {
                    out.insert("always".to_string());
                }
                crate::ir::UnaryOp::TemporalEventually => {
                    out.insert("eventually".to_string());
                }
                crate::ir::UnaryOp::TemporalNext => {
                    out.insert("next".to_string());
                }
                _ => {}
            }
            collect_temporal_operators(expr, out);
        }
        crate::ir::ExprIr::Binary { op, left, right } => {
            if matches!(op, crate::ir::BinaryOp::TemporalUntil) {
                out.insert("until".to_string());
            }
            collect_temporal_operators(left, out);
            collect_temporal_operators(right, out);
        }
    }
}

fn inspect_temporal_capabilities(model: &ModelIr) -> InspectTemporalCapabilities {
    let temporal_properties = model
        .properties
        .iter()
        .filter(|property| matches!(property.kind, PropertyKind::Temporal))
        .collect::<Vec<_>>();
    if temporal_properties.is_empty() {
        return InspectTemporalCapabilities::not_applicable();
    }

    let mut operators = BTreeSet::new();
    for property in &temporal_properties {
        collect_temporal_operators(&property.expr, &mut operators);
    }

    InspectTemporalCapabilities {
        property_ids: temporal_properties
            .iter()
            .map(|property| property.property_id.clone())
            .collect(),
        operators: operators.into_iter().collect(),
        support_level: "explicit_only".to_string(),
        explicit_status: "complete".to_string(),
        solver_status: "unavailable".to_string(),
        reason: "temporal properties currently require backend=explicit; bounded or unavailable temporal support is reported per backend through the capabilities surface".to_string(),
    }
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
    pub seed: Option<u64>,
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
    let supports_solver = model.properties.iter().all(|property| {
        !matches!(
            property.kind,
            PropertyKind::DeadlockFreedom
                | PropertyKind::Cover
                | PropertyKind::Transition
                | PropertyKind::Temporal
        )
    });
    let capability_reasons = if supports_solver {
        Vec::new()
    } else {
        vec!["explicit_only_property_kind_requires_explicit_backend".to_string()]
    };
    let solver_detail = if supports_solver {
        CapabilityDetail::ready()
    } else {
        CapabilityDetail {
            reason: "one or more selected property kinds are explicit-only today".to_string(),
            migration_hint: Some(
                "use backend=explicit for cover, transition, deadlock_freedom, temporal, or scenario-scoped checks".to_string(),
            ),
            unsupported_features: model
                .properties
                .iter()
                .filter(|property| {
                    matches!(
                        property.kind,
                        PropertyKind::DeadlockFreedom
                            | PropertyKind::Cover
                            | PropertyKind::Transition
                            | PropertyKind::Temporal
                    )
                })
                .map(|property| format!("property {} ({})", property.property_id, property.kind))
                .collect(),
        }
    };
    Ok(InspectResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request.request_id.clone(),
        status: "ok".to_string(),
        model_id: model.model_id.clone(),
        machine_ir_ready: true,
        machine_ir_error: None,
        capabilities: InspectCapabilities {
            solver_ready: supports_solver,
            solver: solver_detail,
            temporal: inspect_temporal_capabilities(&model),
            reasons: capability_reasons,
            ..InspectCapabilities::fully_ready()
        },
        state_fields: model.state_fields.iter().map(|f| f.name.clone()).collect(),
        actions: model.actions.iter().map(|a| a.action_id.clone()).collect(),
        predicates: model
            .predicates
            .iter()
            .map(|predicate| predicate.predicate_id.clone())
            .collect(),
        scenarios: model
            .scenarios
            .iter()
            .map(|scenario| scenario.scenario_id.clone())
            .collect(),
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
                    crate::ir::FieldType::String { .. } => "String".to_string(),
                    crate::ir::FieldType::BoundedU8 { .. } => "u8".to_string(),
                    crate::ir::FieldType::BoundedU16 { .. } => "u16".to_string(),
                    crate::ir::FieldType::BoundedU32 { .. } => "u32".to_string(),
                    crate::ir::FieldType::Enum { .. } => "enum".to_string(),
                    crate::ir::FieldType::EnumSet { .. } => "enum_set".to_string(),
                    crate::ir::FieldType::EnumRelation { .. } => "enum_relation".to_string(),
                    crate::ir::FieldType::EnumMap { .. } => "enum_map".to_string(),
                },
                range: match field.ty {
                    crate::ir::FieldType::Bool => None,
                    crate::ir::FieldType::String { min_len, max_len } => match (min_len, max_len) {
                        (Some(min), Some(max)) => Some(format!("{min}..={max}")),
                        _ => None,
                    },
                    crate::ir::FieldType::BoundedU8 { min, max } => Some(format!("{min}..={max}")),
                    crate::ir::FieldType::BoundedU16 { min, max } => Some(format!("{min}..={max}")),
                    crate::ir::FieldType::BoundedU32 { min, max } => Some(format!("{min}..={max}")),
                    crate::ir::FieldType::Enum { .. } => None,
                    crate::ir::FieldType::EnumSet { .. } => None,
                    crate::ir::FieldType::EnumRelation { .. } => None,
                    crate::ir::FieldType::EnumMap { .. } => None,
                },
                variants: match &field.ty {
                    crate::ir::FieldType::Enum { variants }
                    | crate::ir::FieldType::EnumSet { variants } => variants.clone(),
                    crate::ir::FieldType::EnumRelation {
                        left_variants,
                        right_variants,
                    } => vec![
                        format!("left:{}", left_variants.join("|")),
                        format!("right:{}", right_variants.join("|")),
                    ],
                    crate::ir::FieldType::EnumMap {
                        key_variants,
                        value_variants,
                    } => vec![
                        format!("keys:{}", key_variants.join("|")),
                        format!("values:{}", value_variants.join("|")),
                    ],
                    _ => Vec::new(),
                },
                is_set: matches!(field.ty, crate::ir::FieldType::EnumSet { .. }),
            })
            .collect(),
        action_details: model
            .actions
            .iter()
            .map(|action| InspectAction {
                action_id: action.action_id.clone(),
                role: action.role.as_str().to_string(),
                reads: action.reads.clone(),
                writes: action.writes.clone(),
            })
            .collect(),
        predicate_details: model
            .predicates
            .iter()
            .map(|predicate| InspectNamedExpr {
                id: predicate.predicate_id.clone(),
                expr: render_expr_ir(&predicate.expr),
            })
            .collect(),
        scenario_details: model
            .scenarios
            .iter()
            .map(|scenario| InspectNamedExpr {
                id: scenario.scenario_id.clone(),
                expr: render_expr_ir(&scenario.expr),
            })
            .collect(),
        transition_details: model
            .actions
            .iter()
            .map(|action| InspectTransition {
                action_id: action.action_id.clone(),
                role: action.role.as_str().to_string(),
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
                kind: property_kind_label(&property.kind).to_string(),
                expr: property_expr_for_inspect(property),
                scope_expr: property.scope.as_ref().map(render_expr_ir),
                action_filter: property.action_filter.clone(),
            })
            .collect(),
    })
}

pub fn compile_source(source: &str) -> Result<ModelIr, Vec<Diagnostic>> {
    frontend::compile_model(source)
}

pub(crate) fn property_kind_label(kind: &PropertyKind) -> &'static str {
    kind.as_str()
}

fn property_expr_for_inspect(property: &crate::ir::PropertyIr) -> Option<String> {
    match property.kind {
        PropertyKind::Invariant
        | PropertyKind::Reachability
        | PropertyKind::Temporal
        | PropertyKind::Cover
        | PropertyKind::Transition => Some(render_expr_ir(&property.expr)),
        PropertyKind::DeadlockFreedom => None,
    }
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
            Some("sat-varisat") => AdapterConfig::SatVarisat,
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
                manifest: build_run_manifest(
                    request.request_id.clone(),
                    format!(
                        "run-{}",
                        stable_hash_hex(&request.request_id).replace("sha256:", "")
                    ),
                    stable_hash_hex(&request.source_name),
                    stable_hash_hex(&request.source_name),
                    crate::engine::BackendKind::Explicit,
                    env!("CARGO_PKG_VERSION").to_string(),
                    request.seed,
                ),
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
            request.seed,
            Some(&backend),
        )
        .map_err(|message| CheckErrorEnvelope {
            manifest: build_run_manifest(
                request.request_id.clone(),
                format!(
                    "run-{}",
                    stable_hash_hex(&request.request_id).replace("sha256:", "")
                ),
                stable_hash_hex(&request.source_name),
                stable_hash_hex(&request.source_name),
                crate::engine::BackendKind::Explicit,
                env!("CARGO_PKG_VERSION").to_string(),
                request.seed,
            ),
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
            manifest: build_run_manifest(
                request.request_id.clone(),
                format!(
                    "run-{}",
                    stable_hash_hex(&request.request_id).replace("sha256:", "")
                ),
                stable_hash_hex(&request.source),
                "sha256:unknown".to_string(),
                backend_kind_for_config(&backend_fallback),
                backend_version_for_config(&backend_fallback),
                request.seed,
            ),
            status: crate::engine::ErrorStatus::Error,
            assurance_level: crate::engine::AssuranceLevel::Incomplete,
            diagnostics,
        })?;
    let snapshot = snapshot_model(&model);
    let backend =
        backend_config_from_orchestrate_request(request).map_err(|message| CheckErrorEnvelope {
            manifest: build_run_manifest(
                request.request_id.clone(),
                format!(
                    "run-{}",
                    stable_hash_hex(&request.request_id).replace("sha256:", "")
                ),
                stable_hash_hex(&request.source),
                snapshot.contract_hash.clone(),
                crate::engine::BackendKind::Explicit,
                env!("CARGO_PKG_VERSION").to_string(),
                request.seed,
            ),
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
                    manifest: build_run_manifest(
                        request.request_id.clone(),
                        format!(
                            "run-{}",
                            stable_hash_hex(&request.request_id).replace("sha256:", "")
                        ),
                        stable_hash_hex(&request.source_name),
                        stable_hash_hex(&request.source_name),
                        crate::engine::BackendKind::Explicit,
                        env!("CARGO_PKG_VERSION").to_string(),
                        request.seed,
                    ),
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
            request.seed,
            Some(&adapter),
        )
        .unwrap_or_else(|message| {
            CheckOutcome::Errored(CheckErrorEnvelope {
                manifest: build_run_manifest(
                    request.request_id.clone(),
                    format!(
                        "run-{}",
                        stable_hash_hex(&request.request_id).replace("sha256:", "")
                    ),
                    stable_hash_hex(&request.source_name),
                    stable_hash_hex(&request.source_name),
                    crate::engine::BackendKind::Explicit,
                    env!("CARGO_PKG_VERSION").to_string(),
                    request.seed,
                ),
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
            plan.manifest = build_run_manifest(
                request.request_id.clone(),
                format!(
                    "run-{}",
                    stable_hash_hex(&(request.request_id.clone() + &property_id))
                        .replace("sha256:", "")
                ),
                source_hash,
                snapshot.contract_hash,
                backend_kind_for_config(&adapter),
                backend_version_for_config(&adapter),
                request.seed,
            );
            plan.property_selection = PropertySelection::ExactlyOne(property_id);
            plan.scenario_selection = request.scenario_id.clone();
            let selected_property = model
                .properties
                .iter()
                .find(|property| {
                    property.property_id
                        == *match &plan.property_selection {
                            PropertySelection::ExactlyOne(id) => id,
                        }
                })
                .expect("selected property exists");
            let requires_explicit = matches!(
                selected_property.kind,
                PropertyKind::DeadlockFreedom | PropertyKind::Cover | PropertyKind::Transition
            ) || request.scenario_id.is_some();
            if requires_explicit && !matches!(adapter, AdapterConfig::Explicit) {
                return CheckOutcome::Errored(CheckErrorEnvelope {
                    manifest: plan.manifest.clone(),
                    status: crate::engine::ErrorStatus::Error,
                    assurance_level: crate::engine::AssuranceLevel::Incomplete,
                    diagnostics: vec![Diagnostic::new(
                        crate::support::diagnostics::ErrorCode::SearchError,
                        crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                        "selected property/scenario currently requires backend=explicit",
                    )],
                });
            }
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
            manifest: build_run_manifest(
                request.request_id.clone(),
                format!(
                    "run-{}",
                    stable_hash_hex(&request.request_id).replace("sha256:", "")
                ),
                source_hash,
                "sha256:unknown".to_string(),
                backend_kind_for_config(&adapter),
                backend_version_for_config(&adapter),
                request.seed,
            ),
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
                manifest: build_run_manifest(
                    request.request_id.clone(),
                    format!(
                        "run-{}",
                        stable_hash_hex(&request.request_id).replace("sha256:", "")
                    ),
                    stable_hash_hex(&request.source_name),
                    stable_hash_hex(&request.source_name),
                    crate::engine::BackendKind::Explicit,
                    env!("CARGO_PKG_VERSION").to_string(),
                    request.seed,
                ),
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
            let selected_property = compiled_model.as_ref().and_then(|model| {
                model
                    .properties
                    .iter()
                    .find(|property| property.property_id == trace.property_id)
            });
            let changed_fields =
                state_field_diffs(&failure_step.state_before, &failure_step.state_after)
                    .into_iter()
                    .map(|diff| diff.field)
                    .collect::<Vec<_>>();
            let action_metadata = compiled_model.as_ref().and_then(|model| {
                let state = machine_state_from_snapshot(model, &failure_step.state_before)?;
                failure_step.action_id.as_ref().map(|action_id| {
                    let mut roles = std::collections::BTreeSet::new();
                    let mut reads = std::collections::BTreeSet::new();
                    let mut writes = std::collections::BTreeSet::new();
                    let traced_path = failure_step.path.clone();
                    let mut decision_path = failure_step.path.clone().unwrap_or_default();
                    for action in model
                        .actions
                        .iter()
                        .filter(|action| &action.action_id == action_id)
                    {
                        if matches!(evaluate_guard(model, &state, action), Ok(true)) {
                            roles.insert(action.role.as_str().to_string());
                            reads.extend(action.reads.iter().cloned());
                            writes.extend(action.writes.iter().cloned());
                            if traced_path.is_none() {
                                decision_path.extend(action.decision_path());
                            }
                        }
                    }
                    (
                        roles
                            .into_iter()
                            .next()
                            .unwrap_or_else(|| "business".to_string()),
                        action_id.clone(),
                        reads.into_iter().collect::<Vec<_>>(),
                        writes.into_iter().collect::<Vec<_>>(),
                        decision_path,
                    )
                })
            });
            let field_diffs =
                state_field_diffs(&failure_step.state_before, &failure_step.state_after);
            let guard_reviews = explain_guard_reviews(
                action_metadata
                    .as_ref()
                    .map(|(_, _, _, _, path)| path)
                    .or(failure_step.path.as_ref()),
            );
            let coverage_report = compiled_model
                .as_ref()
                .map(|model| collect_coverage(model, std::slice::from_ref(&trace)));
            let property_kind = selected_property
                .map(|property| property.kind)
                .unwrap_or(crate::ir::PropertyKind::Invariant);
            let review_context = build_review_context(
                compiled_model.as_ref(),
                selected_property,
                request.scenario_id.as_deref(),
                &failure_step.state_before,
                &failure_step.state_after,
                result.property_result.vacuous,
            );
            let write_overlap = action_metadata
                .as_ref()
                .map(|(_, _, _, writes, _)| {
                    changed_fields
                        .iter()
                        .filter(|field| writes.contains(*field))
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let involved_fields = merged_involved_fields(
                &changed_fields,
                action_metadata
                    .as_ref()
                    .map(|(_, _, reads, _, _)| reads.as_slice())
                    .unwrap_or(&[]),
                action_metadata
                    .as_ref()
                    .map(|(_, _, _, writes, _)| writes.as_slice())
                    .unwrap_or(&[]),
            );
            let breakpoint_kind = explain_breakpoint_kind(failure_step);
            let candidate_causes = if changed_fields.is_empty() {
                vec![
                    ExplainCandidateCause {
                        kind: "terminal_violation".to_string(),
                        message: format!(
                            "{} property {} reached its terminal condition without a field diff",
                            property_kind, trace.property_id
                        ),
                    },
                    ExplainCandidateCause {
                        kind: "action_semantics".to_string(),
                        message: failure_step
                            .action_id
                            .as_ref()
                            .map(|action| match property_kind {
                                crate::ir::PropertyKind::Invariant => {
                                    format!("{action} reached a violating state without a visible field delta")
                                }
                                crate::ir::PropertyKind::Reachability => {
                                    format!("{action} reached the target state without a visible field delta")
                                }
                                crate::ir::PropertyKind::Cover => {
                                    format!("{action} reached the cover target without a visible field delta")
                                }
                                crate::ir::PropertyKind::Transition => {
                                    format!("{action} produced a transition postcondition violation without a visible field delta")
                                }
                                crate::ir::PropertyKind::DeadlockFreedom => {
                                    format!("{action} led to a deadlocked state without a visible field delta")
                                }
                                crate::ir::PropertyKind::Temporal => {
                                    format!("{action} contributed to a temporal property violation without a visible field delta")
                                }
                            })
                            .unwrap_or_else(|| match property_kind {
                                crate::ir::PropertyKind::Invariant => {
                                    "initial or terminal condition violated the property".to_string()
                                }
                                crate::ir::PropertyKind::Reachability => {
                                    "initial or terminal condition satisfied the reachability target".to_string()
                                }
                                crate::ir::PropertyKind::Cover => {
                                    "initial or terminal condition satisfied the cover target".to_string()
                                }
                                crate::ir::PropertyKind::Transition => {
                                    "the selected transition violated its postcondition".to_string()
                                }
                                crate::ir::PropertyKind::DeadlockFreedom => {
                                    "deadlock detected: no enabled actions from this state".to_string()
                                }
                                crate::ir::PropertyKind::Temporal => {
                                    "temporal property violated on the explored reachable graph".to_string()
                                }
                            }),
                    },
                ]
            } else {
                let mut causes = Vec::new();
                if let Some((_, action_id, reads, writes, decision_path)) = &action_metadata {
                    let path_tags = decision_path.legacy_path_tags();
                    if property_kind == crate::ir::PropertyKind::Reachability {
                        causes.push(ExplainCandidateCause {
                            kind: "reachability_target".to_string(),
                            message: format!(
                                "action {action_id} reached the target state at step {}",
                                failure_step.index
                            ),
                        });
                    }
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
                causes.extend(changed_fields.iter().map(|field| ExplainCandidateCause {
                    kind: "field_flip".to_string(),
                    message: format!("{field} changed at step {}", failure_step.index),
                }));
                if !guard_reviews.is_empty() {
                    causes.push(ExplainCandidateCause {
                        kind: "guard_review".to_string(),
                        message: format!(
                            "review {} relevant guard decision(s) at the breakpoint",
                            guard_reviews.len()
                        ),
                    });
                }
                causes
            };
            let mut repair_hints = vec![
                "review the guard and update set of the failing action".to_string(),
                format!(
                    "verify {} property {} is intended",
                    property_kind, trace.property_id
                ),
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
            if review_context.vacuous {
                repair_hints.push(
                    "check whether scenario selection or property scope made the failure vacuous"
                        .to_string(),
                );
            }
            let repair_targets = build_repair_targets(
                property_kind,
                failure_step.action_id.as_deref(),
                &changed_fields,
                &write_overlap,
                &review_context,
            );
            let next_steps = vec![
                "run explain in text mode for a reviewer-friendly narrative".to_string(),
                "inspect the graph to review guard and update structure".to_string(),
                "generate a regression test from the failing path".to_string(),
            ];
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
                breakpoint_kind: breakpoint_kind.to_string(),
                breakpoint_note: failure_step.note.clone(),
                failure_step_index: failure_step.index,
                failing_action_id: failure_step.action_id.clone(),
                failing_action_role: action_metadata
                    .as_ref()
                    .map(|(role, _, _, _, _)| role.clone()),
                decision_path: action_metadata
                    .as_ref()
                    .map(|(_, _, _, _, path)| path.clone())
                    .or_else(|| failure_step.path.clone())
                    .unwrap_or_default(),
                failing_action_reads: action_metadata
                    .as_ref()
                    .map(|(_, _, reads, _, _)| reads.clone())
                    .unwrap_or_default(),
                failing_action_writes: action_metadata
                    .as_ref()
                    .map(|(_, _, _, writes, _)| writes.clone())
                    .unwrap_or_default(),
                failing_action_path_tags: action_metadata
                    .as_ref()
                    .map(|(_, _, _, _, path)| path.legacy_path_tags())
                    .unwrap_or_default(),
                changed_fields,
                field_diffs,
                guard_reviews,
                write_overlap_fields: write_overlap.clone(),
                involved_fields,
                review_context,
                candidate_causes,
                repair_targets,
                repair_hints,
                next_steps,
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
            manifest: build_run_manifest(
                request.request_id.clone(),
                format!(
                    "run-{}",
                    stable_hash_hex(&request.request_id).replace("sha256:", "")
                ),
                stable_hash_hex(&request.source),
                "sha256:unknown".to_string(),
                crate::engine::BackendKind::Explicit,
                env!("CARGO_PKG_VERSION").to_string(),
                request.seed,
            ),
            status: crate::engine::ErrorStatus::Error,
            assurance_level: crate::engine::AssuranceLevel::Incomplete,
            diagnostics,
        })?;
    let check = check_source(&CheckRequest {
        request_id: request.request_id.clone(),
        source_name: request.source_name.clone(),
        source: request.source.clone(),
        property_id,
        scenario_id: None,
        seed: request.seed,
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
            scenario_id: None,
            seed: request.seed,
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
            request.seed,
            bundled_adapter.as_ref(),
        )
        .map_err(|message| CheckErrorEnvelope {
            manifest: build_run_manifest(
                request.request_id.clone(),
                format!(
                    "run-{}",
                    stable_hash_hex(&request.request_id).replace("sha256:", "")
                ),
                stable_hash_hex(&request.source_name),
                stable_hash_hex(&request.source_name),
                crate::engine::BackendKind::Explicit,
                env!("CARGO_PKG_VERSION").to_string(),
                request.seed,
            ),
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
        scenario_id: None,
        seed: request.seed,
        backend: request.backend.clone(),
        solver_executable: request.solver_executable.clone(),
        solver_args: request.solver_args.clone(),
    });
    let model =
        frontend::compile_model(&request.source).map_err(|diagnostics| CheckErrorEnvelope {
            manifest: build_run_manifest(
                request.request_id.clone(),
                format!(
                    "run-{}",
                    stable_hash_hex(&request.request_id).replace("sha256:", "")
                ),
                stable_hash_hex(&request.source),
                "sha256:unknown".to_string(),
                crate::engine::BackendKind::Explicit,
                env!("CARGO_PKG_VERSION").to_string(),
                request.seed,
            ),
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
    } else if request.strategy == "witness" {
        let trace_vectors = traces
            .iter()
            .filter_map(|trace| build_witness_vector(trace).ok())
            .collect::<Vec<_>>();
        if trace_vectors.is_empty() {
            let mut vectors = build_model_test_vectors_for_strategy(
                &model,
                target_property_id,
                &request.strategy,
            )
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
            if vectors.is_empty() {
                vectors = build_synthetic_witness_vectors(&model, target_property_id);
            }
            vectors
        } else {
            trace_vectors
        }
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
        vectors: vectors
            .iter()
            .map(|vector| TestgenVectorSummary {
                vector_id: vector.vector_id.clone(),
                strictness: vector.strictness.clone(),
                derivation: vector.derivation.clone(),
                source_kind: vector.source_kind.clone(),
                strategy: vector.strategy.clone(),
            })
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
    if let Some(scenario_id) = request.scenario_id.as_deref() {
        require_non_empty(scenario_id, "scenario_id")?;
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
    validate_capability_detail(
        "parse",
        response.capabilities.parse_ready,
        &response.capabilities.parse,
    )?;
    validate_capability_detail(
        "explicit",
        response.capabilities.explicit_ready,
        &response.capabilities.explicit,
    )?;
    validate_capability_detail(
        "ir",
        response.capabilities.ir_ready,
        &response.capabilities.ir,
    )?;
    validate_capability_detail(
        "solver",
        response.capabilities.solver_ready,
        &response.capabilities.solver,
    )?;
    validate_capability_detail(
        "coverage",
        response.capabilities.coverage_ready,
        &response.capabilities.coverage,
    )?;
    validate_capability_detail(
        "explain",
        response.capabilities.explain_ready,
        &response.capabilities.explain,
    )?;
    validate_capability_detail(
        "testgen",
        response.capabilities.testgen_ready,
        &response.capabilities.testgen,
    )?;
    require_non_empty(
        &response.capabilities.temporal.support_level,
        "capabilities.temporal.support_level",
    )?;
    require_non_empty(
        &response.capabilities.temporal.explicit_status,
        "capabilities.temporal.explicit_status",
    )?;
    require_non_empty(
        &response.capabilities.temporal.solver_status,
        "capabilities.temporal.solver_status",
    )?;
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
        response.predicates.len(),
        response.predicate_details.len(),
        "predicates",
        "predicate_details",
    )?;
    require_len_match(
        response.scenarios.len(),
        response.scenario_details.len(),
        "scenarios",
        "scenario_details",
    )?;
    require_len_match(
        response.properties.len(),
        response.property_details.len(),
        "properties",
        "property_details",
    )?;
    Ok(())
}

fn validate_capability_detail(
    name: &str,
    ready: bool,
    detail: &CapabilityDetail,
) -> Result<(), String> {
    if !ready && detail.reason.is_empty() {
        return Err(format!(
            "capabilities.{name}.reason must be non-empty when {name}_ready is false"
        ));
    }
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
    out.push_str(",\"capabilities\":");
    out.push_str(&render_capabilities_json(&response.capabilities));
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
    out.push_str(",\"predicates\":[");
    for (index, predicate) in response.predicates.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\"", escape_json(predicate)));
    }
    out.push(']');
    out.push_str(",\"scenarios\":[");
    for (index, scenario) in response.scenarios.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\"", escape_json(scenario)));
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
            "{{\"name\":\"{}\",\"rust_type\":\"{}\",\"range\":{},\"variants\":{},\"is_set\":{}}}",
            escape_json(&field.name),
            escape_json(&field.rust_type),
            field
                .range
                .as_ref()
                .map(|range| format!("\"{}\"", escape_json(range)))
                .unwrap_or_else(|| "null".to_string()),
            render_string_array(&field.variants),
            field.is_set
        ));
    }
    out.push(']');
    out.push_str(",\"action_details\":[");
    for (index, action) in response.action_details.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"action_id\":\"{}\",\"role\":\"{}\",\"reads\":{},\"writes\":{}}}",
            escape_json(&action.action_id),
            escape_json(&action.role),
            render_string_array(&action.reads),
            render_string_array(&action.writes)
        ));
    }
    out.push(']');
    out.push_str(",\"predicate_details\":[");
    for (index, predicate) in response.predicate_details.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"id\":\"{}\",\"expr\":\"{}\"}}",
            escape_json(&predicate.id),
            escape_json(&predicate.expr)
        ));
    }
    out.push(']');
    out.push_str(",\"scenario_details\":[");
    for (index, scenario) in response.scenario_details.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"id\":\"{}\",\"expr\":\"{}\"}}",
            escape_json(&scenario.id),
            escape_json(&scenario.expr)
        ));
    }
    out.push(']');
    out.push_str(",\"transition_details\":[");
    for (index, transition) in response.transition_details.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"action_id\":\"{}\",\"role\":\"{}\",\"guard\":{},\"effect\":{},\"reads\":{},\"writes\":{},\"path_tags\":{},\"updates\":[{}]}}",
            escape_json(&transition.action_id),
            escape_json(&transition.role),
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
            "{{\"property_id\":\"{}\",\"kind\":\"{}\",\"expr\":{},\"scope_expr\":{},\"action_filter\":{}}}",
            escape_json(&property.property_id),
            escape_json(&property.kind),
            property
                .expr
                .as_ref()
                .map(|expr| format!("\"{}\"", escape_json(expr)))
                .unwrap_or_else(|| "null".to_string()),
            property
                .scope_expr
                .as_ref()
                .map(|expr| format!("\"{}\"", escape_json(expr)))
                .unwrap_or_else(|| "null".to_string()),
            property
                .action_filter
                .as_ref()
                .map(|action| format!("\"{}\"", escape_json(action)))
                .unwrap_or_else(|| "null".to_string())
        ));
    }
    out.push_str("]}");
    out
}

pub fn render_inspect_text(response: &InspectResponse) -> String {
    let mut out = String::new();
    out.push_str(&format!("model: {}\n", response.model_id));
    out.push_str("readiness:\n");
    out.push_str(&format!(
        "- machine_ir_ready: {}\n",
        response.machine_ir_ready
    ));
    if let Some(error) = &response.machine_ir_error {
        out.push_str(&format!(
            "  machine_ir_error:\n{}\n",
            indent_block(error, 4)
        ));
    }
    out.push_str(&format!(
        "- capabilities: parse={} explicit={} ir={} solver={} coverage={} explain={} testgen={}\n",
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
            "- capability_reasons: {}\n",
            response.capabilities.reasons.join(", ")
        ));
    }
    out.push_str(&render_capability_details_text(&response.capabilities));
    out.push_str("summary:\n");
    out.push_str(&format!(
        "- state_fields ({}): {}\n",
        response.state_fields.len(),
        render_csv_or_none(&response.state_fields)
    ));
    out.push_str(&format!(
        "- actions ({}): {}\n",
        response.actions.len(),
        render_csv_or_none(&response.actions)
    ));
    out.push_str(&format!(
        "- predicates ({}): {}\n",
        response.predicates.len(),
        render_csv_or_none(&response.predicates)
    ));
    out.push_str(&format!(
        "- scenarios ({}): {}\n",
        response.scenarios.len(),
        render_csv_or_none(&response.scenarios)
    ));
    out.push_str(&format!(
        "- properties ({}): {}\n",
        response.properties.len(),
        render_csv_or_none(&response.properties)
    ));
    if !response.state_field_details.is_empty() {
        out.push_str("state_fields:\n");
        let rows = response
            .state_field_details
            .iter()
            .map(|field| {
                vec![
                    field.name.clone(),
                    field.rust_type.clone(),
                    field.range.clone().unwrap_or_else(|| "-".to_string()),
                    if field.is_set {
                        "set".to_string()
                    } else {
                        "-".to_string()
                    },
                    if field.variants.is_empty() {
                        "-".to_string()
                    } else {
                        field.variants.join(", ")
                    },
                ]
            })
            .collect::<Vec<_>>();
        out.push_str(&indent_block(
            &render_text_table(&["name", "type", "range", "shape", "variants"], &rows),
            2,
        ));
        out.push('\n');
    }
    if !response.action_details.is_empty() {
        out.push_str("actions:\n");
        let rows = response
            .action_details
            .iter()
            .map(|action| {
                vec![
                    action.action_id.clone(),
                    action.role.clone(),
                    render_csv_or_none(&action.reads),
                    render_csv_or_none(&action.writes),
                ]
            })
            .collect::<Vec<_>>();
        out.push_str(&indent_block(
            &render_text_table(&["action", "role", "reads", "writes"], &rows),
            2,
        ));
        out.push('\n');
    }
    if !response.predicate_details.is_empty() {
        out.push_str("predicates:\n");
        for predicate in &response.predicate_details {
            out.push_str(&format!("- {}\n", predicate.id));
            out.push_str(&format!(
                "  expr:\n{}\n",
                indent_block(&pretty_expr(&predicate.expr), 4)
            ));
        }
    }
    if !response.scenario_details.is_empty() {
        out.push_str("scenarios:\n");
        for scenario in &response.scenario_details {
            out.push_str(&format!("- {}\n", scenario.id));
            out.push_str(&format!(
                "  expr:\n{}\n",
                indent_block(&pretty_expr(&scenario.expr), 4)
            ));
        }
    }
    if !response.transition_details.is_empty() {
        out.push_str("transitions:\n");
        for transition in &response.transition_details {
            out.push_str(&format!("- {}\n", transition.action_id));
            out.push_str(&format!("  role: {}\n", transition.role));
            if let Some(guard) = &transition.guard {
                out.push_str(&format!(
                    "  guard:\n{}\n",
                    indent_block(&pretty_expr(guard), 4)
                ));
            }
            if !transition.updates.is_empty() {
                out.push_str("  updates:\n");
            } else if let Some(effect) = &transition.effect {
                out.push_str(&format!(
                    "  effect:\n{}\n",
                    indent_block(&pretty_expr(effect), 4)
                ));
            }
            for update in &transition.updates {
                out.push_str(&format!(
                    "    - {} :=\n{}\n",
                    update.field,
                    indent_block(&pretty_expr(&update.expr), 8)
                ));
            }
            if !transition.reads.is_empty() {
                out.push_str(&format!(
                    "  reads: {}\n",
                    render_csv_or_none(&transition.reads)
                ));
            }
            if !transition.writes.is_empty() {
                out.push_str(&format!(
                    "  writes: {}\n",
                    render_csv_or_none(&transition.writes)
                ));
            }
            if !transition.path_tags.is_empty() {
                out.push_str(&format!(
                    "  path_tags: {}\n",
                    transition.path_tags.join(", ")
                ));
            }
        }
    }
    if !response.property_details.is_empty() {
        out.push_str("properties:\n");
        for property in &response.property_details {
            out.push_str(&format!("- {} ({})\n", property.property_id, property.kind));
            if let Some(expr) = &property.expr {
                out.push_str(&format!(
                    "  expr:\n{}\n",
                    indent_block(&pretty_expr(expr), 4)
                ));
            }
            if let Some(scope_expr) = &property.scope_expr {
                out.push_str(&format!(
                    "  scope:\n{}\n",
                    indent_block(&pretty_expr(scope_expr), 4)
                ));
            }
            if let Some(action_filter) = &property.action_filter {
                out.push_str(&format!("  on_action: {}\n", action_filter));
            }
        }
    }
    out
}

pub fn render_explain_json(response: &ExplainResponse) -> String {
    format!(
        "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"evidence_id\":\"{}\",\"property_id\":\"{}\",\"breakpoint_kind\":\"{}\",\"breakpoint_note\":{},\"failure_step_index\":{},\"failing_action_id\":{},\"failing_action_role\":{},\"decision_path\":{},\"failing_action_reads\":{},\"failing_action_writes\":{},\"failing_action_path_tags\":{},\"changed_fields\":{},\"field_diffs\":[{}],\"guard_reviews\":[{}],\"write_overlap_fields\":{},\"involved_fields\":{},\"review_context\":{},\"candidate_causes\":[{}],\"repair_targets\":[{}],\"repair_hints\":{},\"next_steps\":{},\"confidence\":{},\"best_practices\":{},\"review_summary\":{{\"headline\":\"{}\",\"review_level\":\"{}\"}}}}",
        escape_json(&response.schema_version),
        escape_json(&response.request_id),
        escape_json(&response.status),
        escape_json(&response.evidence_id),
        escape_json(&response.property_id),
        escape_json(&response.breakpoint_kind),
        render_optional_string(response.breakpoint_note.as_deref()),
        response.failure_step_index,
        response
            .failing_action_id
            .as_ref()
            .map(|value| format!("\"{}\"", escape_json(value)))
            .unwrap_or_else(|| "null".to_string()),
        response
            .failing_action_role
            .as_ref()
            .map(|value| format!("\"{}\"", escape_json(value)))
            .unwrap_or_else(|| "null".to_string()),
        render_path_json(&response.decision_path),
        render_string_array(&response.failing_action_reads),
        render_string_array(&response.failing_action_writes),
        render_string_array(&response.failing_action_path_tags),
        render_string_array(&response.changed_fields),
        response
            .field_diffs
            .iter()
            .map(|diff| format!(
                "{{\"field\":\"{}\",\"before\":{},\"after\":{}}}",
                escape_json(&diff.field),
                render_value_json(&diff.before),
                render_value_json(&diff.after)
            ))
            .collect::<Vec<_>>()
            .join(","),
        response
            .guard_reviews
            .iter()
            .map(|guard| format!(
                "{{\"decision_id\":\"{}\",\"label\":\"{}\",\"outcome\":\"{}\"}}",
                escape_json(&guard.decision_id),
                escape_json(&guard.label),
                escape_json(&guard.outcome)
            ))
            .collect::<Vec<_>>()
            .join(","),
        render_string_array(&response.write_overlap_fields),
        render_string_array(&response.involved_fields),
        render_review_context_json(&response.review_context),
        response
            .candidate_causes
            .iter()
            .map(|cause| format!(
                "{{\"kind\":\"{}\",\"message\":\"{}\"}}",
                escape_json(&cause.kind),
                escape_json(&cause.message)
            ))
            .collect::<Vec<_>>()
            .join(","),
        response
            .repair_targets
            .iter()
            .map(|target| format!(
                "{{\"target\":\"{}\",\"reason\":\"{}\",\"priority\":\"{}\",\"action_id\":{},\"fields\":{}}}",
                escape_json(&target.target),
                escape_json(&target.reason),
                escape_json(&target.priority),
                render_optional_string(target.action_id.as_deref()),
                render_string_array(&target.fields)
            ))
            .collect::<Vec<_>>()
            .join(","),
        render_string_array(&response.repair_hints),
        render_string_array(&response.next_steps),
        response.confidence,
        render_string_array(&response.best_practices),
        escape_json(&format!(
            "{} at step {} for property {}",
            response.breakpoint_kind,
            response.failure_step_index,
            response.property_id
        )),
        if response.confidence >= 0.8 {
            "high"
        } else if response.confidence >= 0.6 {
            "medium"
        } else {
            "low"
        }
    )
}

pub fn render_explain_text(response: &ExplainResponse) -> String {
    let mut out = String::new();
    out.push_str(&format!("property_id: {}\n", response.property_id));
    out.push_str(&format!("evidence_id: {}\n", response.evidence_id));
    out.push_str(&format!("breakpoint_kind: {}\n", response.breakpoint_kind));
    if let Some(note) = &response.breakpoint_note {
        out.push_str(&format!("breakpoint_note: {}\n", note));
    }
    out.push_str(&format!(
        "failure_step_index: {}\n",
        response.failure_step_index
    ));
    if let Some(action_id) = &response.failing_action_id {
        out.push_str(&format!("failing_action_id: {}\n", action_id));
    }
    if let Some(role) = &response.failing_action_role {
        out.push_str(&format!("failing_action_role: {}\n", role));
    }
    if !response.decision_path.decisions.is_empty() {
        out.push_str("decision_path:\n");
        for decision in &response.decision_path.decisions {
            out.push_str(&format!(
                "- {} [{}] {}\n",
                decision.point.decision_id,
                match decision.outcome {
                    DecisionOutcome::GuardTrue => "guard_true",
                    DecisionOutcome::GuardFalse => "guard_false",
                    DecisionOutcome::UpdateApplied => "update_applied",
                },
                decision.point.label
            ));
        }
    }
    if !response.failing_action_reads.is_empty() || !response.failing_action_writes.is_empty() {
        out.push_str(&format!(
            "failing_action_io: reads=[{}] writes=[{}]\n",
            response.failing_action_reads.join(", "),
            response.failing_action_writes.join(", ")
        ));
    }
    if !response.changed_fields.is_empty() {
        out.push_str(&format!(
            "changed_fields: {}\n",
            response.changed_fields.join(", ")
        ));
    }
    if !response.field_diffs.is_empty() {
        out.push_str("field_diffs:\n");
        for diff in &response.field_diffs {
            out.push_str(&format!(
                "- {}: {} -> {}\n",
                diff.field,
                render_value_text(&diff.before),
                render_value_text(&diff.after)
            ));
        }
    }
    if !response.guard_reviews.is_empty() {
        out.push_str("guard_reviews:\n");
        for guard in &response.guard_reviews {
            out.push_str(&format!(
                "- {} [{}] {}\n",
                guard.decision_id, guard.outcome, guard.label
            ));
        }
    }
    if !response.failing_action_path_tags.is_empty() {
        out.push_str(&format!(
            "failing_action_path_tags: {}\n",
            response.failing_action_path_tags.join(", ")
        ));
    }
    if !response.write_overlap_fields.is_empty() {
        out.push_str(&format!(
            "write_overlap_fields: {}\n",
            response.write_overlap_fields.join(", ")
        ));
    }
    if !response.involved_fields.is_empty() {
        out.push_str(&format!(
            "involved_fields: {}\n",
            response.involved_fields.join(", ")
        ));
    }
    render_review_context_text(&mut out, &response.review_context);
    if !response.candidate_causes.is_empty() {
        out.push_str("candidate_causes:\n");
        for cause in &response.candidate_causes {
            out.push_str(&format!("- [{}] {}\n", cause.kind, cause.message));
        }
    }
    if !response.repair_targets.is_empty() {
        out.push_str("repair_targets:\n");
        for target in &response.repair_targets {
            out.push_str(&format!(
                "- [{}:{}] {}",
                target.target, target.priority, target.reason
            ));
            if let Some(action_id) = &target.action_id {
                out.push_str(&format!(" (action: {})", action_id));
            }
            if !target.fields.is_empty() {
                out.push_str(&format!(" (fields: {})", target.fields.join(", ")));
            }
            out.push('\n');
        }
    }
    if !response.repair_hints.is_empty() {
        out.push_str("repair_hints:\n");
        for hint in &response.repair_hints {
            out.push_str(&format!("- {}\n", hint));
        }
    }
    if !response.next_steps.is_empty() {
        out.push_str("next_steps:\n");
        for step in &response.next_steps {
            out.push_str(&format!("- {}\n", step));
        }
    }
    out.push_str(&format!("confidence: {:.2}\n", response.confidence));
    out
}

fn render_review_context_json(context: &ExplainReviewContext) -> String {
    format!(
        "{{\"scenario_id\":{},\"scenario_expr\":{},\"scenario_match_before\":{},\"scenario_match_after\":{},\"property_scope_expr\":{},\"property_scope_match_before\":{},\"property_scope_match_after\":{},\"vacuous\":{}}}",
        render_optional_string(context.scenario_id.as_deref()),
        render_optional_string(context.scenario_expr.as_deref()),
        render_optional_bool(context.scenario_match_before),
        render_optional_bool(context.scenario_match_after),
        render_optional_string(context.property_scope_expr.as_deref()),
        render_optional_bool(context.property_scope_match_before),
        render_optional_bool(context.property_scope_match_after),
        context.vacuous,
    )
}

fn render_review_context_text(out: &mut String, context: &ExplainReviewContext) {
    out.push_str("review_context:\n");
    out.push_str(&format!("  vacuous: {}\n", context.vacuous));
    if let Some(scenario_id) = &context.scenario_id {
        out.push_str(&format!("  scenario_id: {}\n", scenario_id));
    }
    if let Some(expr) = &context.scenario_expr {
        out.push_str(&format!("  scenario_expr: {}\n", expr));
    }
    if let Some(matches) = context.scenario_match_before {
        out.push_str(&format!("  scenario_match_before: {}\n", matches));
    }
    if let Some(matches) = context.scenario_match_after {
        out.push_str(&format!("  scenario_match_after: {}\n", matches));
    }
    if let Some(expr) = &context.property_scope_expr {
        out.push_str(&format!("  property_scope_expr: {}\n", expr));
    }
    if let Some(matches) = context.property_scope_match_before {
        out.push_str(&format!("  property_scope_match_before: {}\n", matches));
    }
    if let Some(matches) = context.property_scope_match_after {
        out.push_str(&format!("  property_scope_match_after: {}\n", matches));
    }
}

fn render_expr_ir(expr: &crate::ir::ExprIr) -> String {
    match expr {
        crate::ir::ExprIr::Literal(value) => match value {
            crate::ir::Value::Bool(value) => value.to_string(),
            crate::ir::Value::UInt(value) => value.to_string(),
            crate::ir::Value::String(value) => format!("{value:?}"),
            crate::ir::Value::EnumVariant { label, .. } => label.clone(),
            crate::ir::Value::PairVariant {
                left_label,
                right_label,
                ..
            } => format!("({}, {})", left_label, right_label),
        },
        crate::ir::ExprIr::FieldRef(field) => field.clone(),
        crate::ir::ExprIr::Unary { op, expr } => match op {
            crate::ir::UnaryOp::Not => format!("!({})", render_expr_ir(expr)),
            crate::ir::UnaryOp::SetIsEmpty => format!("is_empty({})", render_expr_ir(expr)),
            crate::ir::UnaryOp::StringLen => format!("len({})", render_expr_ir(expr)),
            crate::ir::UnaryOp::TemporalAlways => {
                format!("always({})", render_expr_ir(expr))
            }
            crate::ir::UnaryOp::TemporalEventually => {
                format!("eventually({})", render_expr_ir(expr))
            }
            crate::ir::UnaryOp::TemporalNext => format!("next({})", render_expr_ir(expr)),
        },
        crate::ir::ExprIr::Binary { op, left, right } => match op {
            crate::ir::BinaryOp::StringContains => {
                format!(
                    "str_contains({}, {})",
                    render_expr_ir(left),
                    render_expr_ir(right)
                )
            }
            crate::ir::BinaryOp::RegexMatch => {
                format!(
                    "regex_match({}, {})",
                    render_expr_ir(left),
                    render_expr_ir(right)
                )
            }
            crate::ir::BinaryOp::SetContains => {
                format!(
                    "contains({}, {})",
                    render_expr_ir(left),
                    render_expr_ir(right)
                )
            }
            crate::ir::BinaryOp::SetInsert => {
                format!(
                    "insert({}, {})",
                    render_expr_ir(left),
                    render_expr_ir(right)
                )
            }
            crate::ir::BinaryOp::SetRemove => {
                format!(
                    "remove({}, {})",
                    render_expr_ir(left),
                    render_expr_ir(right)
                )
            }
            crate::ir::BinaryOp::RelationContains => {
                format!(
                    "rel_contains({}, {})",
                    render_expr_ir(left),
                    render_expr_ir(right)
                )
            }
            crate::ir::BinaryOp::RelationInsert => {
                format!(
                    "rel_insert({}, {})",
                    render_expr_ir(left),
                    render_expr_ir(right)
                )
            }
            crate::ir::BinaryOp::RelationRemove => {
                format!(
                    "rel_remove({}, {})",
                    render_expr_ir(left),
                    render_expr_ir(right)
                )
            }
            crate::ir::BinaryOp::RelationIntersects => {
                format!(
                    "rel_intersects({}, {})",
                    render_expr_ir(left),
                    render_expr_ir(right)
                )
            }
            crate::ir::BinaryOp::MapContainsKey => {
                format!(
                    "map_contains_key({}, {})",
                    render_expr_ir(left),
                    render_expr_ir(right)
                )
            }
            crate::ir::BinaryOp::MapContainsEntry => {
                format!(
                    "map_contains_entry({}, {})",
                    render_expr_ir(left),
                    render_expr_ir(right)
                )
            }
            crate::ir::BinaryOp::MapPut => {
                format!(
                    "map_put({}, {})",
                    render_expr_ir(left),
                    render_expr_ir(right)
                )
            }
            crate::ir::BinaryOp::MapRemoveKey => {
                format!(
                    "map_remove({}, {})",
                    render_expr_ir(left),
                    render_expr_ir(right)
                )
            }
            crate::ir::BinaryOp::TemporalUntil => {
                format!("until({}, {})", render_expr_ir(left), render_expr_ir(right))
            }
            _ => {
                let operator = match op {
                    crate::ir::BinaryOp::Add => "+",
                    crate::ir::BinaryOp::Sub => "-",
                    crate::ir::BinaryOp::Mod => "%",
                    crate::ir::BinaryOp::LessThan => "<",
                    crate::ir::BinaryOp::LessThanOrEqual => "<=",
                    crate::ir::BinaryOp::GreaterThan => ">",
                    crate::ir::BinaryOp::GreaterThanOrEqual => ">=",
                    crate::ir::BinaryOp::Equal => "==",
                    crate::ir::BinaryOp::NotEqual => "!=",
                    crate::ir::BinaryOp::And => "&&",
                    crate::ir::BinaryOp::Or => "||",
                    crate::ir::BinaryOp::SetContains
                    | crate::ir::BinaryOp::SetInsert
                    | crate::ir::BinaryOp::SetRemove
                    | crate::ir::BinaryOp::RelationContains
                    | crate::ir::BinaryOp::RelationInsert
                    | crate::ir::BinaryOp::RelationRemove
                    | crate::ir::BinaryOp::RelationIntersects
                    | crate::ir::BinaryOp::MapContainsKey
                    | crate::ir::BinaryOp::MapContainsEntry
                    | crate::ir::BinaryOp::MapPut
                    | crate::ir::BinaryOp::MapRemoveKey
                    | crate::ir::BinaryOp::StringContains
                    | crate::ir::BinaryOp::RegexMatch
                    | crate::ir::BinaryOp::TemporalUntil => unreachable!(),
                };
                format!(
                    "({} {} {})",
                    render_expr_ir(left),
                    operator,
                    render_expr_ir(right)
                )
            }
        },
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
    Ok(lint_from_inspect_and_source(
        &inspect,
        Some(&request.source),
    ))
}

pub fn lint_from_inspect(inspect: &InspectResponse) -> LintResponse {
    lint_from_inspect_and_source(inspect, None)
}

pub fn lint_from_inspect_and_source(
    inspect: &InspectResponse,
    source: Option<&str>,
) -> LintResponse {
    let mut findings = Vec::new();
    let declarative_model = inspect
        .transition_details
        .iter()
        .any(|transition| transition.guard.is_some() || !transition.updates.is_empty());
    for reason in &inspect.capabilities.reasons {
        match reason.as_str() {
            "opaque_step_closure" => findings.push(capability_finding(
                "warn",
                "opaque_step_closure",
                "model uses a free-form step closure, so solver lowering is not available",
                Some("rewrite critical actions with declarative transitions { ... }".to_string()),
                None,
            )),
            "missing_declarative_transitions" => findings.push(capability_finding(
                "warn",
                "missing_declarative_transitions",
                "model does not expose declarative transition descriptors",
                Some(
                    "add transitions { transition ... } so guard/effect metadata becomes first-class"
                        .to_string(),
                ),
                None,
            )),
            "unsupported_machine_guard_expr" => findings.push(capability_finding(
                if declarative_model { "error" } else { "warn" },
                "unsupported_machine_guard_expr",
                "one or more guard expressions are outside the current solver-neutral subset",
                Some(
                    "simplify guards to the current IR subset or extend lowering support"
                        .to_string(),
                ),
                None,
            )),
            "unsupported_machine_update_expr" => findings.push(capability_finding(
                if declarative_model { "error" } else { "warn" },
                "unsupported_machine_update_expr",
                "one or more transition updates are outside the current solver-neutral subset",
                Some(
                    "rewrite updates with supported expressions or extend lowering support"
                        .to_string(),
                ),
                None,
            )),
            "unsupported_machine_property_expr" => findings.push(capability_finding(
                if declarative_model { "error" } else { "warn" },
                "unsupported_machine_property_expr",
                "one or more properties cannot be lowered into the current machine IR",
                Some(
                    "keep properties within the supported boolean/arithmetic subset for solver runs"
                        .to_string(),
                ),
                None,
            )),
            "string_fields_require_explicit_backend" => findings.push(capability_finding(
                "warn",
                "string_fields_require_explicit_backend",
                "string fields are currently explicit-only and do not lower to SAT/SMT backends",
                Some(
                    "keep password/text policies on backend=explicit, or abstract them into finite enums for solver runs"
                        .to_string(),
                ),
                None,
            )),
            "string_ops_require_explicit_backend" => findings.push(capability_finding(
                "warn",
                "string_ops_require_explicit_backend",
                "string operations such as len(...) and str_contains(...) currently require the explicit backend",
                Some("use backend=explicit for text-heavy models".to_string()),
                None,
            )),
            "regex_match_requires_explicit_backend" => findings.push(capability_finding(
                "warn",
                "regex_match_requires_explicit_backend",
                "regex_match(...) currently requires the explicit backend",
                Some(
                    "treat regex-based password policies as explicit-first until solver encoding is added"
                        .to_string(),
                ),
                None,
            )),
            other => findings.push(capability_finding(
                "warn",
                other,
                format!("model is not fully analysis-ready: {other}"),
                None,
                None,
            )),
        }
    }
    if inspect
        .action_details
        .iter()
        .any(|action| action.reads.is_empty() && action.writes.is_empty())
    {
        findings.push(capability_finding(
            "info",
            "missing_action_metadata",
            "some actions do not declare reads/writes metadata",
            Some(
                "add reads=[...] and writes=[...] to improve explain, coverage, and testgen"
                    .to_string(),
            ),
            None,
        ));
    }
    if inspect
        .capabilities
        .reasons
        .iter()
        .any(|reason| reason == "opaque_step_closure")
    {
        for action in &inspect.action_details {
            findings.push(capability_finding(
                "info",
                "transition_candidate",
                format!(
                    "action {} is a candidate for declarative transition extraction",
                    action.action_id
                ),
                Some(format!(
                    "start with `transition {} when |state| <guard> => [NextState {{ ... }}];` and carry reads=[{}], writes=[{}]",
                    action.action_id,
                    action.reads.join(", "),
                    action.writes.join(", ")
                )),
                Some(render_transition_migration_snippet(inspect, action)),
            ));
        }
    }
    if inspect
        .transition_details
        .iter()
        .all(|transition| transition.path_tags == ["transition_path".to_string()])
    {
        findings.push(capability_finding(
            "info",
            "generic_decision_paths",
            "decision/path tags are still generic for all transitions",
            Some(
                "use descriptive action ids and metadata so allow/deny/boundary paths become visible"
                    .to_string(),
            ),
            None,
        ));
    }
    findings.extend(maintainability_findings(inspect, source));
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

fn capability_finding(
    severity: &str,
    code: &str,
    message: impl Into<String>,
    suggestion: Option<String>,
    snippet: Option<String>,
) -> LintFinding {
    lint_finding("capability", severity, code, message, suggestion, snippet)
}

fn maintainability_finding(
    severity: &str,
    code: &str,
    message: impl Into<String>,
    suggestion: Option<String>,
    snippet: Option<String>,
) -> LintFinding {
    lint_finding(
        "maintainability",
        severity,
        code,
        message,
        suggestion,
        snippet,
    )
}

fn lint_finding(
    category: &str,
    severity: &str,
    code: &str,
    message: impl Into<String>,
    suggestion: Option<String>,
    snippet: Option<String>,
) -> LintFinding {
    LintFinding {
        category: category.to_string(),
        severity: severity.to_string(),
        code: code.to_string(),
        message: message.into(),
        suggestion,
        snippet,
    }
}

fn maintainability_findings(inspect: &InspectResponse, source: Option<&str>) -> Vec<LintFinding> {
    const OVERSIZED_FIELD_THRESHOLD: usize = 8;
    const OVERSIZED_ACTION_THRESHOLD: usize = 8;
    const OVERSIZED_PROPERTY_THRESHOLD: usize = 6;
    const OVERSIZED_TRANSITION_THRESHOLD: usize = 10;
    const SETUP_HEAVY_MIN_COUNT: usize = 3;
    const SETUP_HEAVY_RATIO_NUMERATOR: usize = 2;
    const SETUP_HEAVY_RATIO_DENOMINATOR: usize = 5;

    let mut findings = Vec::new();

    if let Some(source) = source.filter(|source| !source.trim().is_empty()) {
        if !source_has_model_intent_comment(source) {
            findings.push(maintainability_finding(
                "warn",
                "missing_model_documentation",
                "model source does not start with an intent comment or overview",
                Some(
                    "add a short comment block above the model describing the business rule, boundaries, and why the model exists"
                        .to_string(),
                ),
                None,
            ));
        }
    }

    let mut oversize_parts = Vec::new();
    if inspect.state_fields.len() > OVERSIZED_FIELD_THRESHOLD {
        oversize_parts.push(format!("{} state fields", inspect.state_fields.len()));
    }
    if inspect.action_details.len() > OVERSIZED_ACTION_THRESHOLD {
        oversize_parts.push(format!("{} actions", inspect.action_details.len()));
    }
    if inspect.property_details.len() > OVERSIZED_PROPERTY_THRESHOLD {
        oversize_parts.push(format!("{} properties", inspect.property_details.len()));
    }
    if inspect.transition_details.len() > OVERSIZED_TRANSITION_THRESHOLD {
        oversize_parts.push(format!("{} transitions", inspect.transition_details.len()));
    }
    if !oversize_parts.is_empty() {
        findings.push(maintainability_finding(
            "warn",
            "oversized_model",
            format!(
                "model packs too much behavior into one unit: {}",
                oversize_parts.join(", ")
            ),
            Some(
                "split the model into smaller bounded contexts, move repeated logic into predicates, separate setup-only behavior from business transitions, or move shared-state cross-domain rules into a dedicated integration model"
                    .to_string(),
            ),
            None,
        ));
    }

    let setup_count = inspect
        .transition_details
        .iter()
        .filter(|transition| transition.role == "setup")
        .count();
    let transition_count = inspect.transition_details.len();
    if transition_count >= SETUP_HEAVY_MIN_COUNT
        && setup_count * SETUP_HEAVY_RATIO_DENOMINATOR
            >= transition_count * SETUP_HEAVY_RATIO_NUMERATOR
    {
        findings.push(maintainability_finding(
            "warn",
            "setup_heavy_model",
            format!(
                "setup transitions dominate the model structure ({setup_count} of {transition_count} transitions are marked setup)"
            ),
            Some(
                "move bootstrap-only paths into scenarios or fixtures so business flow coverage stays focused; if the remaining question is cross-domain shared state, keep that review in a small integration model"
                    .to_string(),
            ),
            None,
        ));
    }

    let repeated_conditions = repeated_conditions(inspect);
    if !repeated_conditions.is_empty() {
        let condition_summaries = repeated_conditions
            .iter()
            .map(|(expr, count)| format!("`{expr}` ({count} uses)"))
            .collect::<Vec<_>>();
        findings.push(maintainability_finding(
            "info",
            "repeated_condition_without_predicate",
            format!(
                "repeated conditions should likely become named predicates: {}",
                condition_summaries.join(", ")
            ),
            Some(
                "extract the repeated expression into predicates so standalone and integration models can share one reviewable name for the same condition"
                    .to_string(),
            ),
            repeated_conditions.first().map(|(expr, _)| expr.clone()),
        ));
    }

    findings
}

fn source_has_model_intent_comment(source: &str) -> bool {
    for raw_line in source.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("model ") {
            return false;
        }
        if trimmed.starts_with("//") || trimmed.starts_with("//!") || trimmed.starts_with("///") {
            return true;
        }
        if trimmed.starts_with("# ") {
            return true;
        }
        if trimmed.starts_with("use ")
            || trimmed.starts_with("valid_model!")
            || trimmed.starts_with("valid_state!")
            || trimmed.starts_with("valid_actions!")
            || trimmed.starts_with("struct ")
            || trimmed.starts_with("enum ")
        {
            return false;
        }
    }
    false
}
fn repeated_conditions(inspect: &InspectResponse) -> Vec<(String, usize)> {
    let predicate_exprs = inspect
        .predicate_details
        .iter()
        .map(|predicate| normalize_expr(&predicate.expr))
        .collect::<Vec<_>>();
    let mut counts = BTreeMap::new();
    for expr in inspect
        .transition_details
        .iter()
        .filter_map(|transition| transition.guard.as_ref())
        .chain(
            inspect
                .property_details
                .iter()
                .filter_map(|property| property.expr.as_ref()),
        )
        .chain(
            inspect
                .property_details
                .iter()
                .filter_map(|property| property.scope_expr.as_ref()),
        )
        .chain(
            inspect
                .scenario_details
                .iter()
                .map(|scenario| &scenario.expr),
        )
    {
        let normalized = normalize_expr(expr);
        if normalized.len() < 12 {
            continue;
        }
        if predicate_exprs
            .iter()
            .any(|predicate| predicate == &normalized)
        {
            continue;
        }
        *counts.entry(normalized).or_insert(0usize) += 1;
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .collect()
}

fn normalize_expr(expr: &str) -> String {
    expr.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn explicit_analysis_warning(inspect: &InspectResponse) -> Option<String> {
    let blocking_reasons = inspect
        .capabilities
        .reasons
        .iter()
        .filter(|reason| {
            matches!(
                reason.as_str(),
                "unsupported_machine_guard_expr"
                    | "unsupported_machine_update_expr"
                    | "unsupported_machine_property_expr"
                    | "unsupported_rust_field_type"
                    | "unsupported_field_range"
                    | "machine_ir_lowering_failed"
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let declarative_model = inspect
        .transition_details
        .iter()
        .any(|transition| transition.guard.is_some() || !transition.updates.is_empty());
    if declarative_model && !blocking_reasons.is_empty() {
        Some(format!(
            "warning: declarative model `{}` cannot fully lower to machine IR; explicit verification still ran, but solver/graph/testgen fidelity is reduced. reasons: {}. run `cargo valid readiness {}` for migration guidance.",
            inspect.model_id,
            blocking_reasons.join(", "),
            inspect.model_id
        ))
    } else {
        None
    }
}

pub fn render_lint_json(response: &LintResponse) -> String {
    let findings = response
        .findings
        .iter()
        .map(|finding| {
            format!(
                "{{\"category\":\"{}\",\"severity\":\"{}\",\"code\":\"{}\",\"message\":\"{}\",\"suggestion\":{},\"snippet\":{}}}",
                escape_json(&finding.category),
                escape_json(&finding.severity),
                escape_json(&finding.code),
                escape_json(&finding.message),
                finding
                    .suggestion
                    .as_ref()
                    .map(|value| format!("\"{}\"", escape_json(value)))
                    .unwrap_or_else(|| "null".to_string()),
                finding
                    .snippet
                    .as_ref()
                    .map(|value| format!("\"{}\"", escape_json(value)))
                    .unwrap_or_else(|| "null".to_string())
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"model_id\":\"{}\",\"capabilities\":{},\"findings\":[{}]}}",
        escape_json(&response.schema_version),
        escape_json(&response.request_id),
        escape_json(&response.status),
        escape_json(&response.model_id),
        render_capabilities_json(&response.capabilities),
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
    out.push_str(&render_capability_details_text(&response.capabilities));
    if response.findings.is_empty() {
        out.push_str("findings: none\n");
    } else {
        out.push_str("findings:\n");
        for finding in &response.findings {
            out.push_str(&format!(
                "- [{}:{}] {}: {}{}\n",
                finding.category,
                finding.severity,
                finding.code,
                finding.message,
                finding
                    .suggestion
                    .as_ref()
                    .map(|suggestion| format!(" suggestion={suggestion}"))
                    .unwrap_or_default()
            ));
            if let Some(snippet) = &finding.snippet {
                for line in snippet.lines() {
                    out.push_str(&format!("  | {line}\n"));
                }
            }
        }
    }
    out
}

pub fn migration_from_inspect(
    inspect: &InspectResponse,
    lint: &LintResponse,
    include_check: bool,
) -> MigrationResponse {
    let snippets = lint
        .findings
        .iter()
        .filter_map(|finding| {
            finding.snippet.as_ref().map(|snippet| MigrationSnippet {
                code: finding.code.clone(),
                action: finding
                    .message
                    .strip_prefix("action ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_string),
                snippet: snippet.clone(),
            })
        })
        .collect::<Vec<_>>();
    let check = include_check.then(|| migration_check_from_inspect(inspect, &snippets));
    MigrationResponse {
        schema_version: lint.schema_version.clone(),
        request_id: lint.request_id.clone(),
        status: if snippets.is_empty() {
            "no-op".to_string()
        } else {
            "ok".to_string()
        },
        model_id: lint.model_id.clone(),
        snippets,
        check,
    }
}

fn migration_check_from_inspect(
    inspect: &InspectResponse,
    snippets: &[MigrationSnippet],
) -> MigrationCheckResponse {
    let covered_actions = snippets
        .iter()
        .filter_map(|snippet| snippet.action.clone())
        .collect::<Vec<_>>();
    let missing_actions = inspect
        .action_details
        .iter()
        .map(|action| action.action_id.clone())
        .filter(|action_id| !covered_actions.iter().any(|covered| covered == action_id))
        .collect::<Vec<_>>();
    let mut next_steps = Vec::new();
    let mut reasons = inspect.capabilities.reasons.clone();
    append_capability_guidance(inspect, &mut next_steps);
    let (status, mode, verified_equivalence) = if inspect.machine_ir_ready {
        next_steps.push(
            "model already has declarative transitions; use verify/benchmark directly".to_string(),
        );
        ("already-declarative", "identity", true)
    } else if snippets.is_empty() {
        next_steps.push(
            "no migration snippets were produced; add explicit action metadata before migrating"
                .to_string(),
        );
        ("no-candidates", "heuristic-action-coverage", false)
    } else if missing_actions.is_empty() {
        next_steps.push(
            "review each generated transition and validate property results against the original step model".to_string(),
        );
        if reasons.is_empty() {
            reasons.push("manual_review_required".to_string());
        }
        ("candidate-complete", "heuristic-action-coverage", false)
    } else {
        next_steps.push(format!(
            "fill in declarative transitions for missing actions: {}",
            missing_actions.join(", ")
        ));
        next_steps.push(
            "once all actions are covered, rerun verify and benchmark to compare behavior"
                .to_string(),
        );
        ("partial", "heuristic-action-coverage", false)
    };
    MigrationCheckResponse {
        status: status.to_string(),
        mode: mode.to_string(),
        verified_equivalence,
        total_action_count: inspect.action_details.len(),
        snippet_action_count: covered_actions.len(),
        covered_actions,
        missing_actions,
        reasons,
        next_steps,
    }
}

fn append_capability_guidance(inspect: &InspectResponse, next_steps: &mut Vec<String>) {
    if !inspect.capabilities.ir_ready {
        push_unique(
            next_steps,
            format!("machine IR blocker: {}", inspect.capabilities.ir.reason),
        );
        if !inspect.capabilities.ir.unsupported_features.is_empty() {
            push_unique(
                next_steps,
                format!(
                    "machine IR unsupported features: {}",
                    inspect.capabilities.ir.unsupported_features.join(", ")
                ),
            );
        }
        if let Some(hint) = &inspect.capabilities.ir.migration_hint {
            push_unique(next_steps, hint.clone());
        }
    }
    if inspect.capabilities.ir_ready && !inspect.capabilities.solver_ready {
        push_unique(
            next_steps,
            format!("solver blocker: {}", inspect.capabilities.solver.reason),
        );
        if !inspect.capabilities.solver.unsupported_features.is_empty() {
            push_unique(
                next_steps,
                format!(
                    "solver unsupported features: {}",
                    inspect.capabilities.solver.unsupported_features.join(", ")
                ),
            );
        }
        if let Some(hint) = &inspect.capabilities.solver.migration_hint {
            push_unique(next_steps, hint.clone());
        }
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

pub fn render_migration_json(response: &MigrationResponse) -> String {
    let snippets = response
        .snippets
        .iter()
        .map(|snippet| {
            format!(
                "{{\"code\":\"{}\",\"action\":{},\"snippet\":\"{}\"}}",
                escape_json(&snippet.code),
                snippet
                    .action
                    .as_ref()
                    .map(|value| format!("\"{}\"", escape_json(value)))
                    .unwrap_or_else(|| "null".to_string()),
                escape_json(&snippet.snippet)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let check = response
        .check
        .as_ref()
        .map(|check| {
            format!(
                "{{\"status\":\"{}\",\"mode\":\"{}\",\"verified_equivalence\":{},\"total_action_count\":{},\"snippet_action_count\":{},\"covered_actions\":{},\"missing_actions\":{},\"reasons\":{},\"next_steps\":{}}}",
                escape_json(&check.status),
                escape_json(&check.mode),
                check.verified_equivalence,
                check.total_action_count,
                check.snippet_action_count,
                render_string_array(&check.covered_actions),
                render_string_array(&check.missing_actions),
                render_string_array(&check.reasons),
                render_string_array(&check.next_steps)
            )
        })
        .unwrap_or_else(|| "null".to_string());
    format!(
        "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"model_id\":\"{}\",\"snippets\":[{}],\"check\":{}}}",
        escape_json(&response.schema_version),
        escape_json(&response.request_id),
        escape_json(&response.status),
        escape_json(&response.model_id),
        snippets,
        check
    )
}

pub fn render_migration_text(response: &MigrationResponse) -> String {
    let mut out = String::new();
    out.push_str(&format!("model_id: {}\n", response.model_id));
    out.push_str(&format!("status: {}\n", response.status));
    if response.snippets.is_empty() {
        out.push_str("snippets: none\n");
    } else {
        out.push_str("snippets:\n");
        for snippet in &response.snippets {
            out.push_str(&format!(
                "- {}\n",
                snippet
                    .action
                    .as_ref()
                    .map(|action| format!("action {action}"))
                    .unwrap_or_else(|| snippet.code.clone())
            ));
            for line in snippet.snippet.lines() {
                out.push_str(&format!("  | {line}\n"));
            }
        }
    }
    if let Some(check) = &response.check {
        out.push_str("check:\n");
        out.push_str(&format!("  status: {}\n", check.status));
        out.push_str(&format!("  mode: {}\n", check.mode));
        out.push_str(&format!(
            "  verified_equivalence: {}\n",
            check.verified_equivalence
        ));
        out.push_str(&format!(
            "  covered_actions: {}\n",
            if check.covered_actions.is_empty() {
                "none".to_string()
            } else {
                check.covered_actions.join(", ")
            }
        ));
        out.push_str(&format!(
            "  missing_actions: {}\n",
            if check.missing_actions.is_empty() {
                "none".to_string()
            } else {
                check.missing_actions.join(", ")
            }
        ));
        if !check.reasons.is_empty() {
            out.push_str(&format!("  reasons: {}\n", check.reasons.join(", ")));
        }
        if !check.next_steps.is_empty() {
            out.push_str("  next_steps:\n");
            for step in &check.next_steps {
                out.push_str(&format!("    - {step}\n"));
            }
        }
    }
    out
}

fn render_transition_migration_snippet(
    inspect: &InspectResponse,
    action: &InspectAction,
) -> String {
    let tags = crate::modeling::infer_decision_path_tags(
        &action.action_id,
        action.reads.iter().map(String::as_str),
        action.writes.iter().map(String::as_str),
        Some("<guard>"),
        Some("State { ... }"),
    );
    let tag_block = if tags.is_empty() {
        String::new()
    } else {
        format!(
            " [tags = [{}]]",
            tags.iter()
                .map(|tag| format!("\"{tag}\""))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let updates = inspect
        .state_field_details
        .iter()
        .map(|field| {
            let expr = if action.writes.iter().any(|write| write == &field.name) {
                format!("/* TODO update {} */ state.{}", field.name, field.name)
            } else {
                format!("state.{}", field.name)
            };
            format!("    {}: {}", field.name, expr)
        })
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "transition {}{} when |state| /* TODO guard */ true => [State {{\n{}\n}}];",
        action.action_id, tag_block, updates
    )
}

fn render_capabilities_json(capabilities: &InspectCapabilities) -> String {
    format!(
        "{{\"parse_ready\":{},\"explicit_ready\":{},\"ir_ready\":{},\"solver_ready\":{},\"coverage_ready\":{},\"explain_ready\":{},\"testgen_ready\":{},\"reasons\":{},\"parse\":{},\"explicit\":{},\"ir\":{},\"solver\":{},\"coverage\":{},\"explain\":{},\"testgen\":{},\"temporal\":{}}}",
        capabilities.parse_ready,
        capabilities.explicit_ready,
        capabilities.ir_ready,
        capabilities.solver_ready,
        capabilities.coverage_ready,
        capabilities.explain_ready,
        capabilities.testgen_ready,
        render_string_array(&capabilities.reasons),
        render_capability_detail_json(&capabilities.parse),
        render_capability_detail_json(&capabilities.explicit),
        render_capability_detail_json(&capabilities.ir),
        render_capability_detail_json(&capabilities.solver),
        render_capability_detail_json(&capabilities.coverage),
        render_capability_detail_json(&capabilities.explain),
        render_capability_detail_json(&capabilities.testgen),
        render_temporal_inspect_capabilities_json(&capabilities.temporal),
    )
}

fn render_temporal_inspect_capabilities_json(temporal: &InspectTemporalCapabilities) -> String {
    format!(
        "{{\"property_ids\":{},\"operators\":{},\"support_level\":\"{}\",\"explicit_status\":\"{}\",\"solver_status\":\"{}\",\"reason\":{}}}",
        render_string_array(&temporal.property_ids),
        render_string_array(&temporal.operators),
        escape_json(&temporal.support_level),
        escape_json(&temporal.explicit_status),
        escape_json(&temporal.solver_status),
        if temporal.reason.is_empty() {
            "null".to_string()
        } else {
            format!("\"{}\"", escape_json(&temporal.reason))
        }
    )
}

fn render_capability_detail_json(detail: &CapabilityDetail) -> String {
    format!(
        "{{\"reason\":\"{}\",\"migration_hint\":{},\"unsupported_features\":{}}}",
        escape_json(&detail.reason),
        render_optional_string(detail.migration_hint.as_deref()),
        render_string_array(&detail.unsupported_features),
    )
}

fn render_capability_details_text(capabilities: &InspectCapabilities) -> String {
    let details = [
        ("parse", capabilities.parse_ready, &capabilities.parse),
        (
            "explicit",
            capabilities.explicit_ready,
            &capabilities.explicit,
        ),
        ("ir", capabilities.ir_ready, &capabilities.ir),
        ("solver", capabilities.solver_ready, &capabilities.solver),
        (
            "coverage",
            capabilities.coverage_ready,
            &capabilities.coverage,
        ),
        ("explain", capabilities.explain_ready, &capabilities.explain),
        ("testgen", capabilities.testgen_ready, &capabilities.testgen),
    ];
    let mut lines = Vec::new();
    for (name, ready, detail) in details {
        if ready
            && detail.reason.is_empty()
            && detail.migration_hint.is_none()
            && detail.unsupported_features.is_empty()
        {
            continue;
        }
        let mut line = format!(
            "- {} reason={}",
            name,
            if detail.reason.is_empty() {
                "ready".to_string()
            } else {
                detail.reason.clone()
            }
        );
        if let Some(hint) = &detail.migration_hint {
            line.push_str(&format!(" migration_hint={hint}"));
        }
        if !detail.unsupported_features.is_empty() {
            line.push_str(&format!(
                " unsupported_features=[{}]",
                detail.unsupported_features.join(", ")
            ));
        }
        lines.push(line);
    }
    if lines.is_empty() {
        render_temporal_capability_details_text(&capabilities.temporal)
    } else {
        let body = lines
            .into_iter()
            .map(|line| format!("- {}", line.trim_start_matches("- ")))
            .collect::<Vec<_>>()
            .join("\n");
        let mut out = format!("capability_details:\n{}\n", indent_block(&body, 2));
        out.push_str(&render_temporal_capability_details_text(
            &capabilities.temporal,
        ));
        out
    }
}

fn render_temporal_capability_details_text(temporal: &InspectTemporalCapabilities) -> String {
    if temporal.support_level == "not_applicable" {
        return String::new();
    }
    let mut line = format!(
        "- temporal support_level={} explicit_status={} solver_status={}",
        temporal.support_level, temporal.explicit_status, temporal.solver_status
    );
    if !temporal.property_ids.is_empty() {
        line.push_str(&format!(
            " property_ids=[{}]",
            temporal.property_ids.join(", ")
        ));
    }
    if !temporal.operators.is_empty() {
        line.push_str(&format!(" operators=[{}]", temporal.operators.join(", ")));
    }
    if !temporal.reason.is_empty() {
        line.push_str(&format!(" reason={}", temporal.reason));
    }
    format!("temporal_capabilities:\n{}\n", indent_block(&line, 2))
}

fn render_csv_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn render_text_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut builder = Builder::default();
    builder.push_record(headers.iter().copied());
    for row in rows {
        builder.push_record(row.iter().map(|value| value.as_str()));
    }
    let mut table = builder.build();
    table
        .with(Style::rounded())
        .with(Modify::new(tabled::settings::object::Rows::first()).with(Alignment::center()))
        .with(Modify::new(tabled::settings::object::Rows::new(1..)).with(Padding::new(1, 1, 0, 0)));
    table.to_string()
}

fn indent_block(value: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn pretty_expr(value: &str) -> String {
    let mut out = String::new();
    let mut indent = 0usize;
    let mut pending_space = false;
    for ch in value.chars() {
        match ch {
            '{' => {
                out.push(ch);
                indent += 1;
                out.push('\n');
                out.push_str(&"  ".repeat(indent));
                pending_space = false;
            }
            '}' => {
                indent = indent.saturating_sub(1);
                out.push('\n');
                out.push_str(&"  ".repeat(indent));
                out.push(ch);
                pending_space = false;
            }
            ',' => {
                out.push(',');
                out.push('\n');
                out.push_str(&"  ".repeat(indent));
                pending_space = false;
            }
            ' ' | '\n' | '\t' => {
                pending_space = true;
            }
            _ => {
                if pending_space && !out.ends_with([' ', '\n']) {
                    out.push(' ');
                }
                out.push(ch);
                pending_space = false;
            }
        }
    }
    out.trim().to_string()
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

fn render_optional_bool(value: Option<bool>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn render_optional_string(value: Option<&str>) -> String {
    value
        .map(|value| format!("\"{}\"", escape_json(value)))
        .unwrap_or_else(|| "null".to_string())
}

fn render_value_json(value: &crate::ir::Value) -> String {
    match value {
        crate::ir::Value::Bool(value) => value.to_string(),
        crate::ir::Value::UInt(value) => value.to_string(),
        crate::ir::Value::String(value) => format!("\"{}\"", escape_json(value)),
        crate::ir::Value::EnumVariant { label, index } => format!(
            "{{\"label\":\"{}\",\"index\":{}}}",
            escape_json(label),
            index
        ),
        crate::ir::Value::PairVariant {
            left_label,
            left_index,
            right_label,
            right_index,
        } => format!(
            "{{\"left_label\":\"{}\",\"left_index\":{},\"right_label\":\"{}\",\"right_index\":{}}}",
            escape_json(left_label),
            left_index,
            escape_json(right_label),
            right_index
        ),
    }
}

fn render_value_text(value: &crate::ir::Value) -> String {
    match value {
        crate::ir::Value::Bool(value) => value.to_string(),
        crate::ir::Value::UInt(value) => value.to_string(),
        crate::ir::Value::String(value) => value.clone(),
        crate::ir::Value::EnumVariant { label, .. } => label.clone(),
        crate::ir::Value::PairVariant {
            left_label,
            right_label,
            ..
        } => format!("({}, {})", left_label, right_label),
    }
}

fn eval_bool_expr(
    model: &ModelIr,
    state: &crate::kernel::MachineState,
    expr: &crate::ir::ExprIr,
) -> Result<bool, Diagnostic> {
    match eval_expr(model, state, expr)? {
        crate::ir::Value::Bool(value) => Ok(value),
        _ => Err(Diagnostic::new(
            ErrorCode::EvalError,
            DiagnosticSegment::EngineSearch,
            "scenario/scope expression did not evaluate to bool",
        )),
    }
}

fn state_field_diffs(
    before: &std::collections::BTreeMap<String, crate::ir::Value>,
    after: &std::collections::BTreeMap<String, crate::ir::Value>,
) -> Vec<ExplainFieldDiff> {
    before
        .iter()
        .filter_map(|(field, before_value)| {
            let after_value = after.get(field)?;
            if before_value == after_value {
                None
            } else {
                Some(ExplainFieldDiff {
                    field: field.clone(),
                    before: before_value.clone(),
                    after: after_value.clone(),
                })
            }
        })
        .collect()
}

fn explain_guard_reviews(path: Option<&Path>) -> Vec<ExplainGuardReview> {
    let Some(path) = path else {
        return Vec::new();
    };
    path.decisions
        .iter()
        .filter(|decision| matches!(decision.point.kind, DecisionKind::Guard))
        .map(|decision| ExplainGuardReview {
            decision_id: decision.decision_id(),
            label: decision.point.label.clone(),
            outcome: match decision.outcome {
                DecisionOutcome::GuardTrue => "guard_true".to_string(),
                DecisionOutcome::GuardFalse => "guard_false".to_string(),
                DecisionOutcome::UpdateApplied => "update_applied".to_string(),
            },
        })
        .collect()
}

fn merged_involved_fields(
    changed_fields: &[String],
    reads: &[String],
    writes: &[String],
) -> Vec<String> {
    let mut fields = changed_fields.to_vec();
    for field in writes {
        push_unique(&mut fields, field.clone());
    }
    for field in reads {
        push_unique(&mut fields, field.clone());
    }
    fields
}

fn build_review_context(
    model: Option<&ModelIr>,
    property: Option<&crate::ir::PropertyIr>,
    scenario_id: Option<&str>,
    before_state: &std::collections::BTreeMap<String, crate::ir::Value>,
    after_state: &std::collections::BTreeMap<String, crate::ir::Value>,
    vacuous: bool,
) -> ExplainReviewContext {
    let Some(model) = model else {
        return ExplainReviewContext {
            scenario_id: scenario_id.map(str::to_string),
            scenario_expr: None,
            scenario_match_before: None,
            scenario_match_after: None,
            property_scope_expr: None,
            property_scope_match_before: None,
            property_scope_match_after: None,
            vacuous,
        };
    };
    let before_machine = machine_state_from_snapshot(model, before_state);
    let after_machine = machine_state_from_snapshot(model, after_state);
    let scenario =
        scenario_id.and_then(|id| model.scenarios.iter().find(|entry| entry.scenario_id == id));
    ExplainReviewContext {
        scenario_id: scenario_id.map(str::to_string),
        scenario_expr: scenario.map(|scenario| render_expr_ir(&scenario.expr)),
        scenario_match_before: scenario
            .zip(before_machine.as_ref())
            .and_then(|(scenario, state)| eval_bool_expr(model, state, &scenario.expr).ok()),
        scenario_match_after: scenario
            .zip(after_machine.as_ref())
            .and_then(|(scenario, state)| eval_bool_expr(model, state, &scenario.expr).ok()),
        property_scope_expr: property
            .and_then(|property| property.scope.as_ref().map(|scope| render_expr_ir(scope))),
        property_scope_match_before: property
            .and_then(|property| property.scope.as_ref())
            .zip(before_machine.as_ref())
            .and_then(|(scope, state)| eval_bool_expr(model, state, scope).ok()),
        property_scope_match_after: property
            .and_then(|property| property.scope.as_ref())
            .zip(after_machine.as_ref())
            .and_then(|(scope, state)| eval_bool_expr(model, state, scope).ok()),
        vacuous,
    }
}

fn build_repair_targets(
    property_kind: PropertyKind,
    action_id: Option<&str>,
    changed_fields: &[String],
    write_overlap_fields: &[String],
    review_context: &ExplainReviewContext,
) -> Vec<ExplainRepairTargetHint> {
    let mut targets = Vec::new();
    if property_kind == PropertyKind::Reachability
        || property_kind == PropertyKind::Cover
        || review_context.vacuous
        || review_context.scenario_match_before == Some(false)
        || review_context.property_scope_match_before == Some(false)
    {
        targets.push(ExplainRepairTargetHint {
            target: "requirement_fix".to_string(),
            reason: "review whether the property, scenario, or scope selection expresses the intended requirement".to_string(),
            priority: "medium".to_string(),
            action_id: action_id.map(str::to_string),
            fields: changed_fields.to_vec(),
        });
    }
    if !changed_fields.is_empty() || !write_overlap_fields.is_empty() {
        targets.push(ExplainRepairTargetHint {
            target: "model_fix".to_string(),
            reason: "review the modeled guard/update set around the causal breakpoint".to_string(),
            priority: if !write_overlap_fields.is_empty() {
                "high".to_string()
            } else {
                "medium".to_string()
            },
            action_id: action_id.map(str::to_string),
            fields: if write_overlap_fields.is_empty() {
                changed_fields.to_vec()
            } else {
                write_overlap_fields.to_vec()
            },
        });
    }
    if let Some(action_id) = action_id {
        targets.push(ExplainRepairTargetHint {
            target: "implementation_fix".to_string(),
            reason: format!(
                "inspect the implementation or postcondition of action {} at the failing boundary",
                action_id
            ),
            priority: if changed_fields.is_empty() {
                "medium".to_string()
            } else {
                "high".to_string()
            },
            action_id: Some(action_id.to_string()),
            fields: changed_fields.to_vec(),
        });
    }
    targets
}

fn explain_breakpoint_kind(step: &crate::evidence::TraceStep) -> &'static str {
    match (step.action_id.as_ref(), step.note.as_deref()) {
        (_, Some(note)) if note.contains("deadlock") => "deadlock_boundary",
        (Some(_), _) => "action_boundary",
        (None, Some(_)) => "terminal_boundary",
        (None, None) => "state_boundary",
    }
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
    require_non_empty(&response.breakpoint_kind, "breakpoint_kind")?;
    for decision in &response.decision_path.decisions {
        require_non_empty(
            &decision.point.decision_id,
            "decision_path.decisions[].decision_id",
        )?;
        require_non_empty(
            &decision.point.action_id,
            "decision_path.decisions[].action_id",
        )?;
    }
    for guard in &response.guard_reviews {
        require_non_empty(&guard.decision_id, "guard_reviews[].decision_id")?;
        require_non_empty(&guard.label, "guard_reviews[].label")?;
        require_non_empty(&guard.outcome, "guard_reviews[].outcome")?;
    }
    for target in &response.repair_targets {
        require_non_empty(&target.target, "repair_targets[].target")?;
        require_non_empty(&target.reason, "repair_targets[].reason")?;
        require_non_empty(&target.priority, "repair_targets[].priority")?;
    }
    if !(0.0..=1.0).contains(&response.confidence) {
        return Err("confidence must be between 0.0 and 1.0".to_string());
    }
    Ok(())
}

fn render_path_json(path: &Path) -> String {
    let mut out = String::from("{\"decisions\":[");
    for (index, decision) in path.decisions.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str(&format!(
            "\"decision_id\":\"{}\"",
            escape_json(&decision.decision_id())
        ));
        out.push_str(&format!(
            ",\"action_id\":\"{}\"",
            escape_json(&decision.point.action_id)
        ));
        out.push_str(&format!(
            ",\"kind\":\"{}\"",
            match decision.point.kind {
                DecisionKind::Guard => "guard",
                DecisionKind::StateUpdate => "state_update",
            }
        ));
        out.push_str(&format!(
            ",\"label\":\"{}\"",
            escape_json(&decision.point.label)
        ));
        if let Some(field) = &decision.point.field {
            out.push_str(&format!(",\"field\":\"{}\"", escape_json(field)));
        } else {
            out.push_str(",\"field\":null");
        }
        out.push_str(&format!(
            ",\"reads\":{}",
            render_string_array(&decision.point.reads)
        ));
        out.push_str(&format!(
            ",\"writes\":{}",
            render_string_array(&decision.point.writes)
        ));
        out.push_str(&format!(
            ",\"path_tags\":{}",
            render_string_array(&decision.point.path_tags)
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
    require_non_empty(&response.status, "status")?;
    require_len_match(
        response.vector_ids.len(),
        response.vectors.len(),
        "vector_ids",
        "vectors",
    )?;
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
    require_non_empty(
        &response.capabilities.temporal.status,
        "capabilities.temporal.status",
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
        "counterexample" | "transition" | "witness" | "guard" | "boundary" | "path"
        | "random" => Ok(()),
        other => Err(format!(
            "strategy must be one of counterexample, transition, witness, guard, boundary, path, random, got `{other}`"
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

    use insta::assert_snapshot;

    use crate::{engine::CheckOutcome, modeling::CapabilityDetail};

    use super::{
        capabilities_response, check_source, explain_source, explicit_analysis_warning,
        inspect_source, lint_from_inspect, lint_from_inspect_and_source, lint_source,
        migration_from_inspect, minimize_source, orchestrate_source, render_inspect_json,
        render_inspect_text, render_lint_json, testgen_source, validate_capabilities_request,
        validate_capabilities_response, validate_check_request, validate_explain_request,
        validate_explain_response, validate_inspect_request, validate_inspect_response,
        validate_minimize_request, validate_minimize_response, validate_orchestrate_response,
        validate_testgen_request, validate_testgen_response, CapabilitiesRequest, CheckRequest,
        InspectAction, InspectCapabilities, InspectProperty, InspectRequest, InspectResponse,
        InspectTransition, InspectTransitionUpdate, MinimizeRequest, OrchestrateRequest,
        TestgenRequest,
    };

    fn cleanup_generated_files(paths: &[String]) {
        for path in paths {
            let _ = fs::remove_file(path);
        }
    }

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
        assert_eq!(response.property_details[0].kind, "invariant");
        assert!(response.transition_details.is_empty());
        validate_inspect_response(&response).unwrap();
    }

    #[test]
    fn inspect_reports_reachability_kind() {
        let source = "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\nproperty P_REACH:\n  reachability: x == 2\n";
        let request = InspectRequest {
            request_id: "req-inspect-reach".to_string(),
            source_name: "a.valid".to_string(),
            source: source.to_string(),
        };
        let response = inspect_source(&request).unwrap();
        assert_eq!(response.property_details[0].kind, "reachability");
        validate_inspect_response(&response).unwrap();
    }

    #[test]
    fn inspect_hides_expr_for_deadlock_freedom_property() {
        let source =
            "model A\nstate:\n  x: u8[0..1]\ninit:\n  x = 0\nproperty P_LIVE: deadlock_freedom\n";
        let request = InspectRequest {
            request_id: "req-deadlock".to_string(),
            source_name: "a.valid".to_string(),
            source: source.to_string(),
        };
        let response = inspect_source(&request).unwrap();
        assert_eq!(response.property_details[0].kind, "deadlock_freedom");
        assert_eq!(response.property_details[0].expr, None);
    }

    #[test]
    fn inspect_renders_predicates_scenarios_and_transition_details() {
        let source = "model PostFlow\nstate:\n  visible: bool\n  deleted: bool\ninit:\n  visible = true\n  deleted = false\npredicates:\n  deleted_view: visible == false && deleted == true\nscenarios:\n  DeletedPost: deleted == true\naction Delete:\n  pre: visible == true\n  post:\n    visible = false\n    deleted = true\nproperty P_DELETE_POST:\n  transition: next.deleted == true && prev.visible == true\n  on: Delete\n  when: prev.visible == true\n";
        let response = inspect_source(&InspectRequest {
            request_id: "req-inspect-scenario".to_string(),
            source_name: "post.valid".to_string(),
            source: source.to_string(),
        })
        .unwrap();
        assert_eq!(response.predicates, vec!["deleted_view"]);
        assert_eq!(response.scenarios, vec!["DeletedPost"]);
        assert_eq!(response.predicate_details[0].id, "deleted_view");
        assert_eq!(response.scenario_details[0].id, "DeletedPost");
        assert_eq!(response.property_details[0].kind, "transition");
        assert_eq!(
            response.property_details[0].action_filter.as_deref(),
            Some("Delete")
        );
        assert_eq!(
            response.property_details[0].scope_expr.as_deref(),
            Some("(prev_visible == true)")
        );
        let json = render_inspect_json(&response);
        assert!(json.contains("\"predicates\":[\"deleted_view\"]"));
        assert!(json.contains("\"scenarios\":[\"DeletedPost\"]"));
        assert!(json.contains("\"scope_expr\":\"(prev_visible == true)\""));
        assert!(json.contains("\"action_filter\":\"Delete\""));
        let text = render_inspect_text(&response);
        assert!(text.contains("predicates (1): deleted_view"));
        assert!(text.contains("scenarios (1): DeletedPost"));
        assert!(text.contains("on_action: Delete"));
        validate_inspect_response(&response).unwrap();
    }

    #[test]
    fn scenario_scoped_checks_and_cover_report_scope_metadata() {
        let source = "model PostFlow\nstate:\n  visible: bool\n  deleted: bool\ninit:\n  visible = true\n  deleted = false\npredicates:\n  deleted_view: visible == false && deleted == true\nscenarios:\n  DeletedPost: deleted == true\naction Delete:\n  pre: visible == true\n  post:\n    visible = false\n    deleted = true\nproperty P_VISIBLE_ONLY_AFTER_DELETE:\n  invariant: visible == false\nproperty C_DELETED_VIEW:\n  cover: deleted_view\n";
        let scoped = check_source(&CheckRequest {
            request_id: "req-scenario-pass".to_string(),
            source_name: "post.valid".to_string(),
            source: source.to_string(),
            property_id: Some("P_VISIBLE_ONLY_AFTER_DELETE".to_string()),
            scenario_id: Some("DeletedPost".to_string()),
            seed: None,
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        });
        let CheckOutcome::Completed(scoped_result) = scoped else {
            panic!("expected scoped result");
        };
        assert_eq!(
            scoped_result.property_result.scenario_id.as_deref(),
            Some("DeletedPost")
        );
        assert!(!scoped_result.property_result.vacuous);
        assert_eq!(scoped_result.status, crate::engine::RunStatus::Pass);

        let cover = check_source(&CheckRequest {
            request_id: "req-cover-pass".to_string(),
            source_name: "post.valid".to_string(),
            source: source.to_string(),
            property_id: Some("C_DELETED_VIEW".to_string()),
            scenario_id: Some("DeletedPost".to_string()),
            seed: None,
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        });
        let CheckOutcome::Completed(cover_result) = cover else {
            panic!("expected cover result");
        };
        assert_eq!(cover_result.status, crate::engine::RunStatus::Pass);
        let outcome_json = crate::evidence::render_outcome_json(
            "post.valid",
            &CheckOutcome::Completed(cover_result),
        );
        assert!(outcome_json.contains("\"scenario_id\":\"DeletedPost\""));
        assert!(outcome_json.contains("\"vacuous\":false"));
    }

    #[test]
    fn check_can_fail_deadlock_freedom_property() {
        let outcome = check_source(&CheckRequest {
            request_id: "req-deadlock-fail".to_string(),
            source_name: "deadlock.valid".to_string(),
            source: "model A\nstate:\n  x: u8[0..1]\ninit:\n  x = 0\naction Advance:\n  pre: x == 0\n  post:\n    x = 1\nproperty P_LIVE: deadlock_freedom\n".to_string(),
            property_id: Some("P_LIVE".to_string()),
            scenario_id: None,
            backend: None,
            solver_executable: None,
            solver_args: vec![],
            seed: None,
        });
        let CheckOutcome::Completed(result) = outcome else {
            panic!("expected completed");
        };
        assert_eq!(result.status, crate::engine::RunStatus::Fail);
        assert_eq!(
            result.property_result.property_kind,
            crate::ir::PropertyKind::DeadlockFreedom
        );
        assert_eq!(
            result.property_result.reason_code.as_deref(),
            Some("DEADLOCK_REACHED")
        );
        assert!(result.trace.is_some());
    }

    #[test]
    fn check_can_pass_deadlock_freedom_property() {
        let outcome = check_source(&CheckRequest {
            request_id: "req-deadlock-pass".to_string(),
            source_name: "deadlock.valid".to_string(),
            source: "model A\nstate:\n  x: u8[0..1]\ninit:\n  x = 0\naction Stay:\n  pre: true\n  post:\n    x = x\nproperty P_LIVE: deadlock_freedom\n".to_string(),
            property_id: Some("P_LIVE".to_string()),
            scenario_id: None,
            backend: None,
            solver_executable: None,
            solver_args: vec![],
            seed: None,
        });
        let CheckOutcome::Completed(result) = outcome else {
            panic!("expected completed");
        };
        assert_eq!(result.status, crate::engine::RunStatus::Pass);
        assert_eq!(
            result.property_result.property_kind,
            crate::ir::PropertyKind::DeadlockFreedom
        );
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
        assert!(response.findings.iter().any(
            |finding| finding.code == "opaque_step_closure" && finding.category == "capability"
        ));
    }

    #[test]
    fn lint_treats_unsupported_declarative_lowering_as_error() {
        let inspect = InspectResponse {
            schema_version: "1.0.0".to_string(),
            request_id: "req".to_string(),
            status: "ok".to_string(),
            model_id: "FizzLike".to_string(),
            machine_ir_ready: false,
            machine_ir_error: Some(
                "unsupported machine guard expression `state.i % 3 == 0`".to_string(),
            ),
            capabilities: InspectCapabilities {
                ir_ready: false,
                solver_ready: false,
                ir: CapabilityDetail {
                    reason:
                        "one or more declarative guards use syntax outside the current machine IR subset"
                            .to_string(),
                    migration_hint: Some(
                        "simplify guard expressions to the current IR subset, or extend lowering support for the reported guard form".to_string(),
                    ),
                    unsupported_features: vec!["guard: state.i % 3 == 0".to_string()],
                },
                solver: CapabilityDetail {
                    reason: "solver backends require machine IR first; blocking IR reason: one or more declarative guards use syntax outside the current machine IR subset".to_string(),
                    migration_hint: Some(
                        "simplify guard expressions to the current IR subset, or extend lowering support for the reported guard form".to_string(),
                    ),
                    unsupported_features: vec!["guard: state.i % 3 == 0".to_string()],
                },
                reasons: vec!["unsupported_machine_guard_expr".to_string()],
                ..InspectCapabilities::fully_ready()
            },
            state_fields: vec!["i".to_string()],
            actions: vec!["STEP".to_string()],
            predicates: vec![],
            scenarios: vec![],
            properties: vec!["P_MOD".to_string()],
            state_field_details: vec![],
            action_details: vec![],
            predicate_details: vec![],
            scenario_details: vec![],
            transition_details: vec![InspectTransition {
                action_id: "STEP".to_string(),
                role: "business".to_string(),
                guard: Some("(i % 3 == 0)".to_string()),
                effect: Some("[]".to_string()),
                reads: vec!["i".to_string()],
                writes: vec!["i".to_string()],
                path_tags: vec![],
                updates: vec![InspectTransitionUpdate {
                    field: "i".to_string(),
                    expr: "(i + 1)".to_string(),
                }],
            }],
            property_details: vec![],
        };
        let lint = lint_from_inspect(&inspect);
        assert!(lint
            .findings
            .iter()
            .any(|finding| finding.code == "unsupported_machine_guard_expr"
                && finding.severity == "error"));
        let warning = explicit_analysis_warning(&inspect).expect("warning");
        assert!(warning.contains("cannot fully lower to machine IR"));
    }

    #[test]
    fn lint_distinguishes_capability_and_maintainability_findings() {
        let inspect = InspectResponse {
            schema_version: "1.0.0".to_string(),
            request_id: "req-maint".to_string(),
            status: "ok".to_string(),
            model_id: "LargeApprovalFlow".to_string(),
            machine_ir_ready: true,
            machine_ir_error: None,
            capabilities: InspectCapabilities {
                reasons: vec!["missing_declarative_transitions".to_string()],
                ..InspectCapabilities::fully_ready()
            },
            state_fields: (0..9).map(|index| format!("field_{index}")).collect(),
            actions: (0..9).map(|index| format!("ACTION_{index}")).collect(),
            predicates: vec![],
            scenarios: vec!["Deleted".to_string()],
            properties: (0..7).map(|index| format!("P_{index}")).collect(),
            state_field_details: vec![],
            action_details: (0..9)
                .map(|index| InspectAction {
                    action_id: format!("ACTION_{index}"),
                    role: if index < 4 {
                        "setup".to_string()
                    } else {
                        "business".to_string()
                    },
                    reads: vec!["approved".to_string()],
                    writes: vec!["approved".to_string()],
                })
                .collect(),
            predicate_details: vec![],
            scenario_details: vec![],
            transition_details: (0..10)
                .map(|index| InspectTransition {
                    action_id: format!("ACTION_{index}"),
                    role: if index < 4 {
                        "setup".to_string()
                    } else {
                        "business".to_string()
                    },
                    guard: Some("state.approved == false && state.retries < 2".to_string()),
                    effect: Some("approved := true".to_string()),
                    reads: vec!["approved".to_string(), "retries".to_string()],
                    writes: vec!["approved".to_string()],
                    path_tags: vec!["allow_path".to_string()],
                    updates: vec![InspectTransitionUpdate {
                        field: "approved".to_string(),
                        expr: "true".to_string(),
                    }],
                })
                .collect(),
            property_details: (0..7)
                .map(|index| InspectProperty {
                    property_id: format!("P_{index}"),
                    kind: "invariant".to_string(),
                    expr: Some("state.approved == false && state.retries < 2".to_string()),
                    scope_expr: None,
                    action_filter: None,
                })
                .collect(),
        };

        let lint = lint_from_inspect_and_source(
            &inspect,
            Some("model ApprovalFlow\nstate:\n  approved: bool\ninit:\n  approved = false\n"),
        );

        assert!(lint
            .findings
            .iter()
            .any(|finding| finding.category == "capability"
                && finding.code == "missing_declarative_transitions"));
        assert!(lint
            .findings
            .iter()
            .any(|finding| finding.category == "maintainability"
                && finding.code == "missing_model_documentation"
                && finding.severity == "warn"));
        assert!(lint
            .findings
            .iter()
            .any(|finding| finding.category == "maintainability"
                && finding.code == "oversized_model"
                && finding.severity == "warn"
                && finding
                    .suggestion
                    .as_deref()
                    .unwrap_or("")
                    .contains("integration model")));
        assert!(lint
            .findings
            .iter()
            .any(|finding| finding.category == "maintainability"
                && finding.code == "setup_heavy_model"
                && finding.severity == "warn"
                && finding
                    .suggestion
                    .as_deref()
                    .unwrap_or("")
                    .contains("integration model")));
        assert!(lint
            .findings
            .iter()
            .any(|finding| finding.category == "maintainability"
                && finding.code == "repeated_condition_without_predicate"
                && finding.severity == "info"
                && finding
                    .suggestion
                    .as_deref()
                    .unwrap_or("")
                    .contains("standalone and integration models")));
        let rendered = render_lint_json(&lint);
        assert!(rendered.contains("\"category\":\"capability\""));
        assert!(rendered.contains("\"category\":\"maintainability\""));
        assert!(rendered.contains("\"severity\":\"info\""));
    }

    #[test]
    fn inspect_json_includes_capability_details_and_reason_codes() {
        let request = InspectRequest {
            request_id: "req-json".to_string(),
            source_name: "rust:counter".to_string(),
            source: String::new(),
        };
        let response = inspect_source(&request).unwrap();
        let json = render_inspect_json(&response);
        assert!(
            json.contains(
                "\"reasons\":[\"missing_declarative_transitions\",\"opaque_step_closure\"]"
            ) || json.contains(
                "\"reasons\":[\"opaque_step_closure\",\"missing_declarative_transitions\"]"
            )
        );
        assert!(json.contains(
            "\"ir\":{\"reason\":\"opaque step models cannot be lowered into machine IR\""
        ));
        assert!(json.contains("\"unsupported_features\":[\"step(state, action)\"]"));
        assert!(json.contains("\"temporal\":{\"property_ids\":[]"));
        assert!(json.contains("\"support_level\":\"not_applicable\""));
    }

    #[test]
    fn inspect_text_snapshot_is_structured_and_readable() {
        let response = InspectResponse {
            schema_version: "1.0.0".to_string(),
            request_id: "req".to_string(),
            status: "ok".to_string(),
            model_id: "PasswordPolicySafeModel".to_string(),
            machine_ir_ready: true,
            machine_ir_error: None,
            capabilities: InspectCapabilities {
                solver_ready: false,
                solver: CapabilityDetail {
                    reason: "solver backends only support the scalar IR subset".to_string(),
                    migration_hint: Some(
                        "replace String-heavy state with finite enums or bounded integers"
                            .to_string(),
                    ),
                    unsupported_features: vec![
                        "regex_match(...)".to_string(),
                        "state field `password`: String".to_string(),
                    ],
                },
                reasons: vec!["string_fields_require_explicit_backend".to_string()],
                ..InspectCapabilities::fully_ready()
            },
            state_fields: vec![
                "password".to_string(),
                "password_set".to_string(),
                "compliant".to_string(),
            ],
            actions: vec!["SET_STRONG_PASSWORD".to_string()],
            predicates: vec![],
            scenarios: vec![],
            properties: vec![
                "P_PASSWORD_POLICY_MATCHES_FLAG".to_string(),
                "P_PASSWORD_LENGTH_BOUND".to_string(),
            ],
            state_field_details: vec![
                super::InspectStateField {
                    name: "password".to_string(),
                    rust_type: "String".to_string(),
                    range: Some("0..=64".to_string()),
                    variants: vec![],
                    is_set: false,
                },
                super::InspectStateField {
                    name: "password_set".to_string(),
                    rust_type: "bool".to_string(),
                    range: None,
                    variants: vec![],
                    is_set: false,
                },
            ],
            action_details: vec![super::InspectAction {
                action_id: "SET_STRONG_PASSWORD".to_string(),
                role: "business".to_string(),
                reads: vec!["password_set".to_string()],
                writes: vec![
                    "password".to_string(),
                    "password_set".to_string(),
                    "compliant".to_string(),
                ],
            }],
            predicate_details: vec![],
            scenario_details: vec![],
            transition_details: vec![InspectTransition {
                action_id: "SET_STRONG_PASSWORD".to_string(),
                role: "business".to_string(),
                guard: Some("state.password_set == false".to_string()),
                effect: Some(
                    "PasswordState { password: \"Str0ngPass!\".to_string(), password_set: true, compliant: true }"
                        .to_string(),
                ),
                reads: vec!["password_set".to_string()],
                writes: vec![
                    "password".to_string(),
                    "password_set".to_string(),
                    "compliant".to_string(),
                ],
                path_tags: vec![
                    "allow_path".to_string(),
                    "password_policy_path".to_string(),
                ],
                updates: vec![
                    InspectTransitionUpdate {
                        field: "password".to_string(),
                        expr: "\"Str0ngPass!\".to_string()".to_string(),
                    },
                    InspectTransitionUpdate {
                        field: "password_set".to_string(),
                        expr: "true".to_string(),
                    },
                ],
            }],
            property_details: vec![super::InspectProperty {
                property_id: "P_PASSWORD_POLICY_MATCHES_FLAG".to_string(),
                kind: "invariant".to_string(),
                expr: Some(
                    "iff(state.compliant, state.password_set && len(&state.password) >= 10 && regex_match(&state.password, r\"[A-Z]\"))"
                        .to_string(),
                ),
                scope_expr: None,
                action_filter: None,
            }],
        };

        assert_snapshot!(render_inspect_text(&response), @r###"
        model: PasswordPolicySafeModel
        readiness:
        - machine_ir_ready: true
        - capabilities: parse=true explicit=true ir=true solver=false coverage=true explain=true testgen=true
        - capability_reasons: string_fields_require_explicit_backend
        capability_details:
          - solver reason=solver backends only support the scalar IR subset migration_hint=replace String-heavy state with finite enums or bounded integers unsupported_features=[regex_match(...), state field `password`: String]
        summary:
        - state_fields (3): password, password_set, compliant
        - actions (1): SET_STRONG_PASSWORD
        - predicates (0): none
        - scenarios (0): none
        - properties (2): P_PASSWORD_POLICY_MATCHES_FLAG, P_PASSWORD_LENGTH_BOUND
        state_fields:
          ╭──────────────┬────────┬────────┬───────┬──────────╮
          │     name     │  type  │ range  │ shape │ variants │
          ├──────────────┼────────┼────────┼───────┼──────────┤
          │ password     │ String │ 0..=64 │ -     │ -        │
          │ password_set │ bool   │ -      │ -     │ -        │
          ╰──────────────┴────────┴────────┴───────┴──────────╯
        actions:
          ╭─────────────────────┬──────────┬──────────────┬───────────────────────────────────╮
          │       action        │   role   │    reads     │              writes               │
          ├─────────────────────┼──────────┼──────────────┼───────────────────────────────────┤
          │ SET_STRONG_PASSWORD │ business │ password_set │ password, password_set, compliant │
          ╰─────────────────────┴──────────┴──────────────┴───────────────────────────────────╯
        transitions:
        - SET_STRONG_PASSWORD
          role: business
          guard:
            state.password_set == false
          updates:
            - password :=
                "Str0ngPass!".to_string()
            - password_set :=
                true
          reads: password_set
          writes: password, password_set, compliant
          path_tags: allow_path, password_policy_path
        properties:
        - P_PASSWORD_POLICY_MATCHES_FLAG (invariant)
          expr:
            iff(state.compliant,
            state.password_set && len(&state.password) >= 10 && regex_match(&state.password,
            r"[A-Z]"))
        "###);
    }

    #[test]
    fn migration_check_uses_capability_guidance() {
        let request = InspectRequest {
            request_id: "req-migrate".to_string(),
            source_name: "rust:counter".to_string(),
            source: String::new(),
        };
        let inspect = inspect_source(&request).unwrap();
        let lint = lint_from_inspect(&inspect);
        let migration = migration_from_inspect(&inspect, &lint, true);
        let check = migration.check.expect("check");
        assert!(check.next_steps.iter().any(|step| step
            .contains("machine IR blocker: opaque step models cannot be lowered into machine IR")));
        assert!(check
            .next_steps
            .iter()
            .any(|step| step.contains("machine IR unsupported features: step(state, action)")));
    }

    #[test]
    fn check_wraps_frontend_errors_in_error_outcome() {
        let outcome = check_source(&CheckRequest {
            request_id: "req-1".to_string(),
            source_name: "broken.valid".to_string(),
            source: "model A\nstate:\n  x: u8[0..7]\ninit:\n  y = 0\n".to_string(),
            property_id: None,
            scenario_id: None,
            seed: None,
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
            scenario_id: None,
            seed: Some(41),
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        };
        validate_explain_request(&request).unwrap();
        let response = explain_source(&request).unwrap();
        assert_eq!(response.property_id, "P_SAFE");
        assert_eq!(response.breakpoint_kind, "action_boundary");
        assert_eq!(response.failure_step_index, 0);
        assert_eq!(response.changed_fields, vec!["x".to_string()]);
        assert_eq!(response.involved_fields, vec!["x".to_string()]);
        assert_eq!(response.review_context.vacuous, false);
        assert!(!response.decision_path.decisions.is_empty());
        assert!(!response.field_diffs.is_empty());
        assert!(!response.guard_reviews.is_empty());
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
        assert!(response
            .repair_targets
            .iter()
            .any(|target| target.target == "model_fix"));
        assert!(response
            .repair_targets
            .iter()
            .any(|target| target.target == "implementation_fix"));
        assert!(super::render_explain_json(&response).contains("\"review_context\""));
        assert!(super::render_explain_json(&response).contains("\"decision_path\""));
        validate_explain_response(&response).unwrap();
    }

    #[test]
    fn explain_uses_reachability_specific_wording() {
        let source = "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_REACH:\n  reachability: x == 2\n";
        let request = CheckRequest {
            request_id: "req-explain-reach".to_string(),
            source_name: "a.valid".to_string(),
            source: source.to_string(),
            property_id: Some("P_REACH".to_string()),
            scenario_id: None,
            backend: None,
            solver_executable: None,
            solver_args: vec![],
            seed: None,
        };
        let response = explain_source(&request).unwrap();
        assert!(response
            .repair_hints
            .iter()
            .any(|hint| hint.contains("verify reachability property P_REACH is intended")));
        assert!(response
            .repair_targets
            .iter()
            .any(|target| target.target == "requirement_fix"));
        assert!(response
            .candidate_causes
            .iter()
            .any(|cause| cause.message.contains("target state")));
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
            seed: Some(43),
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
            seed: None,
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
            seed: None,
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
    fn witness_testgen_uses_reachability_trace_when_available() {
        let source = "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_REACH:\n  reachability: x == 2\n";
        let response = testgen_source(&TestgenRequest {
            request_id: "req-witness-reach".to_string(),
            source_name: "a.valid".to_string(),
            source: source.to_string(),
            property_id: Some("P_REACH".to_string()),
            strategy: "witness".to_string(),
            backend: None,
            solver_executable: None,
            solver_args: vec![],
            seed: None,
        })
        .unwrap();
        assert_eq!(response.vector_ids.len(), 1);
        assert_eq!(response.vectors[0].source_kind, "witness");
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
            seed: None,
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
        assert_eq!(response.capabilities.temporal.status, "unavailable");
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
        assert_eq!(response.capabilities.temporal.status, "unavailable");
    }

    #[test]
    fn inspect_reports_temporal_capability_details() {
        let request = InspectRequest {
            request_id: "req-temporal".to_string(),
            source_name: "temporal.valid".to_string(),
            source: "model TemporalDoor\nstate:\n  open: bool\ninit:\n  open = false\nproperty P_EVENTUAL_OPEN:\n  temporal: eventually(open)\n".to_string(),
        };
        let response = inspect_source(&request).unwrap();
        assert_eq!(
            response.capabilities.temporal.support_level,
            "explicit_only"
        );
        assert_eq!(response.capabilities.temporal.explicit_status, "complete");
        assert_eq!(response.capabilities.temporal.solver_status, "unavailable");
        assert_eq!(
            response.capabilities.temporal.property_ids,
            vec!["P_EVENTUAL_OPEN".to_string()]
        );
        assert_eq!(
            response.capabilities.temporal.operators,
            vec!["eventually".to_string()]
        );
    }

    #[test]
    fn request_validation_rejects_empty_source() {
        let error = validate_check_request(&CheckRequest {
            request_id: "req".to_string(),
            source_name: "a.valid".to_string(),
            source: "".to_string(),
            property_id: None,
            scenario_id: None,
            seed: None,
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
            seed: None,
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
            scenario_id: None,
            seed: Some(47),
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
            scenario_id: None,
            seed: None,
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
    fn check_records_generated_seed_and_platform_metadata() {
        let outcome = check_source(&CheckRequest {
            request_id: "req-seed-auto".to_string(),
            source_name: "seed.valid".to_string(),
            source: "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\nproperty P_SAFE:\n  invariant: x <= 7\n".to_string(),
            property_id: Some("P_SAFE".to_string()),
            scenario_id: None,
            seed: None,
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        });
        let CheckOutcome::Completed(result) = outcome else {
            panic!("expected completed outcome");
        };
        assert_ne!(result.manifest.seed, 0);
        assert!(!result.manifest.platform_metadata.os.is_empty());
        assert!(!result.manifest.platform_metadata.arch.is_empty());
        let json =
            crate::evidence::render_outcome_json("seed.valid", &CheckOutcome::Completed(result));
        assert!(json.contains("\"seed\":"));
        assert!(json.contains("\"platform_metadata\""));
    }

    #[test]
    fn check_is_reproducible_with_explicit_seed() {
        let request = CheckRequest {
            request_id: "req-seed-fixed".to_string(),
            source_name: "seed.valid".to_string(),
            source: "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_SAFE:\n  invariant: x <= 1\n".to_string(),
            property_id: Some("P_SAFE".to_string()),
            scenario_id: None,
            seed: Some(99),
            backend: None,
            solver_executable: None,
            solver_args: vec![],
        };
        let first = check_source(&request);
        let second = check_source(&request);
        let first_json = crate::evidence::render_outcome_json("seed.valid", &first);
        let second_json = crate::evidence::render_outcome_json("seed.valid", &second);
        assert_eq!(first_json, second_json);
    }

    #[test]
    fn orchestrate_returns_one_entry_per_property() {
        let response = orchestrate_source(&OrchestrateRequest {
            request_id: "req-orch".to_string(),
            source_name: "a.valid".to_string(),
            source: "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P1:\n  invariant: x <= 1\nproperty P2:\n  invariant: x <= 7\n".to_string(),
            seed: Some(53),
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
        Some("sat-varisat") => Ok(AdapterConfig::SatVarisat),
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
        Some("sat-varisat") => Ok(AdapterConfig::SatVarisat),
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
        AdapterConfig::SatVarisat => crate::engine::BackendKind::SatVarisat,
    }
}

fn backend_version_for_config(config: &AdapterConfig) -> String {
    solver_backend_version_for_config(config)
}
