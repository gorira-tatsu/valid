use std::{collections::BTreeMap, env, fs, path::Path};

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
    pub affected_artifacts: Vec<String>,
    pub repair_surfaces: Vec<String>,
    pub suggested_reruns: Vec<RerunSuggestion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RerunSuggestion {
    pub action: String,
    pub target: String,
    pub reason: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InitScaffoldResult {
    pub status: String,
    pub root: String,
    pub cargo_init_ran: bool,
    pub created: String,
    pub registry: String,
    pub scaffolded_registry: String,
    pub generated_tests_dir: String,
    pub artifacts_dir: String,
    pub benchmarks_baseline_dir: String,
    pub created_files: Vec<String>,
    pub created_directories: Vec<String>,
    pub skipped_existing: Vec<String>,
    pub model_files: Vec<String>,
    pub mcp_configs: Vec<String>,
    pub ai_bootstrap_guide: String,
    pub rdd_guide: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InitCheckResult {
    pub status: String,
    pub root: String,
    pub cargo_project_detected: bool,
    pub valid_toml_detected: bool,
    pub registry: Option<String>,
    pub checked_paths: Vec<String>,
    pub missing_paths: Vec<String>,
    pub mismatched_paths: Vec<String>,
    pub recommended_repairs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InitRepairResult {
    pub status: String,
    pub root: String,
    pub repaired_files: Vec<String>,
    pub repaired_directories: Vec<String>,
    pub skipped_existing: Vec<String>,
    pub remaining_warnings: Vec<String>,
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

pub fn scaffold_project_init(
    root: &Path,
    registry: &str,
    cargo_init_ran: bool,
) -> Result<InitScaffoldResult, String> {
    let config_path = root.join("valid.toml");
    if config_path.exists() {
        return Err(format!("`{}` already exists", config_path.display()));
    }

    let generated_dir = root.join("generated-tests");
    let artifacts_dir = root.join("artifacts");
    let benchmark_baseline_dir = root.join("benchmarks").join("baselines");
    let mcp_dir = root.join(".mcp");
    let docs_ai_dir = root.join("docs").join("ai");
    let docs_rdd_dir = root.join("docs").join("rdd");
    let valid_dir = root.join("valid");
    let valid_models_dir = valid_dir.join("models");
    let registry_path = root.join(registry);
    let model_mod_path = valid_models_dir.join("mod.rs");
    let model_path = valid_models_dir.join("approval.rs");
    let main_path = root.join("src").join("main.rs");
    let codex_config = mcp_dir.join("codex.toml");
    let claude_code_config = mcp_dir.join("claude-code.json");
    let claude_desktop_config = mcp_dir.join("claude-desktop.json");
    let bootstrap_readme = docs_ai_dir.join("bootstrap.md");
    let rdd_readme = docs_rdd_dir.join("README.md");

    let mut created_files = Vec::new();
    let mut created_directories = Vec::new();
    let mut skipped_existing = Vec::new();

    create_dir_if_missing(&generated_dir, &mut created_directories)?;
    create_dir_if_missing(&artifacts_dir, &mut created_directories)?;
    create_dir_if_missing(&benchmark_baseline_dir, &mut created_directories)?;
    create_dir_if_missing(&mcp_dir, &mut created_directories)?;
    create_dir_if_missing(&docs_ai_dir, &mut created_directories)?;
    create_dir_if_missing(&docs_rdd_dir, &mut created_directories)?;
    create_dir_if_missing(&valid_dir, &mut created_directories)?;
    create_dir_if_missing(&valid_models_dir, &mut created_directories)?;
    create_dir_if_missing(&root.join("src"), &mut created_directories)?;
    ensure_valid_dependency(&root.join("Cargo.toml"), &mut created_files)?;

    write_file_if_missing(
        &config_path,
        &render_project_config_template(registry),
        &mut created_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &registry_path,
        &render_registry_source_template(),
        &mut created_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &model_mod_path,
        &render_registry_models_mod_template(),
        &mut created_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &model_path,
        &render_registry_model_template(),
        &mut created_files,
        &mut skipped_existing,
    )?;
    write_main_file(
        &main_path,
        cargo_init_ran,
        &mut created_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &generated_dir.join(".gitkeep"),
        "",
        &mut created_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &artifacts_dir.join(".gitkeep"),
        "",
        &mut created_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &benchmark_baseline_dir.join(".gitkeep"),
        "",
        &mut created_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &codex_config,
        &render_bootstrap_codex_config(),
        &mut created_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &claude_code_config,
        &render_bootstrap_claude_code_config(),
        &mut created_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &claude_desktop_config,
        &render_bootstrap_claude_desktop_config(),
        &mut created_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &bootstrap_readme,
        &render_bootstrap_ai_readme(),
        &mut created_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &rdd_readme,
        &render_rdd_readme(),
        &mut created_files,
        &mut skipped_existing,
    )?;

    Ok(InitScaffoldResult {
        status: "ok".to_string(),
        root: root.display().to_string(),
        cargo_init_ran,
        created: config_path.display().to_string(),
        registry: registry.to_string(),
        scaffolded_registry: registry_path.display().to_string(),
        generated_tests_dir: generated_dir.display().to_string(),
        artifacts_dir: artifacts_dir.display().to_string(),
        benchmarks_baseline_dir: benchmark_baseline_dir.display().to_string(),
        created_files,
        created_directories,
        skipped_existing,
        model_files: vec![
            model_mod_path.display().to_string(),
            model_path.display().to_string(),
        ],
        mcp_configs: vec![
            codex_config.display().to_string(),
            claude_code_config.display().to_string(),
            claude_desktop_config.display().to_string(),
        ],
        ai_bootstrap_guide: bootstrap_readme.display().to_string(),
        rdd_guide: rdd_readme.display().to_string(),
    })
}

pub fn check_project_init(root: &Path, expected_registry: &str) -> InitCheckResult {
    let cargo_toml = root.join("Cargo.toml");
    let valid_toml = root.join("valid.toml");
    let expected_paths = [
        root.join(expected_registry),
        root.join("valid").join("models").join("mod.rs"),
        root.join("valid").join("models").join("approval.rs"),
        root.join(".mcp").join("codex.toml"),
        root.join(".mcp").join("claude-code.json"),
        root.join(".mcp").join("claude-desktop.json"),
        root.join("docs").join("ai").join("bootstrap.md"),
        root.join("docs").join("rdd").join("README.md"),
        root.join("generated-tests"),
        root.join("artifacts"),
        root.join("benchmarks").join("baselines"),
    ];

    let mut checked_paths = vec![
        cargo_toml.display().to_string(),
        valid_toml.display().to_string(),
    ];
    checked_paths.extend(expected_paths.iter().map(|path| path.display().to_string()));

    let cargo_project_detected = cargo_toml.exists();
    let valid_toml_detected = valid_toml.exists();
    let mut missing_paths = Vec::new();
    let mut mismatched_paths = Vec::new();
    let mut recommended_repairs = Vec::new();
    let mut registry = None;

    if !cargo_project_detected {
        missing_paths.push(cargo_toml.display().to_string());
        recommended_repairs
            .push("Run `valid init` from the project root to create Cargo.toml.".to_string());
    }
    if !valid_toml_detected {
        missing_paths.push(valid_toml.display().to_string());
        recommended_repairs.push(
            "Run `valid init` to scaffold valid.toml and the standard valid layout.".to_string(),
        );
    } else {
        match load_project_config(root) {
            Ok(Some(config)) => {
                registry = config.registry.clone();
                match config.registry.as_deref() {
                    Some(path) if path == expected_registry => {}
                    Some(path) => {
                        mismatched_paths.push(format!(
                            "valid.toml registry = {:?}; expected {:?}",
                            path, expected_registry
                        ));
                        recommended_repairs.push(format!(
                            "Update `valid.toml` so `registry = {:?}` for the scaffolded layout.",
                            expected_registry
                        ));
                    }
                    None => {
                        mismatched_paths.push("valid.toml does not set `registry`".to_string());
                        recommended_repairs.push(format!(
                            "Set `registry = {:?}` in valid.toml.",
                            expected_registry
                        ));
                    }
                }
            }
            Ok(None) => {}
            Err(error) => {
                mismatched_paths.push(format!("valid.toml could not be parsed: {error}"));
                recommended_repairs
                    .push("Repair valid.toml so the scaffold can be read again.".to_string());
            }
        }
    }

    for path in expected_paths {
        if !path.exists() {
            missing_paths.push(path.display().to_string());
        }
    }
    if missing_paths
        .iter()
        .any(|path| path.ends_with("valid/registry.rs"))
    {
        recommended_repairs.push(
            "Restore `valid/registry.rs` or rerun `valid init` in a fresh directory.".to_string(),
        );
    }
    if missing_paths
        .iter()
        .any(|path| path.ends_with("docs/ai/bootstrap.md"))
    {
        recommended_repairs.push(
            "Restore `docs/ai/bootstrap.md` to keep the onboarding guide available.".to_string(),
        );
    }
    if missing_paths.iter().any(|path| path.contains(".mcp/")) {
        recommended_repairs.push("Restore the `.mcp/` snippets so local AI clients can attach with `valid mcp --project .`.".to_string());
    }

    let status = if !mismatched_paths.is_empty() {
        "error"
    } else if !missing_paths.is_empty() {
        "warn"
    } else {
        "ok"
    };

    InitCheckResult {
        status: status.to_string(),
        root: root.display().to_string(),
        cargo_project_detected,
        valid_toml_detected,
        registry,
        checked_paths,
        missing_paths,
        mismatched_paths,
        recommended_repairs,
    }
}

pub fn repair_project_init(
    root: &Path,
    expected_registry: &str,
) -> Result<InitRepairResult, String> {
    let cargo_toml = root.join("Cargo.toml");
    if !cargo_toml.exists() {
        return Err(format!(
            "`{}` is missing; run `valid init` before `valid init --repair`",
            cargo_toml.display()
        ));
    }
    let config_path = root.join("valid.toml");
    if !config_path.exists() {
        return Err(format!(
            "`{}` is missing; run `valid init` before `valid init --repair`",
            config_path.display()
        ));
    }

    let generated_dir = root.join("generated-tests");
    let artifacts_dir = root.join("artifacts");
    let benchmark_baseline_dir = root.join("benchmarks").join("baselines");
    let mcp_dir = root.join(".mcp");
    let docs_ai_dir = root.join("docs").join("ai");
    let docs_rdd_dir = root.join("docs").join("rdd");
    let valid_dir = root.join("valid");
    let valid_models_dir = valid_dir.join("models");
    let registry_path = root.join(expected_registry);
    let model_mod_path = valid_models_dir.join("mod.rs");
    let model_path = valid_models_dir.join("approval.rs");
    let codex_config = mcp_dir.join("codex.toml");
    let claude_code_config = mcp_dir.join("claude-code.json");
    let claude_desktop_config = mcp_dir.join("claude-desktop.json");
    let bootstrap_readme = docs_ai_dir.join("bootstrap.md");
    let rdd_readme = docs_rdd_dir.join("README.md");

    let mut repaired_files = Vec::new();
    let mut repaired_directories = Vec::new();
    let mut skipped_existing = Vec::new();

    create_dir_if_missing(&generated_dir, &mut repaired_directories)?;
    create_dir_if_missing(&artifacts_dir, &mut repaired_directories)?;
    create_dir_if_missing(&benchmark_baseline_dir, &mut repaired_directories)?;
    create_dir_if_missing(&mcp_dir, &mut repaired_directories)?;
    create_dir_if_missing(&docs_ai_dir, &mut repaired_directories)?;
    create_dir_if_missing(&docs_rdd_dir, &mut repaired_directories)?;
    create_dir_if_missing(&valid_dir, &mut repaired_directories)?;
    create_dir_if_missing(&valid_models_dir, &mut repaired_directories)?;

    write_file_if_missing(
        &registry_path,
        &render_registry_source_template(),
        &mut repaired_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &model_mod_path,
        &render_registry_models_mod_template(),
        &mut repaired_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &model_path,
        &render_registry_model_template(),
        &mut repaired_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &generated_dir.join(".gitkeep"),
        "",
        &mut repaired_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &artifacts_dir.join(".gitkeep"),
        "",
        &mut repaired_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &benchmark_baseline_dir.join(".gitkeep"),
        "",
        &mut repaired_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &codex_config,
        &render_bootstrap_codex_config(),
        &mut repaired_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &claude_code_config,
        &render_bootstrap_claude_code_config(),
        &mut repaired_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &claude_desktop_config,
        &render_bootstrap_claude_desktop_config(),
        &mut repaired_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &bootstrap_readme,
        &render_bootstrap_ai_readme(),
        &mut repaired_files,
        &mut skipped_existing,
    )?;
    write_file_if_missing(
        &rdd_readme,
        &render_rdd_readme(),
        &mut repaired_files,
        &mut skipped_existing,
    )?;

    let report = check_project_init(root, expected_registry);
    let status = if report.status == "error" {
        "warn"
    } else {
        report.status.as_str()
    };
    let mut remaining_warnings = report.missing_paths;
    remaining_warnings.extend(report.mismatched_paths);
    if report.status != "ok" {
        remaining_warnings.extend(report.recommended_repairs);
    }

    Ok(InitRepairResult {
        status: status.to_string(),
        root: root.display().to_string(),
        repaired_files,
        repaired_directories,
        skipped_existing,
        remaining_warnings,
    })
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
        .collect::<Vec<_>>();
    let doc_path = format!(
        "{}/docs/{}.md",
        config
            .artifacts_dir
            .as_deref()
            .unwrap_or("artifacts")
            .trim_end_matches('/'),
        sanitize_model_id(model_id)
    );
    let generated_tests_dir = config
        .generated_tests_dir
        .as_deref()
        .unwrap_or("generated-tests")
        .trim_end_matches('/')
        .to_string();
    let mut affected_artifacts = vec![doc_path.clone(), format!("{generated_tests_dir}/")];
    let mut repair_surfaces = vec![
        "contract_lock".to_string(),
        "generated_doc".to_string(),
        "generated_tests".to_string(),
    ];
    let mut suggested_reruns = vec![
        RerunSuggestion {
            action: "refresh_contract_lock".to_string(),
            target: model_id.to_string(),
            reason: "contract drift updates the canonical lock entry".to_string(),
        },
        RerunSuggestion {
            action: "regenerate_doc".to_string(),
            target: doc_path,
            reason: "generated documentation embeds the contract hash".to_string(),
        },
        RerunSuggestion {
            action: "regenerate_tests".to_string(),
            target: generated_tests_dir,
            reason: "generated tests may rely on stale traces or contract structure".to_string(),
        },
    ];
    if !affected_critical_properties.is_empty() {
        repair_surfaces.push("critical_properties".to_string());
        affected_artifacts.push("valid.toml#critical_properties".to_string());
        for property_id in &affected_critical_properties {
            suggested_reruns.push(RerunSuggestion {
                action: "rerun_critical_property".to_string(),
                target: property_id.clone(),
                reason: format!("critical property `{property_id}` maps to `{model_id}`"),
            });
        }
    }
    if !affected_property_suites.is_empty() {
        repair_surfaces.push("property_suites".to_string());
        affected_artifacts.push("valid.toml#property_suites".to_string());
        for suite_name in &affected_property_suites {
            suggested_reruns.push(RerunSuggestion {
                action: "rerun_property_suite".to_string(),
                target: suite_name.clone(),
                reason: format!("property suite `{suite_name}` includes `{model_id}`"),
            });
        }
    }
    RerunRecommendations {
        affected_critical_properties,
        affected_property_suites,
        affected_artifacts,
        repair_surfaces,
        suggested_reruns,
    }
}

pub fn doc_repair_surfaces(drift_sections: &[String]) -> Vec<String> {
    let mut repair_surfaces = vec!["generated_doc".to_string()];
    if drift_sections
        .iter()
        .any(|section| matches!(section.as_str(), "source_hash" | "contract_hash"))
    {
        repair_surfaces.push("contract_metadata".to_string());
    }
    if drift_sections.iter().any(|section| section == "mermaid") {
        repair_surfaces.push("graph_rendering".to_string());
    }
    repair_surfaces
}

pub fn doc_suggested_reruns(output_path: &str, drift_sections: &[String]) -> Vec<RerunSuggestion> {
    let mut suggested_reruns = vec![RerunSuggestion {
        action: "regenerate_doc".to_string(),
        target: output_path.to_string(),
        reason: "doc drift is repaired by regenerating the derived markdown".to_string(),
    }];
    if drift_sections
        .iter()
        .any(|section| matches!(section.as_str(), "source_hash" | "contract_hash"))
    {
        suggested_reruns.push(RerunSuggestion {
            action: "review_contract_inputs".to_string(),
            target: output_path.to_string(),
            reason: "doc metadata drift usually means the contract or source changed".to_string(),
        });
    }
    if drift_sections.iter().any(|section| section == "mermaid") {
        suggested_reruns.push(RerunSuggestion {
            action: "review_graph_rendering".to_string(),
            target: output_path.to_string(),
            reason: "diagram drift usually comes from graph rendering changes".to_string(),
        });
    }
    suggested_reruns
}

fn sanitize_model_id(model_id: &str) -> String {
    model_id
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '-',
        })
        .collect()
}

pub fn render_registry_source_template() -> String {
    r#"use valid::{registry::run_registry_cli, valid_models};

include!("models/approval.rs");

pub fn run() {
    run_registry_cli(valid_models![
        "approval-model" => ApprovalModel,
    ]);
}
"#
    .to_string()
}

pub fn render_registry_models_mod_template() -> String {
    "// Add model modules in this directory and include them from ../registry.rs.\n".to_string()
}

pub fn render_registry_model_template() -> String {
    r#"use valid::{valid_actions, valid_model, valid_state};

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
"#
    .to_string()
}

pub fn render_registry_main_template() -> String {
    r#"#[path = "../valid/registry.rs"]
mod valid_registry;

fn main() {
    valid_registry::run();
}
"#
    .to_string()
}

pub fn render_bootstrap_codex_config() -> String {
    r#"[mcp_servers.valid_registry]
command = "valid"
args = ["mcp", "--project", "."]
"#
    .to_string()
}

pub fn render_bootstrap_claude_code_config() -> String {
    r#"{
  "mcpServers": {
    "valid-registry": {
      "command": "valid",
      "args": [
        "mcp",
        "--project",
        "."
      ]
    }
  }
}
"#
    .to_string()
}

pub fn render_bootstrap_claude_desktop_config() -> String {
    r#"{
  "mcpServers": {
    "valid-registry": {
      "command": "valid",
      "args": [
        "mcp",
        "--project",
        "."
      ],
      "env": {}
    }
  }
}
"#
    .to_string()
}

