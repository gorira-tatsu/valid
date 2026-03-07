use std::process::Command;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process,
};

use valid::{
    api::{
        check_source, explain_source, inspect_source, lint_source, orchestrate_source,
        render_explain_json, render_explain_text, render_inspect_json, render_inspect_text,
        render_lint_json, render_lint_text, testgen_source, CheckRequest, InspectRequest,
        OrchestrateRequest, TestgenRequest,
    },
    bundled_models::{coverage_bundled_model, list_bundled_models},
    coverage::{render_coverage_json, render_coverage_text},
    engine::CheckOutcome,
    evidence::{render_diagnostics_json, render_outcome_json, render_outcome_text},
    project::{
        load_project_config, render_project_config_template, render_registry_source_template,
        ProjectConfig,
    },
    reporter::{render_model_dot, render_model_mermaid, render_model_svg},
};

fn main() {
    let parsed = maybe_auto_discover_external(parse_cli(env::args().skip(1).collect()));
    if parsed.command == "init" {
        cmd_init(&parsed);
    }
    if parsed.command == "clean" {
        cmd_clean(&parsed);
    }
    if parsed.manifest_path.is_some()
        || parsed.example.is_some()
        || parsed.bin.is_some()
        || parsed.file.is_some()
    {
        run_external_registry(parsed);
    }

    let local_args = ParsedArgs {
        json: parsed.json,
        model: parsed.model,
        strategy: parsed.strategy,
        format: parsed.format,
        property_id: parsed.property_id,
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
        actions: parsed.actions,
        focus_action_id: parsed.focus_action_id,
        suite_models: parsed.suite_models,
    };

    match parsed.command.as_str() {
        "list" => cmd_list(local_args),
        "inspect" => cmd_inspect(local_args),
        "graph" => cmd_graph(local_args),
        "lint" => cmd_lint(local_args),
        "check" => cmd_check(local_args),
        "all" => cmd_all(local_args),
        "explain" => cmd_explain(local_args),
        "coverage" => cmd_coverage(local_args),
        "orchestrate" => cmd_orchestrate(local_args),
        "testgen" => cmd_testgen(local_args),
        "replay" => cmd_replay(local_args),
        "init" => usage_exit(&primary_usage()),
        "help" => usage_exit(&primary_usage()),
        _ => {
            usage_exit(&primary_usage());
        }
    }
}

fn primary_usage() -> String {
    "usage: cargo valid [--manifest-path <path>] [--registry <path>|--file <path>|--example <name>|--bin <name>] <init|models|inspect|graph|readiness|verify|suite|explain|coverage|orchestrate|generate-tests|replay|clean> [model] [--json] [--format=<mermaid|dot|svg|text|json>] [--property=<id>] [--backend=<explicit|mock-bmc|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>] [--focus-action=<id>] [--actions=a,b,c] [--strategy=<counterexample|transition|witness|guard|boundary|path|random>]".to_string()
}

fn run_external_registry(parsed: CliArgs) -> ! {
    if parsed.command == "all" {
        run_external_all(parsed);
    }

    let status = build_external_command(&parsed)
        .status()
        .unwrap_or_else(|err| {
            eprintln!("failed to execute target registry: {err}");
            process::exit(3);
        });
    process::exit(status.code().unwrap_or(1));
}

