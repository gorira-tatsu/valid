use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::json;

use crate::{
    api::{
        ExplainResponse, InspectProperty, InspectResponse, OrchestratedRunSummary, TestgenResponse,
        TestgenVectorSummary,
    },
    cli::{text_bullet, text_command, text_header, text_hint, text_kv, text_section},
    coverage::CoverageReport,
    support::{
        artifact::{generated_test_path, handoff_path},
        hash::stable_hash_hex,
        io::write_text_file,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandoffSection {
    pub id: String,
    pub title: String,
    pub bullets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedHandoff {
    pub schema_version: String,
    pub model_id: String,
    pub property_id: Option<String>,
    pub source_hash: String,
    pub contract_hash: String,
    pub generated_hash: String,
    pub sections: Vec<HandoffSection>,
    pub testgen_summary: HandoffTestgenSummary,
    pub failing_properties: Vec<String>,
    pub open_ambiguities: Vec<String>,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoffCheckReport {
    pub schema_version: String,
    pub status: String,
    pub model_id: String,
    pub property_id: Option<String>,
    pub output_path: String,
    pub source_hash: String,
    pub existing_hash: Option<String>,
    pub generated_hash: String,
    pub contract_hash: String,
    pub drift_sections: Vec<String>,
    pub sections: Vec<HandoffSection>,
    pub testgen_summary: HandoffTestgenSummary,
    pub failing_properties: Vec<String>,
    pub open_ambiguities: Vec<String>,
    pub markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandoffTestgenSummary {
    pub status: String,
    pub generated_at_strategy: String,
    pub vector_count: usize,
    pub generated_files: Vec<String>,
    pub recommended_next_step: Option<String>,
    pub recommended_conformance_surface: String,
    pub recommended_docs: Vec<String>,
    pub recommended_mcp_tool: Option<String>,
    pub recommended_testgen_strategy: Option<String>,
    pub recommended_conformance_command: Option<String>,
    pub recommended_vectors: Vec<HandoffRecommendedVector>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandoffRecommendedVector {
    pub vector_id: String,
    pub property_id: String,
    pub strategy: String,
    pub counterexample_kind: Option<String>,
    pub witness_kind: Option<String>,
    pub priority: String,
    pub selection_reason: String,
    pub grouping: Vec<String>,
    pub observation_layers: Vec<String>,
    pub oracle_targets: Vec<String>,
    pub suggested_surface: String,
    pub state_visibility: String,
    pub parameter_bindings: Vec<crate::ir::ActionParameterBinding>,
    pub canonical_witness: Vec<String>,
    pub artifact_paths: Vec<String>,
    pub recommended_next_command: String,
    pub recommended_conformance_surface: String,
    pub recommended_docs: Vec<String>,
    pub recommended_mcp_tool: Option<String>,
    pub recommended_testgen_strategy: Option<String>,
    pub recommended_conformance_command: String,
    pub why_this_vector_matters: String,
}

pub struct HandoffInputs<'a> {
    pub inspect: &'a InspectResponse,
    pub runs: &'a [OrchestratedRunSummary],
    pub coverage: &'a CoverageReport,
    pub explanations: &'a [ExplainResponse],
    pub testgen: Option<&'a TestgenResponse>,
    pub testgen_error: Option<&'a str>,
    pub property_id: Option<&'a str>,
    pub source_hash: &'a str,
    pub contract_hash: &'a str,
}

pub fn default_handoff_path(model_id: &str) -> String {
    handoff_path(model_id)
}

pub fn generate_handoff(inputs: HandoffInputs<'_>) -> GeneratedHandoff {
    let selected_properties = selected_properties(inputs.inspect, inputs.property_id);
    let selected_runs = selected_runs(inputs.runs, inputs.property_id);
    let explain_by_property = inputs
        .explanations
        .iter()
        .map(|response| (response.property_id.clone(), response))
        .collect::<BTreeMap<_, _>>();
    let failing_properties = selected_runs
        .iter()
        .filter(|run| run.status == "FAIL")
        .map(|run| run.property_id.clone())
        .collect::<Vec<_>>();
    let testgen_summary =
        summarize_handoff_testgen(inputs.testgen, inputs.testgen_error, &failing_properties);
    let open_ambiguities = build_open_ambiguities(
        inputs.inspect,
        &selected_runs,
        inputs.coverage,
        inputs.property_id,
    );
    let sections = vec![
        section_feature_goal(
            inputs.inspect,
            &selected_runs,
            inputs.property_id,
            inputs.contract_hash,
        ),
        section_execution_contract(),
        section_recommended_test_vectors(&testgen_summary),
        section_required_behaviors(&selected_properties, &selected_runs),
        section_forbidden_behaviors(&selected_runs, &explain_by_property),
        section_state_facts(inputs.inspect),
        section_properties_to_preserve(&selected_properties, &selected_runs),
        section_implementation_implications(&failing_properties, &explain_by_property),
        section_test_checklist(inputs.coverage, &failing_properties, &explain_by_property),
        section_open_ambiguities(&open_ambiguities),
        section_source_evidence(
            &selected_runs,
            &failing_properties,
            &explain_by_property,
            inputs.source_hash,
            inputs.contract_hash,
            inputs.coverage,
        ),
    ];
    let markdown = render_handoff_markdown(
        inputs.inspect.model_id.as_str(),
        inputs.property_id,
        inputs.source_hash,
        inputs.contract_hash,
        &sections,
    );
    GeneratedHandoff {
        schema_version: "1.0.0".to_string(),
        model_id: inputs.inspect.model_id.clone(),
        property_id: inputs.property_id.map(str::to_string),
        source_hash: inputs.source_hash.to_string(),
        contract_hash: inputs.contract_hash.to_string(),
        generated_hash: stable_hash_hex(&markdown),
        sections,
        testgen_summary,
        failing_properties,
        open_ambiguities,
        markdown,
    }
}

fn section_execution_contract() -> HandoffSection {
    HandoffSection {
        id: "execution-contract".to_string(),
        title: "Execution Contract".to_string(),
        bullets: vec![
            "Treat generated vectors as language-agnostic test specs, not framework-specific test code.".to_string(),
            "Use observations as the primary oracle; use state snapshots only as optional debug or projection hints.".to_string(),
            "Let the implementation test layer choose hooks, mocks, fixtures, and assertion style.".to_string(),
            "Prefer API or handler-facing checks unless the model or path tags clearly require a UI surface.".to_string(),
        ],
    }
}

fn summarize_handoff_testgen(
    response: Option<&TestgenResponse>,
    error: Option<&str>,
    failing_properties: &[String],
) -> HandoffTestgenSummary {
    match response {
        Some(response) => {
            let generated_at_strategy = response
                .vectors
                .first()
                .map(|vector| vector.strategy.clone())
                .unwrap_or_else(|| "counterexample".to_string());
            let recommended_vectors = recommended_handoff_vectors(
                &response.vectors,
                &response.generated_files,
                failing_properties,
            );
            HandoffTestgenSummary {
                status: "available".to_string(),
                generated_at_strategy,
                vector_count: response.vectors.len(),
                generated_files: response.generated_files.clone(),
                recommended_next_step: Some(recommended_next_step(&recommended_vectors)),
                recommended_conformance_surface: summarize_conformance_surface(
                    &recommended_vectors,
                ),
                recommended_docs: summarize_recommended_docs(&recommended_vectors),
                recommended_mcp_tool: summarize_recommended_mcp_tool(&recommended_vectors),
                recommended_testgen_strategy: summarize_recommended_testgen_strategy(
                    &recommended_vectors,
                ),
                recommended_conformance_command: summarize_recommended_conformance_command(
                    &recommended_vectors,
                ),
                recommended_vectors,
                error: None,
            }
        }
        None => HandoffTestgenSummary {
            status: "unavailable".to_string(),
            generated_at_strategy: "counterexample".to_string(),
            vector_count: 0,
            generated_files: Vec::new(),
            recommended_next_step: None,
            recommended_conformance_surface: "mixed".to_string(),
            recommended_docs: Vec::new(),
            recommended_mcp_tool: None,
            recommended_testgen_strategy: None,
            recommended_conformance_command: None,
            recommended_vectors: Vec::new(),
            error: error.map(str::to_string),
        },
    }
}

fn recommended_handoff_vectors(
    vectors: &[TestgenVectorSummary],
    generated_files: &[String],
    failing_properties: &[String],
) -> Vec<HandoffRecommendedVector> {
    let mut ordered = vectors
        .iter()
        .map(|vector| {
            let mut score = 0i32;
            if failing_properties
                .iter()
                .any(|property| property == &vector.property_id)
            {
                score += 100;
            }
            if matches!(
                vector.strategy.as_str(),
                "deadlock" | "enablement" | "boundary" | "path"
            ) {
                score += 50;
            }
            if !vector.requirement_clusters.is_empty() || !vector.risk_clusters.is_empty() {
                score += 25;
            }
            (score, vector)
        })
        .collect::<Vec<_>>();
    ordered.sort_by(|(left_score, left), (right_score, right)| {
        right_score
            .cmp(left_score)
            .then_with(|| left.vector_id.cmp(&right.vector_id))
    });

    let mut selected = Vec::new();
    for (_, vector) in ordered.into_iter().take(5) {
        let mut grouping = vector.requirement_clusters.clone();
        grouping.extend(vector.risk_clusters.clone());
        let artifact_paths = artifact_paths_for_vector(vector, generated_files);
        let recommended_conformance_surface =
            recommended_conformance_surface(&vector.suggested_surface).to_string();
        let recommended_docs = recommended_docs_for_vector(vector);
        let recommended_mcp_tool = recommended_mcp_tool_for_vector(vector);
        let recommended_testgen_strategy = recommended_testgen_strategy_for_vector(vector);
        let recommended_conformance_command =
            recommended_conformance_command(vector, &recommended_conformance_surface);
        selected.push(HandoffRecommendedVector {
            vector_id: vector.vector_id.clone(),
            property_id: vector.property_id.clone(),
            strategy: vector.strategy.clone(),
            counterexample_kind: vector.counterexample_kind.clone(),
            witness_kind: vector
                .notes
                .iter()
                .find_map(|note| note.strip_prefix("witness_kind:"))
                .map(str::to_string),
            priority: vector.priority.clone(),
            selection_reason: vector.selection_reason.clone(),
            grouping,
            observation_layers: vector.observation_layers.clone(),
            oracle_targets: vector.oracle_targets.clone(),
            suggested_surface: vector.suggested_surface.clone(),
            state_visibility: vector.state_visibility.clone(),
            parameter_bindings: vector.parameter_bindings.clone(),
            canonical_witness: vector
                .notes
                .iter()
                .find_map(|note| note.strip_prefix("canonical_witness:"))
                .map(|value| value.split(',').map(str::to_string).collect())
                .unwrap_or_default(),
            artifact_paths,
            recommended_next_command: recommended_next_command(vector),
            recommended_conformance_surface,
            recommended_docs,
            recommended_mcp_tool,
            recommended_testgen_strategy,
            recommended_conformance_command,
            why_this_vector_matters: why_vector_matters(vector, failing_properties),
        });
    }
    selected
}

fn artifact_paths_for_vector(
    vector: &TestgenVectorSummary,
    generated_files: &[String],
) -> Vec<String> {
    let expected = generated_test_path(&vector.vector_id);
    if generated_files.iter().any(|path| path == &expected) {
        vec![expected]
    } else {
        Vec::new()
    }
}

fn recommended_next_command(vector: &TestgenVectorSummary) -> String {
    if vector.strategy == "enablement" {
        if let Some(action_id) = &vector.focus_action_id {
            return format!(
                "cargo valid testgen <model> --strategy=enablement --focus-action={action_id} --json"
            );
        }
    }
    if vector.strategy == "deadlock" {
        return "cargo valid testgen <model> --strategy=deadlock --json".to_string();
    }
    match recommended_conformance_surface(&vector.suggested_surface) {
        "api" => "cargo valid conformance <model> --runner <api-runner>".to_string(),
        "ui" => "cargo valid conformance <model> --runner <ui-runner>".to_string(),
        "handler" => "cargo valid conformance <model> --runner <handler-runner>".to_string(),
        _ => "cargo valid testgen <model> --json".to_string(),
    }
}

fn recommended_docs_for_vector(vector: &TestgenVectorSummary) -> Vec<String> {
    let mut docs = vec!["testgen-and-handoff-guide".to_string()];
    match vector.strategy.as_str() {
        "deadlock" | "enablement" => docs.push("testgen-strategies-guide".to_string()),
        _ => {}
    }
    if vector.suggested_surface == "ui" || vector.suggested_surface == "api" {
        docs.push("ai-conformance-workflow".to_string());
    }
    if !vector.requirement_clusters.is_empty() || !vector.risk_clusters.is_empty() {
        docs.push("graph-and-review-guide".to_string());
    }
    docs.sort();
    docs.dedup();
    docs
}

fn recommended_mcp_tool_for_vector(vector: &TestgenVectorSummary) -> Option<String> {
    match vector.strategy.as_str() {
        "deadlock" | "enablement" | "counterexample" | "boundary" | "path" => {
            Some("valid_testgen".to_string())
        }
        _ if vector.suggested_surface == "api" || vector.suggested_surface == "ui" => {
            Some("valid_handoff".to_string())
        }
        _ => Some("valid_check".to_string()),
    }
}

fn recommended_testgen_strategy_for_vector(vector: &TestgenVectorSummary) -> Option<String> {
    Some(vector.strategy.clone())
}

fn recommended_conformance_command(
    vector: &TestgenVectorSummary,
    recommended_surface: &str,
) -> String {
    match recommended_surface {
        "api" => "cargo valid conformance <model> --runner <api-runner> --json".to_string(),
        "ui" => "cargo valid conformance <model> --runner <ui-runner> --json".to_string(),
        "handler" => "cargo valid conformance <model> --runner <handler-runner> --json".to_string(),
        _ => {
            if vector.strategy == "enablement" {
                "cargo valid testgen <model> --strategy=enablement --json".to_string()
            } else if vector.strategy == "deadlock" {
                "cargo valid testgen <model> --strategy=deadlock --json".to_string()
            } else {
                "cargo valid conformance <model> --runner <external-runner> --json".to_string()
            }
        }
    }
}

fn recommended_conformance_surface(suggested_surface: &str) -> &'static str {
    match suggested_surface {
        "api" => "api",
        "ui" => "ui",
        "handler" | "api_or_handler" => "handler",
        _ => "external-runner",
    }
}

fn summarize_conformance_surface(vectors: &[HandoffRecommendedVector]) -> String {
    let mut surfaces = vectors
        .iter()
        .map(|vector| vector.recommended_conformance_surface.as_str())
        .collect::<Vec<_>>();
    surfaces.sort_unstable();
    surfaces.dedup();
    if surfaces.len() == 1 {
        surfaces[0].to_string()
    } else {
        "mixed".to_string()
    }
}

fn recommended_next_step(vectors: &[HandoffRecommendedVector]) -> String {
    vectors
        .first()
        .map(|vector| vector.recommended_next_command.clone())
        .unwrap_or_else(|| "cargo valid testgen <model> --json".to_string())
}

fn summarize_recommended_docs(vectors: &[HandoffRecommendedVector]) -> Vec<String> {
    let mut docs = vectors
        .iter()
        .flat_map(|vector| vector.recommended_docs.clone())
        .collect::<Vec<_>>();
    docs.sort();
    docs.dedup();
    docs
}

fn summarize_recommended_mcp_tool(vectors: &[HandoffRecommendedVector]) -> Option<String> {
    vectors
        .first()
        .and_then(|vector| vector.recommended_mcp_tool.clone())
}

fn summarize_recommended_testgen_strategy(vectors: &[HandoffRecommendedVector]) -> Option<String> {
    vectors
        .first()
        .and_then(|vector| vector.recommended_testgen_strategy.clone())
}

fn summarize_recommended_conformance_command(
    vectors: &[HandoffRecommendedVector],
) -> Option<String> {
    vectors
        .first()
        .map(|vector| vector.recommended_conformance_command.clone())
}

fn why_vector_matters(vector: &TestgenVectorSummary, failing_properties: &[String]) -> String {
    if failing_properties
        .iter()
        .any(|property| property == &vector.property_id)
    {
        return "reproduces or guards a failing behavior".to_string();
    }
    if vector.strategy == "deadlock" {
        return "captures a deadlock-focused path worth guarding in implementation".to_string();
    }
    if vector.strategy == "enablement" {
        return "shows how to enable a currently blocked action in the SUT".to_string();
    }
    if !vector.requirement_clusters.is_empty() || !vector.risk_clusters.is_empty() {
        return "covers a named requirement or risk cluster for regression review".to_string();
    }
    "provides a representative execution contract for implementation tests".to_string()
}

fn section_recommended_test_vectors(summary: &HandoffTestgenSummary) -> HandoffSection {
    let mut bullets = Vec::new();
    if summary.status != "available" {
        bullets.push(format!(
            "Test vectors unavailable: {}.",
            summary
                .error
                .as_deref()
                .unwrap_or("no testgen summary was produced")
        ));
    } else {
        bullets.push(format!(
            "Generated {} vector(s) using `{}` strategy; {} generated test file(s) are available.",
            summary.vector_count,
            summary.generated_at_strategy,
            summary.generated_files.len()
        ));
        bullets.push(format!(
            "Recommended next step: `{}` on conformance surface `{}`.",
            summary
                .recommended_next_step
                .as_deref()
                .unwrap_or("cargo valid testgen <model> --json"),
            summary.recommended_conformance_surface
        ));
        if !summary.recommended_docs.is_empty() {
            bullets.push(format!(
                "Recommended docs: {}.",
                comma_or_none(&summary.recommended_docs)
            ));
        }
        if let Some(tool) = &summary.recommended_mcp_tool {
            bullets.push(format!("Recommended MCP tool: `{tool}`."));
        }
        if let Some(strategy) = &summary.recommended_testgen_strategy {
            bullets.push(format!("Recommended testgen strategy: `{strategy}`."));
        }
        if let Some(command) = &summary.recommended_conformance_command {
            bullets.push(format!("Recommended conformance command: `{command}`."));
        }
        for vector in &summary.recommended_vectors {
            bullets.push(format!(
                "`{}` for `{}` via `{}` on `{}` [{} -> {}] priority=`{}` reason=`{}` params=[{}] because {}. Next: `{}`. Conformance: `{}`. MCP: `{}`. Docs: [{}]. Artifacts: [{}].",
                vector.vector_id,
                vector.property_id,
                vector.strategy,
                vector.suggested_surface,
                comma_or_none(&vector.observation_layers),
                comma_or_none(&vector.oracle_targets),
                vector.priority,
                vector.selection_reason,
                vector
                    .parameter_bindings
                    .iter()
                    .map(|binding| format!("{}={}", binding.name, binding.value))
                    .collect::<Vec<_>>()
                    .join(", "),
                vector.why_this_vector_matters,
                vector.recommended_next_command,
                vector.recommended_conformance_command,
                vector
                    .recommended_mcp_tool
                    .as_deref()
                    .unwrap_or("n/a"),
                comma_or_none(&vector.recommended_docs),
                comma_or_none(&vector.artifact_paths)
            ));
        }
    }
    HandoffSection {
        id: "recommended-test-vectors".to_string(),
        title: "Recommended Test Vectors".to_string(),
        bullets,
    }
}

pub fn check_handoff(
    output_path: String,
    existing: Option<&str>,
    generated: &GeneratedHandoff,
) -> HandoffCheckReport {
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
    HandoffCheckReport {
        schema_version: "1.0.0".to_string(),
        status: if drift_sections.is_empty() {
            "unchanged".to_string()
        } else {
            "changed".to_string()
        },
        model_id: generated.model_id.clone(),
        property_id: generated.property_id.clone(),
        output_path,
        source_hash: generated.source_hash.clone(),
        existing_hash,
        generated_hash: generated.generated_hash.clone(),
        contract_hash: generated.contract_hash.clone(),
        drift_sections,
        sections: generated.sections.clone(),
        testgen_summary: generated.testgen_summary.clone(),
        failing_properties: generated.failing_properties.clone(),
        open_ambiguities: generated.open_ambiguities.clone(),
        markdown: generated.markdown.clone(),
    }
}

pub fn write_handoff(path: &str, generated: &GeneratedHandoff) -> Result<(), String> {
    write_text_file(path, &generated.markdown)
}

pub fn render_handoff_text(generated: &GeneratedHandoff, output_path: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&text_header(&format!(
        "Implementation handoff: {}",
        generated.model_id
    )));
    out.push_str(&format!(
        "{} {}\n",
        crate::cli::text_status_badge("generated"),
        text_kv("contract_hash", generated.contract_hash.as_str())
    ));
    out.push_str(&text_section("What To Do Next"));
    if let Some(step) = &generated.testgen_summary.recommended_next_step {
        out.push_str(&format!(
            "{}\n",
            text_bullet(&format!("next command: {}", text_command(step)))
        ));
    }
    if let Some(command) = &generated.testgen_summary.recommended_conformance_command {
        out.push_str(&format!(
            "{}\n",
            text_bullet(&format!("conformance command: {}", text_command(command)))
        ));
    }
    if let Some(tool) = &generated.testgen_summary.recommended_mcp_tool {
        out.push_str(&format!(
            "{}\n",
            text_bullet(&format!("mcp tool: `{tool}`"))
        ));
    }
    if !generated.testgen_summary.recommended_docs.is_empty() {
        out.push_str(&format!(
            "{}\n",
            text_bullet(&format!(
                "docs: {}",
                generated.testgen_summary.recommended_docs.join(", ")
            ))
        ));
    }
    out.push('\n');
    out.push_str(&generated.markdown);
    if let Some(path) = output_path {
        out.push_str(&format!(
            "\n{}\n",
            text_hint(&format!("output_path: {path}"))
        ));
    }
    out
}

pub fn render_handoff_json(generated: &GeneratedHandoff, output_path: Option<&str>) -> String {
    json!({
        "schema_version": generated.schema_version,
        "status": "generated",
        "model_id": generated.model_id,
        "property_id": generated.property_id,
        "source_hash": generated.source_hash,
        "contract_hash": generated.contract_hash,
        "generated_hash": generated.generated_hash,
        "output_path": output_path,
        "sections": generated.sections,
        "testgen_summary": generated.testgen_summary,
        "failing_properties": generated.failing_properties,
        "open_ambiguities": generated.open_ambiguities,
        "markdown": generated.markdown,
    })
    .to_string()
}

pub fn render_handoff_check_text(report: &HandoffCheckReport) -> String {
    let mut out = String::new();
    out.push_str(&text_header("handoff --check"));
    out.push_str(&format!(
        "{} {}\n",
        crate::cli::text_status_badge(report.status.as_str()),
        text_kv("model_id", report.model_id.as_str())
    ));
    if let Some(property_id) = &report.property_id {
        out.push_str(&format!("{}\n", text_kv("property_id", property_id)));
    }
    out.push_str(&format!(
        "{}\n",
        text_kv("output_path", report.output_path.as_str())
    ));
    out.push_str(&format!(
        "{}\n",
        text_kv("source_hash", report.source_hash.as_str())
    ));
    out.push_str(&format!(
        "{}\n",
        text_kv("generated_hash", report.generated_hash.as_str())
    ));
    out.push_str(&format!(
        "{}\n",
        text_kv("contract_hash", report.contract_hash.as_str())
    ));
    if let Some(existing_hash) = &report.existing_hash {
        out.push_str(&format!("{}\n", text_kv("existing_hash", existing_hash)));
    } else {
        out.push_str(&format!("{}\n", text_kv("existing_hash", "<missing>")));
    }
    if !report.drift_sections.is_empty() {
        out.push_str(&text_section("Drift Sections"));
        for section in &report.drift_sections {
            out.push_str(&format!("{}\n", text_bullet(section)));
        }
    }
    out
}

pub fn render_handoff_check_json(report: &HandoffCheckReport) -> String {
    json!({
        "schema_version": report.schema_version,
        "status": report.status,
        "model_id": report.model_id,
        "property_id": report.property_id,
        "output_path": report.output_path,
        "source_hash": report.source_hash,
        "existing_hash": report.existing_hash,
        "generated_hash": report.generated_hash,
        "contract_hash": report.contract_hash,
        "drift_sections": report.drift_sections,
        "sections": report.sections,
        "testgen_summary": report.testgen_summary,
        "failing_properties": report.failing_properties,
        "open_ambiguities": report.open_ambiguities,
        "markdown": report.markdown,
    })
    .to_string()
}

fn render_handoff_markdown(
    model_id: &str,
    property_id: Option<&str>,
    source_hash: &str,
    contract_hash: &str,
    sections: &[HandoffSection],
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "<!-- valid-handoff: model_id={} property_id={} source_hash={} contract_hash={} -->\n\n",
        model_id,
        property_id.unwrap_or(""),
        source_hash,
        contract_hash
    ));
    out.push_str(&format!("# Implementation Handoff: {}\n\n", model_id));
    if let Some(property_id) = property_id {
        out.push_str(&format!("Target property filter: `{property_id}`\n\n"));
    }
    for section in sections {
        out.push_str(&format!("## {}\n\n", section.title));
        if section.bullets.is_empty() {
            out.push_str("- none\n\n");
            continue;
        }
        for bullet in &section.bullets {
            out.push_str(&format!("- {bullet}\n"));
        }
        out.push('\n');
    }
    out
}

fn selected_runs<'a>(
    runs: &'a [OrchestratedRunSummary],
    property_id: Option<&str>,
) -> Vec<&'a OrchestratedRunSummary> {
    runs.iter()
        .filter(|run| {
            property_id
                .map(|candidate| run.property_id == candidate)
                .unwrap_or(true)
        })
        .collect()
}

