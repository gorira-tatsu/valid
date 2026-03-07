use serde_json::json;

use crate::{
    api::InspectResponse,
    support::{hash::stable_hash_hex, io::write_text_file},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedDoc {
    pub schema_version: String,
    pub model_id: String,
    pub source_hash: String,
    pub contract_hash: String,
    pub generated_hash: String,
    pub markdown: String,
    pub mermaid: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocCheckReport {
    pub schema_version: String,
    pub status: String,
    pub model_id: String,
    pub output_path: String,
    pub source_hash: String,
    pub existing_hash: Option<String>,
    pub generated_hash: String,
    pub contract_hash: String,
    pub drift_sections: Vec<String>,
    pub markdown: String,
    pub mermaid: String,
}

pub fn default_doc_path(model_id: &str) -> String {
    format!("artifacts/docs/{}.md", sanitize_model_id(model_id))
}

pub fn generate_doc(
    inspect: &InspectResponse,
    mermaid: String,
    source_hash: String,
    contract_hash: String,
) -> GeneratedDoc {
    let markdown = render_doc_markdown(inspect, &mermaid, &source_hash, &contract_hash);
    GeneratedDoc {
        schema_version: "1.0.0".to_string(),
        model_id: inspect.model_id.clone(),
        source_hash,
        contract_hash,
        generated_hash: stable_hash_hex(&markdown),
        markdown,
        mermaid,
    }
}

pub fn check_doc(
    output_path: String,
    existing: Option<&str>,
    generated: &GeneratedDoc,
) -> DocCheckReport {
    let existing_hash = existing.map(stable_hash_hex);
    let mut drift_sections = Vec::new();
    match existing {
        None => drift_sections.push("missing".to_string()),
        Some(body) => {
            if extract_metadata(body, "source_hash").as_deref()
                != Some(generated.source_hash.as_str())
            {
                drift_sections.push("source_hash".to_string());
            }
            if extract_mermaid(body)
                .map(|value| value.trim_end().to_string())
                .as_deref()
                != Some(generated.mermaid.trim_end())
            {
                drift_sections.push("mermaid".to_string());
            }
            if extract_metadata(body, "contract_hash").as_deref()
                != Some(generated.contract_hash.as_str())
            {
                drift_sections.push("contract_hash".to_string());
            }
            if body != generated.markdown {
                drift_sections.push("markdown".to_string());
            }
        }
    }
    DocCheckReport {
        schema_version: "1.0.0".to_string(),
        status: if drift_sections.is_empty() {
            "unchanged".to_string()
        } else {
            "changed".to_string()
        },
        model_id: generated.model_id.clone(),
        output_path,
        source_hash: generated.source_hash.clone(),
        existing_hash,
        generated_hash: generated.generated_hash.clone(),
        contract_hash: generated.contract_hash.clone(),
        drift_sections,
        markdown: generated.markdown.clone(),
        mermaid: generated.mermaid.clone(),
    }
}

pub fn write_doc(path: &str, generated: &GeneratedDoc) -> Result<(), String> {
    write_text_file(path, &generated.markdown)
}

pub fn render_doc_text(generated: &GeneratedDoc, output_path: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&generated.markdown);
    if let Some(path) = output_path {
        out.push_str(&format!("\noutput_path: {path}\n"));
    }
    out
}

pub fn render_doc_json(generated: &GeneratedDoc, output_path: Option<&str>) -> String {
    json!({
        "schema_version": generated.schema_version,
        "status": "generated",
        "model_id": generated.model_id,
        "source_hash": generated.source_hash,
        "contract_hash": generated.contract_hash,
        "generated_hash": generated.generated_hash,
        "output_path": output_path,
        "markdown": generated.markdown,
        "mermaid": generated.mermaid,
    })
    .to_string()
}

pub fn render_doc_check_text(report: &DocCheckReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("status: {}\n", report.status));
    out.push_str(&format!("model_id: {}\n", report.model_id));
    out.push_str(&format!("output_path: {}\n", report.output_path));
    out.push_str(&format!("source_hash: {}\n", report.source_hash));
    out.push_str(&format!("generated_hash: {}\n", report.generated_hash));
    out.push_str(&format!("contract_hash: {}\n", report.contract_hash));
    if let Some(existing_hash) = &report.existing_hash {
        out.push_str(&format!("existing_hash: {existing_hash}\n"));
    } else {
        out.push_str("existing_hash: <missing>\n");
    }
    if !report.drift_sections.is_empty() {
        out.push_str(&format!(
            "drift_sections: {}\n",
            report.drift_sections.join(",")
        ));
    }
    out
}

pub fn render_doc_check_json(report: &DocCheckReport) -> String {
    json!({
        "schema_version": report.schema_version,
        "status": report.status,
        "model_id": report.model_id,
        "output_path": report.output_path,
        "source_hash": report.source_hash,
        "existing_hash": report.existing_hash,
        "generated_hash": report.generated_hash,
        "contract_hash": report.contract_hash,
        "drift_sections": report.drift_sections,
        "markdown": report.markdown,
        "mermaid": report.mermaid,
    })
    .to_string()
}