fn run_external_all(parsed: CliArgs) -> ! {
    let models = if parsed.suite_models.is_empty() {
        fetch_external_models(&parsed)
    } else {
        parsed.suite_models.clone()
    };
    let mut aggregate_status = 0;
    let mut json_runs = Vec::new();

    for model in models {
        let mut command = build_external_command(&CliArgs {
            command: "check".to_string(),
            model: Some(model.clone()),
            strategy: None,
            format: parsed.format.clone(),
            property_id: None,
            backend: parsed.backend.clone(),
            solver_executable: parsed.solver_executable.clone(),
            solver_args: parsed.solver_args.clone(),
            actions: Vec::new(),
            focus_action_id: None,
            json: parsed.json,
            manifest_path: parsed.manifest_path.clone(),
            example: parsed.example.clone(),
            bin: parsed.bin.clone(),
            file: parsed.file.clone(),
            suite_models: Vec::new(),
        });
        let output = command.output().unwrap_or_else(|err| {
            eprintln!("failed to execute target registry: {err}");
            process::exit(3);
        });
        let code = output.status.code().unwrap_or(1);
        aggregate_status = aggregate_exit_code(aggregate_status, code);
        if parsed.json {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            json_runs.push(stdout);
        } else {
            println!("== {model} ==");
            print!("{}", String::from_utf8_lossy(&output.stdout));
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.trim().is_empty() {
                eprint!("{stderr}");
            }
        }
    }

    if parsed.json {
        println!("{{\"runs\":[{}]}}", json_runs.join(","));
    }
    process::exit(aggregate_status);
}

fn build_external_command(parsed: &CliArgs) -> Command {
    let target = resolve_external_target(parsed);
    let mut command = Command::new("cargo");
    command.arg("run");
    if let Some(manifest_path) = target.manifest_path {
        command.arg("--manifest-path").arg(manifest_path);
    }
    command.arg(target.kind).arg(target.name);
    if let Some(manifest_path) = &parsed.manifest_path {
        command.env("VALID_REGISTRY_MANIFEST_PATH", manifest_path);
    }
    if let Some(file) = &parsed.file {
        command.env("VALID_REGISTRY_FILE", file);
    }
    if let Some(model) = &parsed.model {
        command.env("VALID_REGISTRY_MODEL_NAME", model);
    }
    command.arg("--");
    command.arg(&parsed.command);
    if let Some(model) = &parsed.model {
        command.arg(model);
    }
    if let Some(strategy) = &parsed.strategy {
        command.arg(format!("--strategy={strategy}"));
    }
    if let Some(format) = &parsed.format {
        command.arg(format!("--format={format}"));
    }
    if let Some(property_id) = &parsed.property_id {
        command.arg(format!("--property={property_id}"));
    }
    if let Some(backend) = &parsed.backend {
        command.arg(format!("--backend={backend}"));
    }
    if let Some(solver_executable) = &parsed.solver_executable {
        command.arg("--solver-exec").arg(solver_executable);
    }
    for solver_arg in &parsed.solver_args {
        command.arg("--solver-arg").arg(solver_arg);
    }
    if let Some(focus_action_id) = &parsed.focus_action_id {
        command.arg(format!("--focus-action={focus_action_id}"));
    }
    if !parsed.actions.is_empty() {
        command.arg(format!("--actions={}", parsed.actions.join(",")));
    }
    if parsed.json {
        command.arg("--json");
    }
    command
}

fn fetch_external_models(parsed: &CliArgs) -> Vec<String> {
    let output = build_external_command(&CliArgs {
        command: "list".to_string(),
        model: None,
        strategy: None,
        format: None,
        property_id: None,
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
        actions: Vec::new(),
        focus_action_id: None,
        json: true,
        manifest_path: parsed.manifest_path.clone(),
        example: parsed.example.clone(),
        bin: parsed.bin.clone(),
        file: parsed.file.clone(),
        suite_models: Vec::new(),
    })
    .output()
    .unwrap_or_else(|err| {
        eprintln!("failed to execute target registry: {err}");
        process::exit(3);
    });
    if !output.status.success() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        process::exit(output.status.code().unwrap_or(3));
    }
    parse_models_json(&String::from_utf8_lossy(&output.stdout))
}

fn cmd_all(parsed: ParsedArgs) {
    let models = if parsed.suite_models.is_empty() {
        list_bundled_models()
    } else {
        parsed
            .suite_models
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    };
    let mut aggregate_status = 0;
    let mut json_runs = Vec::new();

    for model in models {
        let request = CheckRequest {
            request_id: format!("cargo-valid-check-{model}"),
            source_name: normalized_model_ref(model),
            source: String::new(),
            property_id: parsed.property_id.clone(),
            backend: parsed.backend.clone(),
            solver_executable: parsed.solver_executable.clone(),
            solver_args: parsed.solver_args.clone(),
        };
        let outcome = check_source(&request);
        let exit_code = outcome_exit_code(&outcome);
        aggregate_status = aggregate_exit_code(aggregate_status, exit_code);
        if parsed.json {
            json_runs.push(render_outcome_json(model, &outcome));
        } else {
            println!("== {model} ==");
            print!("{}", render_outcome_text(&outcome));
        }
    }

    if parsed.json {
        println!("{{\"runs\":[{}]}}", json_runs.join(","));
    }
    process::exit(aggregate_status);
}