pub fn render_bootstrap_ai_readme() -> String {
    r#"# AI Bootstrap

This project was initialized with `valid init`.

For the shortest human-friendly walkthrough, run:

1. `valid onboarding`

Recommended first steps:

1. Run `valid init --check` to confirm the scaffold is still healthy.
2. Run `cargo valid models` to confirm the registry loads.
3. Run `cargo valid inspect approval-model`.
4. Run `cargo valid handoff approval-model`.
5. If onboarding or project checks fail, run `valid doctor`.
6. If `valid doctor` reports safe missing scaffold files, run `valid init --repair`.
7. Review `valid.toml` and set:
   - `suite_models`
   - `critical_properties`
   - `property_suites`
8. Copy one of the MCP snippets from `.mcp/` into your client config.

Generated bootstrap files:

- `.mcp/codex.toml`
- `.mcp/claude-code.json`
- `.mcp/claude-desktop.json`

These snippets keep the project-first path canonical by using:

```text
valid mcp --project .
```

That avoids hard-coded local build paths and lets the MCP server discover
`valid.toml` and the registry target from the current project.

Where files go:

- add new models under `valid/models/`
- keep the registry entrypoint in `valid/registry.rs`
- generated test specs go under `generated-tests/`
- run history and reports go under `artifacts/`
- RDD notes stay in `docs/rdd/`

When the scaffold stops being enough:

- keep the project-first layout
- split real workflows into more model files under `valid/models/`
- update `valid/registry.rs` to export only the models you want to review together
"#
    .to_string()
}

