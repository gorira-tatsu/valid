use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

use serde_json::Value;

use crate::project::{load_project_config, ProjectConfig};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExternalTargetOptions {
    pub manifest_path: Option<String>,
    pub file: Option<String>,
    pub example: Option<String>,
    pub bin: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DiscoveredExternalProject {
    pub options: ExternalTargetOptions,
    pub config: Option<ProjectConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalTargetKind {
    Example,
    Bin,
}

impl ExternalTargetKind {
    pub fn cargo_flag(self) -> &'static str {
        match self {
            Self::Example => "--example",
            Self::Bin => "--bin",
        }
    }

    fn manifest_kind(self) -> &'static str {
        match self {
            Self::Example => "example",
            Self::Bin => "bin",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalTarget {
    pub manifest_path: Option<String>,
    pub kind: ExternalTargetKind,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RegistryBuildOptions {
    pub locked: bool,
    pub offline: bool,
    pub extra_features: Vec<String>,
}

pub fn project_root(manifest_path: Option<&str>) -> PathBuf {
    if let Some(manifest_path) = manifest_path {
        let path = PathBuf::from(manifest_path);
        return path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn discover_external_project(
    options: &ExternalTargetOptions,
) -> Result<DiscoveredExternalProject, String> {
    let mut discovered = options.clone();
    let current_dir = project_root(discovered.manifest_path.as_deref());
    let built_in_manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cargo_toml = current_dir.join("Cargo.toml");
    let config = load_project_config(&current_dir)?;
    let explicit_registry_target =
        discovered.file.is_some() || discovered.example.is_some() || discovered.bin.is_some();

    if let Some(config) = &config {
        if !explicit_registry_target {
            if let Some(registry) = config.registry.clone() {
                discovered.file = Some(current_dir.join(registry).to_string_lossy().to_string());
            }
        }
        if discovered.manifest_path.is_none() && cargo_toml.exists() {
            discovered.manifest_path = Some(cargo_toml.to_string_lossy().to_string());
        }
    }

    if discovered.file.is_none() && discovered.example.is_none() && discovered.bin.is_none() {
        if current_dir != built_in_manifest_dir && cargo_toml.exists() {
            let candidates = [
                current_dir.join("examples").join("valid_models.rs"),
                current_dir.join("src").join("bin").join("valid_models.rs"),
            ];
            if let Some(file) = candidates.into_iter().find(|path| path.exists()) {
                if discovered.manifest_path.is_none() {
                    discovered.manifest_path = Some(cargo_toml.to_string_lossy().to_string());
                }
                discovered.file = Some(file.to_string_lossy().to_string());
            }
        }
    }

    Ok(DiscoveredExternalProject {
        options: discovered,
        config,
    })
}

pub fn resolve_external_target(options: &ExternalTargetOptions) -> Result<ExternalTarget, String> {
    if let Some(file) = &options.file {
        return target_from_file(options.manifest_path.clone(), file);
    }
    if let Some(example) = &options.example {
        return Ok(ExternalTarget {
            manifest_path: options.manifest_path.clone(),
            kind: ExternalTargetKind::Example,
            name: example.clone(),
        });
    }
    if let Some(bin) = &options.bin {
        return Ok(ExternalTarget {
            manifest_path: options.manifest_path.clone(),
            kind: ExternalTargetKind::Bin,
            name: bin.clone(),
        });
    }
    Ok(ExternalTarget {
        manifest_path: options.manifest_path.clone(),
        kind: ExternalTargetKind::Example,
        name: "valid_models".to_string(),
    })
}

pub fn build_registry_binary(
    target: &ExternalTarget,
    options: &RegistryBuildOptions,
) -> Result<String, String> {
    let mut command = Command::new("cargo");
    command.arg("build");
    let mut features = default_build_features(target);
    for feature in &options.extra_features {
        if !features.iter().any(|existing| existing == feature) {
            features.push(feature.clone());
        }
    }
    if options.locked {
        command.arg("--locked");
    }
    if options.offline {
        command.arg("--offline");
    }
    if !features.is_empty() {
        command.arg("--features").arg(features.join(","));
    }
    if let Some(manifest_path) = &target.manifest_path {
        command.arg("--manifest-path").arg(manifest_path);
    }
    command
        .arg(target.kind.cargo_flag())
        .arg(&target.name)
        .arg("--message-format")
        .arg("json-render-diagnostics");

    let output = command
        .output()
        .map_err(|err| format!("failed to execute cargo build: {err}"))?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if !stderr.trim().is_empty() {
            stderr.trim().to_string()
        } else if !stdout.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            format!("cargo build exited with status {}", output.status)
        };
        return Err(format!(
            "failed to build registry target `{}`: {detail}",
            target.name
        ));
    }

    parse_executable_path(&output.stdout, target)
        .ok_or_else(|| format!("failed to locate built executable for `{}`", target.name))
}

fn default_build_features(target: &ExternalTarget) -> Vec<String> {
    let built_in_manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if project_root(target.manifest_path.as_deref()) == built_in_manifest_dir
        && matches!(
            target.kind,
            ExternalTargetKind::Example | ExternalTargetKind::Bin
        )
    {
        return vec!["verification-runtime".to_string()];
    }
    Vec::new()
}

fn parse_executable_path(stdout: &[u8], target: &ExternalTarget) -> Option<String> {
    let stdout = String::from_utf8_lossy(stdout);
    let mut executable = None;
    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("reason").and_then(Value::as_str) != Some("compiler-artifact") {
            continue;
        }
        if value
            .get("target")
            .and_then(Value::as_object)
            .and_then(|target_value| target_value.get("name"))
            .and_then(Value::as_str)
            != Some(target.name.as_str())
        {
            continue;
        }
        let kind_matches = value
            .get("target")
            .and_then(Value::as_object)
            .and_then(|target_value| target_value.get("kind"))
            .and_then(Value::as_array)
            .map(|kinds| {
                kinds
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|kind| kind == target.kind.manifest_kind())
            })
            .unwrap_or(false);
        if !kind_matches {
            continue;
        }
        if let Some(path) = value.get("executable").and_then(Value::as_str) {
            executable = Some(path.to_string());
        }
    }
    executable
}

fn target_from_file(manifest_path: Option<String>, file: &str) -> Result<ExternalTarget, String> {
    let path = Path::new(file);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| "expected a Rust source file path for --file".to_string())?;
    let normalized = file.replace('\\', "/");
    let kind = if normalized.ends_with(&format!("/examples/{stem}.rs"))
        || normalized == format!("examples/{stem}.rs")
        || normalized.ends_with(&format!("/benchmarks/registries/{stem}.rs"))
        || normalized == format!("benchmarks/registries/{stem}.rs")
    {
        ExternalTargetKind::Example
    } else if normalized.ends_with(&format!("/src/bin/{stem}.rs"))
        || normalized == format!("src/bin/{stem}.rs")
    {
        ExternalTargetKind::Bin
    } else {
        return Err(
            "`--file` currently supports files under `examples/`, `benchmarks/registries/`, or `src/bin/`"
                .to_string(),
        );
    };
    Ok(ExternalTarget {
        manifest_path,
        kind,
        name: stem.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        discover_external_project, project_root, resolve_external_target, ExternalTargetKind,
        ExternalTargetOptions,
    };
    use std::path::PathBuf;

    #[test]
    fn resolves_example_target_from_file() {
        let target = resolve_external_target(&ExternalTargetOptions {
            manifest_path: Some("Cargo.toml".to_string()),
            file: Some("examples/valid_models.rs".to_string()),
            ..ExternalTargetOptions::default()
        })
        .unwrap();
        assert_eq!(target.kind, ExternalTargetKind::Example);
        assert_eq!(target.name, "valid_models");
    }

    #[test]
    fn resolves_bin_target_from_file() {
        let target = resolve_external_target(&ExternalTargetOptions {
            file: Some("src/bin/custom_registry.rs".to_string()),
            ..ExternalTargetOptions::default()
        })
        .unwrap();
        assert_eq!(target.kind, ExternalTargetKind::Bin);
        assert_eq!(target.name, "custom_registry");
    }

    #[test]
    fn project_root_uses_manifest_parent() {
        let root = project_root(Some("/tmp/example/Cargo.toml"));
        assert_eq!(root, PathBuf::from("/tmp/example"));
    }

    #[test]
    fn discovers_in_repo_valid_toml_target() {
        let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let discovered = discover_external_project(&ExternalTargetOptions {
            manifest_path: Some(repo_root.join("Cargo.toml").to_string_lossy().to_string()),
            ..ExternalTargetOptions::default()
        })
        .unwrap();
        assert!(discovered.options.manifest_path.is_some());
        assert!(discovered.options.file.is_some());
        assert!(discovered.config.is_some());
    }
}
