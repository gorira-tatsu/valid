//! Presenters for human-readable derived outputs such as Mermaid.

use crate::{api::InspectResponse, evidence::EvidenceTrace};

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

pub fn render_model_mermaid(response: &InspectResponse) -> String {
    let mut out = String::from("flowchart LR\n");
    let model_node = sanitize_id(&format!("model_{}", response.model_id));
    out.push_str(&format!(
        "  {model_node}[\"{}\"]\n",
        mermaid_label(&[format!("model: {}", response.model_id)])
    ));
    let capability_node = sanitize_id(&format!("capability_{}", response.model_id));
    let capability_mode = if response.machine_ir_ready {
        "analysis mode: declarative / solver-ready".to_string()
    } else {
        "analysis mode: explicit-only / opaque-step".to_string()
    };
    let mut capability_lines = vec![capability_mode];
    if !response.capabilities.reasons.is_empty() {
        capability_lines.push(format!(
            "reasons: {}",
            response.capabilities.reasons.join(", ")
        ));
    }
    out.push_str(&format!(
        "  {capability_node}[\"{}\"]\n",
        mermaid_label(&capability_lines)
    ));
    out.push_str(&format!("  {model_node} --> {capability_node}\n"));

    if !response.state_field_details.is_empty() {
        out.push_str("  subgraph state_fields[\"State Fields\"]\n");
        for field in &response.state_field_details {
            let node = sanitize_id(&format!("field_{}", field.name));
            let mut lines = vec![format!("{}: {}", field.name, field.rust_type)];
            if let Some(range) = &field.range {
                lines.push(format!("range: {range}"));
            }
            out.push_str(&format!("    {node}[\"{}\"]\n", mermaid_label(&lines)));
            out.push_str(&format!("  {model_node} --> {node}\n"));
        }
        out.push_str("  end\n");
    }

    if !response.action_details.is_empty() {
        out.push_str("  subgraph actions[\"Actions\"]\n");
        for action in &response.action_details {
            let node = sanitize_id(&format!("action_{}", action.action_id));
            let mut lines = vec![action.action_id.clone()];
            if !action.reads.is_empty() {
                lines.push(format!("reads: {}", action.reads.join(", ")));
            }
            if !action.writes.is_empty() {
                lines.push(format!("writes: {}", action.writes.join(", ")));
            }
            out.push_str(&format!("    {node}[\"{}\"]\n", mermaid_label(&lines)));
            out.push_str(&format!("  {model_node} --> {node}\n"));
        }
        out.push_str("  end\n");
    }

    if response.machine_ir_ready && !response.transition_details.is_empty() {
        out.push_str("  subgraph transitions[\"Transitions\"]\n");
        for (index, transition) in response.transition_details.iter().enumerate() {
            let node = sanitize_id(&format!("transition_{}_{}", transition.action_id, index));
            let mut lines = vec![transition.action_id.clone()];
            if let Some(guard) = &transition.guard {
                lines.push(format!("guard: {guard}"));
            }
            if !transition.path_tags.is_empty() {
                lines.push(format!("tags: {}", transition.path_tags.join(", ")));
            }
            out.push_str(&format!("    {node}[\"{}\"]\n", mermaid_label(&lines)));
            let action_node = sanitize_id(&format!("action_{}", transition.action_id));
            out.push_str(&format!("  {action_node} --> {node}\n"));

            if let Some(guard) = &transition.guard {
                let guard_node = sanitize_id(&format!("guard_{}_{}", transition.action_id, index));
                out.push_str(&format!(
                    "    {guard_node}[\"{}\"]\n",
                    mermaid_label(&["guard".to_string(), guard.clone()])
                ));
                out.push_str(&format!("  {node} --> {guard_node}\n"));
            }

            if !transition.updates.is_empty() {
                let update_node =
                    sanitize_id(&format!("updates_{}_{}", transition.action_id, index));
                let mut update_lines = vec!["updates".to_string()];
                update_lines.extend(
                    transition
                        .updates
                        .iter()
                        .map(|update| format!("{} := {}", update.field, update.expr)),
                );
                out.push_str(&format!(
                    "    {update_node}[\"{}\"]\n",
                    mermaid_label(&update_lines)
                ));
                out.push_str(&format!("  {node} --> {update_node}\n"));
            } else if let Some(effect) = &transition.effect {
                let effect_node =
                    sanitize_id(&format!("effect_{}_{}", transition.action_id, index));
                out.push_str(&format!(
                    "    {effect_node}[\"{}\"]\n",
                    mermaid_label(&["effect".to_string(), effect.clone()])
                ));
                out.push_str(&format!("  {node} --> {effect_node}\n"));
            }
        }
        out.push_str("  end\n");
    } else if !response.transition_details.is_empty() {
        let node = sanitize_id(&format!("opaque_{}", response.model_id));
        out.push_str(&format!(
            "  {node}[\"{}\"]\n",
            mermaid_label(&[
                "transition internals hidden".to_string(),
                "declarative transitions unavailable".to_string(),
            ])
        ));
        out.push_str(&format!("  {capability_node} --> {node}\n"));
    }

    if !response.property_details.is_empty() {
        out.push_str("  subgraph properties[\"Properties\"]\n");
        for property in &response.property_details {
            let node = sanitize_id(&format!("property_{}", property.property_id));
            out.push_str(&format!(
                "    {node}[\"{}\"]\n",
                mermaid_label(&[
                    property.property_id.clone(),
                    format!("kind: {}", property.kind)
                ])
            ));
            out.push_str(&format!("  {model_node} --> {node}\n"));
        }
        out.push_str("  end\n");
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

fn mermaid_label(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| sanitize_label(line))
        .collect::<Vec<_>>()
        .join("<br/>")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        api::{
            InspectAction, InspectCapabilities, InspectProperty, InspectResponse,
            InspectStateField, InspectTransition, InspectTransitionUpdate,
        },
        engine::AssuranceLevel,
        evidence::{EvidenceKind, EvidenceTrace, TraceStep},
        ir::Value,
    };

    use super::{render_model_mermaid, render_trace_mermaid, render_trace_sequence_mermaid};

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

    #[test]
    fn renders_model_mermaid() {
        let inspect = InspectResponse {
            schema_version: "1.0.0".to_string(),
            request_id: "req-1".to_string(),
            status: "ok".to_string(),
            model_id: "CounterModel".to_string(),
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
            state_fields: vec!["x".to_string()],
            actions: vec!["INC".to_string()],
            properties: vec!["P_RANGE".to_string()],
            state_field_details: vec![InspectStateField {
                name: "x".to_string(),
                rust_type: "u8".to_string(),
                range: Some("0..=3".to_string()),
            }],
            action_details: vec![InspectAction {
                action_id: "INC".to_string(),
                reads: vec!["x".to_string()],
                writes: vec!["x".to_string()],
            }],
            transition_details: vec![InspectTransition {
                action_id: "INC".to_string(),
                guard: Some("x < 3".to_string()),
                effect: Some("x := x + 1".to_string()),
                reads: vec!["x".to_string()],
                writes: vec!["x".to_string()],
                path_tags: vec!["allow_path".to_string()],
                updates: vec![InspectTransitionUpdate {
                    field: "x".to_string(),
                    expr: "x + 1".to_string(),
                }],
            }],
            property_details: vec![InspectProperty {
                property_id: "P_RANGE".to_string(),
                kind: "Invariant".to_string(),
            }],
        };
        let mermaid = render_model_mermaid(&inspect);
        assert!(mermaid.contains("flowchart LR"));
        assert!(mermaid.contains("CounterModel"));
        assert!(mermaid.contains("INC"));
        assert!(mermaid.contains("allow_path"));
        assert!(mermaid.contains("updates"));
        assert!(mermaid.contains("P_RANGE"));
    }

    #[test]
    fn renders_step_model_as_explicit_only() {
        let inspect = InspectResponse {
            schema_version: "1.0.0".to_string(),
            request_id: "req-1".to_string(),
            status: "ok".to_string(),
            model_id: "CounterModel".to_string(),
            machine_ir_ready: false,
            machine_ir_error: Some("step models are opaque".to_string()),
            capabilities: InspectCapabilities {
                parse_ready: true,
                explicit_ready: true,
                ir_ready: false,
                solver_ready: false,
                coverage_ready: true,
                explain_ready: true,
                testgen_ready: true,
                reasons: vec![
                    "opaque_step_closure".to_string(),
                    "missing_declarative_transitions".to_string(),
                ],
            },
            state_fields: vec!["x".to_string()],
            actions: vec!["INC".to_string()],
            properties: vec!["P_RANGE".to_string()],
            state_field_details: vec![InspectStateField {
                name: "x".to_string(),
                rust_type: "u8".to_string(),
                range: Some("0..=3".to_string()),
            }],
            action_details: vec![InspectAction {
                action_id: "INC".to_string(),
                reads: vec!["x".to_string()],
                writes: vec!["x".to_string()],
            }],
            transition_details: vec![InspectTransition {
                action_id: "INC".to_string(),
                guard: None,
                effect: None,
                reads: vec!["x".to_string()],
                writes: vec!["x".to_string()],
                path_tags: vec!["transition_path".to_string()],
                updates: Vec::new(),
            }],
            property_details: vec![InspectProperty {
                property_id: "P_RANGE".to_string(),
                kind: "Invariant".to_string(),
            }],
        };
        let mermaid = render_model_mermaid(&inspect);
        assert!(mermaid.contains("explicit-only / opaque-step"));
        assert!(mermaid.contains("opaque_step_closure"));
        assert!(mermaid.contains("transition internals hidden"));
        assert!(!mermaid.contains("transition_INC_0"));
    }
}
