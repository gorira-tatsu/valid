//! Evidence and reporting.

use std::collections::BTreeMap;

use crate::{
    engine::{
        explicit::{CheckErrorEnvelope, CheckOutcome, ExplicitRunResult},
        ArtifactPolicy, AssuranceLevel, ErrorStatus, RunStatus, UnknownReason,
    },
    ir::{DecisionKind, DecisionOutcome, Path, Value},
    support::{
        artifact::{evidence_path, run_result_path, vector_path},
        artifact_index::{record_artifact, ArtifactRecord},
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
    Counterexample,
    Witness,
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
    pub path: Option<Path>,
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
            record_artifact(ArtifactRecord {
                artifact_kind: "check_result".to_string(),
                path: result_path.clone(),
                run_id: result.manifest.run_id.clone(),
                model_id: Some(model_id.to_string()),
                property_id: Some(result.property_result.property_id.clone()),
                evidence_id: None,
                vector_id: None,
                suite_id: None,
            })?;
            paths.push(result_path);
            if let Some(trace) = &result.trace {
                validate_trace(trace)?;
                let trace_path = evidence_path(&trace.run_id, &trace.evidence_id);
                write_text_file(&trace_path, &render_trace_json(trace))?;
                record_artifact(ArtifactRecord {
                    artifact_kind: "evidence_trace".to_string(),
                    path: trace_path.clone(),
                    run_id: trace.run_id.clone(),
                    model_id: Some(model_id.to_string()),
                    property_id: Some(trace.property_id.clone()),
                    evidence_id: Some(trace.evidence_id.clone()),
                    vector_id: None,
                    suite_id: None,
                })?;
                paths.push(trace_path);
            }
        }
        CheckOutcome::Errored(error) => {
            let result_path = run_result_path(&error.manifest.run_id);
            write_text_file(&result_path, &render_outcome_json(model_id, outcome))?;
            record_artifact(ArtifactRecord {
                artifact_kind: "check_result".to_string(),
                path: result_path.clone(),
                run_id: error.manifest.run_id.clone(),
                model_id: Some(model_id.to_string()),
                property_id: None,
                evidence_id: None,
                vector_id: None,
                suite_id: None,
            })?;
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
    record_artifact(ArtifactRecord {
        artifact_kind: "test_vector".to_string(),
        path: path.clone(),
        run_id: run_id.to_string(),
        model_id: None,
        property_id: None,
        evidence_id: None,
        vector_id: Some(vector_id.to_string()),
        suite_id: None,
    })?;
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
        if let Some(path) = &step.path {
            for decision in &path.decisions {
                require_non_empty(
                    &decision.point.decision_id,
                    "steps[].path.decisions[].decision_id",
                )?;
                require_non_empty(
                    &decision.point.action_id,
                    "steps[].path.decisions[].action_id",
                )?;
                require_non_empty(&decision.point.label, "steps[].path.decisions[].label")?;
            }
        }
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
    require_non_empty(
        &manifest.platform_metadata.os,
        "manifest.platform_metadata.os",
    )?;
    require_non_empty(
        &manifest.platform_metadata.arch,
        "manifest.platform_metadata.arch",
    )?;
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
        if let Some(path) = &step.path {
            out.push_str(&format!(",\"path\":{}", render_path_json(path)));
        } else {
            out.push_str(",\"path\":null");
        }
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
        if let Some(path) = step_object.get("path") {
            if !matches!(path, crate::support::json::JsonValue::Null) {
                let path_object = require_object(path, "trace.steps[].path")?;
                for decision in require_array_field(path_object, "decisions")? {
                    let decision_object =
                        require_object(decision, "trace.steps[].path.decisions[]")?;
                    require_string_field(decision_object, "decision_id")?;
                    require_string_field(decision_object, "action_id")?;
                    require_string_field(decision_object, "kind")?;
                    require_string_field(decision_object, "label")?;
                    require_string_field(decision_object, "outcome")?;
                }
            }
        }
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
    require_number_field(manifest, "seed")?;
    let platform_metadata = require_object(
        manifest
            .get("platform_metadata")
            .ok_or_else(|| "platform_metadata must be present".to_string())?,
        "platform_metadata",
    )?;
    require_string_field(platform_metadata, "os")?;
    require_string_field(platform_metadata, "arch")?;
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
        RunStatus::Pass => "PASS ",
        RunStatus::Fail => "FAIL ",
        RunStatus::Unknown => "UNKNOWN ",
    });
    out.push_str(backend_label(result.manifest.backend_name));
    out.push('\n');
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
        if let Some(action_id) = trace
            .steps
            .last()
            .and_then(|step| step.action_id.as_deref())
        {
            out.push_str(&format!("failing_action_id: {action_id}\n"));
        }
        out.push_str(&render_traceback_text(trace));
    }
    out
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
        append_string_array(&mut out, "reads", &decision.point.reads);
        append_string_array(&mut out, "writes", &decision.point.writes);
        append_string_array(&mut out, "path_tags", &decision.point.path_tags);
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