fn render_doc_markdown(
    inspect: &InspectResponse,
    mermaid: &str,
    source_hash: &str,
    contract_hash: &str,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "<!-- valid-doc: model_id={} source_hash={} contract_hash={} -->\n\n",
        inspect.model_id, source_hash, contract_hash
    ));
    out.push_str(&format!("# {}\n\n", inspect.model_id));
    out.push_str("## Overview\n\n");
    out.push_str(&format!(
        "- machine_ir_ready: {}\n- explicit_ready: {}\n- solver_ready: {}\n- contract_hash: `{}`\n\n",
        inspect.machine_ir_ready,
        inspect.capabilities.explicit_ready,
        inspect.capabilities.solver_ready,
        contract_hash
    ));
    out.push_str("## State Fields\n\n");
    out.push_str("| name | type | range | variants |\n| --- | --- | --- | --- |\n");
    for field in &inspect.state_field_details {
        let variants = if field.variants.is_empty() {
            String::new()
        } else {
            field.variants.join(", ")
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            field.name,
            field.rust_type,
            field.range.clone().unwrap_or_default(),
            variants
        ));
    }
    out.push_str("\n## Actions\n\n");
    out.push_str("| action_id | reads | writes |\n| --- | --- | --- |\n");
    for action in &inspect.action_details {
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            action.action_id,
            action.reads.join(", "),
            action.writes.join(", ")
        ));
    }
    out.push_str("\n## Properties\n\n");
    out.push_str("| property_id | kind | expr |\n| --- | --- | --- |\n");
    for property in &inspect.property_details {
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            property.property_id,
            property.kind,
            property.expr.clone().unwrap_or_default().replace('\n', " ")
        ));
    }
    out.push_str("\n## Transitions\n\n");
    for transition in &inspect.transition_details {
        out.push_str(&format!(
            "- `{}` guard=`{}` writes=[{}] tags=[{}]\n",
            transition.action_id,
            transition
                .guard
                .clone()
                .unwrap_or_else(|| "true".to_string()),
            transition.writes.join(", "),
            transition.path_tags.join(", ")
        ));
    }
    out.push_str("\n## Mermaid\n\n```mermaid\n");
    out.push_str(mermaid);
    if !mermaid.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("```\n");
    out
}

fn sanitize_model_id(model_id: &str) -> String {
    model_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn extract_metadata(body: &str, key: &str) -> Option<String> {
    let line = body.lines().next()?;
    let marker = format!("{key}=");
    let start = line.find(&marker)? + marker.len();
    let rest = &line[start..];
    let end = rest.find([' ', '>']).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn extract_mermaid(body: &str) -> Option<String> {
    let marker = "```mermaid\n";
    let start = body.find(marker)? + marker.len();
    let rest = &body[start..];
    let end = rest.find("\n```")?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::{check_doc, generate_doc};
    use crate::api::{
        InspectAction, InspectCapabilities, InspectProperty, InspectResponse, InspectStateField,
        InspectTransition,
    };
    use crate::modeling::CapabilityDetail;

    fn sample_inspect() -> InspectResponse {
        InspectResponse {
            schema_version: "1.0.0".to_string(),
            request_id: "req-doc".to_string(),
            status: "ok".to_string(),
            model_id: "CounterModel".to_string(),
            machine_ir_ready: true,
            machine_ir_error: None,
            capabilities: InspectCapabilities {
                parse_ready: true,
                parse: CapabilityDetail::ready(),
                explicit_ready: true,
                explicit: CapabilityDetail::ready(),
                ir_ready: true,
                ir: CapabilityDetail::ready(),
                solver_ready: false,
                solver: CapabilityDetail {
                    reason: "opaque".to_string(),
                    migration_hint: None,
                    unsupported_features: Vec::new(),
                },
                coverage_ready: true,
                coverage: CapabilityDetail::ready(),
                explain_ready: true,
                explain: CapabilityDetail::ready(),
                testgen_ready: true,
                testgen: CapabilityDetail::ready(),
                reasons: Vec::new(),
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
                guard: Some("state.x < 3".to_string()),
                effect: Some("x = state.x + 1".to_string()),
                reads: vec!["x".to_string()],
                writes: vec!["x".to_string()],
                path_tags: vec!["allow_path".to_string()],
                updates: Vec::new(),
            }],
            property_details: vec![InspectProperty {
                property_id: "P_RANGE".to_string(),
                kind: "invariant".to_string(),
                expr: Some("state.x <= 3".to_string()),
            }],
        }
    }

    #[test]
    fn generated_doc_embeds_metadata_and_mermaid() {
        let doc = generate_doc(
            &sample_inspect(),
            "flowchart TD\n  s0 --> s1".to_string(),
            "source-hash".to_string(),
            "contract-hash".to_string(),
        );
        assert!(doc.markdown.contains("<!-- valid-doc:"));
        assert!(
            doc.markdown.contains("contract_hash=`contract-hash`")
                || doc.markdown.contains("contract_hash: `contract-hash`")
        );
        assert!(doc.markdown.contains("```mermaid"));
        assert_eq!(doc.mermaid, "flowchart TD\n  s0 --> s1");
    }

    #[test]
    fn doc_check_reports_mermaid_drift() {
        let generated = generate_doc(
            &sample_inspect(),
            "flowchart TD\n  s0 --> s1".to_string(),
            "source-hash".to_string(),
            "contract-hash".to_string(),
        );
        let mut existing = generated.markdown.clone();
        existing = existing.replace("s0 --> s1", "s0 --> s2");
        let report = check_doc(
            "artifacts/docs/CounterModel.md".to_string(),
            Some(&existing),
            &generated,
        );
        assert_eq!(report.status, "changed");
        assert!(report.drift_sections.contains(&"mermaid".to_string()));
        assert!(report.drift_sections.contains(&"markdown".to_string()));
    }
}
