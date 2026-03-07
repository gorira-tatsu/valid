use std::{fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectConfig {
    pub registry: Option<String>,
    pub default_backend: Option<String>,
    pub suite_models: Vec<String>,
    pub generated_tests_dir: Option<String>,
    pub artifacts_dir: Option<String>,
    pub default_graph_format: Option<String>,
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
    for (index, raw_line) in body.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!(
                "invalid config line {}: expected `key = value`",
                index + 1
            ));
        };
        match key.trim() {
            "registry" => config.registry = Some(parse_string(value.trim(), index + 1)?),
            "default_backend" => {
                config.default_backend = Some(parse_string(value.trim(), index + 1)?)
            }
            "suite_models" => config.suite_models = parse_string_array(value.trim(), index + 1)?,
            "generated_tests_dir" => {
                config.generated_tests_dir = Some(parse_string(value.trim(), index + 1)?)
            }
            "artifacts_dir" => config.artifacts_dir = Some(parse_string(value.trim(), index + 1)?),
            "default_graph_format" => {
                config.default_graph_format = Some(parse_string(value.trim(), index + 1)?)
            }
            other => {
                return Err(format!(
                    "unsupported config key `{other}` on line {}",
                    index + 1
                ));
            }
        }
    }
    Ok(config)
}

pub fn render_project_config_template(registry: &str) -> String {
    format!(
        "registry = {:?}\ndefault_backend = \"explicit\"\nsuite_models = []\ngenerated_tests_dir = \"tests/generated\"\nartifacts_dir = \"artifacts\"\ndefault_graph_format = \"mermaid\"\n",
        registry
    )
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
    body.split(',')
        .map(|entry| parse_string(entry.trim(), line))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        parse_project_config, render_project_config_template, render_registry_source_template,
        ProjectConfig,
    };

    #[test]
    fn parses_project_config_subset() {
        let config = parse_project_config(
            r#"
registry = "examples/valid_models.rs"
default_backend = "explicit"
suite_models = ["counter", "failing-counter"]
generated_tests_dir = "tests/generated"
artifacts_dir = "artifacts"
default_graph_format = "mermaid"
"#,
        )
        .unwrap();
        assert_eq!(
            config,
            ProjectConfig {
                registry: Some("examples/valid_models.rs".to_string()),
                default_backend: Some("explicit".to_string()),
                suite_models: vec!["counter".to_string(), "failing-counter".to_string()],
                generated_tests_dir: Some("tests/generated".to_string()),
                artifacts_dir: Some("artifacts".to_string()),
                default_graph_format: Some("mermaid".to_string()),
            }
        );
    }

    #[test]
    fn renders_project_template() {
        let body = render_project_config_template("examples/valid_models.rs");
        assert!(body.contains("registry = \"examples/valid_models.rs\""));
        assert!(body.contains("default_backend = \"explicit\""));
        assert!(body.contains("generated_tests_dir = \"tests/generated\""));
    }

    #[test]
    fn renders_registry_source_template() {
        let body = render_registry_source_template();
        assert!(body.contains("valid_model!"));
        assert!(body.contains("\"approval-model\""));
    }
}
