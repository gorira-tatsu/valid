use std::{collections::BTreeMap, fs, path::Path};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PropertySuiteEntry {
    pub model: String,
    pub properties: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct RerunRecommendations {
    pub affected_critical_properties: Vec<String>,
    pub affected_property_suites: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct CoverageGates {
    pub minimum_overall_coverage_percent: Option<u32>,
    pub minimum_business_coverage_percent: Option<u32>,
    pub minimum_setup_coverage_percent: Option<u32>,
    pub minimum_requirement_coverage_percent: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct VerificationPolicy {
    pub suite_models: Vec<String>,
    pub critical_properties: BTreeMap<String, Vec<String>>,
    pub property_suites: BTreeMap<String, Vec<PropertySuiteEntry>>,
    pub preferred_backends: Vec<String>,
    pub default_suite: Option<String>,
    pub coverage_gates: CoverageGates,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ProjectConfig {
    pub registry: Option<String>,
    pub default_backend: Option<String>,
    pub default_property: Option<String>,
    pub default_solver_executable: Option<String>,
    pub default_solver_args: Vec<String>,
    pub suite_models: Vec<String>,
    pub critical_properties: BTreeMap<String, Vec<String>>,
    pub property_suites: BTreeMap<String, Vec<PropertySuiteEntry>>,
    pub preferred_backends: Vec<String>,
    pub default_suite: Option<String>,
    pub coverage_gates: CoverageGates,
    pub benchmark_models: Vec<String>,
    pub benchmark_repeats: Option<usize>,
    pub generated_tests_dir: Option<String>,
    pub artifacts_dir: Option<String>,
    pub benchmarks_dir: Option<String>,
    pub benchmark_baseline_dir: Option<String>,
    pub benchmark_regression_threshold_percent: Option<u32>,
    pub default_graph_format: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConfigSection {
    TopLevel,
    CriticalProperties,
    PropertySuite(String),
}

pub fn load_project_config(root: &Path) -> Result<Option<ProjectConfig>, String> {
    let path = root.join("valid.toml");
    if !path.exists() {
        return Ok(None);
    }
    let body = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read `{}`: {err}", path.display()))?;
    let config = parse_project_config(&body)?;
    Ok(Some(config))
}

pub fn parse_project_config(body: &str) -> Result<ProjectConfig, String> {
    let mut config = ProjectConfig::default();
    let mut section = ConfigSection::TopLevel;
    for (index, raw_line) in body.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = parse_section_header(line, index + 1)?;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!(
                "invalid config line {}: expected `key = value`",
                index + 1
            ));
        };
        match &section {
            ConfigSection::TopLevel => {
                if !assign_top_level_key(&mut config, key.trim(), value.trim(), index + 1)? {
                    return Err(format!(
                        "unsupported config key `{}` on line {}",
                        key.trim(),
                        index + 1
                    ));
                }
            }
            ConfigSection::CriticalProperties => {
                if assign_top_level_key(&mut config, key.trim(), value.trim(), index + 1)? {
                    section = ConfigSection::TopLevel;
                    continue;
                }
                config.critical_properties.insert(
                    key.trim().to_string(),
                    parse_string_array(value.trim(), index + 1)?,
                );
            }
            ConfigSection::PropertySuite(name) => match key.trim() {
                "entries" => {
                    config.property_suites.insert(
                        name.clone(),
                        parse_property_suite_entries(value.trim(), index + 1)?,
                    );
                }
                other => {
                    if assign_top_level_key(&mut config, other, value.trim(), index + 1)? {
                        section = ConfigSection::TopLevel;
                        continue;
                    }
                    return Err(format!(
                        "unsupported property suite key `{other}` on line {}",
                        index + 1
                    ));
                }
            },
        }
    }
    Ok(config)
}

pub fn verification_policy(config: &ProjectConfig) -> VerificationPolicy {
    VerificationPolicy {
        suite_models: config.suite_models.clone(),
        critical_properties: config.critical_properties.clone(),
        property_suites: config.property_suites.clone(),
        preferred_backends: config.preferred_backends.clone(),
        default_suite: config.default_suite.clone(),
        coverage_gates: config.coverage_gates.clone(),
    }
}

pub fn render_project_config_template(registry: &str) -> String {
    format!(
        "registry = {:?}\ndefault_backend = \"explicit\"\ndefault_property = \"\"\ndefault_solver_executable = \"\"\ndefault_solver_args = []\nsuite_models = []\npreferred_backends = [\"explicit\"]\ndefault_suite = \"smoke\"\nminimum_overall_coverage_percent = 80\nminimum_business_coverage_percent = 75\nminimum_setup_coverage_percent = 100\nminimum_requirement_coverage_percent = 70\n\n[critical_properties]\n# approval-model = [\"P_APPROVAL_IS_BOOLEAN\"]\n\n[property_suites.smoke]\nentries = []\n\nbenchmark_models = []\nbenchmark_repeats = 3\ngenerated_tests_dir = \"generated-tests\"\nartifacts_dir = \"artifacts\"\nbenchmarks_dir = \"artifacts/benchmarks\"\nbenchmark_baseline_dir = \"benchmarks/baselines\"\nbenchmark_regression_threshold_percent = 25\ndefault_graph_format = \"mermaid\"\n",
        registry
    )
}

pub fn rerun_recommendations(config: &ProjectConfig, model_id: &str) -> RerunRecommendations {
    let affected_critical_properties = config
        .critical_properties
        .get(model_id)
        .cloned()
        .unwrap_or_default();
    let affected_property_suites = config
        .property_suites
        .iter()
        .filter(|(_, entries)| entries.iter().any(|entry| entry.model == model_id))
        .map(|(suite_name, _)| suite_name.clone())
        .collect();
    RerunRecommendations {
        affected_critical_properties,
        affected_property_suites,
    }
}

pub fn render_registry_source_template() -> String {
    r#"use valid::{registry::run_registry_cli, valid_actions, valid_model, valid_models, valid_state};

valid_state! {
    struct State {
        approved: bool,
    }
}

valid_actions! {
    enum Action {
        Approve => "APPROVE" [reads = ["approved"], writes = ["approved"]],
    }
}

valid_model! {
    model ApprovalModel<State, Action>;
    init [State {
        approved: false,
    }];
    transitions {
        transition Approve [tags = ["allow_path"]] when |state| state.approved == false => [State {
            approved: true,
        }];
    }
    properties {
        invariant P_APPROVAL_IS_BOOLEAN |state| state.approved == false || state.approved == true;
    }
}

fn main() {
    run_registry_cli(valid_models![
        "approval-model" => ApprovalModel,
    ]);
}
"#
    .to_string()
}

fn parse_section_header(input: &str, line: usize) -> Result<ConfigSection, String> {
    let body = &input[1..input.len() - 1];
    match body.trim() {
        "critical_properties" => Ok(ConfigSection::CriticalProperties),
        section if section.starts_with("property_suites.") => Ok(ConfigSection::PropertySuite(
            section.trim_start_matches("property_suites.").to_string(),
        )),
        other => Err(format!(
            "unsupported config section `{other}` on line {line}"
        )),
    }
}

fn assign_top_level_key(
    config: &mut ProjectConfig,
    key: &str,
    value: &str,
    line: usize,
) -> Result<bool, String> {
    match key {
        "registry" => config.registry = Some(parse_string(value, line)?),
        "default_backend" => config.default_backend = Some(parse_string(value, line)?),
        "default_property" => config.default_property = Some(parse_string(value, line)?),
        "default_solver_executable" => {
            config.default_solver_executable = Some(parse_string(value, line)?)
        }
        "default_solver_args" => config.default_solver_args = parse_string_array(value, line)?,
        "suite_models" => config.suite_models = parse_string_array(value, line)?,
        "preferred_backends" => config.preferred_backends = parse_string_array(value, line)?,
        "default_suite" => config.default_suite = Some(parse_string(value, line)?),
        "minimum_overall_coverage_percent" => {
            config.coverage_gates.minimum_overall_coverage_percent = Some(parse_u32(value, line)?)
        }
        "minimum_business_coverage_percent" => {
            config.coverage_gates.minimum_business_coverage_percent = Some(parse_u32(value, line)?)
        }
        "minimum_setup_coverage_percent" => {
            config.coverage_gates.minimum_setup_coverage_percent = Some(parse_u32(value, line)?)
        }
        "minimum_requirement_coverage_percent" => {
            config.coverage_gates.minimum_requirement_coverage_percent =
                Some(parse_u32(value, line)?)
        }
        "benchmark_models" => config.benchmark_models = parse_string_array(value, line)?,
        "benchmark_repeats" => config.benchmark_repeats = Some(parse_usize(value, line)?),
        "generated_tests_dir" => config.generated_tests_dir = Some(parse_string(value, line)?),
        "artifacts_dir" => config.artifacts_dir = Some(parse_string(value, line)?),
        "benchmarks_dir" => config.benchmarks_dir = Some(parse_string(value, line)?),
        "benchmark_baseline_dir" => {
            config.benchmark_baseline_dir = Some(parse_string(value, line)?)
        }
        "benchmark_regression_threshold_percent" => {
            config.benchmark_regression_threshold_percent = Some(parse_u32(value, line)?)
        }
        "default_graph_format" => config.default_graph_format = Some(parse_string(value, line)?),
        _ => return Ok(false),
    }
    Ok(true)
}

fn parse_string(input: &str, line: usize) -> Result<String, String> {
    let trimmed = input.trim();
    if !(trimmed.starts_with('"') && trimmed.ends_with('"')) {
        return Err(format!("expected quoted string on line {line}"));
    }
    Ok(trimmed[1..trimmed.len() - 1].to_string())
}

fn parse_string_array(input: &str, line: usize) -> Result<Vec<String>, String> {
    let trimmed = input.trim();
    if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
        return Err(format!("expected string array on line {line}"));
    }
    let body = &trimmed[1..trimmed.len() - 1];
    if body.trim().is_empty() {
        return Ok(Vec::new());
    }
    split_top_level_items(body)
        .into_iter()
        .map(|entry| parse_string(entry.trim(), line))
        .collect()
}

fn parse_property_suite_entries(
    input: &str,
    line: usize,
) -> Result<Vec<PropertySuiteEntry>, String> {
    let trimmed = input.trim();
    if !(trimmed.starts_with('[') && trimmed.ends_with(']')) {
        return Err(format!(
            "expected property suite entries array on line {line}"
        ));
    }
    let body = trimmed[1..trimmed.len() - 1].trim();
    if body.is_empty() {
        return Ok(Vec::new());
    }
    split_top_level_items(body)
        .into_iter()
        .map(|item| parse_property_suite_entry(item.trim(), line))
        .collect()
}

fn parse_property_suite_entry(input: &str, line: usize) -> Result<PropertySuiteEntry, String> {
    let trimmed = input.trim();
    if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
        return Err(format!("expected inline suite entry on line {line}"));
    }
    let body = trimmed[1..trimmed.len() - 1].trim();
    let mut model = None;
    let mut properties = None;
    for part in split_top_level_items(body) {
        let Some((key, value)) = part.split_once('=') else {
            return Err(format!("invalid suite entry on line {line}"));
        };
        match key.trim() {
            "model" => model = Some(parse_string(value.trim(), line)?),
            "properties" => properties = Some(parse_string_array(value.trim(), line)?),
            other => {
                return Err(format!(
                    "unsupported suite entry key `{other}` on line {line}"
                ));
            }
        }
    }
    Ok(PropertySuiteEntry {
        model: model.ok_or_else(|| format!("missing suite entry model on line {line}"))?,
        properties: properties
            .ok_or_else(|| format!("missing suite entry properties on line {line}"))?,
    })
}

