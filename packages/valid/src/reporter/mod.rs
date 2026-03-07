//! Presenters for human-readable derived outputs such as Mermaid, DOT, and SVG.

use crate::{api::InspectResponse, evidence::EvidenceTrace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphView {
    Overview,
    Logic,
}

impl GraphView {
    pub fn parse(input: Option<&str>) -> Self {
        match input {
            Some("logic") | Some("detailed") | Some("full") => Self::Logic,
            _ => Self::Overview,
        }
    }
}

fn capability_mode_label(response: &InspectResponse) -> String {
    if response.machine_ir_ready && response.capabilities.solver_ready {
        "analysis mode: declarative / solver-ready".to_string()
    } else if response.machine_ir_ready {
        "analysis mode: declarative / explicit-ready".to_string()
    } else {
        "analysis mode: explicit-only / opaque-step".to_string()
    }
}

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
    render_model_mermaid_with_view(response, GraphView::Overview)
}

pub fn render_model_mermaid_with_view(response: &InspectResponse, view: GraphView) -> String {
    match view {
        GraphView::Overview => render_model_mermaid_overview(response),
        GraphView::Logic => render_model_mermaid_logic(response),
    }
}

fn render_model_mermaid_logic(response: &InspectResponse) -> String {
    let mut out = String::from("flowchart LR\n");
    let model_node = sanitize_id(&format!("model_{}", response.model_id));
    out.push_str(&format!(
        "  {model_node}[\"{}\"]\n",
        mermaid_label(&[format!("model: {}", response.model_id)])
    ));
    let capability_node = sanitize_id(&format!("capability_{}", response.model_id));
    let capability_mode = capability_mode_label(response);
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
            let lines = field_label_lines(field);
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
            let lines = transition_label_lines(transition);
            out.push_str(&format!("    {node}[\"{}\"]\n", mermaid_label(&lines)));
            let action_node = sanitize_id(&format!("action_{}", transition.action_id));
            out.push_str(&format!("  {action_node} --> {node}\n"));
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

pub fn render_model_dot(response: &InspectResponse) -> String {
    render_model_dot_with_view(response, GraphView::Overview)
}

pub fn render_model_dot_with_view(response: &InspectResponse, view: GraphView) -> String {
    match view {
        GraphView::Overview => render_model_dot_overview(response),
        GraphView::Logic => render_model_dot_logic(response),
    }
}

fn render_model_dot_logic(response: &InspectResponse) -> String {
    let mut out = String::from(
        "digraph model {\n  rankdir=LR;\n  node [shape=box, fontname=\"Helvetica\"];\n",
    );
    let model_node = sanitize_id(&format!("model_{}", response.model_id));
    out.push_str(&format!(
        "  {model_node} [label=\"{}\"];\n",
        dot_label(&[format!("model: {}", response.model_id)])
    ));
    let capability_node = sanitize_id(&format!("capability_{}", response.model_id));
    let capability_mode = capability_mode_label(response);
    let mut capability_lines = vec![capability_mode];
    if !response.capabilities.reasons.is_empty() {
        capability_lines.push(format!(
            "reasons: {}",
            response.capabilities.reasons.join(", ")
        ));
    }
    out.push_str(&format!(
        "  {capability_node} [label=\"{}\", shape=note];\n",
        dot_label(&capability_lines)
    ));
    out.push_str(&format!("  {model_node} -> {capability_node};\n"));

    append_dot_cluster(
        &mut out,
        "state_fields",
        "State Fields",
        response.state_field_details.iter().map(|field| {
            let node = sanitize_id(&format!("field_{}", field.name));
            let lines = field_label_lines(field);
            (node, dot_label(&lines), model_node.clone())
        }),
    );

    append_dot_cluster(
        &mut out,
        "actions",
        "Actions",
        response.action_details.iter().map(|action| {
            let node = sanitize_id(&format!("action_{}", action.action_id));
            let mut lines = vec![action.action_id.clone()];
            if !action.reads.is_empty() {
                lines.push(format!("reads: {}", action.reads.join(", ")));
            }
            if !action.writes.is_empty() {
                lines.push(format!("writes: {}", action.writes.join(", ")));
            }
            (node, dot_label(&lines), model_node.clone())
        }),
    );

    if response.machine_ir_ready && !response.transition_details.is_empty() {
        out.push_str("  subgraph cluster_transitions {\n    label=\"Transitions\";\n");
        for (index, transition) in response.transition_details.iter().enumerate() {
            let node = sanitize_id(&format!("transition_{}_{}", transition.action_id, index));
            let lines = transition_label_lines(transition);
            out.push_str(&format!("    {node} [label=\"{}\"];\n", dot_label(&lines)));
            let action_node = sanitize_id(&format!("action_{}", transition.action_id));
            out.push_str(&format!("  {action_node} -> {node};\n"));
        }
        out.push_str("  }\n");
    } else if !response.transition_details.is_empty() {
        let node = sanitize_id(&format!("opaque_{}", response.model_id));
        out.push_str(&format!(
            "  {node} [label=\"{}\", shape=note];\n",
            dot_label(&[
                "transition internals hidden".to_string(),
                "declarative transitions unavailable".to_string(),
            ])
        ));
        out.push_str(&format!("  {capability_node} -> {node};\n"));
    }

    append_dot_cluster(
        &mut out,
        "properties",
        "Properties",
        response.property_details.iter().map(|property| {
            let node = sanitize_id(&format!("property_{}", property.property_id));
            (
                node,
                dot_label(&[
                    property.property_id.clone(),
                    format!("kind: {}", property.kind),
                ]),
                model_node.clone(),
            )
        }),
    );

    out.push_str("}\n");
    out
}

pub fn render_model_svg(response: &InspectResponse) -> String {
    render_model_svg_with_view(response, GraphView::Overview)
}

pub fn render_model_svg_with_view(response: &InspectResponse, view: GraphView) -> String {
    match view {
        GraphView::Overview => render_model_svg_overview(response),
        GraphView::Logic => render_model_svg_logic(response),
    }
}

fn render_model_svg_logic(response: &InspectResponse) -> String {
    let mut sections = Vec::new();
    let capability_mode = capability_mode_label(response);
    let mut capability_lines = vec![capability_mode];
    if !response.capabilities.reasons.is_empty() {
        capability_lines.push(format!(
            "reasons: {}",
            response.capabilities.reasons.join(", ")
        ));
    }
    sections.push(("Capabilities".to_string(), capability_lines));

    sections.push((
        "State Fields".to_string(),
        response
            .state_field_details
            .iter()
            .flat_map(field_label_lines)
            .collect(),
    ));
    sections.push((
        "Actions".to_string(),
        response
            .action_details
            .iter()
            .map(|action| {
                let mut parts = vec![action.action_id.clone()];
                if !action.reads.is_empty() {
                    parts.push(format!("reads: {}", action.reads.join(", ")));
                }
                if !action.writes.is_empty() {
                    parts.push(format!("writes: {}", action.writes.join(", ")));
                }
                parts.join(" | ")
            })
            .collect(),
    ));
    let transition_lines = if response.machine_ir_ready {
        response
            .transition_details
            .iter()
            .flat_map(transition_label_lines)
            .collect()
    } else if response.transition_details.is_empty() {
        Vec::new()
    } else {
        vec![
            "transition internals hidden".to_string(),
            "declarative transitions unavailable".to_string(),
        ]
    };
    sections.push(("Transitions".to_string(), transition_lines));
    sections.push((
        "Properties".to_string(),
        response
            .property_details
            .iter()
            .map(|property| format!("{} | kind: {}", property.property_id, property.kind))
            .collect(),
    ));

    let width = 1200;
    let section_width = 1160;
    let mut y = 90i32;
    let mut body = String::new();
    for (title, lines) in sections {
        let line_count = usize::max(lines.len(), 1);
        let height = 44 + (line_count as i32 * 22) + 12;
        body.push_str(&svg_section(20, y, section_width, height, &title, &lines));
        y += height + 18;
    }
    let total_height = y + 20;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{total_height}\" viewBox=\"0 0 {width} {total_height}\" role=\"img\" aria-label=\"Model graph for {title}\"><style>text{{font-family:Helvetica,Arial,sans-serif;fill:#1f2937}} .title{{font-size:28px;font-weight:700}} .section-title{{font-size:18px;font-weight:700}} .line{{font-size:14px}} .section{{fill:#f8fafc;stroke:#cbd5e1;stroke-width:1.5}} .accent{{fill:#dbeafe;stroke:#93c5fd;stroke-width:1.5}}</style><rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/><rect x=\"20\" y=\"20\" width=\"1160\" height=\"48\" rx=\"10\" class=\"accent\"/><text x=\"40\" y=\"50\" class=\"title\">{title}</text>{body}</svg>",
        title = escape_xml(&format!("model: {}", response.model_id)),
        body = body,
    )
}