fn selected_properties<'a>(
    inspect: &'a InspectResponse,
    property_id: Option<&str>,
) -> Vec<&'a InspectProperty> {
    inspect
        .property_details
        .iter()
        .filter(|property| {
            property_id
                .map(|candidate| property.property_id == candidate)
                .unwrap_or(true)
        })
        .collect()
}

fn section_feature_goal(
    inspect: &InspectResponse,
    runs: &[&OrchestratedRunSummary],
    property_id: Option<&str>,
    contract_hash: &str,
) -> HandoffSection {
    let property_scope = property_id
        .map(|value| format!("property `{value}` only"))
        .unwrap_or_else(|| format!("{} verified properties", runs.len()));
    HandoffSection {
        id: "feature-goal".to_string(),
        title: "Feature / Goal".to_string(),
        bullets: vec![
            format!(
                "Implement behavior for model `{}` with handoff scope {}.",
                inspect.model_id, property_scope
            ),
            format!(
                "Preserve {} state field(s), {} action(s), and the contract snapshot `{}`.",
                inspect.state_field_details.len(),
                inspect.action_details.len(),
                contract_hash
            ),
        ],
    }
}

fn section_required_behaviors(
    properties: &[&InspectProperty],
    runs: &[&OrchestratedRunSummary],
) -> HandoffSection {
    let statuses = runs
        .iter()
        .map(|run| (run.property_id.as_str(), run))
        .collect::<BTreeMap<_, _>>();
    let bullets = properties
        .iter()
        .map(|property| {
            let status = statuses
                .get(property.property_id.as_str())
                .map(|run| format!("{} / {}", run.status, run.assurance_level))
                .unwrap_or_else(|| "not_run".to_string());
            format!(
                "`{}` [{}] {}",
                property.property_id,
                status,
                property_description(property)
            )
        })
        .collect::<Vec<_>>();
    HandoffSection {
        id: "required-behaviors".to_string(),
        title: "Required Behaviors".to_string(),
        bullets,
    }
}