fn append_string_array(out: &mut String, key: &str, values: &[String]) {
    out.push_str(&format!(",\"{}\":", key));
    out.push('[');
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\"", escape_json(value)));
    }
    out.push(']');
}

fn render_error_text(error: &CheckErrorEnvelope) -> String {
    let mut out = String::new();
    out.push_str("ERROR ");
    out.push_str(backend_label(error.manifest.backend_name));
    out.push('\n');
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
    let trace_steps = result
        .trace
        .as_ref()
        .map(|trace| trace.steps.len())
        .unwrap_or(0);
    let action_sequence = result
        .trace
        .as_ref()
        .map(|trace| {
            trace
                .steps
                .iter()
                .filter_map(|step| step.action_id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let failing_action_id = result
        .trace
        .as_ref()
        .and_then(|trace| trace.steps.last())
        .and_then(|step| step.action_id.clone());
    let ci_exit_code = match result.status {
        RunStatus::Pass => 0,
        RunStatus::Fail => 2,
        RunStatus::Unknown => 4,
    };
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
    if let Some(scenario_id) = &result.property_result.scenario_id {
        out.push_str(&format!(
            ",\"scenario_id\":\"{}\"",
            escape_json(scenario_id)
        ));
    } else {
        out.push_str(",\"scenario_id\":null");
    }
    out.push_str(&format!(",\"vacuous\":{}", result.property_result.vacuous));
    out.push_str(&format!(
        ",\"summary\":\"{}\"",
        escape_json(&result.property_result.summary)
    ));
    out.push('}');
    if let Some(trace) = &result.trace {
        out.push_str(&format!(",\"trace\":{}", render_trace_json(trace)));
        out.push_str(&format!(",\"traceback\":{}", render_traceback_json(trace)));
    } else {
        out.push_str(",\"trace\":null");
        out.push_str(",\"traceback\":null");
    }
    out.push_str(&format!(
        ",\"ci\":{{\"exit_code\":{},\"status\":\"{}\",\"backend\":\"{}\"}}",
        ci_exit_code,
        status_label(result.status),
        backend_label(result.manifest.backend_name)
    ));
    out.push_str(",\"review_summary\":{");
    out.push_str(&format!(
        "\"headline\":\"{}\"",
        escape_json(&format!(
            "{} {} for {}",
            status_label(result.status),
            result.property_result.property_id,
            model_id
        ))
    ));
    out.push_str(&format!(",\"trace_steps\":{}", trace_steps));
    if let Some(action_id) = failing_action_id {
        out.push_str(&format!(
            ",\"failing_action_id\":\"{}\"",
            escape_json(&action_id)
        ));
    } else {
        out.push_str(",\"failing_action_id\":null");
    }
    out.push_str(&format!(
        ",\"action_sequence\":[{}]",
        action_sequence
            .iter()
            .map(|action| format!("\"{}\"", escape_json(action)))
            .collect::<Vec<_>>()
            .join(",")
    ));
    let mut next_steps =
        vec!["run `cargo valid explain <model>` for a reviewer-oriented diagnosis".to_string()];
    if matches!(result.status, RunStatus::Fail) {
        next_steps.push(
            "run `cargo valid generate-tests <model> --strategy=counterexample` to capture a regression"
                .to_string(),
        );
    }
    out.push_str(&format!(
        ",\"next_steps\":[{}]",
        next_steps
            .iter()
            .map(|step| format!("\"{}\"", escape_json(step)))
            .collect::<Vec<_>>()
            .join(",")
    ));
    out.push('}');
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
    out.push_str(",\"ci\":{\"exit_code\":3,\"status\":\"ERROR\"");
    out.push_str(&format!(
        ",\"backend\":\"{}\"}}",
        backend_label(error.manifest.backend_name)
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
    out.push_str(&format!(",\"seed\":{}", manifest.seed));
    out.push_str(",\"platform_metadata\":{");
    out.push_str(&format!(
        "\"os\":\"{}\"",
        escape_json(&manifest.platform_metadata.os)
    ));
    out.push_str(&format!(
        ",\"arch\":\"{}\"",
        escape_json(&manifest.platform_metadata.arch)
    ));
    out.push('}');
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

fn render_traceback_text(trace: &EvidenceTrace) -> String {
    let Some(step) = trace.steps.last() else {
        return String::new();
    };
    let (reads, writes, path_tags, involved_fields) = traceback_fields(step);
    let mut out = String::new();
    out.push_str("traceback:\n");
    out.push_str(&format!("  failure_step_index: {}\n", step.index));
    out.push_str(&format!("  from_state_id: {}\n", step.from_state_id));
    out.push_str(&format!("  to_state_id: {}\n", step.to_state_id));
    out.push_str(&format!("  depth: {}\n", step.depth));
    if let Some(action_id) = &step.action_id {
        out.push_str(&format!("  action_id: {action_id}\n"));
    }
    if !reads.is_empty() {
        out.push_str(&format!("  reads: {}\n", reads.join(", ")));
    }
    if !writes.is_empty() {
        out.push_str(&format!("  writes: {}\n", writes.join(", ")));
    }
    if !involved_fields.is_empty() {
        out.push_str(&format!(
            "  involved_fields: {}\n",
            involved_fields.join(", ")
        ));
    }
    if !path_tags.is_empty() {
        out.push_str(&format!("  path_tags: {}\n", path_tags.join(", ")));
    }
    out
}

fn render_traceback_json(trace: &EvidenceTrace) -> String {
    let Some(step) = trace.steps.last() else {
        return "null".to_string();
    };
    let (reads, writes, path_tags, involved_fields) = traceback_fields(step);
    let action_id = step
        .action_id
        .as_ref()
        .map(|value| format!("\"{}\"", escape_json(value)))
        .unwrap_or_else(|| "null".to_string());
    format!(
        "{{\"failure_step_index\":{},\"from_state_id\":\"{}\",\"to_state_id\":\"{}\",\"depth\":{},\"action_id\":{},\"reads\":[{}],\"writes\":[{}],\"involved_fields\":[{}],\"path_tags\":[{}]}}",
        step.index,
        escape_json(&step.from_state_id),
        escape_json(&step.to_state_id),
        step.depth,
        action_id,
        reads
            .iter()
            .map(|field| format!("\"{}\"", escape_json(field)))
            .collect::<Vec<_>>()
            .join(","),
        writes
            .iter()
            .map(|field| format!("\"{}\"", escape_json(field)))
            .collect::<Vec<_>>()
            .join(","),
        involved_fields
            .iter()
            .map(|field| format!("\"{}\"", escape_json(field)))
            .collect::<Vec<_>>()
            .join(","),
        path_tags
            .iter()
            .map(|tag| format!("\"{}\"", escape_json(tag)))
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn traceback_fields(step: &TraceStep) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    let Some(path) = &step.path else {
        return (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    };
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    let mut path_tags = Vec::new();
    for decision in &path.decisions {
        for field in &decision.point.reads {
            push_unique(&mut reads, field.clone());
        }
        for field in &decision.point.writes {
            push_unique(&mut writes, field.clone());
        }
        for tag in &decision.point.path_tags {
            push_unique(&mut path_tags, tag.clone());
        }
    }
    let mut involved_fields = writes.clone();
    for field in &reads {
        push_unique(&mut involved_fields, field.clone());
    }
    (reads, writes, path_tags, involved_fields)
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
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
    kind.as_str()
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
        crate::engine::BackendKind::SatVarisat => "sat-varisat",
    }
}
fn evidence_kind_label(kind: &EvidenceKind) -> &'static str {
    match kind {
        EvidenceKind::Trace => "trace",
        EvidenceKind::Counterexample => "counterexample",
        EvidenceKind::Witness => "witness",
        EvidenceKind::Certificate => "certificate",
    }
}
fn value_json(value: &Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::UInt(value) => value.to_string(),
        Value::String(value) => format!("\"{}\"", escape_json(value)),
        Value::EnumVariant { label, .. } => format!("\"{}\"", escape_json(label)),
        Value::PairVariant {
            left_label,
            right_label,
            ..
        } => format!(
            "[\"{}\",\"{}\"]",
            escape_json(left_label),
            escape_json(right_label)
        ),
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
        build_run_manifest,
        explicit::{CheckOutcome, ExplicitRunResult, PropertyResult},
        AssuranceLevel, BackendKind, RunStatus,
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
            manifest: build_run_manifest(
                "req-1".to_string(),
                "run-1".to_string(),
                "sha256:a".to_string(),
                "sha256:b".to_string(),
                BackendKind::Explicit,
                "0.1.0".to_string(),
                Some(11),
            ),
            status: RunStatus::Fail,
            assurance_level: AssuranceLevel::Complete,
            property_result: PropertyResult {
                property_id: "SAFE".to_string(),
                property_kind: crate::ir::PropertyKind::Invariant,
                status: RunStatus::Fail,
                assurance_level: AssuranceLevel::Complete,
                scenario_id: None,
                vacuous: false,
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
                    path: None,
                    note: None,
                }],
            }),
        };
        let outcome = CheckOutcome::Completed(result);
        let json = render_outcome_json("Counter", &outcome);
        assert!(json.contains("\"kind\":\"completed\""));
        assert!(json.contains("\"request_id\":\"req-1\""));
        assert!(json.contains("\"seed\":11"));
        assert!(json.contains("\"platform_metadata\""));
        assert!(json.contains("\"status\":\"FAIL\""));
        assert!(json.contains("\"ci\":{\"exit_code\":2"));
        assert!(json.contains("\"review_summary\""));
        validate_rendered_outcome_json(&json).unwrap();
    }

    #[test]
    fn writes_artifacts_for_completed_outcome() {
        let result = ExplicitRunResult {
            manifest: build_run_manifest(
                "req-1".to_string(),
                "run-artifact-test".to_string(),
                "sha256:a".to_string(),
                "sha256:b".to_string(),
                BackendKind::Explicit,
                "0.1.0".to_string(),
                Some(13),
            ),
            status: RunStatus::Fail,
            assurance_level: AssuranceLevel::Complete,
            property_result: PropertyResult {
                property_id: "SAFE".to_string(),
                property_kind: crate::ir::PropertyKind::Invariant,
                status: RunStatus::Fail,
                assurance_level: AssuranceLevel::Complete,
                scenario_id: None,
                vacuous: false,
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
                path: None,
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