fn render_model_mermaid_overview(response: &InspectResponse) -> String {
    let mut out = String::from("flowchart LR\n");
    let model_node = sanitize_id(&format!("model_{}", response.model_id));
    out.push_str(&format!(
        "  {model_node}[\"{}\"]\n",
        mermaid_label(&[format!("model: {}", response.model_id)])
    ));
    let capability_node = sanitize_id(&format!("capability_{}", response.model_id));
    out.push_str(&format!(
        "  {capability_node}[\"{}\"]\n",
        mermaid_label(&capability_lines(response))
    ));
    out.push_str(&format!("  {model_node} --> {capability_node}\n"));

    if !response.state_field_details.is_empty() {
        out.push_str("  subgraph state_fields[\"State Fields\"]\n");
        for field in &response.state_field_details {
            let node = sanitize_id(&format!("field_{}", field.name));
            out.push_str(&format!(
                "    {node}[\"{}\"]\n",
                mermaid_label(&field_label_lines(field))
            ));
        }
        out.push_str("  end\n");
    }

    if !response.action_details.is_empty() {
        out.push_str("  subgraph actions[\"Actions\"]\n");
        for action in &response.action_details {
            let node = sanitize_id(&format!("action_{}", action.action_id));
            let lines = action_overview_lines(action, response);
            out.push_str(&format!("    {node}[\"{}\"]\n", mermaid_label(&lines)));
            out.push_str(&format!("  {model_node} --> {node}\n"));
            for read in &action.reads {
                let field = sanitize_id(&format!("field_{read}"));
                out.push_str(&format!("  {node} -. reads .-> {field}\n"));
            }
            for write in &action.writes {
                let field = sanitize_id(&format!("field_{write}"));
                out.push_str(&format!("  {node} -->|writes| {field}\n"));
            }
        }
        out.push_str("  end\n");
    }

    if !response.property_details.is_empty() {
        out.push_str("  subgraph properties[\"Properties\"]\n");
        for property in &response.property_details {
            let node = sanitize_id(&format!("property_{}", property.property_id));
            out.push_str(&format!(
                "    {node}[\"{}\"]\n",
                mermaid_label(&property_overview_lines(property))
            ));
            out.push_str(&format!("  {model_node} --> {node}\n"));
            for field in property_field_refs(property, &response.state_field_details) {
                let field = sanitize_id(&format!("field_{field}"));
                out.push_str(&format!("  {node} -. depends .-> {field}\n"));
            }
        }
        out.push_str("  end\n");
    }

    out
}