fn section_forbidden_behaviors(
    runs: &[&OrchestratedRunSummary],
    explain_by_property: &BTreeMap<String, &ExplainResponse>,
) -> HandoffSection {
    let mut bullets = Vec::new();
    for run in runs.iter().filter(|run| run.status == "FAIL") {
        if let Some(explain) = explain_by_property.get(&run.property_id) {
            bullets.push(format!(
                "`{}` currently fails after action `{}` at step {} with changed fields [{}].",
                run.property_id,
                explain.failing_action_id.as_deref().unwrap_or("INITIAL"),
                explain.failure_step_index,
                comma_or_none(&explain.changed_fields)
            ));
        } else {
            bullets.push(format!(
                "`{}` is failing in run `{}` and must remain impossible in implementation.",
                run.property_id, run.run_id
            ));
        }
    }
    if bullets.is_empty() {
        bullets.push("No failing counterexample was reported in the selected run set.".to_string());
    }
    HandoffSection {
        id: "forbidden-behaviors".to_string(),
        title: "Forbidden Behaviors".to_string(),
        bullets,
    }
}

fn section_state_facts(inspect: &InspectResponse) -> HandoffSection {
    let mut bullets = inspect
        .state_field_details
        .iter()
        .map(|field| {
            let mut facts = vec![field.rust_type.clone()];
            if let Some(range) = &field.range {
                facts.push(format!("range {range}"));
            }
            if !field.variants.is_empty() {
                facts.push(format!("variants {}", field.variants.join(", ")));
            }
            format!("state `{}`: {}", field.name, facts.join("; "))
        })
        .collect::<Vec<_>>();
    bullets.extend(inspect.transition_details.iter().map(|transition| {
        format!(
            "action `{}` reads [{}], writes [{}], guard `{}`, tags [{}].",
            transition.action_id,
            comma_or_none(&transition.reads),
            comma_or_none(&transition.writes),
            transition.guard.as_deref().unwrap_or("true"),
            comma_or_none(&transition.path_tags)
        )
    }));
    HandoffSection {
        id: "state-facts".to_string(),
        title: "State Facts From Valid".to_string(),
        bullets,
    }
}