pub fn render_rdd_readme() -> String {
    r#"# RDD

Use this directory for requirement, domain, and verification notes that should
stay close to the `valid` models in this project.

Recommended first files:

- `00_scope.md`
- `01_requirements.md`
- `02_properties.md`
- `03_open_questions.md`
"#
    .to_string()
}

fn create_dir_if_missing(path: &Path, created_directories: &mut Vec<String>) -> Result<(), String> {
    if path.exists() {
        return Ok(());
    }
    fs::create_dir_all(path)
        .map_err(|err| format!("failed to create `{}`: {err}", path.display()))?;
    created_directories.push(path.display().to_string());
    Ok(())
}

fn write_file_if_missing(
    path: &Path,
    contents: &str,
    created_files: &mut Vec<String>,
    skipped_existing: &mut Vec<String>,
) -> Result<(), String> {
    if path.exists() {
        skipped_existing.push(path.display().to_string());
        return Ok(());
    }
    fs::write(path, contents)
        .map_err(|err| format!("failed to write `{}`: {err}", path.display()))?;
    created_files.push(path.display().to_string());
    Ok(())
}

fn write_main_file(
    path: &Path,
    cargo_init_ran: bool,
    created_files: &mut Vec<String>,
    skipped_existing: &mut Vec<String>,
) -> Result<(), String> {
    let desired = render_registry_main_template();
    if !path.exists() {
        fs::write(path, desired)
            .map_err(|err| format!("failed to write `{}`: {err}", path.display()))?;
        created_files.push(path.display().to_string());
        return Ok(());
    }
    if cargo_init_ran {
        let existing = fs::read_to_string(path)
            .map_err(|err| format!("failed to read `{}`: {err}", path.display()))?;
        let trimmed = existing.trim();
        if trimmed == "fn main() {\n    println!(\"Hello, world!\");\n}"
            || trimmed == "fn main(){println!(\"Hello, world!\");}"
        {
            fs::write(path, desired)
                .map_err(|err| format!("failed to write `{}`: {err}", path.display()))?;
            created_files.push(path.display().to_string());
            return Ok(());
        }
    }
    skipped_existing.push(path.display().to_string());
    Ok(())
}