fn render_model_dot_overview(response: &InspectResponse) -> String {
    let mut out = String::from(
        "digraph model {\n  rankdir=LR;\n  node [shape=box, fontname=\"Helvetica\"];\n",
    );
    let model_node = sanitize_id(&format!("model_{}", response.model_id));
    out.push_str(&format!(
        "  {model_node} [label=\"{}\"];\n",
        dot_label(&[format!("model: {}", response.model_id)])
    ));
    let capability_node = sanitize_id(&format!("capability_{}", response.model_id));
    out.push_str(&format!(
        "  {capability_node} [label=\"{}\", shape=note];\n",
        dot_label(&capability_lines(response))
    ));
    out.push_str(&format!("  {model_node} -> {capability_node};\n"));

    append_dot_cluster(
        &mut out,
        "state_fields",
        "State Fields",
        response.state_field_details.iter().map(|field| {
            (
                sanitize_id(&format!("field_{}", field.name)),
                dot_label(&field_label_lines(field)),
                model_node.clone(),
            )
        }),
    );

    out.push_str("  subgraph cluster_actions {\n    label=\"Actions\";\n");
    for action in &response.action_details {
        let node = sanitize_id(&format!("action_{}", action.action_id));
        out.push_str(&format!(
            "    {node} [label=\"{}\"];\n",
            dot_label(&action_overview_lines(action, response))
        ));
        out.push_str(&format!("  {model_node} -> {node};\n"));
        for read in &action.reads {
            let field = sanitize_id(&format!("field_{read}"));
            out.push_str(&format!(
                "  {node} -> {field} [style=dashed, label=\"reads\"];\n"
            ));
        }
        for write in &action.writes {
            let field = sanitize_id(&format!("field_{write}"));
            out.push_str(&format!("  {node} -> {field} [label=\"writes\"];\n"));
        }
    }
    out.push_str("  }\n");

    out.push_str("  subgraph cluster_properties {\n    label=\"Properties\";\n");
    for property in &response.property_details {
        let node = sanitize_id(&format!("property_{}", property.property_id));
        out.push_str(&format!(
            "    {node} [label=\"{}\"];\n",
            dot_label(&property_overview_lines(property))
        ));
        out.push_str(&format!("  {model_node} -> {node};\n"));
        for field in property_field_refs(property, &response.state_field_details) {
            let field = sanitize_id(&format!("field_{field}"));
            out.push_str(&format!(
                "  {node} -> {field} [style=dashed, label=\"depends\"];\n"
            ));
        }
    }
    out.push_str("  }\n");
    out.push_str("}\n");
    out
}