fn section_properties_to_preserve(
    properties: &[&InspectProperty],
    runs: &[&OrchestratedRunSummary],
) -> HandoffSection {
    let mut run_map = BTreeMap::new();
    for run in runs {
        run_map.insert(run.property_id.as_str(), *run);
    }
    let bullets = properties
        .iter()
        .map(|property| {
            let run = run_map.get(property.property_id.as_str());
            let status = run.map(|item| item.status.as_str()).unwrap_or("not_run");
            let assurance = run
                .map(|item| item.assurance_level.as_str())
                .unwrap_or("n/a");
            format!(
                "`{}` kind=`{}` status=`{}` assurance=`{}`.",
                property.property_id, property.kind, status, assurance
            )
        })
        .collect::<Vec<_>>();
    HandoffSection {
        id: "properties-to-preserve".to_string(),
        title: "Properties To Preserve".to_string(),
        bullets,
    }
}

fn section_implementation_implications(
    failing_properties: &[String],
    explain_by_property: &BTreeMap<String, &ExplainResponse>,
) -> HandoffSection {
    let mut bullets = Vec::new();
    for property_id in failing_properties {
        if let Some(explain) = explain_by_property.get(property_id) {
            for target in &explain.repair_targets {
                bullets.push(format!(
                    "`{}` repair target `{}` priority=`{}` reason=`{}` fields [{}].",
                    property_id,
                    target.target,
                    target.priority,
                    target.reason,
                    comma_or_none(&target.fields)
                ));
            }
            for hint in explain
                .repair_hints
                .iter()
                .chain(explain.next_steps.iter())
                .chain(explain.best_practices.iter())
            {
                bullets.push(format!("`{}`: {}", property_id, hint));
            }
            for cause in &explain.candidate_causes {
                bullets.push(format!(
                    "`{}` likely cause [{}]: {}",
                    property_id, cause.kind, cause.message
                ));
            }
        }
    }
    if bullets.is_empty() {
        bullets.push("No failing property currently contributes repair guidance.".to_string());
    }
    HandoffSection {
        id: "implementation-implications".to_string(),
        title: "Implementation Implications".to_string(),
        bullets,
    }
}

