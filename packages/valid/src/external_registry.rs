use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Output},
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RegistryCommandRequest {
    pub command: String,
    pub model: Option<String>,
    pub extra_positionals: Vec<String>,
    pub seed: Option<u64>,
    pub repeat: Option<usize>,
    pub baseline_mode: Option<String>,
    pub threshold_percent: Option<u32>,
    pub strategy: Option<String>,
    pub format: Option<String>,
    pub view: Option<String>,
    pub property_id: Option<String>,
    pub backend: Option<String>,
    pub solver_executable: Option<String>,
    pub solver_args: Vec<String>,
    pub actions: Vec<String>,
    pub focus_action_id: Option<String>,
    pub json: bool,
    pub progress_json: bool,
    pub write_path: Option<String>,
    pub check: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RegistryCommandEnvironment {
    pub manifest_path: Option<String>,
    pub file: Option<String>,
    pub model_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PreparedRegistryCommand {
    pub target: ExternalTarget,
    pub command: RegistryCommandRequest,
    pub environment: RegistryCommandEnvironment,
    pub build_options: RegistryBuildOptions,
    pub project_config: Option<ProjectConfig>,
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

pub fn prepare_registry_command(
    target_options: &ExternalTargetOptions,
    mut request: RegistryCommandRequest,
) -> Result<PreparedRegistryCommand, String> {
    let discovered = discover_external_project(target_options)?;
    if let Some(config) = &discovered.config {
        if request.backend.is_none() {
            request.backend = config
                .default_backend
                .clone()
                .filter(|value| !value.trim().is_empty());
        }
        if request.property_id.is_none() {
            request.property_id = config
                .default_property
                .clone()
                .filter(|value| !value.trim().is_empty());
        }
        if request.solver_executable.is_none() {
            request.solver_executable = config
                .default_solver_executable
                .clone()
                .filter(|value| !value.trim().is_empty());
        }
        if request.solver_args.is_empty() && !config.default_solver_args.is_empty() {
            request.solver_args = config.default_solver_args.clone();
        }
    }

    let mut build_options = RegistryBuildOptions::default();
    if matches!(request.backend.as_deref(), Some("sat-varisat")) {
        build_options
            .extra_features
            .push("varisat-backend".to_string());
    }

    let target = resolve_external_target(&discovered.options)?;
    let environment = RegistryCommandEnvironment {
        manifest_path: discovered.options.manifest_path.clone(),
        file: discovered.options.file.clone(),
        model_name: request.model.clone(),
    };

    Ok(PreparedRegistryCommand {
        target,
        command: request,
        environment,
        build_options,
        project_config: discovered.config,
    })
}

pub fn run_prepared_registry_command(prepared: &PreparedRegistryCommand) -> Result<Output, String> {
    let registry_binary = build_registry_binary(&prepared.target, &prepared.build_options)?;
    execute_registry_binary(
        &registry_binary,
        &prepared.target,
        &prepared.command,
        &prepared.environment,
    )
}

pub fn run_registry_command(
    target_options: &ExternalTargetOptions,
    request: RegistryCommandRequest,
) -> Result<Output, String> {
    let prepared = prepare_registry_command(target_options, request)?;
    run_prepared_registry_command(&prepared)
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

fn execute_registry_binary(
    registry_binary: &str,
    target: &ExternalTarget,
    request: &RegistryCommandRequest,
    environment: &RegistryCommandEnvironment,
) -> Result<Output, String> {
    let mut command = Command::new(registry_binary);
    command.current_dir(project_root(target.manifest_path.as_deref()));
    if let Some(manifest_path) = &environment.manifest_path {
        command.env("VALID_REGISTRY_MANIFEST_PATH", manifest_path);
    }
    if let Some(file) = &environment.file {
        command.env("VALID_REGISTRY_FILE", file);
    }
    if let Some(model_name) = &environment.model_name {
        command.env("VALID_REGISTRY_MODEL_NAME", model_name);
    }
    for arg in registry_command_args(request) {
        command.arg(arg);
    }
    command
        .output()
        .map_err(|err| format!("failed to execute target registry `{registry_binary}`: {err}"))
}

fn registry_command_args(request: &RegistryCommandRequest) -> Vec<String> {
    let command_name = normalize_registry_command(&request.command);
    let mut args = vec![command_name.clone()];
    if let Some(model) = &request.model {
        args.push(model.clone());
    }
    args.extend(request.extra_positionals.clone());
    if command_name == "benchmark" {
        if let Some(repeat) = request.repeat {
            args.push(format!("--repeat={repeat}"));
        }
        if let Some(baseline_mode) = &request.baseline_mode {
            args.push(format!("--baseline={baseline_mode}"));
        }
        if let Some(threshold_percent) = request.threshold_percent {
            args.push(format!("--threshold-percent={threshold_percent}"));
        }
    }
    if command_supports_strategy(&command_name) {
        if let Some(strategy) = &request.strategy {
            args.push(format!("--strategy={strategy}"));
        }
    }
    if command_supports_format(&command_name) {
        if let Some(format) = &request.format {
            args.push(format!("--format={format}"));
        }
    }
    if command_supports_view(&command_name) {
        if let Some(view) = &request.view {
            args.push(format!("--view={view}"));
        }
    }
    if command_supports_property(&command_name) {
        if let Some(property_id) = &request.property_id {
            args.push(format!("--property={property_id}"));
        }
    }
    if command_supports_seed(&command_name) {
        if let Some(seed) = request.seed {
            args.push(format!("--seed={seed}"));
        }
    }
    if command_supports_backend(&command_name) {
        if let Some(backend) = &request.backend {
            args.push(format!("--backend={backend}"));
        }
    }
    if command_supports_solver(&command_name) {
        if let Some(solver_executable) = &request.solver_executable {
            args.push("--solver-exec".to_string());
            args.push(solver_executable.clone());
        }
        for solver_arg in &request.solver_args {
            args.push("--solver-arg".to_string());
            args.push(solver_arg.clone());
        }
    }
    if command_supports_focus_action(&command_name) {
        if let Some(focus_action_id) = &request.focus_action_id {
            args.push(format!("--focus-action={focus_action_id}"));
        }
    }
    if command_supports_actions(&command_name) && !request.actions.is_empty() {
        args.push(format!("--actions={}", request.actions.join(",")));
    }
    if command_supports_write_path(&command_name) {
        if let Some(write_path) = &request.write_path {
            if write_path.is_empty() {
                args.push("--write".to_string());
            } else {
                args.push(format!("--write={write_path}"));
            }
        }
    }
    if command_supports_check_flag(&command_name) && request.check {
        args.push("--check".to_string());
    }
    if request.json {
        args.push("--json".to_string());
    }
    if request.progress_json {
        args.push("--progress=json".to_string());
    }
    args
}

fn normalize_registry_command(command: &str) -> String {
    match command {
        "models" => "list",
        "diagram" => "graph",
        "readiness" => "lint",
        "verify" => "check",
        "bench" => "benchmark",
        "suite" => "all",
        "generate-tests" => "testgen",
        other => other,
    }
    .to_string()
}

fn command_supports_backend(command: &str) -> bool {
    matches!(
        command,
        "check" | "benchmark" | "orchestrate" | "testgen" | "trace" | "coverage"
    )
}

fn command_supports_solver(command: &str) -> bool {
    command_supports_backend(command)
}

fn command_supports_property(command: &str) -> bool {
    matches!(
        command,
        "check"
            | "benchmark"
            | "graph"
            | "testgen"
            | "trace"
            | "coverage"
            | "replay"
            | "all"
            | "handoff"
    )
}

fn command_supports_seed(command: &str) -> bool {
    matches!(
        command,
        "check" | "testgen" | "trace" | "coverage" | "orchestrate" | "all"
    )
}

fn command_supports_strategy(command: &str) -> bool {
    matches!(command, "testgen")
}

fn command_supports_format(command: &str) -> bool {
    matches!(command, "graph")
}

fn command_supports_view(command: &str) -> bool {
    matches!(command, "graph")
}

fn command_supports_focus_action(command: &str) -> bool {
    matches!(command, "replay" | "testgen")
}

fn command_supports_actions(command: &str) -> bool {
    matches!(command, "replay")
}

fn command_supports_write_path(command: &str) -> bool {
    matches!(command, "migrate" | "doc" | "handoff")
}

fn command_supports_check_flag(command: &str) -> bool {
    matches!(command, "migrate" | "doc" | "handoff")
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
    } else if normalized.ends_with("/valid/registry.rs") || normalized == "valid/registry.rs" {
        return Ok(ExternalTarget {
            manifest_path: manifest_path.clone(),
            kind: ExternalTargetKind::Bin,
            name: bin_target_name_from_manifest(manifest_path.as_deref())?,
        });
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

fn bin_target_name_from_manifest(manifest_path: Option<&str>) -> Result<String, String> {
    let manifest_path = manifest_path
        .ok_or_else(|| "`valid/registry.rs` requires a Cargo manifest path".to_string())?;
    let body = std::fs::read_to_string(manifest_path)
        .map_err(|err| format!("failed to read manifest `{manifest_path}`: {err}"))?;
    let mut in_package = false;
    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "name" {
            continue;
        }
        let value = value.trim().trim_matches('"');
        if value.is_empty() {
            break;
        }
        return Ok(value.to_string());
    }
    Err(format!(
        "failed to determine package binary name from manifest `{manifest_path}`"
    ))
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
    fn resolves_scaffold_registry_file_to_package_bin() {
        let temp_dir =
            std::env::temp_dir().join(format!("valid-external-target-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("valid")).expect("temp dir");
        let manifest = temp_dir.join("Cargo.toml");
        std::fs::write(
            &manifest,
            "[package]\nname = \"scaffold-demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .expect("manifest");
        let target = resolve_external_target(&ExternalTargetOptions {
            manifest_path: Some(manifest.to_string_lossy().to_string()),
            file: Some(
                temp_dir
                    .join("valid/registry.rs")
                    .to_string_lossy()
                    .to_string(),
            ),
            ..ExternalTargetOptions::default()
        })
        .unwrap();
        assert_eq!(target.kind, ExternalTargetKind::Bin);
        assert_eq!(target.name, "scaffold-demo");
        let _ = std::fs::remove_dir_all(&temp_dir);
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
