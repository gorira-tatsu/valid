//! Evidence and reporting.

use std::collections::BTreeMap;

use crate::{
    engine::{AssuranceLevel, ExplicitRunResult, RunStatus, UnknownReason},
    ir::Value,
    support::diagnostics::Diagnostic,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceKind {
    Trace,
    Certificate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceTrace {
    pub evidence_kind: EvidenceKind,
    pub assurance_level: AssuranceLevel,
    pub steps: Vec<TraceStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceStep {
    pub index: usize,
    pub action_id: Option<String>,
    pub state: BTreeMap<String, Value>,
    pub note: Option<String>,
}

pub fn render_result_text(result: &ExplicitRunResult) -> String {
    let mut out = String::new();
    out.push_str(match result.status {
        RunStatus::Pass => "PASS explicit\n",
        RunStatus::Fail => "FAIL explicit\n",
        RunStatus::Unknown => "UNKNOWN explicit\n",
    });
    out.push_str(&format!("assurance_level: {}\n", assurance_label(result.assurance_level)));
    out.push_str(&format!("explored_states: {}\n", result.explored_states));
    out.push_str(&format!("explored_transitions: {}\n", result.explored_transitions));
    if let Some(property_id) = &result.property_id {
        out.push_str(&format!("property_id: {property_id}\n"));
    }
    if let Some(reason) = result.unknown_reason {
        out.push_str(&format!("unknown_reason: {}\n", unknown_reason_label(reason)));
    }
    if let Some(trace) = &result.trace {
        out.push_str(&format!("trace_steps: {}\n", trace.steps.len()));
    }
    out
}

pub fn render_result_json(model_id: &str, result: &ExplicitRunResult) -> String {
    let mut out = String::new();
    out.push('{');
    out.push_str(&format!("\"model_id\":\"{}\"", escape_json(model_id)));
    out.push_str(&format!(",\"status\":\"{}\"", status_label(result.status)));
    out.push_str(&format!(",\"assurance_level\":\"{}\"", assurance_label(result.assurance_level)));
    out.push_str(&format!(",\"explored_states\":{}", result.explored_states));
    out.push_str(&format!(",\"explored_transitions\":{}", result.explored_transitions));
    if let Some(property_id) = &result.property_id {
        out.push_str(&format!(",\"property_id\":\"{}\"", escape_json(property_id)));
    } else {
        out.push_str(",\"property_id\":null");
    }
    if let Some(reason) = result.unknown_reason {
        out.push_str(&format!(",\"unknown_reason\":\"{}\"", unknown_reason_label(reason)));
    } else {
        out.push_str(",\"unknown_reason\":null");
    }
    if let Some(trace) = &result.trace {
        out.push_str(",\"trace\":{");
        out.push_str("\"evidence_kind\":\"trace\"");
        out.push_str(&format!(",\"assurance_level\":\"{}\"", assurance_label(trace.assurance_level)));
        out.push_str(",\"steps\":[");
        for (index, step) in trace.steps.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push('{');
            out.push_str(&format!("\"index\":{}", step.index));
            if let Some(action_id) = &step.action_id {
                out.push_str(&format!(",\"action_id\":\"{}\"", escape_json(action_id)));
            } else {
                out.push_str(",\"action_id\":null");
            }
            out.push_str(",\"state\":{");
            for (field_index, (name, value)) in step.state.iter().enumerate() {
                if field_index > 0 {
                    out.push(',');
                }
                out.push_str(&format!("\"{}\":{}", escape_json(name), value_json(value)));
            }
            out.push('}');
            if let Some(note) = &step.note {
                out.push_str(&format!(",\"note\":\"{}\"", escape_json(note)));
            } else {
                out.push_str(",\"note\":null");
            }
            out.push('}');
        }
        out.push_str("]}");
    } else {
        out.push_str(",\"trace\":null");
    }
    out.push('}');
    out
}

pub fn render_diagnostics_json(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::from("{\"diagnostics\":[");
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str(&format!("\"error_code\":\"{}\"", diagnostic.error_code.as_str()));
        out.push_str(&format!(",\"segment\":\"{}\"", diagnostic.segment.as_str()));
        out.push_str(&format!(",\"message\":\"{}\"", escape_json(&diagnostic.message)));
        if let Some(span) = &diagnostic.primary_span {
            out.push_str(&format!(",\"primary_span\":{{\"source\":\"{}\",\"line\":{},\"column\":{}}}", escape_json(&span.source), span.line, span.column));
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

fn status_label(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Pass => "pass",
        RunStatus::Fail => "fail",
        RunStatus::Unknown => "unknown",
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
        UnknownReason::UnsatInit => "UNSAT_INIT",
        UnknownReason::StateLimitReached => "STATE_LIMIT_REACHED",
        UnknownReason::DepthLimitReached => "DEPTH_LIMIT_REACHED",
        UnknownReason::TimeLimitReached => "TIME_LIMIT_REACHED",
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

    use crate::engine::{AssuranceLevel, ExplicitRunResult, RunStatus};

    use super::{render_result_json, EvidenceKind, EvidenceTrace, TraceStep};

    #[test]
    fn renders_result_json() {
        let result = ExplicitRunResult {
            status: RunStatus::Fail,
            assurance_level: AssuranceLevel::Complete,
            property_id: Some("SAFE".to_string()),
            explored_states: 2,
            explored_transitions: 1,
            unknown_reason: None,
            trace: Some(EvidenceTrace {
                evidence_kind: EvidenceKind::Trace,
                assurance_level: AssuranceLevel::Complete,
                steps: vec![TraceStep {
                    index: 0,
                    action_id: None,
                    state: BTreeMap::from([("x".to_string(), crate::ir::Value::UInt(0))]),
                    note: Some("initial state".to_string()),
                }],
            }),
        };
        let json = render_result_json("Counter", &result);
        assert!(json.contains("\"model_id\":\"Counter\""));
        assert!(json.contains("\"status\":\"fail\""));
        assert!(json.contains("\"property_id\":\"SAFE\""));
    }
}