fn section_test_checklist(
    coverage: &CoverageReport,
    failing_properties: &[String],
    explain_by_property: &BTreeMap<String, &ExplainResponse>,
) -> HandoffSection {
    let mut bullets = Vec::new();
    let uncovered_actions = coverage
        .total_actions
        .difference(&coverage.covered_actions)
        .cloned()
        .collect::<Vec<_>>();
    if !uncovered_actions.is_empty() {
        bullets.push(format!(
            "Add execution coverage for uncovered actions [{}].",
            uncovered_actions.join(", ")
        ));
    }
    if !coverage.uncovered_guards.is_empty() {
        bullets.push(format!(
            "Add tests for unresolved guard polarities [{}].",
            coverage.uncovered_guards.join(", ")
        ));
    }
    let uncovered_requirement_tags = coverage
        .total_requirement_tags
        .difference(&coverage.covered_requirement_tags)
        .cloned()
        .collect::<Vec<_>>();
    if !uncovered_requirement_tags.is_empty() {
        bullets.push(format!(
            "Cover missing requirement tags [{}].",
            uncovered_requirement_tags.join(", ")
        ));
    }
    for property_id in failing_properties {
        if let Some(explain) = explain_by_property.get(property_id) {
            bullets.push(format!(
                "Add a regression for `{}` around action `{}` and changed fields [{}].",
                property_id,
                explain.failing_action_id.as_deref().unwrap_or("INITIAL"),
                comma_or_none(&explain.changed_fields)
            ));
        }
    }
    if bullets.is_empty() {
        bullets.push(
            "Current traces cover all discovered actions, guards, and requirement tags."
                .to_string(),
        );
    }
    HandoffSection {
        id: "test-checklist".to_string(),
        title: "Test Checklist".to_string(),
        bullets,
    }
}