fn cmd_list(parsed: ParsedArgs) {
    let models = list_bundled_models();
    if parsed.json {
        println!(
            "{{\"models\":[{}]}}",
            models
                .iter()
                .map(|model| format!("\"{}\"", model))
                .collect::<Vec<_>>()
                .join(",")
        );
    } else {
        for model in models {
            println!("{model}");
        }
    }
}

fn cmd_inspect(parsed: ParsedArgs) {
    let model = parsed
        .model
        .unwrap_or_else(|| usage_exit("usage: cargo valid inspect <model> [--json]"));
    let request = InspectRequest {
        request_id: "cargo-valid-inspect".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
    };
    match inspect_source(&request) {
        Ok(response) => {
            if parsed.json {
                println!("{}", render_inspect_json(&response));
            } else {
                print!("{}", render_inspect_text(&response));
            }
        }
        Err(diagnostics) => {
            if parsed.json {
                println!("{}", render_diagnostics_json(&diagnostics));
            } else {
                for diagnostic in diagnostics {
                    eprintln!("{}", diagnostic.message);
                }
            }
            process::exit(3);
        }
    }
}

fn cmd_graph(parsed: ParsedArgs) {
    let model = parsed.model.unwrap_or_else(|| {
        usage_exit("usage: cargo valid graph <model> [--format=mermaid|dot|svg|text|json]")
    });
    let request = InspectRequest {
        request_id: "cargo-valid-graph".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
    };
    let env_default_format = env::var("VALID_DEFAULT_GRAPH_FORMAT").ok();
    let default_format = parsed
        .format
        .as_deref()
        .or(env_default_format.as_deref())
        .unwrap_or("mermaid");
    match inspect_source(&request) {
        Ok(response) => match default_format {
            "json" => println!("{}", render_inspect_json(&response)),
            "text" => print!("{}", render_inspect_text(&response)),
            "dot" => println!("{}", render_model_dot(&response)),
            "svg" => println!("{}", render_model_svg(&response)),
            _ => println!("{}", render_model_mermaid(&response)),
        },
        Err(diagnostics) => {
            if parsed.json || matches!(parsed.format.as_deref(), Some("json")) {
                println!("{}", render_diagnostics_json(&diagnostics));
            } else {
                for diagnostic in diagnostics {
                    eprintln!("{}", diagnostic.message);
                }
            }
            process::exit(3);
        }
    }
}

fn cmd_lint(parsed: ParsedArgs) {
    let model = parsed
        .model
        .unwrap_or_else(|| usage_exit("usage: cargo valid lint <model> [--json]"));
    let request = InspectRequest {
        request_id: "cargo-valid-lint".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
    };
    match lint_source(&request) {
        Ok(response) => {
            if parsed.json {
                println!("{}", render_lint_json(&response));
            } else {
                print!("{}", render_lint_text(&response));
            }
            let has_findings = response
                .findings
                .iter()
                .any(|finding| matches!(finding.severity.as_str(), "warn" | "error"));
            process::exit(if has_findings { 2 } else { 0 });
        }
        Err(diagnostics) => {
            if parsed.json {
                println!("{}", render_diagnostics_json(&diagnostics));
            } else {
                for diagnostic in diagnostics {
                    eprintln!("{}", diagnostic.message);
                }
            }
            process::exit(3);
        }
    }
}

