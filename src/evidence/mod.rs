//! Evidence and reporting.

use std::collections::BTreeMap;

use crate::{
    engine::{
        explicit::{CheckErrorEnvelope, CheckOutcome, ExplicitRunResult},
        ArtifactPolicy, AssuranceLevel, ErrorStatus, RunStatus, UnknownReason,
    },
    ir::Value,
    support::{
        artifact::{evidence_path, run_result_path, vector_path},
        diagnostics::Diagnostic,
        io::write_text_file,
        json::{
            parse_json, require_array_field, require_number_field, require_object,
            require_string_field,
        },
        schema::{require_non_empty, require_schema_version},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceKind {
    Trace,
    Certificate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceTrace {
    pub schema_version: String,
    pub evidence_id: String,
    pub run_id: String,
    pub property_id: String,
    pub evidence_kind: EvidenceKind,
    pub assurance_level: AssuranceLevel,
    pub trace_hash: String,
    pub steps: Vec<TraceStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceStep {
    pub index: usize,
    pub from_state_id: String,
    pub action_id: Option<String>,
    pub action_label: Option<String>,
    pub to_state_id: String,
    pub depth: u32,
    pub state_before: BTreeMap<String, Value>,
    pub state_after: BTreeMap<String, Value>,
    pub note: Option<String>,
}

pub fn write_outcome_artifacts(
    model_id: &str,
    policy: ArtifactPolicy,
    outcome: &CheckOutcome,
) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    if !should_emit_artifacts(policy, outcome) {
        return Ok(paths);
    }
    validate_outcome(model_id, outcome)?;
    match outcome {
        CheckOutcome::Completed(result) => {
            let result_path = run_result_path(&result.manifest.run_id);
            write_text_file(&result_path, &render_outcome_json(model_id, outcome))?;
            paths.push(result_path);
            if let Some(trace) = &result.trace {
                validate_trace(trace)?;
                let trace_path = evidence_path(&trace.run_id, &trace.evidence_id);
                write_text_file(&trace_path, &render_trace_json(trace))?;
                paths.push(trace_path);
            }
        }
        CheckOutcome::Errored(error) => {
            let result_path = run_result_path(&error.manifest.run_id);
            write_text_file(&result_path, &render_outcome_json(model_id, outcome))?;
            paths.push(result_path);
        }
    }
    Ok(paths)
}

fn should_emit_artifacts(policy: ArtifactPolicy, outcome: &CheckOutcome) -> bool {
    match policy {
        ArtifactPolicy::EmitAll => true,
        ArtifactPolicy::EmitOnFailure => match outcome {
            CheckOutcome::Completed(result) => {
                matches!(result.status, RunStatus::Fail | RunStatus::Unknown)
            }
            CheckOutcome::Errored(_) => true,
        },
        ArtifactPolicy::EmitNothing => match outcome {
            CheckOutcome::Completed(result) => result.status == RunStatus::Fail,
            CheckOutcome::Errored(_) => true,
        },
    }
}

pub fn write_vector_artifact(run_id: &str, vector_id: &str, body: &str) -> Result<String, String> {
    let path = vector_path(run_id, vector_id);
    write_text_file(&path, body)?;
    Ok(path)
}

pub fn validate_trace(trace: &EvidenceTrace) -> Result<(), String> {
    require_schema_version(&trace.schema_version)?;
    require_non_empty(&trace.evidence_id, "evidence_id")?;
    require_non_empty(&trace.run_id, "run_id")?;
    require_non_empty(&trace.property_id, "property_id")?;
    require_non_empty(&trace.trace_hash, "trace_hash")?;
    for (expected_index, step) in trace.steps.iter().enumerate() {
        if step.index != expected_index {
            return Err("trace step indexes must be contiguous and zero-based".to_string());
        }
        require_non_empty(&step.from_state_id, "steps[].from_state_id")?;
        require_non_empty(&step.to_state_id, "steps[].to_state_id")?;
    }
    Ok(())
}

pub fn validate_outcome(model_id: &str, outcome: &CheckOutcome) -> Result<(), String> {
    require_non_empty(model_id, "model_id")?;
    match outcome {
        CheckOutcome::Completed(result) => {
            validate_manifest(&result.manifest)?;
            validate_property_result(&result.property_result)?;
            if let Some(trace) = &result.trace {
                validate_trace(trace)?;
            }
        }
        CheckOutcome::Errored(error) => {
            validate_manifest(&error.manifest)?;
            if error.diagnostics.is_empty() {
                return Err("error outcome must contain at least one diagnostic".to_string());
            }
        }
    }
    Ok(())
}

fn validate_manifest(manifest: &crate::engine::RunManifest) -> Result<(), String> {
    require_schema_version(&manifest.schema_version)?;
    require_non_empty(&manifest.request_id, "manifest.request_id")?;
    require_non_empty(&manifest.run_id, "manifest.run_id")?;
    require_non_empty(&manifest.source_hash, "manifest.source_hash")?;
    require_non_empty(&manifest.contract_hash, "manifest.contract_hash")?;
    require_non_empty(&manifest.engine_version, "manifest.engine_version")?;
    require_non_empty(&manifest.backend_version, "manifest.backend_version")?;
    Ok(())
}

fn validate_property_result(result: &crate::engine::PropertyResult) -> Result<(), String> {
    require_non_empty(&result.property_id, "property_result.property_id")?;
    require_non_empty(&result.summary, "property_result.summary")?;
    Ok(())
}

pub fn render_trace_json(trace: &EvidenceTrace) -> String {
    let mut out = String::from("{");
    out.push_str(&format!("\"schema_version\":\"{}\"", trace.schema_version));
    out.push_str(&format!(",\"evidence_id\":\"{}\"", trace.evidence_id));
    out.push_str(&format!(",\"run_id\":\"{}\"", trace.run_id));
    out.push_str(&format!(",\"property_id\":\"{}\"", trace.property_id));
    out.push_str(&format!(
        ",\"evidence_kind\":\"{}\"",
        evidence_kind_label(&trace.evidence_kind)
    ));
    out.push_str(&format!(
        ",\"assurance_level\":\"{}\"",
        assurance_label(trace.assurance_level)
    ));
    out.push_str(&format!(",\"trace_hash\":\"{}\"", trace.trace_hash));
    out.push_str(",\"steps\":[");
    for (index, step) in trace.steps.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str(&format!("\"index\":{}", step.index));
        out.push_str(&format!(",\"from_state_id\":\"{}\"", step.from_state_id));
        if let Some(action_id) = &step.action_id {
            out.push_str(&format!(",\"action_id\":\"{}\"", action_id));
        } else {
            out.push_str(",\"action_id\":null");
        }
        if let Some(action_label) = &step.action_label {
            out.push_str(&format!(",\"action_label\":\"{}\"", action_label));
        } else {
            out.push_str(",\"action_label\":null");
        }
        out.push_str(&format!(",\"to_state_id\":\"{}\"", step.to_state_id));
        out.push_str(&format!(",\"depth\":{}", step.depth));
        append_state_map(&mut out, "state_before", &step.state_before);
        append_state_map(&mut out, "state_after", &step.state_after);
        out.push('}');
    }
    out.push_str("]}");
    out
}

pub fn render_outcome_text(outcome: &CheckOutcome) -> String {
    match outcome {
        CheckOutcome::Completed(result) => render_completed_text(result),
        CheckOutcome::Errored(error) => render_error_text(error),
    }
}

pub fn render_outcome_json(model_id: &str, outcome: &CheckOutcome) -> String {
    match outcome {
        CheckOutcome::Completed(result) => render_completed_json(model_id, result),
        CheckOutcome::Errored(error) => render_error_json(model_id, error),
    }
}

pub fn render_diagnostics_json(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::from("{\"diagnostics\":[");
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str(&format!(
            "\"error_code\":\"{}\"",
            diagnostic.error_code.as_str()
        ));
        out.push_str(&format!(",\"segment\":\"{}\"", diagnostic.segment.as_str()));
        out.push_str(&format!(
            ",\"message\":\"{}\"",
            escape_json(&diagnostic.message)
        ));
        if let Some(span) = &diagnostic.primary_span {
            out.push_str(&format!(
                ",\"primary_span\":{{\"source\":\"{}\",\"line\":{},\"column\":{}}}",
                escape_json(&span.source),
                span.line,
                span.column
            ));
        } else {
            out.push_str(",\"primary_span\":null");
        }
        out.push_str(",\"help\":[");
        for (help_index, item) in diagnostic.help.iter().enumerate() {
            if help_index > 0 {
                out.push(',');
            }
            out.push_str(&format!("\"{}\"", escape_json(item)));
        }
        out.push(']');
        out.push_str(",\"best_practices\":[");
        for (best_index, item) in diagnostic.best_practices.iter().enumerate() {
            if best_index > 0 {
                out.push(',');
            }
            out.push_str(&format!("\"{}\"", escape_json(item)));
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}

pub fn validate_rendered_trace_json(body: &str) -> Result<(), String> {
    let root = parse_json(body)?;
    let object = require_object(&root, "trace")?;
    require_string_field(object, "schema_version")?;
    require_string_field(object, "evidence_id")?;
    require_string_field(object, "run_id")?;
    require_string_field(object, "property_id")?;
    require_string_field(object, "evidence_kind")?;
    require_string_field(object, "assurance_level")?;
    require_string_field(object, "trace_hash")?;
    for step in require_array_field(object, "steps")? {
        let step_object = require_object(step, "trace.steps[]")?;
        require_number_field(step_object, "index")?;
        require_string_field(step_object, "from_state_id")?;
        require_string_field(step_object, "to_state_id")?;
        require_number_field(step_object, "depth")?;
    }
    Ok(())
}

pub fn validate_rendered_outcome_json(body: &str) -> Result<(), String> {
    let root = parse_json(body)?;
    let object = require_object(&root, "outcome")?;
    require_string_field(object, "kind")?;
    require_string_field(object, "model_id")?;
    let manifest = require_object(
        object
            .get("manifest")
            .ok_or_else(|| "manifest must be present".to_string())?,
        "manifest",
    )?;
    require_string_field(manifest, "request_id")?;
    require_string_field(manifest, "run_id")?;
    require_string_field(manifest, "schema_version")?;
    require_string_field(manifest, "source_hash")?;
    require_string_field(manifest, "contract_hash")?;
    require_string_field(manifest, "engine_version")?;
    require_string_field(manifest, "backend_name")?;
    require_string_field(manifest, "backend_version")?;
    require_string_field(object, "assurance_level")?;
    Ok(())
}

pub fn validate_rendered_diagnostics_json(body: &str) -> Result<(), String> {
    let root = parse_json(body)?;
    let object = require_object(&root, "diagnostics")?;
    for item in require_array_field(object, "diagnostics")? {
        let diag = require_object(item, "diagnostics[]")?;
        require_string_field(diag, "error_code")?;
        require_string_field(diag, "segment")?;
        require_string_field(diag, "message")?;
    }
    Ok(())
}

fn render_completed_text(result: &ExplicitRunResult) -> String {
    let mut out = String::new();
    out.push_str(match result.status {
        RunStatus::Pass => "PASS explicit\n",
        RunStatus::Fail => "FAIL explicit\n",
        RunStatus::Unknown => "UNKNOWN explicit\n",
    });
    out.push_str(&format!("run_id: {}\n", result.manifest.run_id));
    out.push_str(&format!("request_id: {}\n", result.manifest.request_id));
    out.push_str(&format!(
        "assurance_level: {}\n",
        assurance_label(result.assurance_level)
    ));
    out.push_str(&format!(
        "property_id: {}\n",
        result.property_result.property_id
    ));
    out.push_str(&format!("summary: {}\n", result.property_result.summary));
    out.push_str(&format!("explored_states: {}\n", result.explored_states));
    out.push_str(&format!(
        "explored_transitions: {}\n",
        result.explored_transitions
    ));
    if let Some(reason) = result.property_result.unknown_reason {
        out.push_str(&format!(
            "unknown_reason: {}\n",
            unknown_reason_label(reason)
        ));
    }
    if let Some(trace) = &result.trace {
        out.push_str(&format!("trace_steps: {}\n", trace.steps.len()));
    }
    out
}

fn render_error_text(error: &CheckErrorEnvelope) -> String {
    let mut out = String::new();
    out.push_str("ERROR explicit\n");
    out.push_str(&format!("run_id: {}\n", error.manifest.run_id));
    out.push_str(&format!("request_id: {}\n", error.manifest.request_id));
    out.push_str(&format!(
        "assurance_level: {}\n",
        assurance_label(error.assurance_level)
    ));
    out.push_str(&format!("status: {}\n", error_status_label(error.status)));
    out
}

fn render_completed_json(model_id: &str, result: &ExplicitRunResult) -> String {
    let mut out = String::new();
    out.push('{');
    out.push_str("\"kind\":\"completed\"");
    out.push_str(&format!(",\"model_id\":\"{}\"", escape_json(model_id)));
    append_manifest(&mut out, &result.manifest);
    out.push_str(&format!(",\"status\":\"{}\"", status_label(result.status)));
    out.push_str(&format!(
        ",\"assurance_level\":\"{}\"",
        assurance_label(result.assurance_level)
    ));
    out.push_str(&format!(",\"explored_states\":{}", result.explored_states));
    out.push_str(&format!(
        ",\"explored_transitions\":{}",
        result.explored_transitions
    ));
    out.push_str(",\"property_result\":{");
    out.push_str(&format!(
        "\"property_id\":\"{}\"",
        escape_json(&result.property_result.property_id)
    ));
    out.push_str(&format!(
        ",\"property_kind\":\"{}\"",
        property_kind_label(&result.property_result.property_kind)
    ));
    out.push_str(&format!(
        ",\"status\":\"{}\"",
        status_label(result.property_result.status)
    ));
    out.push_str(&format!(
        ",\"assurance_level\":\"{}\"",
        assurance_label(result.property_result.assurance_level)
    ));
    if let Some(reason_code) = &result.property_result.reason_code {
        out.push_str(&format!(
            ",\"reason_code\":\"{}\"",
            escape_json(reason_code)
        ));
    } else {
        out.push_str(",\"reason_code\":null");
    }
    if let Some(reason) = result.property_result.unknown_reason {
        out.push_str(&format!(
            ",\"unknown_reason\":\"{}\"",
            unknown_reason_label(reason)
        ));
    } else {
        out.push_str(",\"unknown_reason\":null");
    }
    if let Some(terminal_state_id) = &result.property_result.terminal_state_id {
        out.push_str(&format!(
            ",\"terminal_state_id\":\"{}\"",
            escape_json(terminal_state_id)
        ));
    } else {
        out.push_str(",\"terminal_state_id\":null");
    }
    if let Some(evidence_id) = &result.property_result.evidence_id {
        out.push_str(&format!(
            ",\"evidence_id\":\"{}\"",
            escape_json(evidence_id)
        ));
    } else {
        out.push_str(",\"evidence_id\":null");
    }
    out.push_str(&format!(
        ",\"summary\":\"{}\"",
        escape_json(&result.property_result.summary)
    ));
    out.push('}');
    if let Some(trace) = &result.trace {
        out.push_str(&format!(",\"trace\":{}", render_trace_json(trace)));
    } else {
        out.push_str(",\"trace\":null");
    }
    out.push('}');
    out
}

fn render_error_json(model_id: &str, error: &CheckErrorEnvelope) -> String {
    let mut out = String::new();
    out.push('{');
    out.push_str("\"kind\":\"error\"");
    out.push_str(&format!(",\"model_id\":\"{}\"", escape_json(model_id)));
    append_manifest(&mut out, &error.manifest);
    out.push_str(&format!(
        ",\"status\":\"{}\"",
        error_status_label(error.status)
    ));
    out.push_str(&format!(
        ",\"assurance_level\":\"{}\"",
        assurance_label(error.assurance_level)
    ));
    out.push_str(&format!(
        ",\"diagnostics\":{}",
        render_diagnostics_json(&error.diagnostics)
    ));
    out.push('}');
    out
}

fn append_manifest(out: &mut String, manifest: &crate::engine::RunManifest) {
    out.push_str(",\"manifest\":{");
    out.push_str(&format!(
        "\"request_id\":\"{}\"",
        escape_json(&manifest.request_id)
    ));
    out.push_str(&format!(
        ",\"run_id\":\"{}\"",
        escape_json(&manifest.run_id)
    ));
    out.push_str(&format!(
        ",\"schema_version\":\"{}\"",
        escape_json(&manifest.schema_version)
    ));
    out.push_str(&format!(
        ",\"source_hash\":\"{}\"",
        escape_json(&manifest.source_hash)
    ));
    out.push_str(&format!(
        ",\"contract_hash\":\"{}\"",
        escape_json(&manifest.contract_hash)
    ));
    out.push_str(&format!(
        ",\"engine_version\":\"{}\"",
        escape_json(&manifest.engine_version)
    ));
    out.push_str(&format!(
        ",\"backend_name\":\"{}\"",
        backend_label(manifest.backend_name)
    ));
    out.push_str(&format!(
        ",\"backend_version\":\"{}\"",
        escape_json(&manifest.backend_version)
    ));
    if let Some(seed) = manifest.seed {
        out.push_str(&format!(",\"seed\":{}", seed));
    } else {
        out.push_str(",\"seed\":null");
    }
    out.push('}');
}

fn append_state_map(out: &mut String, name: &str, state: &BTreeMap<String, Value>) {
    out.push_str(&format!(",\"{}\":{{", name));
    for (index, (field, value)) in state.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\":{}", escape_json(field), value_json(value)));
    }
    out.push('}');
}