fn section_open_ambiguities(open_ambiguities: &[String]) -> HandoffSection {
    HandoffSection {
        id: "open-ambiguities".to_string(),
        title: "Open Ambiguities".to_string(),
        bullets: if open_ambiguities.is_empty() {
            vec![
                "No UNKNOWN verification result or unresolved target ambiguity was reported."
                    .to_string(),
            ]
        } else {
            open_ambiguities.to_vec()
        },
    }
}

fn section_source_evidence(
    runs: &[&OrchestratedRunSummary],
    failing_properties: &[String],
    explain_by_property: &BTreeMap<String, &ExplainResponse>,
    source_hash: &str,
    contract_hash: &str,
    coverage: &CoverageReport,
) -> HandoffSection {
    let mut bullets = vec![
        format!("source_hash: `{source_hash}`"),
        format!("contract_hash: `{contract_hash}`"),
        format!(
            "coverage summary: transition={}%, guards={}%, requirement_tags={}%.",
            coverage.transition_coverage_percent,
            coverage.guard_full_coverage_percent,
            coverage.requirement_tag_coverage_percent
        ),
    ];
    bullets.extend(runs.iter().map(|run| {
        format!(
            "run `{}` property `{}` => status=`{}` assurance=`{}`.",
            run.run_id, run.property_id, run.status, run.assurance_level
        )
    }));
    for property_id in failing_properties {
        if let Some(explain) = explain_by_property.get(property_id) {
            bullets.push(format!(
                "explain `{}` evidence_id=`{}` breakpoint=`{}` confidence={:.2}.",
                property_id, explain.evidence_id, explain.breakpoint_kind, explain.confidence
            ));
        }
    }
    HandoffSection {
        id: "source-evidence".to_string(),
        title: "Source Evidence".to_string(),
        bullets,
    }
}

