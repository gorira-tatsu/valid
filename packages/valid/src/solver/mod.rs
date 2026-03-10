//! Solver capability descriptions and adapter traits.

pub mod smt;
pub mod varisat;

use crate::{
    engine::{
        check_explicit, AssuranceLevel, BackendKind, CheckErrorEnvelope, CheckOutcome, ErrorStatus,
        ExplicitRunResult, PropertyResult, ResourceLimits, RunPlan, RunStatus, SearchStrategy,
        UnknownReason,
    },
    evidence::{
        counterexample_kind_for_property, CounterexampleKind, EvidenceKind, EvidenceTrace,
        TraceStep,
    },
    ir::ModelIr,
    kernel::replay::replay_actions,
    kernel::transition::{apply_action, build_initial_state},
    support::{
        diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode},
        hash::stable_hash_hex,
        schema::require_non_empty,
    },
};
use std::process::Command;

use self::{
    smt::{run_bounded_invariant_check, SmtCliDialect, SmtSolveStatus},
    varisat::{run_bounded_invariant_check_varisat, VarisatSolveStatus},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityMatrix {
    pub backend_name: String,
    pub preferred: bool,
    pub builtin: bool,
    pub compiled_in: bool,
    pub available: bool,
    pub availability_reason: Option<String>,
    pub remediation: Option<String>,
    pub supports_explicit: bool,
    pub supports_bmc: bool,
    pub supports_certificate: bool,
    pub supports_trace: bool,
    pub supports_witness: bool,
    pub selfcheck_compatible: bool,
    pub selfcheck_status: String,
    pub parity_status: String,
    pub temporal: TemporalCapabilityMatrix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalCapabilityMatrix {
    pub status: String,
    pub semantics: String,
    pub fairness_support: String,
    pub fairness_kinds: Vec<String>,
    pub semantics_scope: String,
    pub assurance_levels: Vec<String>,
    pub supported_operators: Vec<String>,
    pub unsupported_operators: Vec<String>,
    pub notes: Vec<String>,
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
pub struct Cvc5Adapter {
    pub executable: String,
    pub args: Vec<String>,
}
pub struct VarisatAdapter;
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
    pub scenario_selection: Option<String>,
    pub horizon: Option<u32>,
    pub encoded_model_hash: String,
    pub strategy: SearchStrategy,
    pub resource_limits: ResourceLimits,
    pub detect_deadlocks: bool,
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
    pub assurance_level: Option<String>,
    pub reason_code: Option<String>,
    pub summary: Option<String>,
    pub unknown_reason: Option<String>,
    pub raw_output: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedRunResult {
    pub outcome: CheckOutcome,
    pub trace: Option<EvidenceTrace>,
}

fn rebase_manifest(
    base: &RunPlan,
    run_id: String,
    backend_name: BackendKind,
    backend_version: String,
) -> crate::engine::RunManifest {
    let mut manifest = base.manifest.clone();
    manifest.run_id = run_id;
    manifest.backend_name = backend_name;
    manifest.backend_version = backend_version;
    manifest
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterConfig {
    Explicit,
    MockBmc,
    SmtCvc5 {
        executable: String,
        args: Vec<String>,
    },
    SatVarisat,
    Command {
        backend_name: String,
        executable: String,
        args: Vec<String>,
    },
}

pub const fn sat_varisat_compiled_in() -> bool {
    cfg!(feature = "varisat-backend")
}

pub fn mcp_backend_names() -> Vec<&'static str> {
    let mut backends = vec!["explicit", "mock-bmc", "smt-cvc5", "command"];
    if sat_varisat_compiled_in() {
        backends.insert(1, "sat-varisat");
    }
    backends
}

pub fn render_capability_matrix_json(matrix: &CapabilityMatrix) -> String {
    format!(
        "{{\"backend\":\"{}\",\"capabilities\":{{\"backend_name\":\"{}\",\"preferred\":{},\"builtin\":{},\"compiled_in\":{},\"available\":{},\"availability_reason\":{},\"remediation\":{},\"supports_explicit\":{},\"supports_bmc\":{},\"supports_certificate\":{},\"supports_trace\":{},\"supports_witness\":{},\"selfcheck_compatible\":{},\"selfcheck_status\":\"{}\",\"parity_status\":\"{}\",\"temporal\":{}}}}}",
        matrix.backend_name,
        matrix.backend_name,
        matrix.preferred,
        matrix.builtin,
        matrix.compiled_in,
        matrix.available,
        render_optional_string(matrix.availability_reason.as_deref()),
        render_optional_string(matrix.remediation.as_deref()),
        matrix.supports_explicit,
        matrix.supports_bmc,
        matrix.supports_certificate,
        matrix.supports_trace,
        matrix.supports_witness,
        matrix.selfcheck_compatible,
        matrix.selfcheck_status,
        matrix.parity_status,
        render_temporal_capability_json(&matrix.temporal),
    )
}

pub fn validate_capability_matrix(matrix: &CapabilityMatrix) -> Result<(), String> {
    require_non_empty(&matrix.backend_name, "backend_name")?;
    if let Some(reason) = &matrix.availability_reason {
        require_non_empty(reason, "availability_reason")?;
    }
    if let Some(remediation) = &matrix.remediation {
        require_non_empty(remediation, "remediation")?;
    }
    require_non_empty(&matrix.selfcheck_status, "selfcheck_status")?;
    require_non_empty(&matrix.parity_status, "parity_status")?;
    require_non_empty(&matrix.temporal.status, "temporal.status")?;
    require_non_empty(&matrix.temporal.semantics, "temporal.semantics")?;
    Ok(())
}

fn render_optional_string(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")),
        None => "null".to_string(),
    }
}

fn render_temporal_capability_json(temporal: &TemporalCapabilityMatrix) -> String {
    format!(
        "{{\"status\":\"{}\",\"semantics\":\"{}\",\"assurance_levels\":{},\"supported_operators\":{},\"unsupported_operators\":{},\"notes\":{}}}",
        temporal.status,
        temporal.semantics,
        render_string_array(&temporal.assurance_levels),
        render_string_array(&temporal.supported_operators),
        render_string_array(&temporal.unsupported_operators),
        render_string_array(&temporal.notes),
    )
}

fn render_string_array(values: &[String]) -> String {
    let body = values
        .iter()
        .map(|value| format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{}]", body)
}

fn all_temporal_operators() -> Vec<String> {
    vec![
        "always".to_string(),
        "eventually".to_string(),
        "next".to_string(),
        "until".to_string(),
    ]
}

fn complete_temporal_support(notes: Vec<String>) -> TemporalCapabilityMatrix {
    TemporalCapabilityMatrix {
        status: "complete".to_string(),
        semantics: "reachable_graph_fixpoint".to_string(),
        fairness_support: "supported".to_string(),
        fairness_kinds: vec!["weak".to_string(), "strong".to_string()],
        semantics_scope: "reachable_graph".to_string(),
        assurance_levels: vec!["complete".to_string(), "bounded".to_string()],
        supported_operators: all_temporal_operators(),
        unsupported_operators: Vec::new(),
        notes,
    }
}

fn bounded_temporal_support(notes: Vec<String>) -> TemporalCapabilityMatrix {
    TemporalCapabilityMatrix {
        status: "bounded".to_string(),
        semantics: "depth_bounded_search".to_string(),
        fairness_support: "unsupported".to_string(),
        fairness_kinds: Vec::new(),
        semantics_scope: "bounded_lasso".to_string(),
        assurance_levels: vec!["bounded".to_string()],
        supported_operators: all_temporal_operators(),
        unsupported_operators: Vec::new(),
        notes,
    }
}

fn unavailable_temporal_support(note: impl Into<String>) -> TemporalCapabilityMatrix {
    TemporalCapabilityMatrix {
        status: "unavailable".to_string(),
        semantics: "unavailable".to_string(),
        fairness_support: "unsupported".to_string(),
        fairness_kinds: Vec::new(),
        semantics_scope: "not_available".to_string(),
        assurance_levels: Vec::new(),
        supported_operators: Vec::new(),
        unsupported_operators: all_temporal_operators(),
        notes: vec![note.into()],
    }
}

pub fn capabilities_for_config(config: &AdapterConfig) -> CapabilityMatrix {
    match config {
        AdapterConfig::Explicit => ExplicitAdapter.capabilities(),
        AdapterConfig::MockBmc => MockBmcAdapter.capabilities(),
        AdapterConfig::SmtCvc5 { executable, args } => Cvc5Adapter {
            executable: executable.clone(),
            args: args.clone(),
        }
        .capabilities(),
        AdapterConfig::SatVarisat => VarisatAdapter.capabilities(),
        AdapterConfig::Command {
            backend_name,
            executable,
            args,
        } => CommandSolverAdapter {
            backend_name: backend_name.clone(),
            executable: executable.clone(),
            args: args.clone(),
        }
        .capabilities(),
    }
}

pub fn backend_version_for_config(config: &AdapterConfig) -> String {
    match config {
        AdapterConfig::Explicit | AdapterConfig::MockBmc | AdapterConfig::SatVarisat => {
            env!("CARGO_PKG_VERSION").to_string()
        }
        AdapterConfig::SmtCvc5 { executable, .. } => {
            detect_external_backend_version(executable, &["--version", "-V"])
        }
        AdapterConfig::Command { executable, .. } => {
            detect_external_backend_version(executable, &["--version", "-V", "version"])
        }
    }
}

pub fn run_with_adapter(
    model: &ModelIr,
    run_plan: &RunPlan,
    config: &AdapterConfig,
) -> Result<NormalizedRunResult, String> {
    match config {
        AdapterConfig::Explicit => {
            let adapter = ExplicitAdapter;
            let plan = adapter.build_plan(model, run_plan)?;
            let raw = adapter.run(model, &plan)?;
            adapter.normalize(model, run_plan, raw)
        }
        AdapterConfig::MockBmc => {
            let adapter = MockBmcAdapter;
            let plan = adapter.build_plan(model, run_plan)?;
            let raw = adapter.run(model, &plan)?;
            adapter.normalize(model, run_plan, raw)
        }
        AdapterConfig::SmtCvc5 { executable, args } => {
            let adapter = Cvc5Adapter {
                executable: executable.clone(),
                args: args.clone(),
            };
            let plan = adapter.build_plan(model, run_plan)?;
            let raw = adapter.run(model, &plan)?;
            adapter.normalize(model, run_plan, raw)
        }
        AdapterConfig::SatVarisat => {
            let adapter = VarisatAdapter;
            let plan = adapter.build_plan(model, run_plan)?;
            let raw = adapter.run(model, &plan)?;
            adapter.normalize(model, run_plan, raw)
        }
        AdapterConfig::Command {
            backend_name,
            executable,
            args,
        } => {
            let adapter = CommandSolverAdapter {
                backend_name: backend_name.clone(),
                executable: executable.clone(),
                args: args.clone(),
            };
            let plan = adapter.build_plan(model, run_plan)?;
            let raw = adapter.run(model, &plan)?;
            adapter.normalize(model, run_plan, raw)
        }
    }
}

fn detect_external_backend_version(executable: &str, version_args: &[&str]) -> String {
    for arg in version_args {
        let Ok(output) = Command::new(executable).arg(arg).output() else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if let Some(version) = normalize_backend_version_output(&stdout, &stderr) {
            return version;
        }
    }
    "external:unknown".to_string()
}

fn normalize_backend_version_output(stdout: &str, stderr: &str) -> Option<String> {
    stdout
        .lines()
        .chain(stderr.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

impl SolverAdapter for ExplicitAdapter {
    fn backend_kind(&self) -> BackendKind {
        BackendKind::Explicit
    }

    fn capabilities(&self) -> CapabilityMatrix {
        CapabilityMatrix {
            backend_name: "explicit".to_string(),
            preferred: false,
            builtin: true,
            compiled_in: true,
            available: true,
            availability_reason: None,
            remediation: None,
            supports_explicit: true,
            supports_bmc: false,
            supports_certificate: false,
            supports_trace: true,
            supports_witness: true,
            selfcheck_compatible: true,
            selfcheck_status: "verified".to_string(),
            parity_status: "reference".to_string(),
            temporal: complete_temporal_support(vec![
                "evaluated over the explored reachable graph with fixpoint semantics".to_string(),
                "when max_depth is configured, the same operators remain available but the assurance level becomes bounded".to_string(),
            ]),
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
            scenario_selection: run_plan.scenario_selection.clone(),
            horizon: run_plan.search_bounds.max_depth.map(|value| value as u32),
            encoded_model_hash: format!("encoded:{}", run_plan.manifest.source_hash),
            strategy: run_plan.strategy,
            resource_limits: run_plan.resource_limits.clone(),
            detect_deadlocks: run_plan.detect_deadlocks,
        })
    }

    fn run(&self, model: &ModelIr, plan: &SolverRunPlan) -> Result<RawSolverResult, String> {
        let mut run_plan = RunPlan::default();
        run_plan.manifest.run_id = plan.run_id.clone();
        if let Some(property_id) = plan.target_property_ids.first() {
            run_plan.property_selection =
                crate::engine::PropertySelection::ExactlyOne(property_id.clone());
        }
        run_plan.scenario_selection = plan.scenario_selection.clone();
        run_plan.strategy = plan.strategy;
        run_plan.search_bounds.max_depth = plan.horizon.map(|value| value as usize);
        run_plan.resource_limits = plan.resource_limits.clone();
        run_plan.detect_deadlocks = plan.detect_deadlocks;
        Ok(RawSolverResult::Explicit(check_explicit(model, &run_plan)))
    }

    fn normalize(
        &self,
        _model: &ModelIr,
        run_plan: &RunPlan,
        raw: RawSolverResult,
    ) -> Result<NormalizedRunResult, String> {
        match raw {
            RawSolverResult::Explicit(outcome) => {
                let outcome = match outcome {
                    CheckOutcome::Completed(mut result) => {
                        result.manifest = rebase_manifest(
                            run_plan,
                            result.manifest.run_id.clone(),
                            BackendKind::Explicit,
                            run_plan.manifest.backend_version.clone(),
                        );
                        CheckOutcome::Completed(result)
                    }
                    CheckOutcome::Errored(mut error) => {
                        error.manifest = rebase_manifest(
                            run_plan,
                            error.manifest.run_id.clone(),
                            BackendKind::Explicit,
                            run_plan.manifest.backend_version.clone(),
                        );
                        CheckOutcome::Errored(error)
                    }
                };
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
            preferred: false,
            builtin: true,
            compiled_in: true,
            available: true,
            availability_reason: None,
            remediation: None,
            supports_explicit: false,
            supports_bmc: true,
            supports_certificate: false,
            supports_trace: true,
            supports_witness: true,
            selfcheck_compatible: false,
            selfcheck_status: "unsupported".to_string(),
            parity_status: "unsupported".to_string(),
            temporal: bounded_temporal_support(vec![
                "temporal properties are checked only within the configured depth bound"
                    .to_string(),
                "PASS means no counterexample was found within the bounded horizon; it is not a complete liveness proof".to_string(),
            ]),
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
            scenario_selection: run_plan.scenario_selection.clone(),
            horizon: run_plan
                .search_bounds
                .max_depth
                .map(|value| value as u32)
                .or(Some(8)),
            encoded_model_hash: format!("bmc:{}", run_plan.manifest.source_hash),
            strategy: run_plan.strategy,
            resource_limits: run_plan.resource_limits.clone(),
            detect_deadlocks: run_plan.detect_deadlocks,
        })
    }

    fn run(&self, model: &ModelIr, plan: &SolverRunPlan) -> Result<RawSolverResult, String> {
        let mut run_plan = RunPlan::default();
        run_plan.manifest.run_id = plan.run_id.clone();
        if let Some(property_id) = plan.target_property_ids.first() {
            run_plan.property_selection =
                crate::engine::PropertySelection::ExactlyOne(property_id.clone());
        }
        run_plan.strategy = plan.strategy;
        run_plan.search_bounds.max_depth = plan.horizon.map(|value| value as usize);
        run_plan.resource_limits = plan.resource_limits.clone();
        run_plan.detect_deadlocks = plan.detect_deadlocks;
        Ok(RawSolverResult::Explicit(check_explicit(model, &run_plan)))
    }

    fn normalize(
        &self,
        _model: &ModelIr,
        run_plan: &RunPlan,
        raw: RawSolverResult,
    ) -> Result<NormalizedRunResult, String> {
        match raw {
            RawSolverResult::Explicit(outcome) => {
                let outcome = match outcome {
                    CheckOutcome::Completed(mut result) => {
                        result.manifest = rebase_manifest(
                            run_plan,
                            result.manifest.run_id.clone(),
                            BackendKind::MockBmc,
                            run_plan.manifest.backend_version.clone(),
                        );
                        CheckOutcome::Completed(result)
                    }
                    CheckOutcome::Errored(mut error) => {
                        error.manifest = rebase_manifest(
                            run_plan,
                            error.manifest.run_id.clone(),
                            BackendKind::MockBmc,
                            run_plan.manifest.backend_version.clone(),
                        );
                        CheckOutcome::Errored(error)
                    }
                };
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

impl SolverAdapter for Cvc5Adapter {
    fn backend_kind(&self) -> BackendKind {
        BackendKind::SmtCvc5
    }

    fn capabilities(&self) -> CapabilityMatrix {
        CapabilityMatrix {
            backend_name: "smt-cvc5".to_string(),
            preferred: false,
            builtin: false,
            compiled_in: false,
            available: true,
            availability_reason: None,
            remediation: None,
            supports_explicit: false,
            supports_bmc: true,
            supports_certificate: false,
            supports_trace: true,
            supports_witness: true,
            selfcheck_compatible: false,
            selfcheck_status: "unsupported".to_string(),
            parity_status: "experimental".to_string(),
            temporal: unavailable_temporal_support(
                "SMT adapter does not yet lower temporal expressions; use backend=explicit or mock-bmc for bounded temporal checks",
            ),
        }
    }

    fn build_plan(&self, _model: &ModelIr, run_plan: &RunPlan) -> Result<SolverRunPlan, String> {
        let target_property_ids = match &run_plan.property_selection {
            crate::engine::PropertySelection::ExactlyOne(id) => vec![id.clone()],
        };
        Ok(SolverRunPlan {
            run_id: format!("{}-cvc5", run_plan.manifest.run_id),
            backend: BackendKind::SmtCvc5,
            target_property_ids,
            scenario_selection: run_plan.scenario_selection.clone(),
            horizon: run_plan
                .search_bounds
                .max_depth
                .map(|value| value as u32)
                .or(Some(16)),
            encoded_model_hash: format!("cvc5:{}", run_plan.manifest.source_hash),
            strategy: run_plan.strategy,
            resource_limits: run_plan.resource_limits.clone(),
            detect_deadlocks: run_plan.detect_deadlocks,
        })
    }

    fn run(&self, _model: &ModelIr, plan: &SolverRunPlan) -> Result<RawSolverResult, String> {
        let horizon = plan.horizon.unwrap_or(16) as usize;
        match run_bounded_invariant_check(
            &self.executable,
            &self.args,
            &plan.run_id,
            SmtCliDialect::Cvc5,
            _model,
            &plan.target_property_ids,
            horizon,
        )? {
            SmtSolveStatus::Sat(actions) => Ok(RawSolverResult::Protocol(CommandProtocolResult {
                status: "FAIL".to_string(),
                actions,
                assurance_level: Some("BOUNDED".to_string()),
                reason_code: Some("CVC5_COUNTEREXAMPLE".to_string()),
                summary: Some(format!(
                    "cvc5 found a counterexample within depth {}",
                    horizon
                )),
                unknown_reason: None,
                raw_output: "sat".to_string(),
            })),
            SmtSolveStatus::Unsat => Ok(RawSolverResult::Protocol(CommandProtocolResult {
                status: "PASS".to_string(),
                actions: Vec::new(),
                assurance_level: Some("BOUNDED".to_string()),
                reason_code: Some("CVC5_BOUNDED_NO_COUNTEREXAMPLE".to_string()),
                summary: Some(format!(
                    "cvc5 found no counterexample within depth {}",
                    horizon
                )),
                unknown_reason: None,
                raw_output: "unsat".to_string(),
            })),
            SmtSolveStatus::Unknown => Ok(RawSolverResult::Protocol(CommandProtocolResult {
                status: "UNKNOWN".to_string(),
                actions: Vec::new(),
                assurance_level: Some("INCOMPLETE".to_string()),
                reason_code: Some("CVC5_UNKNOWN".to_string()),
                summary: Some("cvc5 returned unknown".to_string()),
                unknown_reason: Some("ENGINE_ABORTED".to_string()),
                raw_output: "unknown".to_string(),
            })),
        }
    }

    fn normalize(
        &self,
        model: &ModelIr,
        run_plan: &RunPlan,
        raw: RawSolverResult,
    ) -> Result<NormalizedRunResult, String> {
        match raw {
            RawSolverResult::Protocol(protocol) => {
                let mut normalized = normalize_protocol_result(model, run_plan, protocol)?;
                rebase_normalized_outcome(
                    &mut normalized.outcome,
                    run_plan,
                    BackendKind::SmtCvc5,
                    run_plan.manifest.backend_version.clone(),
                );
                Ok(normalized)
            }
            RawSolverResult::Explicit(_) => {
                Err("smt-cvc5 adapter cannot normalize explicit results".to_string())
            }
        }
    }
}

impl SolverAdapter for VarisatAdapter {
    fn backend_kind(&self) -> BackendKind {
        BackendKind::SatVarisat
    }

    fn capabilities(&self) -> CapabilityMatrix {
        CapabilityMatrix {
            backend_name: "sat-varisat".to_string(),
            preferred: true,
            builtin: true,
            compiled_in: cfg!(feature = "varisat-backend"),
            available: cfg!(feature = "varisat-backend"),
            availability_reason: if cfg!(feature = "varisat-backend") {
                None
            } else {
                Some("this binary was built without the varisat-backend feature".to_string())
            },
            remediation: if cfg!(feature = "varisat-backend") {
                None
            } else {
                Some(
                    "reinstall or rebuild valid with `--features varisat-backend`, or use `cargo valid --backend=sat-varisat` so the feature is added automatically".to_string(),
                )
            },
            supports_explicit: false,
            supports_bmc: true,
            supports_certificate: false,
            supports_trace: true,
            supports_witness: true,
            selfcheck_compatible: true,
            selfcheck_status: if cfg!(feature = "varisat-backend") {
                "verifiable".to_string()
            } else {
                "unavailable".to_string()
            },
            parity_status: if cfg!(feature = "varisat-backend") {
                "ready".to_string()
            } else {
                "unavailable".to_string()
            },
            temporal: unavailable_temporal_support(
                "SAT adapter does not yet lower temporal expressions; use backend=explicit or mock-bmc for bounded temporal checks",
            ),
        }
    }

    fn build_plan(&self, _model: &ModelIr, run_plan: &RunPlan) -> Result<SolverRunPlan, String> {
        let target_property_ids = match &run_plan.property_selection {
            crate::engine::PropertySelection::ExactlyOne(id) => vec![id.clone()],
        };
        Ok(SolverRunPlan {
            run_id: format!("{}-varisat", run_plan.manifest.run_id),
            backend: BackendKind::SatVarisat,
            target_property_ids,
            scenario_selection: run_plan.scenario_selection.clone(),
            horizon: run_plan
                .search_bounds
                .max_depth
                .map(|value| value as u32)
                .or(Some(16)),
            encoded_model_hash: format!("varisat:{}", run_plan.manifest.source_hash),
            strategy: run_plan.strategy,
            resource_limits: run_plan.resource_limits.clone(),
            detect_deadlocks: run_plan.detect_deadlocks,
        })
    }

    fn run(&self, model: &ModelIr, plan: &SolverRunPlan) -> Result<RawSolverResult, String> {
        let horizon = plan.horizon.unwrap_or(16) as usize;
        match run_bounded_invariant_check_varisat(model, &plan.target_property_ids, horizon)? {
            VarisatSolveStatus::Sat(actions) => {
                Ok(RawSolverResult::Protocol(CommandProtocolResult {
                    status: "FAIL".to_string(),
                    actions,
                    assurance_level: Some("BOUNDED".to_string()),
                    reason_code: Some("VARISAT_COUNTEREXAMPLE".to_string()),
                    summary: Some(format!(
                        "varisat found a counterexample within depth {}",
                        horizon
                    )),
                    unknown_reason: None,
                    raw_output: "sat".to_string(),
                }))
            }
            VarisatSolveStatus::Unsat => Ok(RawSolverResult::Protocol(CommandProtocolResult {
                status: "PASS".to_string(),
                actions: Vec::new(),
                assurance_level: Some("BOUNDED".to_string()),
                reason_code: Some("VARISAT_BOUNDED_NO_COUNTEREXAMPLE".to_string()),
                summary: Some(format!(
                    "varisat found no counterexample within depth {}",
                    horizon
                )),
                unknown_reason: None,
                raw_output: "unsat".to_string(),
            })),
            VarisatSolveStatus::Unknown => Ok(RawSolverResult::Protocol(CommandProtocolResult {
                status: "UNKNOWN".to_string(),
                actions: Vec::new(),
                assurance_level: Some("INCOMPLETE".to_string()),
                reason_code: Some("VARISAT_UNKNOWN".to_string()),
                summary: Some("varisat returned unknown".to_string()),
                unknown_reason: Some("ENGINE_ABORTED".to_string()),
                raw_output: "unknown".to_string(),
            })),
        }
    }

    fn normalize(
        &self,
        model: &ModelIr,
        run_plan: &RunPlan,
        raw: RawSolverResult,
    ) -> Result<NormalizedRunResult, String> {
        match raw {
            RawSolverResult::Protocol(protocol) => {
                let mut normalized = normalize_protocol_result(model, run_plan, protocol)?;
                rebase_normalized_outcome(
                    &mut normalized.outcome,
                    run_plan,
                    BackendKind::SatVarisat,
                    run_plan.manifest.backend_version.clone(),
                );
                Ok(normalized)
            }
            RawSolverResult::Explicit(_) => {
                Err("sat-varisat adapter cannot normalize explicit results".to_string())
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
            preferred: false,
            builtin: false,
            compiled_in: false,
            available: true,
            availability_reason: None,
            remediation: None,
            supports_explicit: false,
            supports_bmc: true,
            supports_certificate: false,
            supports_trace: true,
            supports_witness: true,
            selfcheck_compatible: false,
            selfcheck_status: "unsupported".to_string(),
            parity_status: "unsupported".to_string(),
            temporal: unavailable_temporal_support(
                "command backends do not declare temporal semantics in the normalized protocol; advertise temporal support explicitly before relying on them",
            ),
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
            scenario_selection: run_plan.scenario_selection.clone(),
            horizon: run_plan.search_bounds.max_depth.map(|value| value as u32),
            encoded_model_hash: format!("cmd:{}", run_plan.manifest.source_hash),
            strategy: run_plan.strategy,
            resource_limits: run_plan.resource_limits.clone(),
            detect_deadlocks: run_plan.detect_deadlocks,
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
            assurance_level: parse_protocol_value(&body, "ASSURANCE_LEVEL"),
            reason_code: parse_protocol_value(&body, "REASON_CODE"),
            summary: parse_protocol_value(&body, "SUMMARY"),
            unknown_reason: parse_protocol_value(&body, "UNKNOWN_REASON"),
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
                        counterexample_kind: None,
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
                            path: None,
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
    let property_layer = model
        .properties
        .iter()
        .find(|property| property.property_id == property_id)
        .map(|property| property.layer)
        .ok_or_else(|| format!("unknown property `{}`", property_id))?;
    let assurance_level = protocol
        .assurance_level
        .as_deref()
        .map(parse_assurance_level)
        .transpose()?
        .unwrap_or_else(|| {
            if run_plan.search_bounds.max_depth.is_some() {
                AssuranceLevel::Bounded
            } else {
                AssuranceLevel::Incomplete
            }
        });
    let trace = build_protocol_trace(
        model,
        run_plan,
        &property_id,
        protocol_trace_kind(property_kind.clone(), protocol.status.as_str()),
        protocol_counterexample_kind(property_kind, protocol.status.as_str()),
        assurance_level,
        &protocol.actions,
    )?;

    let outcome = match protocol.status.as_str() {
        "PASS" => CheckOutcome::Completed(ExplicitRunResult {
            manifest: run_plan.manifest.clone(),
            status: RunStatus::Pass,
            assurance_level,
            property_result: PropertyResult {
                property_id: property_id.clone(),
                property_kind: property_kind.clone(),
                property_layer,
                status: RunStatus::Pass,
                assurance_level,
                counterexample_kind: None,
                scenario_id: run_plan.scenario_selection.clone(),
                vacuous: false,
                reason_code: Some(
                    protocol
                        .reason_code
                        .clone()
                        .unwrap_or_else(|| "SOLVER_REPORTED_PASS".to_string()),
                ),
                unknown_reason: None,
                terminal_state_id: None,
                evidence_id: trace.as_ref().map(|item| item.evidence_id.clone()),
                summary: protocol
                    .summary
                    .clone()
                    .unwrap_or_else(|| "external solver reported pass".to_string()),
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
                    property_layer,
                    status: RunStatus::Fail,
                    assurance_level,
                    counterexample_kind: counterexample_kind_for_property(property_kind),
                    scenario_id: run_plan.scenario_selection.clone(),
                    vacuous: false,
                    reason_code: Some(
                        protocol
                            .reason_code
                            .clone()
                            .unwrap_or_else(|| "SOLVER_REPORTED_FAIL".to_string()),
                    ),
                    unknown_reason: None,
                    terminal_state_id: Some("s-000001".to_string()),
                    evidence_id: trace.as_ref().map(|item| item.evidence_id.clone()),
                    summary: protocol
                        .summary
                        .clone()
                        .unwrap_or_else(|| "external solver reported fail".to_string()),
                },
                explored_states: 0,
                explored_transitions: trace.as_ref().map(|item| item.steps.len()).unwrap_or(0),
                trace: trace.clone(),
            })
        }
        "UNKNOWN" => CheckOutcome::Completed(ExplicitRunResult {
            manifest: run_plan.manifest.clone(),
            status: RunStatus::Unknown,
            assurance_level,
            property_result: PropertyResult {
                property_id,
                property_kind,
                property_layer,
                status: RunStatus::Unknown,
                assurance_level,
                counterexample_kind: None,
                scenario_id: run_plan.scenario_selection.clone(),
                vacuous: false,
                reason_code: Some(
                    protocol
                        .reason_code
                        .clone()
                        .unwrap_or_else(|| "SOLVER_REPORTED_UNKNOWN".to_string()),
                ),
                unknown_reason: protocol
                    .unknown_reason
                    .as_deref()
                    .map(parse_unknown_reason)
                    .transpose()?
                    .or(Some(UnknownReason::EngineAborted)),
                terminal_state_id: trace.as_ref().map(|_| "s-000001".to_string()),
                evidence_id: trace.as_ref().map(|item| item.evidence_id.clone()),
                summary: protocol
                    .summary
                    .clone()
                    .unwrap_or_else(|| "external solver reported unknown".to_string()),
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

fn build_protocol_trace(
    model: &ModelIr,
    run_plan: &RunPlan,
    property_id: &str,
    evidence_kind: EvidenceKind,
    counterexample_kind: Option<CounterexampleKind>,
    assurance_level: AssuranceLevel,
    actions: &[String],
) -> Result<Option<EvidenceTrace>, String> {
    if actions.is_empty() {
        return Ok(None);
    }

    let mut state = build_initial_state(model).map_err(|diagnostic| diagnostic.message)?;
    let mut steps = Vec::with_capacity(actions.len());
    for (index, action_id) in actions.iter().enumerate() {
        let next = apply_action(model, &state, action_id)
            .map_err(|diagnostic| diagnostic.message)?
            .ok_or_else(|| format!("action `{action_id}` was not enabled during solver replay"))?;
        let action = model
            .actions
            .iter()
            .find(|candidate| candidate.action_id == *action_id);
        steps.push(TraceStep {
            index,
            from_state_id: format!("s-{index:06}"),
            action_id: Some(action_id.clone()),
            action_label: action
                .map(|candidate| candidate.label.clone())
                .or_else(|| Some(action_id.clone())),
            to_state_id: format!("s-{:06}", index + 1),
            depth: (index + 1) as u32,
            state_before: state.as_named_map(model),
            state_after: next.as_named_map(model),
            path: action.map(|candidate| candidate.decision_path()),
            note: if index + 1 == actions.len() {
                Some("normalized from solver adapter".to_string())
            } else {
                None
            },
        });
        state = next;
    }

    Ok(Some(EvidenceTrace {
        schema_version: "1.0.0".to_string(),
        evidence_id: format!("ev-{}", run_plan.manifest.run_id),
        run_id: run_plan.manifest.run_id.clone(),
        property_id: property_id.to_string(),
        evidence_kind,
        counterexample_kind,
        assurance_level,
        trace_hash: stable_hash_hex(&actions.join("\u{1f}")),
        steps,
    }))
}

fn protocol_counterexample_kind(
    property_kind: crate::ir::PropertyKind,
    status: &str,
) -> Option<CounterexampleKind> {
    if status != "FAIL" {
        return None;
    }
    counterexample_kind_for_property(property_kind)
}

fn protocol_trace_kind(property_kind: crate::ir::PropertyKind, status: &str) -> EvidenceKind {
    match (property_kind, status) {
        (crate::ir::PropertyKind::Invariant, "FAIL")
        | (crate::ir::PropertyKind::DeadlockFreedom, "FAIL") => EvidenceKind::Counterexample,
        (crate::ir::PropertyKind::Reachability, "FAIL") => EvidenceKind::Witness,
        _ => EvidenceKind::Trace,
    }
}

fn rebase_normalized_outcome(
    outcome: &mut CheckOutcome,
    run_plan: &RunPlan,
    backend_name: BackendKind,
    backend_version: String,
) {
    match outcome {
        CheckOutcome::Completed(result) => {
            result.manifest = rebase_manifest(
                run_plan,
                result.manifest.run_id.clone(),
                backend_name,
                backend_version,
            );
        }
        CheckOutcome::Errored(error) => {
            error.manifest = rebase_manifest(
                run_plan,
                error.manifest.run_id.clone(),
                backend_name,
                backend_version,
            );
        }
    }
}

fn parse_assurance_level(value: &str) -> Result<AssuranceLevel, String> {
    match value {
        "COMPLETE" => Ok(AssuranceLevel::Complete),
        "BOUNDED" => Ok(AssuranceLevel::Bounded),
        "INCOMPLETE" => Ok(AssuranceLevel::Incomplete),
        other => Err(format!("unsupported ASSURANCE_LEVEL `{other}`")),
    }
}

fn parse_unknown_reason(value: &str) -> Result<UnknownReason, String> {
    match value {
        "UNKNOWN_ENGINE_ABORTED" | "ENGINE_ABORTED" => Ok(UnknownReason::EngineAborted),
        "UNKNOWN_STATE_LIMIT_REACHED" | "STATE_LIMIT_REACHED" => {
            Ok(UnknownReason::StateLimitReached)
        }
        "UNKNOWN_DEPTH_LIMIT_REACHED" | "DEPTH_LIMIT_REACHED" => Ok(UnknownReason::EngineAborted),
        "UNKNOWN_TIME_LIMIT_REACHED" | "TIME_LIMIT_REACHED" => Ok(UnknownReason::TimeLimitReached),
        other => Err(format!("unsupported UNKNOWN_REASON `{other}`")),
    }
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
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use crate::{
        engine::{PropertySelection, RunPlan, UnknownReason},
        frontend::compile_model,
    };

    use super::{
        backend_version_for_config, render_capability_matrix_json, validate_capability_matrix,
        AdapterConfig, CommandSolverAdapter, Cvc5Adapter, ExplicitAdapter, MockBmcAdapter,
        SolverAdapter,
    };

    #[cfg(unix)]
    fn write_version_script(body: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "valid-solver-version-{}.sh",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::write(&path, body).expect("script written");
        let mut permissions = fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("permissions updated");
        path
    }

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
        assert!(!caps.preferred);
        assert_eq!(caps.selfcheck_status, "unsupported");
        assert_eq!(caps.temporal.status, "bounded");
        validate_capability_matrix(&caps).unwrap();
        assert!(render_capability_matrix_json(&caps).contains("\"backend\":\"mock-bmc\""));
        assert!(
            render_capability_matrix_json(&caps).contains("\"temporal\":{\"status\":\"bounded\"")
        );
    }

    #[test]
    fn command_adapter_executes_process() {
        let adapter = CommandSolverAdapter {
            backend_name: "cmd".to_string(),
            executable: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'STATUS=UNKNOWN\\nACTIONS=Jump\\nASSURANCE_LEVEL=BOUNDED\\nREASON_CODE=SOLVER_REPORTED_UNKNOWN\\nSUMMARY=command%20backend\\nUNKNOWN_REASON=TIME_LIMIT_REACHED'".to_string(),
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
        let crate::engine::CheckOutcome::Completed(result) = normalized.outcome else {
            panic!("expected completed outcome");
        };
        assert_eq!(
            result.assurance_level,
            crate::engine::AssuranceLevel::Bounded
        );
        assert_eq!(
            result.property_result.reason_code.as_deref(),
            Some("SOLVER_REPORTED_UNKNOWN")
        );
        assert_eq!(
            result.property_result.unknown_reason,
            Some(UnknownReason::TimeLimitReached)
        );
        assert!(result.property_result.summary.contains("command"));
        let trace = normalized.trace.expect("normalized trace");
        assert_eq!(trace.steps.len(), 1);
        assert_eq!(trace.steps[0].action_id.as_deref(), Some("Jump"));
    }

    #[test]
    fn protocol_normalization_preserves_stepwise_actions() {
        let adapter = CommandSolverAdapter {
            backend_name: "cmd".to_string(),
            executable: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "printf 'STATUS=FAIL\\nACTIONS=Step,Jump\\nASSURANCE_LEVEL=BOUNDED\\nREASON_CODE=SOLVER_REPORTED_FAIL\\nSUMMARY=command%20backend'".to_string(),
            ],
        };
        let model = compile_model(
            "model A\nstate:\n  x: u8[0..3]\ninit:\n  x = 0\naction Step:\n  pre: x == 0\n  post:\n    x = 1\naction Jump:\n  pre: x == 1\n  post:\n    x = 3\nproperty P_SAFE:\n  invariant: x <= 2\n",
        )
        .unwrap();
        let mut run_plan = RunPlan::default();
        run_plan.property_selection = PropertySelection::ExactlyOne("P_SAFE".to_string());
        let plan = adapter.build_plan(&model, &run_plan).unwrap();
        let raw = adapter.run(&model, &plan).unwrap();
        let normalized = adapter.normalize(&model, &run_plan, raw).unwrap();
        let trace = normalized.trace.expect("normalized trace");
        assert_eq!(trace.steps.len(), 2);
        assert_eq!(trace.steps[0].action_id.as_deref(), Some("Step"));
        assert_eq!(trace.steps[1].action_id.as_deref(), Some("Jump"));
        assert_eq!(
            trace.steps[1].state_after.get("x"),
            Some(&crate::ir::Value::UInt(3))
        );
    }

    #[test]
    fn cvc5_adapter_normalizes_protocol_result() {
        let adapter = Cvc5Adapter {
            executable: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "input=$(cat); if printf '%s' \"$input\" | grep -q '(declare-fun action_0 () Int)'; then printf 'sat\\n((action_0 0))\\n'; else printf 'unsat\\n'; fi".to_string(),
            ],
        };
        let model = compile_model(
            "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_SAFE:\n  invariant: x <= 1\n",
        )
        .unwrap();
        let mut run_plan = RunPlan::default();
        run_plan.property_selection = PropertySelection::ExactlyOne("P_SAFE".to_string());
        run_plan.search_bounds.max_depth = Some(1);
        let plan = adapter.build_plan(&model, &run_plan).unwrap();
        let raw = adapter.run(&model, &plan).unwrap();
        let normalized = adapter.normalize(&model, &run_plan, raw).unwrap();
        let crate::engine::CheckOutcome::Completed(result) = normalized.outcome else {
            panic!("expected completed outcome");
        };
        assert_eq!(result.status, crate::engine::RunStatus::Fail);
        assert_eq!(
            result.manifest.backend_name,
            crate::engine::BackendKind::SmtCvc5
        );
        assert_eq!(
            result.property_result.reason_code.as_deref(),
            Some("CVC5_COUNTEREXAMPLE")
        );
        assert!(normalized.trace.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn command_backend_version_is_detected() {
        let script = write_version_script(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  printf 'fake-solver 9.9.9\\n'\nelse\n  printf 'STATUS=PASS\\n'\nfi\n",
        );
        let version = backend_version_for_config(&AdapterConfig::Command {
            backend_name: "cmd".to_string(),
            executable: script.display().to_string(),
            args: vec![],
        });
        let _ = fs::remove_file(&script);
        assert_eq!(version, "fake-solver 9.9.9");
    }
}