fn cmd_check(parsed: ParsedArgs) {
    let model = parsed.model.unwrap_or_else(|| usage_exit("usage: cargo valid check <model> [--json] [--property=<id>] [--backend=<explicit|mock-bmc|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]"));
    let request = CheckRequest {
        request_id: "cargo-valid-check".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
        property_id: parsed.property_id.clone(),
        backend: parsed.backend.clone(),
        solver_executable: parsed.solver_executable.clone(),
        solver_args: parsed.solver_args.clone(),
    };
    let outcome = check_source(&request);
    if parsed.json {
        println!("{}", render_outcome_json(&model, &outcome));
    } else {
        print!("{}", render_outcome_text(&outcome));
    }
    process::exit(match outcome {
        CheckOutcome::Completed(result) => match result.status {
            valid::engine::RunStatus::Pass => 0,
            valid::engine::RunStatus::Fail => 2,
            valid::engine::RunStatus::Unknown => 4,
        },
        CheckOutcome::Errored(_) => 3,
    });
}

fn cmd_explain(parsed: ParsedArgs) {
    let model = parsed
        .model
        .unwrap_or_else(|| usage_exit("usage: cargo valid explain <model> [--json]"));
    let request = CheckRequest {
        request_id: "cargo-valid-explain".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
        property_id: parsed.property_id.clone(),
        backend: parsed.backend.clone(),
        solver_executable: parsed.solver_executable.clone(),
        solver_args: parsed.solver_args.clone(),
    };
    match explain_source(&request) {
        Ok(response) => {
            if parsed.json {
                println!("{}", render_explain_json(&response));
            } else {
                print!("{}", render_explain_text(&response));
            }
        }
        Err(error) => {
            if parsed.json {
                println!("{}", render_diagnostics_json(&error.diagnostics));
            } else {
                for diagnostic in error.diagnostics {
                    eprintln!("{}", diagnostic.message);
                }
            }
            process::exit(3);
        }
    }
}

fn cmd_coverage(parsed: ParsedArgs) {
    let model = parsed
        .model
        .unwrap_or_else(|| usage_exit("usage: cargo valid coverage <model> [--json]"));
    let report = coverage_bundled_model(&normalized_model_ref(&model)).unwrap_or_else(|message| {
        eprintln!("{message}");
        process::exit(3);
    });
    if parsed.json {
        println!("{}", render_coverage_json(&report));
    } else {
        println!("{}", render_coverage_text(&report));
    }
}