fn build_open_ambiguities(
    inspect: &InspectResponse,
    runs: &[&OrchestratedRunSummary],
    coverage: &CoverageReport,
    property_id: Option<&str>,
) -> Vec<String> {
    let mut ambiguities = Vec::new();
    for run in runs.iter().filter(|run| run.status == "UNKNOWN") {
        ambiguities.push(format!(
            "Property `{}` returned UNKNOWN with assurance `{}` in run `{}` and must be reviewed before implementation is considered complete.",
            run.property_id, run.assurance_level, run.run_id
        ));
    }
    if property_id.is_some() && runs.is_empty() {
        ambiguities.push(
            "The selected property filter did not match any orchestrated property.".to_string(),
        );
    }
    if !inspect.capabilities.reasons.is_empty() {
        ambiguities.push(format!(
            "Capability limitations reported by inspect: {}.",
            inspect.capabilities.reasons.join(", ")
        ));
    }
    if coverage.guard_full_coverage_percent < 100 {
        ambiguities.push(format!(
            "Guard coverage is incomplete at {}%, so some negative-path behavior remains unobserved.",
            coverage.guard_full_coverage_percent
        ));
    }
    ambiguities
}

fn property_description(property: &InspectProperty) -> String {
    let mut parts = Vec::new();
    if let Some(expr) = &property.expr {
        parts.push(format!("expr `{}`", expr.replace('\n', " ")));
    }
    if let Some(scope_expr) = &property.scope_expr {
        parts.push(format!("scope `{}`", scope_expr.replace('\n', " ")));
    }
    if let Some(action_filter) = &property.action_filter {
        parts.push(format!("action_filter `{action_filter}`"));
    }
    if parts.is_empty() {
        property.kind.clone()
    } else {
        format!("{}; {}", property.kind, parts.join("; "))
    }
}