fn split_top_level_items(input: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut start = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in input.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '{' => brace_depth += 1,
            '}' => brace_depth = brace_depth.saturating_sub(1),
            ',' if bracket_depth == 0 && brace_depth == 0 => {
                items.push(input[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    items.push(input[start..].trim());
    items
        .into_iter()
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>()
}

fn parse_usize(input: &str, line: usize) -> Result<usize, String> {
    input
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("expected positive integer on line {line}"))
}

fn parse_u32(input: &str, line: usize) -> Result<u32, String> {
    input
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("expected non-negative integer on line {line}"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{
        parse_project_config, render_project_config_template, render_registry_source_template,
        rerun_recommendations, verification_policy, CoverageGates, ProjectConfig,
        PropertySuiteEntry, RerunRecommendations, VerificationPolicy,
    };

    #[test]
    fn parses_project_config_subset() {
        let config = parse_project_config(
            r#"
registry = "examples/valid_models.rs"
default_backend = "explicit"
default_property = "P_SAFE"
default_solver_executable = "cvc5"
default_solver_args = ["--lang", "smt2"]
suite_models = ["counter", "failing-counter"]
preferred_backends = ["explicit", "smt-cvc5"]
default_suite = "smoke"
minimum_overall_coverage_percent = 90
minimum_business_coverage_percent = 80
minimum_setup_coverage_percent = 100
minimum_requirement_coverage_percent = 70

[critical_properties]
counter = ["P_SAFE", "P_STRONG"]

[property_suites.smoke]
entries = [{ model = "counter", properties = ["P_SAFE"] }]

benchmark_models = ["counter"]
benchmark_repeats = 5
generated_tests_dir = "generated-tests"
artifacts_dir = "artifacts"
benchmarks_dir = "artifacts/benchmarks"
benchmark_baseline_dir = "benchmarks/baselines"
benchmark_regression_threshold_percent = 30
default_graph_format = "mermaid"
"#,
        )
        .unwrap();
        assert_eq!(
            config,
            ProjectConfig {
                registry: Some("examples/valid_models.rs".to_string()),
                default_backend: Some("explicit".to_string()),
                default_property: Some("P_SAFE".to_string()),
                default_solver_executable: Some("cvc5".to_string()),
                default_solver_args: vec!["--lang".to_string(), "smt2".to_string()],
                suite_models: vec!["counter".to_string(), "failing-counter".to_string()],
                preferred_backends: vec!["explicit".to_string(), "smt-cvc5".to_string()],
                default_suite: Some("smoke".to_string()),
                coverage_gates: CoverageGates {
                    minimum_overall_coverage_percent: Some(90),
                    minimum_business_coverage_percent: Some(80),
                    minimum_setup_coverage_percent: Some(100),
                    minimum_requirement_coverage_percent: Some(70),
                },
                critical_properties: BTreeMap::from([(
                    "counter".to_string(),
                    vec!["P_SAFE".to_string(), "P_STRONG".to_string()],
                )]),
                property_suites: BTreeMap::from([(
                    "smoke".to_string(),
                    vec![PropertySuiteEntry {
                        model: "counter".to_string(),
                        properties: vec!["P_SAFE".to_string()],
                    }],
                )]),
                benchmark_models: vec!["counter".to_string()],
                benchmark_repeats: Some(5),
                generated_tests_dir: Some("generated-tests".to_string()),
                artifacts_dir: Some("artifacts".to_string()),
                benchmarks_dir: Some("artifacts/benchmarks".to_string()),
                benchmark_baseline_dir: Some("benchmarks/baselines".to_string()),
                benchmark_regression_threshold_percent: Some(30),
                default_graph_format: Some("mermaid".to_string()),
            }
        );
    }

    #[test]
    fn renders_project_template() {
        let body = render_project_config_template("examples/valid_models.rs");
        assert!(body.contains("registry = \"examples/valid_models.rs\""));
        assert!(body.contains("default_backend = \"explicit\""));
        assert!(body.contains("generated_tests_dir = \"generated-tests\""));
        assert!(body.contains("benchmark_repeats = 3"));
        assert!(body.contains("benchmark_baseline_dir = \"benchmarks/baselines\""));
        assert!(body.contains("preferred_backends = [\"explicit\"]"));
        assert!(body.contains("default_suite = \"smoke\""));
        assert!(body.contains("minimum_overall_coverage_percent = 80"));
        assert!(body.contains("[critical_properties]"));
        assert!(body.contains("[property_suites.smoke]"));
    }

    #[test]
    fn renders_registry_source_template() {
        let body = render_registry_source_template();
        assert!(body.contains("valid_model!"));
        assert!(body.contains("\"approval-model\""));
    }

    #[test]
    fn computes_rerun_recommendations_from_project_config() {
        let config = ProjectConfig {
            critical_properties: BTreeMap::from([(
                "counter".to_string(),
                vec!["P_SAFE".to_string()],
            )]),
            property_suites: BTreeMap::from([(
                "smoke".to_string(),
                vec![PropertySuiteEntry {
                    model: "counter".to_string(),
                    properties: vec!["P_SAFE".to_string()],
                }],
            )]),
            ..ProjectConfig::default()
        };
        assert_eq!(
            rerun_recommendations(&config, "counter"),
            RerunRecommendations {
                affected_critical_properties: vec!["P_SAFE".to_string()],
                affected_property_suites: vec!["smoke".to_string()],
            }
        );
    }

    #[test]
    fn builds_verification_policy_view() {
        let config = ProjectConfig {
            suite_models: vec!["counter".to_string()],
            critical_properties: BTreeMap::from([(
                "counter".to_string(),
                vec!["P_SAFE".to_string()],
            )]),
            property_suites: BTreeMap::from([(
                "smoke".to_string(),
                vec![PropertySuiteEntry {
                    model: "counter".to_string(),
                    properties: vec!["P_SAFE".to_string()],
                }],
            )]),
            preferred_backends: vec!["explicit".to_string(), "smt-cvc5".to_string()],
            default_suite: Some("smoke".to_string()),
            coverage_gates: CoverageGates {
                minimum_overall_coverage_percent: Some(80),
                minimum_business_coverage_percent: Some(70),
                minimum_setup_coverage_percent: Some(100),
                minimum_requirement_coverage_percent: Some(60),
            },
            ..ProjectConfig::default()
        };
        assert_eq!(
            verification_policy(&config),
            VerificationPolicy {
                suite_models: vec!["counter".to_string()],
                critical_properties: BTreeMap::from([(
                    "counter".to_string(),
                    vec!["P_SAFE".to_string()],
                )]),
                property_suites: BTreeMap::from([(
                    "smoke".to_string(),
                    vec![PropertySuiteEntry {
                        model: "counter".to_string(),
                        properties: vec!["P_SAFE".to_string()],
                    }],
                )]),
                preferred_backends: vec!["explicit".to_string(), "smt-cvc5".to_string()],
                default_suite: Some("smoke".to_string()),
                coverage_gates: CoverageGates {
                    minimum_overall_coverage_percent: Some(80),
                    minimum_business_coverage_percent: Some(70),
                    minimum_setup_coverage_percent: Some(100),
                    minimum_requirement_coverage_percent: Some(60),
                },
            }
        );
    }
}
