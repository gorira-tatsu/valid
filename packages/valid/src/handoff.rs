use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::json;

use crate::{
    api::{ExplainResponse, InspectProperty, InspectResponse, OrchestratedRunSummary},
    coverage::CoverageReport,
    support::{artifact::handoff_path, hash::stable_hash_hex, io::write_text_file},
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
    pub failing_properties: Vec<String>,
    pub open_ambiguities: Vec<String>,
    pub markdown: String,
}

pub struct HandoffInputs<'a> {
    pub inspect: &'a InspectResponse,
    pub runs: &'a [OrchestratedRunSummary],
    pub coverage: &'a CoverageReport,
    pub explanations: &'a [ExplainResponse],
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
        failing_properties,
        open_ambiguities,
        markdown,
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
    out.push_str(&generated.markdown);
    if let Some(path) = output_path {
        out.push_str(&format!("\noutput_path: {path}\n"));
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
        "failing_properties": generated.failing_properties,
        "open_ambiguities": generated.open_ambiguities,
        "markdown": generated.markdown,
    })
    .to_string()
}

pub fn render_handoff_check_text(report: &HandoffCheckReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("status: {}\n", report.status));
    out.push_str(&format!("model_id: {}\n", report.model_id));
    if let Some(property_id) = &report.property_id {
        out.push_str(&format!("property_id: {property_id}\n"));
    }
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
            }],
            action_details: vec![crate::api::InspectAction {
                action_id: "Inc".to_string(),
                role: "business".to_string(),
                reads: vec!["x".to_string()],
                writes: vec!["x".to_string()],
            }],
            predicate_details: Vec::new(),
            scenario_details: Vec::new(),
            transition_details: vec![crate::api::InspectTransition {
                action_id: "Inc".to_string(),
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
            business_transition_coverage_percent: 100,
            setup_transition_coverage_percent: 100,
            requirement_tag_coverage_percent: 100,
            decision_coverage_percent: 100,
            guard_full_coverage_percent: 100,
            business_guard_full_coverage_percent: 100,
            setup_guard_full_coverage_percent: 100,
            covered_actions: std::collections::BTreeSet::from(["Inc".to_string()]),
            covered_decisions: std::collections::BTreeSet::new(),
            total_actions: std::collections::BTreeSet::from(["Inc".to_string()]),
            total_decisions: std::collections::BTreeSet::new(),
            action_roles: BTreeMap::from([("Inc".to_string(), "business".to_string())]),
            action_execution_counts: BTreeMap::from([("Inc".to_string(), 1)]),
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
        let generated = generate_handoff(HandoffInputs {
            inspect: &inspect,
            runs: &[OrchestratedRunSummary {
                property_id: "P_SAFE".to_string(),
                status: "PASS".to_string(),
                assurance_level: "complete".to_string(),
                run_id: "run-1".to_string(),
            }],
            coverage: &coverage,
            explanations: &[],
            property_id: None,
            source_hash: "sha256:source",
            contract_hash: "sha256:contract",
        });
        let report = check_handoff(
            "artifacts/handoff/Demo.md".to_string(),
            Some(&(generated.markdown.clone() + "\nmanual drift\n")),
            &generated,
        );
        assert_eq!(report.status, "changed");
        assert!(report.drift_sections.contains(&"markdown".to_string()));
    }
}