fn comma_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn extract_metadata(body: &str, key: &str) -> Option<String> {
    let line = body.lines().next()?;
    if !line.starts_with("<!-- valid-handoff:") {
        return None;
    }
    let trimmed = line
        .trim_start_matches("<!-- valid-handoff:")
        .trim_end_matches("-->")
        .trim();
    for part in trimmed.split_whitespace() {
        let (candidate_key, value) = part.split_once('=')?;
        if candidate_key == key {
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::{
        api::{InspectCapabilities, InspectTemporalCapabilities},
        coverage::CoverageReport,
    };

    use super::*;

    #[test]
    fn generated_handoff_uses_stable_default_path() {
        assert_eq!(
            default_handoff_path("Model/Name"),
            "artifacts/handoff/Model-Name.md"
        );
    }

    #[test]
    fn handoff_check_detects_markdown_drift() {
        let inspect = InspectResponse {
            schema_version: "1.0.0".to_string(),
            request_id: "req".to_string(),
            status: "ok".to_string(),
            model_id: "Demo".to_string(),
            default_profile_id: "default".to_string(),
            machine_ir_ready: true,
            machine_ir_error: None,
            capabilities: InspectCapabilities {
                parse_ready: true,
                parse: crate::modeling::CapabilityDetail::ready(),
                explicit_ready: true,
                explicit: crate::modeling::CapabilityDetail::ready(),
                ir_ready: true,
                ir: crate::modeling::CapabilityDetail::ready(),
                solver_ready: true,
                solver: crate::modeling::CapabilityDetail::ready(),
                coverage_ready: true,
                coverage: crate::modeling::CapabilityDetail::ready(),
                explain_ready: true,
                explain: crate::modeling::CapabilityDetail::ready(),
                testgen_ready: true,
                testgen: crate::modeling::CapabilityDetail::ready(),
                temporal: InspectTemporalCapabilities {
                    property_ids: Vec::new(),
                    operators: Vec::new(),
                    support_level: "not_applicable".to_string(),
                    explicit_status: "not_applicable".to_string(),
                    solver_status: "not_applicable".to_string(),
                    reason: String::new(),
                    fairness_support: "not_applicable".to_string(),
                    fairness_kinds: Vec::new(),
                    semantics_scope: "not_applicable".to_string(),
                    backend_statuses: Vec::new(),
                },
                reasons: Vec::new(),
            },
            state_fields: vec!["x".to_string()],
            actions: vec!["Inc".to_string()],
            predicates: Vec::new(),
            scenarios: Vec::new(),
            properties: vec!["P_SAFE".to_string()],
            state_field_details: vec![crate::api::InspectStateField {
                name: "x".to_string(),
                rust_type: "u8".to_string(),
                range: Some("0..=1".to_string()),
                variants: Vec::new(),
                is_set: false,
                domain: crate::api::InspectBoundedDomain {
                    kind: "range".to_string(),
                    summary: "0..=1".to_string(),
                    cardinality: Some(2),
                    min: Some("0".to_string()),
                    max: Some("1".to_string()),
                    values: Vec::new(),
                },
            }],
            action_details: vec![crate::api::InspectAction {
                action_id: "Inc".to_string(),
                conceptual_action_id: "Inc".to_string(),
                concrete_action_id: None,
                parameter_bindings: Vec::new(),
                parameter_domains: Vec::new(),
                expanded_choice_count: 1,
                role: "business".to_string(),
                reads: vec!["x".to_string()],
                writes: vec!["x".to_string()],
            }],
            predicate_details: Vec::new(),
            scenario_details: Vec::new(),
            analysis_profiles: vec![crate::api::InspectAnalysisProfile {
                profile_id: "default".to_string(),
                scenario_id: None,
                scope_expr: None,
                backend_hint: None,
                doc_graph_policy: "full".to_string(),
                deadlock_check: true,
                notes: vec!["test profile".to_string()],
            }],
            transition_details: vec![crate::api::InspectTransition {
                action_id: "Inc".to_string(),
                conceptual_action_id: "Inc".to_string(),
                concrete_action_id: None,
                parameter_bindings: Vec::new(),
                role: "business".to_string(),
                guard: Some("x <= 0".to_string()),
                effect: None,
                reads: vec!["x".to_string()],
                writes: vec!["x".to_string()],
                path_tags: vec!["inc_path".to_string()],
                updates: Vec::new(),
            }],
            property_details: vec![crate::api::InspectProperty {
                property_id: "P_SAFE".to_string(),
                kind: "invariant".to_string(),
                layer: "assert".to_string(),
                expr: Some("x <= 1".to_string()),
                scope_expr: None,
                action_filter: None,
            }],
        };
        let coverage = CoverageReport {
            schema_version: "1.0.0".to_string(),
            model_id: "Demo".to_string(),
            transition_coverage_percent: 100,
            conceptual_transition_coverage_percent: 100,
            business_transition_coverage_percent: 100,
            business_conceptual_transition_coverage_percent: 100,
            setup_transition_coverage_percent: 100,
            requirement_tag_coverage_percent: 100,
            decision_coverage_percent: 100,
            guard_full_coverage_percent: 100,
            business_guard_full_coverage_percent: 100,
            setup_guard_full_coverage_percent: 100,
            covered_actions: std::collections::BTreeSet::from(["Inc".to_string()]),
            covered_conceptual_actions: std::collections::BTreeSet::from(["Inc".to_string()]),
            covered_decisions: std::collections::BTreeSet::new(),
            total_actions: std::collections::BTreeSet::from(["Inc".to_string()]),
            total_conceptual_actions: std::collections::BTreeSet::from(["Inc".to_string()]),
            total_decisions: std::collections::BTreeSet::new(),
            action_roles: BTreeMap::from([("Inc".to_string(), "business".to_string())]),
            action_execution_counts: BTreeMap::from([("Inc".to_string(), 1)]),
            conceptual_action_execution_counts: BTreeMap::from([("Inc".to_string(), 1)]),
            decision_counts: BTreeMap::new(),
            covered_requirement_tags: std::collections::BTreeSet::new(),
            total_requirement_tags: std::collections::BTreeSet::new(),
            requirement_tag_counts: BTreeMap::new(),
            visited_state_count: 1,
            repeated_state_count: 0,
            max_depth_observed: 1,
            guard_true_actions: std::collections::BTreeSet::from(["Inc".to_string()]),
            guard_false_actions: std::collections::BTreeSet::from(["Inc".to_string()]),
            guard_true_counts: BTreeMap::from([("Inc".to_string(), 1)]),
            guard_false_counts: BTreeMap::from([("Inc".to_string(), 1)]),
            uncovered_guards: Vec::new(),
            path_tag_counts: BTreeMap::from([("inc_path".to_string(), 1)]),
            depth_histogram: BTreeMap::from([(0, 1)]),
            step_count: 1,
        };
        let testgen = crate::api::TestgenResponse {
            schema_version: "1.0.0".to_string(),
            request_id: "req-testgen".to_string(),
            status: "ok".to_string(),
            vector_ids: vec!["vec-1".to_string()],
            vectors: vec![crate::api::TestgenVectorSummary {
                vector_id: "vec-1".to_string(),
                run_id: "run-1".to_string(),
                property_id: "P_SAFE".to_string(),
                strictness: "strict".to_string(),
                derivation: "explicit".to_string(),
                source_kind: "counterexample".to_string(),
                counterexample_kind: Some("invariant".to_string()),
                witness_kind: Some("positive".to_string()),
                strategy: "counterexample".to_string(),
                requirement_clusters: vec!["property:P_SAFE".to_string()],
                risk_clusters: Vec::new(),
                observation_mode: "exact".to_string(),
                observation_layers: vec!["output".to_string()],
                oracle_targets: vec!["observations".to_string()],
                suggested_surface: "api_or_handler".to_string(),
                state_visibility: "optional".to_string(),
                focus_action_id: None,
                expected_guard_enabled: None,
                priority: "high".to_string(),
                selection_reason: "shortest failing reproduction".to_string(),
                novelty_key: "P_SAFE|Inc".to_string(),
                conceptual_action_ids: vec!["Inc".to_string()],
                concrete_action_ids: vec!["Inc".to_string()],
                parameter_bindings: Vec::new(),
                canonical_witness: vec!["Inc".to_string()],
                notes: vec!["counterexample vector".to_string()],
            }],
            vector_groups: vec![crate::api::TestgenGroupSummary {
                group_kind: "requirement".to_string(),
                group_id: "property:P_SAFE".to_string(),
                vector_ids: vec!["vec-1".to_string()],
            }],
            generated_files: vec!["generated-tests/vec-1.rs".to_string()],
        };
        let generated = generate_handoff(HandoffInputs {
            inspect: &inspect,
            runs: &[OrchestratedRunSummary {
                property_id: "P_SAFE".to_string(),
                counterexample_kind: None,
                status: "PASS".to_string(),
                assurance_level: "complete".to_string(),
                run_id: "run-1".to_string(),
            }],
            coverage: &coverage,
            explanations: &[],
            testgen: Some(&testgen),
            testgen_error: None,
            property_id: None,
            source_hash: "sha256:source",
            contract_hash: "sha256:contract",
        });
        assert_eq!(generated.testgen_summary.status, "available");
        assert_eq!(generated.testgen_summary.recommended_vectors.len(), 1);
        assert_eq!(
            generated.testgen_summary.recommended_conformance_surface,
            "handler"
        );
        assert!(generated
            .testgen_summary
            .recommended_next_step
            .as_deref()
            .unwrap_or_default()
            .contains("cargo valid conformance"));
        assert_eq!(
            generated.testgen_summary.recommended_vectors[0].artifact_paths,
            vec!["generated-tests/vec-1.rs".to_string()]
        );
        assert!(generated.markdown.contains("Recommended Test Vectors"));
        let report = check_handoff(
            "artifacts/handoff/Demo.md".to_string(),
            Some(&(generated.markdown.clone() + "\nmanual drift\n")),
            &generated,
        );
        assert_eq!(report.status, "changed");
        assert!(report.drift_sections.contains(&"markdown".to_string()));
    }
}