fn cmd_orchestrate(parsed: ParsedArgs) {
    let model = parsed.model.unwrap_or_else(|| usage_exit("usage: cargo valid orchestrate <model> [--json] [--backend=<explicit|mock-bmc|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]"));
    let request = OrchestrateRequest {
        request_id: "cargo-valid-orchestrate".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
        backend: parsed.backend.clone(),
        solver_executable: parsed.solver_executable.clone(),
        solver_args: parsed.solver_args.clone(),
    };
    match orchestrate_source(&request) {
        Ok(response) => {
            if parsed.json {
                let body = response
                    .runs
                    .iter()
                    .map(|run| {
                        format!(
                            "{{\"property_id\":\"{}\",\"status\":\"{}\",\"assurance_level\":\"{}\",\"run_id\":\"{}\"}}",
                            run.property_id, run.status, run.assurance_level, run.run_id
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let aggregate_coverage = response
                    .aggregate_coverage
                    .as_ref()
                    .map(render_coverage_json)
                    .unwrap_or_else(|| "null".to_string());
                println!(
                    "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"runs\":[{}],\"aggregate_coverage\":{}}}",
                    response.schema_version, response.request_id, body, aggregate_coverage
                );
            } else {
                for run in response.runs {
                    println!("property_id: {} status: {}", run.property_id, run.status);
                }
            }
        }
        Err(error) => {
            if parsed.json {
                println!("{}", render_diagnostics_json(&error.diagnostics));
            } else {
                for diagnostic in error.diagnostics {
                    eprintln!("{}", diagnostic.message);
                }
            }
            process::exit(3);
        }
    }
}

fn cmd_testgen(parsed: ParsedArgs) {
    let model = parsed.model.unwrap_or_else(|| usage_exit("usage: cargo valid testgen <model> [--json] [--strategy=<counterexample|transition|witness|guard|boundary|path|random>]"));
    let request = TestgenRequest {
        request_id: "cargo-valid-testgen".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
        property_id: parsed.property_id.clone(),
        strategy: parsed
            .strategy
            .clone()
            .unwrap_or_else(|| "counterexample".to_string()),
        backend: parsed.backend.clone(),
        solver_executable: parsed.solver_executable.clone(),
        solver_args: parsed.solver_args.clone(),
    };
    match testgen_source(&request) {
        Ok(response) => {
            if parsed.json {
                println!(
                    "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"vector_ids\":[{}],\"generated_files\":[{}]}}",
                    response.schema_version,
                    response.request_id,
                    response.status,
                    response.vector_ids.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
                    response.generated_files.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(",")
                );
            } else {
                println!("vector_ids: {}", response.vector_ids.join(", "));
            }
        }
        Err(error) => {
            if parsed.json {
                println!("{}", render_diagnostics_json(&error.diagnostics));
            } else {
                for diagnostic in error.diagnostics {
                    eprintln!("{}", diagnostic.message);
                }
            }
            process::exit(3);
        }
    }
}

fn cmd_replay(parsed: ParsedArgs) {
    let model = parsed
        .model
        .unwrap_or_else(|| usage_exit("usage: cargo valid replay <model> [--json] [--property=<id>] [--focus-action=<id>] [--actions=a,b,c]"));
    let output = valid::bundled_models::replay_bundled_model(
        &normalized_model_ref(&model),
        parsed.property_id.as_deref(),
        &parsed.actions,
        parsed.focus_action_id.as_deref(),
    )
    .unwrap_or_else(|message| {
        eprintln!("{message}");
        process::exit(3);
    });
    println!("{output}");
}

fn cmd_clean(parsed: &CliArgs) -> ! {
    let scope = parsed.model.as_deref().unwrap_or("all");
    let root = clean_root(parsed);
    let mut removed = Vec::new();
    match scope {
        "all" => {
            removed.extend(clean_generated_tests(&root));
            removed.extend(clean_artifacts(&root));
        }
        "generated" | "generated-tests" => {
            removed.extend(clean_generated_tests(&root));
        }
        "artifacts" => {
            removed.extend(clean_artifacts(&root));
        }
        other => usage_exit(&format!(
            "usage: cargo valid clean [generated|artifacts|all] [--json]\nunknown clean scope `{other}`"
        )),
    }
    if parsed.json {
        println!(
            "{{\"status\":\"ok\",\"root\":\"{}\",\"removed\":[{}]}}",
            root.display(),
            removed
                .iter()
                .map(|path| format!("\"{}\"", path))
                .collect::<Vec<_>>()
                .join(",")
        );
    } else {
        println!("clean root: {}", root.display());
        if removed.is_empty() {
            println!("removed: none");
        } else {
            for path in &removed {
                println!("removed: {path}");
            }
        }
    }
    process::exit(0);
}

#[derive(Default)]
struct ParsedArgs {
    json: bool,
    model: Option<String>,
    strategy: Option<String>,
    format: Option<String>,
    property_id: Option<String>,
    backend: Option<String>,
    solver_executable: Option<String>,
    solver_args: Vec<String>,
    actions: Vec<String>,
    focus_action_id: Option<String>,
    suite_models: Vec<String>,
}

#[derive(Default)]
struct CliArgs {
    manifest_path: Option<String>,
    example: Option<String>,
    bin: Option<String>,
    file: Option<String>,
    command: String,
    model: Option<String>,
    strategy: Option<String>,
    format: Option<String>,
    property_id: Option<String>,
    backend: Option<String>,
    solver_executable: Option<String>,
    solver_args: Vec<String>,
    actions: Vec<String>,
    focus_action_id: Option<String>,
    json: bool,
    suite_models: Vec<String>,
}

fn parse_cli(args: Vec<String>) -> CliArgs {
    let mut parsed = CliArgs::default();
    let normalized_args = if matches!(args.first().map(String::as_str), Some("valid")) {
        args.into_iter().skip(1).collect::<Vec<_>>()
    } else {
        args
    };
    let mut iter = normalized_args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--manifest-path" => {
                parsed.manifest_path = Some(next_arg(&mut iter, "--manifest-path"))
            }
            "--registry" => parsed.file = Some(next_arg(&mut iter, "--registry")),
            "--example" => parsed.example = Some(next_arg(&mut iter, "--example")),
            "--bin" => parsed.bin = Some(next_arg(&mut iter, "--bin")),
            "--file" => parsed.file = Some(next_arg(&mut iter, "--file")),
            "--solver-exec" => {
                parsed.solver_executable = Some(next_arg(&mut iter, "--solver-exec"))
            }
            "--solver-arg" => parsed.solver_args.push(next_arg(&mut iter, "--solver-arg")),
            "--json" => parsed.json = true,
            _ if arg.starts_with("--strategy=") => {
                parsed.strategy = Some(arg.trim_start_matches("--strategy=").to_string())
            }
            _ if arg.starts_with("--format=") => {
                parsed.format = Some(arg.trim_start_matches("--format=").to_string())
            }
            _ if arg.starts_with("--property=") => {
                parsed.property_id = Some(arg.trim_start_matches("--property=").to_string())
            }
            _ if arg.starts_with("--backend=") => {
                parsed.backend = Some(arg.trim_start_matches("--backend=").to_string())
            }
            _ if arg.starts_with("--actions=") => {
                parsed.actions = arg
                    .trim_start_matches("--actions=")
                    .split(',')
                    .filter(|item| !item.is_empty())
                    .map(|item| item.to_string())
                    .collect()
            }
            _ if arg.starts_with("--focus-action=") => {
                parsed.focus_action_id = Some(arg.trim_start_matches("--focus-action=").to_string())
            }
            _ if parsed.command.is_empty() => parsed.command = normalize_command(&arg),
            _ if parsed.model.is_none() => parsed.model = Some(arg),
            _ => usage_exit(&primary_usage()),
        }
    }
    if parsed.command.is_empty() {
        usage_exit(&primary_usage());
    }
    if parsed.file.is_some() && (parsed.example.is_some() || parsed.bin.is_some()) {
        usage_exit("use either --file or --example/--bin, not both");
    }
    parsed
}

