//! Presenters for human-readable derived outputs such as Mermaid.

use crate::evidence::EvidenceTrace;

pub fn render_trace_mermaid(trace: &EvidenceTrace) -> String {
    let mut out = String::from("stateDiagram-v2\n");
    if trace.steps.is_empty() {
        out.push_str("  [*] --> EmptyTrace\n");
        return out;
    }
    out.push_str("  [*] --> ");
    out.push_str(&sanitize_id(&trace.steps[0].from_state_id));
    out.push('\n');
    for step in &trace.steps {
        let from = sanitize_id(&step.from_state_id);
        let to = sanitize_id(&step.to_state_id);
        let label = step
            .action_label
            .as_ref()
            .or(step.action_id.as_ref())
            .map(|value| sanitize_label(value))
            .unwrap_or_else(|| "transition".to_string());
        out.push_str(&format!("  {from} --> {to}: {label}\n"));
    }
    out
}

pub fn render_trace_sequence_mermaid(trace: &EvidenceTrace) -> String {
    let mut out = String::from("sequenceDiagram\n");
    out.push_str("  participant Engine\n");
    out.push_str("  participant Model\n");
    if trace.steps.is_empty() {
        out.push_str("  Engine->>Model: empty trace\n");
        return out;
    }
    for step in &trace.steps {
        let label = step
            .action_label
            .as_ref()
            .or(step.action_id.as_ref())
            .map(|value| sanitize_label(value))
            .unwrap_or_else(|| "transition".to_string());
        out.push_str(&format!(
            "  Engine->>Model: step {} {}\n",
            step.index, label
        ));
    }
    out
}

fn sanitize_id(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn sanitize_label(input: &str) -> String {
    input.replace('"', "\\\"").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        engine::AssuranceLevel,
        evidence::{EvidenceKind, EvidenceTrace, TraceStep},
        ir::Value,
    };

    use super::{render_trace_mermaid, render_trace_sequence_mermaid};

    #[test]
    fn renders_state_mermaid() {
        let trace = EvidenceTrace {
            schema_version: "1.0.0".to_string(),
            evidence_id: "ev-1".to_string(),
            run_id: "run-1".to_string(),
            property_id: "P_SAFE".to_string(),
            evidence_kind: EvidenceKind::Trace,
            assurance_level: AssuranceLevel::Complete,
            trace_hash: "sha256:x".to_string(),
            steps: vec![TraceStep {
                index: 0,
                from_state_id: "s-0".to_string(),
                action_id: Some("Jump".to_string()),
                action_label: Some("Jump".to_string()),
                to_state_id: "s-1".to_string(),
                depth: 1,
                state_before: BTreeMap::from([("x".to_string(), Value::UInt(0))]),
                state_after: BTreeMap::from([("x".to_string(), Value::UInt(2))]),
                note: None,
            }],
        };
        let mermaid = render_trace_mermaid(&trace);
        assert!(mermaid.contains("stateDiagram-v2"));
        assert!(mermaid.contains("s-0 --> s-1: Jump"));
        let seq = render_trace_sequence_mermaid(&trace);
        assert!(seq.contains("sequenceDiagram"));
    }
}