fn render_model_svg_overview(response: &InspectResponse) -> String {
    let mut sections = Vec::new();
    sections.push(("Capabilities".to_string(), capability_lines(response)));
    sections.push((
        "State Fields".to_string(),
        response
            .state_field_details
            .iter()
            .flat_map(field_label_lines)
            .collect(),
    ));
    sections.push((
        "Actions".to_string(),
        response
            .action_details
            .iter()
            .flat_map(|action| action_overview_lines(action, response))
            .collect(),
    ));
    sections.push((
        "Properties".to_string(),
        response
            .property_details
            .iter()
            .flat_map(property_overview_lines)
            .collect(),
    ));
    let width = 1200;
    let section_width = 1160;
    let mut y = 90i32;
    let mut body = String::new();
    for (title, lines) in sections {
        let line_count = usize::max(lines.len(), 1);
        let height = 44 + (line_count as i32 * 22) + 12;
        body.push_str(&svg_section(20, y, section_width, height, &title, &lines));
        y += height + 18;
    }
    let total_height = y + 20;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{total_height}\" viewBox=\"0 0 {width} {total_height}\" role=\"img\" aria-label=\"Model graph for {title}\"><style>text{{font-family:Helvetica,Arial,sans-serif;fill:#1f2937}} .title{{font-size:28px;font-weight:700}} .section-title{{font-size:18px;font-weight:700}} .line{{font-size:14px}} .section{{fill:#f8fafc;stroke:#cbd5e1;stroke-width:1.5}} .accent{{fill:#dbeafe;stroke:#93c5fd;stroke-width:1.5}}</style><rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/><rect x=\"20\" y=\"20\" width=\"1160\" height=\"48\" rx=\"10\" class=\"accent\"/><text x=\"40\" y=\"50\" class=\"title\">{title}</text>{body}</svg>",
        title = escape_xml(&format!("model: {}", response.model_id)),
        body = body,
    )
}