fn normalize_command(command: &str) -> String {
    match command {
        "models" => "list",
        "diagram" => "graph",
        "readiness" => "lint",
        "verify" => "check",
        "suite" => "all",
        "generate-tests" => "testgen",
        other => other,
    }
    .to_string()
}

fn next_arg(iter: &mut impl Iterator<Item = String>, flag: &str) -> String {
    iter.next()
        .unwrap_or_else(|| usage_exit(&format!("missing value for {flag}")))
}

fn normalized_model_ref(model: &str) -> String {
    if model.starts_with("rust:") {
        model.to_string()
    } else {
        format!("rust:{model}")
    }
}

struct ExternalTarget {
    manifest_path: Option<String>,
    kind: &'static str,
    name: String,
}

fn maybe_auto_discover_external(mut parsed: CliArgs) -> CliArgs {
    if matches!(parsed.command.as_str(), "clean" | "init" | "help") {
        return parsed;
    }
    let current_dir = project_root(&parsed);
    let built_in_manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cargo_toml = current_dir.join("Cargo.toml");
    match load_project_config(&current_dir) {
        Ok(Some(config)) => {
            apply_project_runtime_config(&config);
            let explicit_registry_target =
                parsed.file.is_some() || parsed.example.is_some() || parsed.bin.is_some();
            if parsed.backend.is_none() {
                parsed.backend = config.default_backend.clone();
            }
            if parsed.suite_models.is_empty() && !explicit_registry_target {
                parsed.suite_models = config.suite_models.clone();
            }
            if !explicit_registry_target {
                if let Some(registry) = config.registry.clone() {
                    parsed.file = Some(current_dir.join(registry).to_string_lossy().to_string());
                }
            }
            if parsed.manifest_path.is_none() && cargo_toml.exists() {
                parsed.manifest_path = Some(cargo_toml.to_string_lossy().to_string());
            }
        }
        Ok(None) => {}
        Err(message) => {
            eprintln!("{message}");
            process::exit(3);
        }
    }
    if parsed.file.is_some() || parsed.example.is_some() || parsed.bin.is_some() {
        return parsed;
    }
    if current_dir == built_in_manifest_dir {
        return parsed;
    }
    if !cargo_toml.exists() {
        return parsed;
    }
    let candidates = [
        current_dir.join("examples").join("valid_models.rs"),
        current_dir.join("src").join("bin").join("valid_models.rs"),
    ];
    if let Some(file) = candidates.into_iter().find(|path| path.exists()) {
        if parsed.manifest_path.is_none() {
            parsed.manifest_path = Some(cargo_toml.to_string_lossy().to_string());
        }
        parsed.file = Some(file.to_string_lossy().to_string());
    }
    parsed
}