fn ensure_valid_dependency(
    cargo_toml: &Path,
    created_files: &mut Vec<String>,
) -> Result<(), String> {
    let body = fs::read_to_string(cargo_toml)
        .map_err(|err| format!("failed to read `{}`: {err}", cargo_toml.display()))?;
    if body
        .lines()
        .any(|line| line.trim_start().starts_with("valid ="))
    {
        return Ok(());
    }
    let dependency = if let Ok(local_path) = env::var("VALID_LOCAL_DEP_PATH") {
        format!(
            "valid = {{ path = {:?}, features = [\"verification-runtime\"] }}\n",
            local_path
        )
    } else {
        "valid = { git = \"https://github.com/gorira-tatsu/valid\", branch = \"main\" }\n"
            .to_string()
    };
    let next = if body.contains("[dependencies]") {
        body.replacen(
            "[dependencies]\n",
            &format!("[dependencies]\n{dependency}"),
            1,
        )
    } else if body.ends_with('\n') {
        format!("{body}\n[dependencies]\n{dependency}")
    } else {
        format!("{body}\n\n[dependencies]\n{dependency}")
    };
    fs::write(cargo_toml, next)
        .map_err(|err| format!("failed to write `{}`: {err}", cargo_toml.display()))?;
    created_files.push(cargo_toml.display().to_string());
    Ok(())
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
    use std::{collections::BTreeMap, fs};

    use super::{
        check_project_init, parse_project_config, render_bootstrap_ai_readme,
        render_bootstrap_claude_code_config, render_bootstrap_claude_desktop_config,
        render_bootstrap_codex_config, render_project_config_template,
        render_registry_source_template, rerun_recommendations, verification_policy, CoverageGates,
        ProjectConfig, PropertySuiteEntry, RerunRecommendations, RerunSuggestion,
        VerificationPolicy,
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
        assert!(body.contains("pub fn run()"));
        assert!(body.contains("include!(\"models/approval.rs\")"));
        assert!(body.contains("\"approval-model\""));
    }

    #[test]
    fn renders_project_bootstrap_templates() {
        let codex = render_bootstrap_codex_config();
        let claude_code = render_bootstrap_claude_code_config();
        let claude_desktop = render_bootstrap_claude_desktop_config();
        let guide = render_bootstrap_ai_readme();
        assert!(codex.contains("command = \"valid\""));
        assert!(codex.contains("\"mcp\""));
        assert!(codex.contains("--project"));
        assert!(claude_code.contains("\"valid-registry\""));
        assert!(claude_desktop.contains("\"env\": {}"));
        assert!(guide.contains("valid init"));
        assert!(guide.contains(".mcp/codex.toml"));
        assert!(guide.contains("critical_properties"));
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
                affected_artifacts: vec![
                    "artifacts/docs/counter.md".to_string(),
                    "generated-tests/".to_string(),
                    "valid.toml#critical_properties".to_string(),
                    "valid.toml#property_suites".to_string(),
                ],
                repair_surfaces: vec![
                    "contract_lock".to_string(),
                    "generated_doc".to_string(),
                    "generated_tests".to_string(),
                    "critical_properties".to_string(),
                    "property_suites".to_string(),
                ],
                suggested_reruns: vec![
                    RerunSuggestion {
                        action: "refresh_contract_lock".to_string(),
                        target: "counter".to_string(),
                        reason: "contract drift updates the canonical lock entry".to_string(),
                    },
                    RerunSuggestion {
                        action: "regenerate_doc".to_string(),
                        target: "artifacts/docs/counter.md".to_string(),
                        reason: "generated documentation embeds the contract hash".to_string(),
                    },
                    RerunSuggestion {
                        action: "regenerate_tests".to_string(),
                        target: "generated-tests".to_string(),
                        reason: "generated tests may rely on stale traces or contract structure"
                            .to_string(),
                    },
                    RerunSuggestion {
                        action: "rerun_critical_property".to_string(),
                        target: "P_SAFE".to_string(),
                        reason: "critical property `P_SAFE` maps to `counter`".to_string(),
                    },
                    RerunSuggestion {
                        action: "rerun_property_suite".to_string(),
                        target: "smoke".to_string(),
                        reason: "property suite `smoke` includes `counter`".to_string(),
                    },
                ],
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

    #[test]
    fn bootstrap_ai_readme_mentions_first_run_sequence() {
        let guide = render_bootstrap_ai_readme();
        assert!(guide.contains("cargo valid models"));
        assert!(guide.contains("cargo valid inspect approval-model"));
        assert!(guide.contains("cargo valid handoff approval-model"));
        assert!(guide.contains("generated-tests/"));
        assert!(guide.contains("artifacts/"));
    }

    #[test]
    fn init_check_reports_missing_registry_as_warning() {
        let root = std::env::temp_dir().join(format!("valid-init-check-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("temp dir");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("cargo toml");
        fs::write(
            root.join("valid.toml"),
            "registry = \"valid/registry.rs\"\n",
        )
        .expect("valid toml");
        let report = check_project_init(&root, "valid/registry.rs");
        assert_eq!(report.status, "warn");
        assert!(report
            .missing_paths
            .iter()
            .any(|path| path.ends_with("valid/registry.rs")));
        let _ = fs::remove_dir_all(&root);
    }
}