fn append_dot_cluster<I>(out: &mut String, name: &str, label: &str, entries: I)
where
    I: IntoIterator<Item = (String, String, String)>,
{
    let entries = entries.into_iter().collect::<Vec<_>>();
    if entries.is_empty() {
        return;
    }
    out.push_str(&format!(
        "  subgraph cluster_{name} {{\n    label={label:?};\n"
    ));
    for (node, label, parent) in entries {
        out.push_str(&format!("    {node} [label=\"{label}\"];\n"));
        out.push_str(&format!("  {parent} -> {node};\n"));
    }
    out.push_str("  }\n");
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

fn dot_label(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| line.replace('\\', "\\\\").replace('"', "\\\""))
        .collect::<Vec<_>>()
        .join("\\n")
}

fn mermaid_label(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| sanitize_label(line))
        .collect::<Vec<_>>()
        .join("<br/>")
}

fn capability_lines(response: &InspectResponse) -> Vec<String> {
    let capability_mode = capability_mode_label(response);
    let mut capability_lines = vec![capability_mode];
    if !response.capabilities.reasons.is_empty() {
        capability_lines.push(format!(
            "reasons: {}",
            response.capabilities.reasons.join(", ")
        ));
    }
    capability_lines
}

fn field_label_lines(field: &crate::api::InspectStateField) -> Vec<String> {
    let mut lines = vec![format!(
        "{}: {}",
        field.name,
        compact_rust_type(&field.rust_type)
    )];
    if let Some(range) = &field.range {
        lines.push(format!("range: {range}"));
    }
    if let Some(left) = prefixed_variant(&field.variants, "left:") {
        lines.push(format!("left: {left}"));
    }
    if let Some(right) = prefixed_variant(&field.variants, "right:") {
        lines.push(format!("right: {right}"));
    }
    if let Some(keys) = prefixed_variant(&field.variants, "keys:") {
        lines.push(format!("keys: {keys}"));
    }
    if let Some(values) = prefixed_variant(&field.variants, "values:") {
        lines.push(format!("values: {values}"));
    }
    if !field.variants.is_empty()
        && !field
            .variants
            .iter()
            .any(|variant| variant.starts_with("left:") || variant.starts_with("keys:"))
    {
        lines.push(format!("variants: {}", field.variants.join(", ")));
    }
    lines
}

fn transition_label_lines(transition: &crate::api::InspectTransition) -> Vec<String> {
    let mut lines = vec![transition.action_id.clone()];
    if let Some(guard) = &transition.guard {
        lines.push(format!("when: {}", compact_inline(guard)));
    }
    let changed = meaningful_updates(&transition.updates);
    if !changed.is_empty() {
        lines.push(format!("changes: {}", changed.join("; ")));
    } else if let Some(effect) = &transition.effect {
        lines.push(format!("effect: {}", compact_inline(effect)));
    }
    let tags = visible_path_tags(&transition.path_tags);
    if !tags.is_empty() {
        lines.push(format!("tags: {}", tags.join(", ")));
    }
    lines
}

fn compact_rust_type(input: &str) -> String {
    let normalized = input
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    if let Some(inner) = normalized
        .strip_prefix("FiniteRelation<")
        .and_then(|value| value.strip_suffix('>'))
    {
        let parts = inner.split(',').collect::<Vec<_>>();
        if parts.len() == 2 {
            return format!("relation({} x {})", parts[0], parts[1]);
        }
    }
    if let Some(inner) = normalized
        .strip_prefix("FiniteMap<")
        .and_then(|value| value.strip_suffix('>'))
    {
        let parts = inner.split(',').collect::<Vec<_>>();
        if parts.len() == 2 {
            return format!("map({} -> {})", parts[0], parts[1]);
        }
    }
    if let Some(inner) = normalized
        .strip_prefix("FiniteEnumSet<")
        .and_then(|value| value.strip_suffix('>'))
    {
        return format!("set({inner})");
    }
    input.to_string()
}

fn prefixed_variant(variants: &[String], prefix: &str) -> Option<String> {
    variants
        .iter()
        .find_map(|variant| variant.strip_prefix(prefix).map(str::to_string))
}