fn status_label(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Pass => "PASS",
        RunStatus::Fail => "FAIL",
        RunStatus::Unknown => "UNKNOWN",
    }
}
fn error_status_label(status: ErrorStatus) -> &'static str {
    match status {
        ErrorStatus::Error => "ERROR",
    }
}
fn property_kind_label(kind: &crate::ir::PropertyKind) -> &'static str {
    match kind {
        crate::ir::PropertyKind::Invariant => "invariant",
    }
}
fn assurance_label(level: AssuranceLevel) -> &'static str {
    match level {
        AssuranceLevel::Complete => "complete",
        AssuranceLevel::Bounded => "bounded",
        AssuranceLevel::Incomplete => "incomplete",
    }
}
fn unknown_reason_label(reason: UnknownReason) -> &'static str {
    match reason {
        UnknownReason::StateLimitReached => "UNKNOWN_STATE_LIMIT_REACHED",
        UnknownReason::TimeLimitReached => "UNKNOWN_TIME_LIMIT_REACHED",
        UnknownReason::EngineAborted => "UNKNOWN_ENGINE_ABORTED",
    }
}
fn backend_label(kind: crate::engine::BackendKind) -> &'static str {
    match kind {
        crate::engine::BackendKind::Explicit => "explicit",
        crate::engine::BackendKind::MockBmc => "mock-bmc",
        crate::engine::BackendKind::SmtCvc5 => "smt-cvc5",
    }
}
fn evidence_kind_label(kind: &EvidenceKind) -> &'static str {
    match kind {
        EvidenceKind::Trace => "trace",
        EvidenceKind::Certificate => "certificate",
    }
}
fn value_json(value: &Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::UInt(value) => value.to_string(),
    }
}
fn escape_json(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::engine::{
        explicit::{CheckOutcome, ExplicitRunResult, PropertyResult},
        AssuranceLevel, BackendKind, RunManifest, RunStatus,
    };

    use super::{
        render_diagnostics_json, render_outcome_json, render_trace_json,
        validate_rendered_diagnostics_json, validate_rendered_outcome_json,
        validate_rendered_trace_json, write_outcome_artifacts, EvidenceKind, EvidenceTrace,
        TraceStep,
    };

    #[test]
    fn renders_completed_outcome_json() {
        let result = ExplicitRunResult {
            manifest: RunManifest {
                request_id: "req-1".to_string(),
                run_id: "run-1".to_string(),
                schema_version: "1.0.0".to_string(),
                source_hash: "sha256:a".to_string(),
                contract_hash: "sha256:b".to_string(),
                engine_version: "0.1.0".to_string(),
                backend_name: BackendKind::Explicit,
                backend_version: "0.1.0".to_string(),
                seed: None,
            },
            status: RunStatus::Fail,
            assurance_level: AssuranceLevel::Complete,
            property_result: PropertyResult {
                property_id: "SAFE".to_string(),
                property_kind: crate::ir::PropertyKind::Invariant,
                status: RunStatus::Fail,
                assurance_level: AssuranceLevel::Complete,
                reason_code: Some("PROPERTY_FAILED".to_string()),
                unknown_reason: None,
                terminal_state_id: Some("s-000001".to_string()),
                evidence_id: Some("ev-1".to_string()),
                summary: "failed".to_string(),
            },
            explored_states: 2,
            explored_transitions: 1,
            trace: Some(EvidenceTrace {
                schema_version: "1.0.0".to_string(),
                evidence_id: "ev-1".to_string(),
                run_id: "run-1".to_string(),
                property_id: "SAFE".to_string(),
                evidence_kind: EvidenceKind::Trace,
                assurance_level: AssuranceLevel::Complete,
                trace_hash: "tracehash".to_string(),
                steps: vec![TraceStep {
                    index: 0,
                    from_state_id: "s-000000".to_string(),
                    action_id: Some("Jump".to_string()),
                    action_label: Some("Jump".to_string()),
                    to_state_id: "s-000001".to_string(),
                    depth: 1,
                    state_before: BTreeMap::from([("x".to_string(), crate::ir::Value::UInt(0))]),
                    state_after: BTreeMap::from([("x".to_string(), crate::ir::Value::UInt(2))]),
                    note: None,
                }],
            }),
        };
        let outcome = CheckOutcome::Completed(result);
        let json = render_outcome_json("Counter", &outcome);
        assert!(json.contains("\"kind\":\"completed\""));
        assert!(json.contains("\"request_id\":\"req-1\""));
        assert!(json.contains("\"status\":\"FAIL\""));
        validate_rendered_outcome_json(&json).unwrap();
    }

    #[test]
    fn writes_artifacts_for_completed_outcome() {
        let result = ExplicitRunResult {
            manifest: RunManifest {
                request_id: "req-1".to_string(),
                run_id: "run-artifact-test".to_string(),
                schema_version: "1.0.0".to_string(),
                source_hash: "sha256:a".to_string(),
                contract_hash: "sha256:b".to_string(),
                engine_version: "0.1.0".to_string(),
                backend_name: BackendKind::Explicit,
                backend_version: "0.1.0".to_string(),
                seed: None,
            },
            status: RunStatus::Fail,
            assurance_level: AssuranceLevel::Complete,
            property_result: PropertyResult {
                property_id: "SAFE".to_string(),
                property_kind: crate::ir::PropertyKind::Invariant,
                status: RunStatus::Fail,
                assurance_level: AssuranceLevel::Complete,
                reason_code: Some("PROPERTY_FAILED".to_string()),
                unknown_reason: None,
                terminal_state_id: Some("s-000001".to_string()),
                evidence_id: Some("ev-artifact-test".to_string()),
                summary: "failed".to_string(),
            },
            explored_states: 2,
            explored_transitions: 1,
            trace: Some(EvidenceTrace {
                schema_version: "1.0.0".to_string(),
                evidence_id: "ev-artifact-test".to_string(),
                run_id: "run-artifact-test".to_string(),
                property_id: "SAFE".to_string(),
                evidence_kind: EvidenceKind::Trace,
                assurance_level: AssuranceLevel::Complete,
                trace_hash: "tracehash".to_string(),
                steps: vec![],
            }),
        };
        let paths = write_outcome_artifacts(
            "Counter",
            crate::engine::ArtifactPolicy::EmitAll,
            &CheckOutcome::Completed(result),
        )
        .unwrap();
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn rendered_trace_and_diagnostics_json_validate() {
        let trace = EvidenceTrace {
            schema_version: "1.0.0".to_string(),
            evidence_id: "ev-1".to_string(),
            run_id: "run-1".to_string(),
            property_id: "SAFE".to_string(),
            evidence_kind: EvidenceKind::Trace,
            assurance_level: AssuranceLevel::Complete,
            trace_hash: "tracehash".to_string(),
            steps: vec![TraceStep {
                index: 0,
                from_state_id: "s-0".to_string(),
                action_id: Some("Jump".to_string()),
                action_label: Some("Jump".to_string()),
                to_state_id: "s-1".to_string(),
                depth: 1,
                state_before: BTreeMap::new(),
                state_after: BTreeMap::new(),
                note: None,
            }],
        };
        validate_rendered_trace_json(&render_trace_json(&trace)).unwrap();
        let diagnostics_json =
            render_diagnostics_json(&[crate::support::diagnostics::Diagnostic::new(
                crate::support::diagnostics::ErrorCode::SearchError,
                crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                "boom",
            )]);
        validate_rendered_diagnostics_json(&diagnostics_json).unwrap();
    }
}