fn project_root(parsed: &CliArgs) -> PathBuf {
    if let Some(manifest_path) = &parsed.manifest_path {
        let path = PathBuf::from(manifest_path);
        return path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn resolve_external_target(parsed: &CliArgs) -> ExternalTarget {
    if let Some(file) = &parsed.file {
        return target_from_file(parsed.manifest_path.clone(), file);
    }
    if let Some(example) = &parsed.example {
        return ExternalTarget {
            manifest_path: parsed.manifest_path.clone(),
            kind: "--example",
            name: example.clone(),
        };
    }
    if let Some(bin) = &parsed.bin {
        return ExternalTarget {
            manifest_path: parsed.manifest_path.clone(),
            kind: "--bin",
            name: bin.clone(),
        };
    }
    ExternalTarget {
        manifest_path: parsed.manifest_path.clone(),
        kind: "--example",
        name: "valid_models".to_string(),
    }
}

fn target_from_file(manifest_path: Option<String>, file: &str) -> ExternalTarget {
    let path = Path::new(file);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_else(|| usage_exit("expected a Rust source file path for --file"));
    let normalized = file.replace('\\', "/");
    let kind = if normalized.ends_with(&format!("/examples/{stem}.rs"))
        || normalized == format!("examples/{stem}.rs")
    {
        "--example"
    } else if normalized.ends_with(&format!("/src/bin/{stem}.rs"))
        || normalized == format!("src/bin/{stem}.rs")
    {
        "--bin"
    } else {
        usage_exit("`--file` currently supports files under `examples/` or `src/bin/`");
    };
    ExternalTarget {
        manifest_path,
        kind,
        name: stem.to_string(),
    }
}

fn parse_models_json(stdout: &str) -> Vec<String> {
    let start = stdout
        .find('[')
        .unwrap_or_else(|| usage_exit("registry list output did not contain a models array"));
    let end = stdout[start..]
        .find(']')
        .map(|offset| start + offset)
        .unwrap_or_else(|| {
            usage_exit("registry list output did not contain a closing models array")
        });
    stdout[start + 1..end]
        .split(',')
        .filter_map(|entry| {
            let trimmed = entry.trim().trim_matches('"');
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect()
}

fn outcome_exit_code(outcome: &CheckOutcome) -> i32 {
    match outcome {
        CheckOutcome::Completed(result) => match result.status {
            valid::engine::RunStatus::Pass => 0,
            valid::engine::RunStatus::Fail => 2,
            valid::engine::RunStatus::Unknown => 4,
        },
        CheckOutcome::Errored(_) => 3,
    }
}

fn aggregate_exit_code(current: i32, next: i32) -> i32 {
    match (current, next) {
        (3, _) | (_, 3) => 3,
        (2, _) | (_, 2) => 2,
        (4, _) | (_, 4) => 4,
        _ => 0,
    }
}

fn usage_exit(usage: &str) -> ! {
    eprintln!("{usage}");
    process::exit(3);
}

fn clean_root(parsed: &CliArgs) -> PathBuf {
    project_root(parsed)
}

fn cmd_init(parsed: &CliArgs) -> ! {
    let root = project_root(parsed);
    let cargo_toml = root.join("Cargo.toml");
    if !cargo_toml.exists() {
        eprintln!("expected Cargo.toml in {}", root.display());
        process::exit(3);
    }
    let config_path = root.join("valid.toml");
    if config_path.exists() {
        eprintln!("`{}` already exists", config_path.display());
        process::exit(3);
    }
    let registry = parsed.file.as_deref().unwrap_or("examples/valid_models.rs");
    let body = render_project_config_template(registry);
    let registry_path = root.join(registry);
    if let Some(parent) = registry_path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|err| {
            eprintln!("failed to create `{}`: {err}", parent.display());
            process::exit(3);
        });
    }
    if !registry_path.exists() {
        fs::write(&registry_path, render_registry_source_template()).unwrap_or_else(|err| {
            eprintln!("failed to write `{}`: {err}", registry_path.display());
            process::exit(3);
        });
    }
    let generated_dir = root.join("tests").join("generated");
    fs::create_dir_all(&generated_dir).unwrap_or_else(|err| {
        eprintln!("failed to create `{}`: {err}", generated_dir.display());
        process::exit(3);
    });
    let gitkeep = generated_dir.join(".gitkeep");
    if !gitkeep.exists() {
        fs::write(&gitkeep, "").unwrap_or_else(|err| {
            eprintln!("failed to write `{}`: {err}", gitkeep.display());
            process::exit(3);
        });
    }
    fs::write(&config_path, body).unwrap_or_else(|err| {
        eprintln!("failed to write `{}`: {err}", config_path.display());
        process::exit(3);
    });
    if parsed.json {
        println!(
            "{{\"status\":\"ok\",\"created\":\"{}\",\"registry\":\"{}\",\"scaffolded_registry\":\"{}\",\"generated_tests_dir\":\"{}\"}}",
            config_path.display(),
            registry,
            registry_path.display(),
            generated_dir.display(),
        );
    } else {
        println!("created: {}", config_path.display());
        println!("registry: {registry}");
        println!("scaffolded_registry: {}", registry_path.display());
        println!("generated_tests_dir: {}", generated_dir.display());
    }
    process::exit(0);
}

fn clean_generated_tests(root: &Path) -> Vec<String> {
    let generated_dir = resolve_project_dir(root, "VALID_GENERATED_TESTS_DIR", "tests/generated");
    let mut removed = Vec::new();
    let Ok(entries) = fs::read_dir(&generated_dir) else {
        return removed;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let keep = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == ".gitkeep" || name == ".gitignore")
            .unwrap_or(false);
        let removable = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext == "rs")
            .unwrap_or(false);
        if !keep && removable && fs::remove_file(&path).is_ok() {
            removed.push(path.display().to_string());
        }
    }
    removed
}

fn clean_artifacts(root: &Path) -> Vec<String> {
    let artifacts_dir = resolve_project_dir(root, "VALID_ARTIFACTS_DIR", "artifacts");
    if !artifacts_dir.exists() {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(&artifacts_dir) else {
        return Vec::new();
    };
    let mut removed = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let result = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        if result.is_ok() {
            removed.push(path.display().to_string());
        }
    }
    removed
}

fn resolve_project_dir(root: &Path, env_key: &str, default_rel: &str) -> PathBuf {
    env::var(env_key)
        .ok()
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        })
        .unwrap_or_else(|| root.join(default_rel))
}

fn apply_project_runtime_config(config: &ProjectConfig) {
    if let Some(generated_tests_dir) = &config.generated_tests_dir {
        env::set_var("VALID_GENERATED_TESTS_DIR", generated_tests_dir);
    }
    if let Some(artifacts_dir) = &config.artifacts_dir {
        env::set_var("VALID_ARTIFACTS_DIR", artifacts_dir);
    }
    if let Some(default_graph_format) = &config.default_graph_format {
        env::set_var("VALID_DEFAULT_GRAPH_FORMAT", default_graph_format);
    }
}