fn compact_inline(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn meaningful_updates(updates: &[crate::api::InspectTransitionUpdate]) -> Vec<String> {
    updates
        .iter()
        .filter(|update| {
            let expr = update
                .expr
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>();
            expr != format!("state.{}", update.field) && expr != update.field
        })
        .map(|update| format!("{} := {}", update.field, compact_inline(&update.expr)))
        .collect()
}

fn visible_path_tags(tags: &[String]) -> Vec<String> {
    let specific = tags
        .iter()
        .filter(|tag| !matches!(tag.as_str(), "guard_path" | "read_path" | "write_path"))
        .cloned()
        .collect::<Vec<_>>();
    if specific.is_empty() {
        tags.to_vec()
    } else {
        specific
    }
}

fn action_overview_lines(
    action: &crate::api::InspectAction,
    response: &InspectResponse,
) -> Vec<String> {
    let mut lines = vec![action.action_id.clone()];
    let tags = action_tags(&action.action_id, response);
    if !tags.is_empty() {
        lines.push(format!("paths: {}", tags.join(", ")));
    }
    lines
}

fn property_overview_lines(property: &crate::api::InspectProperty) -> Vec<String> {
    let mut lines = vec![
        property.property_id.clone(),
        format!("kind: {}", property.kind),
    ];
    if let Some(expr) = &property.expr {
        lines.push(format!("rule: {}", compact_inline(expr)));
    }
    lines
}

fn action_tags(action_id: &str, response: &InspectResponse) -> Vec<String> {
    let mut tags = response
        .transition_details
        .iter()
        .filter(|transition| transition.action_id == action_id)
        .flat_map(|transition| visible_path_tags(&transition.path_tags))
        .collect::<Vec<_>>();
    tags.sort();
    tags.dedup();
    tags
}

fn property_field_refs(
    property: &crate::api::InspectProperty,
    fields: &[crate::api::InspectStateField],
) -> Vec<String> {
    let Some(expr) = &property.expr else {
        return Vec::new();
    };
    let normalized = expr.replace('\n', " ");
    fields
        .iter()
        .filter_map(|field| {
            let with_state = format!("state.{}", field.name);
            if normalized.contains(&with_state) || contains_identifier(&normalized, &field.name) {
                Some(field.name.clone())
            } else {
                None
            }
        })
        .collect()
}

fn contains_identifier(input: &str, ident: &str) -> bool {
    input
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .any(|part| part == ident)
}

fn svg_section(x: i32, y: i32, width: i32, height: i32, title: &str, lines: &[String]) -> String {
    let mut text = format!(
        "<rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" rx=\"12\" class=\"section\"/><text x=\"{tx}\" y=\"{ty}\" class=\"section-title\">{title}</text>",
        tx = x + 20,
        ty = y + 28,
        title = escape_xml(title)
    );
    let mut line_y = y + 54;
    if lines.is_empty() {
        text.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" class=\"line\">{}</text>",
            x + 20,
            line_y,
            escape_xml("none")
        ));
    } else {
        for line in lines {
            text.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" class=\"line\">{}</text>",
                x + 20,
                line_y,
                escape_xml(line)
            ));
            line_y += 22;
        }
    }
    text
}

fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
        modeling::CapabilityDetail,
    };

    use super::{
        render_model_dot, render_model_dot_with_view, render_model_mermaid,
        render_model_mermaid_with_view, render_model_svg, render_trace_mermaid,
        render_trace_sequence_mermaid, GraphView,
    };

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
                path: None,
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
            capabilities: InspectCapabilities::fully_ready(),
            state_fields: vec!["x".to_string()],
            actions: vec!["INC".to_string()],
            properties: vec!["P_RANGE".to_string()],
            state_field_details: vec![InspectStateField {
                name: "x".to_string(),
                rust_type: "u8".to_string(),
                range: Some("0..=3".to_string()),
                variants: Vec::new(),
                is_set: false,
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
                expr: None,
            }],
        };
        let mermaid = render_model_mermaid(&inspect);
        assert!(mermaid.contains("flowchart LR"));
        assert!(mermaid.contains("CounterModel"));
        assert!(mermaid.contains("INC"));
        assert!(mermaid.contains("allow_path"));
        assert!(mermaid.contains("-. reads .->"));
        assert!(mermaid.contains("-->|writes|"));
        assert!(mermaid.contains("P_RANGE"));
    }

    #[test]
    fn renders_model_dot_and_svg() {
        let inspect = InspectResponse {
            schema_version: "1.0.0".to_string(),
            request_id: "req-1".to_string(),
            status: "ok".to_string(),
            model_id: "CounterModel".to_string(),
            machine_ir_ready: true,
            machine_ir_error: None,
            capabilities: InspectCapabilities::fully_ready(),
            state_fields: vec!["x".to_string()],
            actions: vec!["INC".to_string()],
            properties: vec!["P_RANGE".to_string()],
            state_field_details: vec![InspectStateField {
                name: "x".to_string(),
                rust_type: "u8".to_string(),
                range: Some("0..=3".to_string()),
                variants: Vec::new(),
                is_set: false,
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
                expr: None,
            }],
        };
        let dot = render_model_dot(&inspect);
        assert!(dot.contains("digraph model"));
        assert!(dot.contains("label=\"reads\""));
        assert!(dot.contains("label=\"writes\""));
        let svg = render_model_svg(&inspect);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("CounterModel"));
        assert!(svg.contains("allow_path"));
    }

    #[test]
    fn renders_logic_view_with_guard_and_changes() {
        let inspect = InspectResponse {
            schema_version: "1.0.0".to_string(),
            request_id: "req-1".to_string(),
            status: "ok".to_string(),
            model_id: "CounterModel".to_string(),
            machine_ir_ready: true,
            machine_ir_error: None,
            capabilities: InspectCapabilities::fully_ready(),
            state_fields: vec!["x".to_string()],
            actions: vec!["INC".to_string()],
            properties: vec!["P_RANGE".to_string()],
            state_field_details: vec![InspectStateField {
                name: "x".to_string(),
                rust_type: "u8".to_string(),
                range: Some("0..=3".to_string()),
                variants: Vec::new(),
                is_set: false,
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
                expr: None,
            }],
        };
        let mermaid = render_model_mermaid_with_view(&inspect, GraphView::Logic);
        assert!(mermaid.contains("when: x < 3"));
        assert!(mermaid.contains("changes: x := x + 1"));
        let dot = render_model_dot_with_view(&inspect, GraphView::Logic);
        assert!(dot.contains("when: x < 3"));
        assert!(dot.contains("changes: x := x + 1"));
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
                ir_ready: false,
                solver_ready: false,
                ir: CapabilityDetail {
                    reason: "opaque step models cannot be lowered into machine IR".to_string(),
                    migration_hint: Some(
                        "rewrite the model using declarative `transitions { ... }` blocks so guards and updates become first-class IR".to_string(),
                    ),
                    unsupported_features: vec!["step(state, action)".to_string()],
                },
                solver: CapabilityDetail {
                    reason: "solver backends require machine IR first; blocking IR reason: opaque step models cannot be lowered into machine IR".to_string(),
                    migration_hint: Some(
                        "rewrite the model using declarative `transitions { ... }` blocks so guards and updates become first-class IR".to_string(),
                    ),
                    unsupported_features: vec!["step(state, action)".to_string()],
                },
                reasons: vec![
                    "opaque_step_closure".to_string(),
                    "missing_declarative_transitions".to_string(),
                ],
                ..InspectCapabilities::fully_ready()
            },
            state_fields: vec!["x".to_string()],
            actions: vec!["INC".to_string()],
            properties: vec!["P_RANGE".to_string()],
            state_field_details: vec![InspectStateField {
                name: "x".to_string(),
                rust_type: "u8".to_string(),
                range: Some("0..=3".to_string()),
                variants: Vec::new(),
                is_set: false,
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
                expr: None,
            }],
        };
        let mermaid = render_model_mermaid(&inspect);
        assert!(mermaid.contains("explicit-only / opaque-step"));
        assert!(mermaid.contains("opaque_step_closure"));
        assert!(mermaid.contains("-. reads .->"));
        assert!(mermaid.contains("-->|writes|"));
        let logic = render_model_mermaid_with_view(&inspect, GraphView::Logic);
        assert!(logic.contains("transition internals hidden"));
        assert!(!logic.contains("transition_INC_0"));
    }
}
