use std::{env, path::Path, process};
use std::process::Command;

use valid::{
    api::{
        check_source, explain_source, inspect_source, orchestrate_source, testgen_source,
        CheckRequest, InspectRequest, OrchestrateRequest, TestgenRequest,
    },
    bundled_models::{coverage_bundled_model, list_bundled_models},
    coverage::{render_coverage_json, render_coverage_text},
    engine::CheckOutcome,
    evidence::{render_diagnostics_json, render_outcome_json, render_outcome_text},
};

fn main() {
    let parsed = parse_cli(env::args().skip(1).collect());
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
    };

    match parsed.command.as_str() {
        "list" => cmd_list(local_args),
        "inspect" => cmd_inspect(local_args),
        "check" => cmd_check(local_args),
        "all" => cmd_all(local_args),
        "explain" => cmd_explain(local_args),
        "coverage" => cmd_coverage(local_args),
        "orchestrate" => cmd_orchestrate(local_args),
        "testgen" => cmd_testgen(local_args),
        _ => {
            eprintln!(
                "usage: cargo valid <list|inspect|check|all|explain|coverage|orchestrate|testgen> ... [--strategy=<counterexample|transition|witness|guard|boundary|random>]"
            );
            process::exit(3);
        }
    }
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
    let models = fetch_external_models(&parsed);
    let mut aggregate_status = 0;
    let mut json_runs = Vec::new();

    for model in models {
        let mut command = build_external_command(&CliArgs {
            command: "check".to_string(),
            model: Some(model.clone()),
            strategy: None,
            json: parsed.json,
            manifest_path: parsed.manifest_path.clone(),
            example: parsed.example.clone(),
            bin: parsed.bin.clone(),
            file: parsed.file.clone(),
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
    command.arg("--");
    command.arg(&parsed.command);
    if let Some(model) = &parsed.model {
        command.arg(model);
    }
    if let Some(strategy) = &parsed.strategy {
        command.arg(format!("--strategy={strategy}"));
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
        json: true,
        manifest_path: parsed.manifest_path.clone(),
        example: parsed.example.clone(),
        bin: parsed.bin.clone(),
        file: parsed.file.clone(),
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
    let models = list_bundled_models();
    let mut aggregate_status = 0;
    let mut json_runs = Vec::new();

    for model in models {
        let request = CheckRequest {
            request_id: format!("cargo-valid-check-{model}"),
            source_name: normalized_model_ref(model),
            source: String::new(),
            property_id: None,
            backend: None,
            solver_executable: None,
            solver_args: Vec::new(),
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
    let model = parsed.model.unwrap_or_else(|| usage_exit("usage: cargo valid inspect <model> [--json]"));
    let request = InspectRequest {
        request_id: "cargo-valid-inspect".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
    };
    match inspect_source(&request) {
        Ok(response) => {
            if parsed.json {
                println!(
                    "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"model_id\":\"{}\",\"state_fields\":[{}],\"actions\":[{}],\"properties\":[{}]}}",
                    response.schema_version,
                    response.request_id,
                    response.status,
                    response.model_id,
                    response.state_fields.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
                    response.actions.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
                    response.properties.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(",")
                );
            } else {
                println!("model_id: {}", response.model_id);
                println!("state_fields: {}", response.state_fields.join(", "));
                println!("actions: {}", response.actions.join(", "));
                println!("properties: {}", response.properties.join(", "));
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

fn cmd_check(parsed: ParsedArgs) {
    let model = parsed.model.unwrap_or_else(|| usage_exit("usage: cargo valid check <model> [--json]"));
    let request = CheckRequest {
        request_id: "cargo-valid-check".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
        property_id: None,
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
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
    let model = parsed.model.unwrap_or_else(|| usage_exit("usage: cargo valid explain <model> [--json]"));
    let request = CheckRequest {
        request_id: "cargo-valid-explain".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
        property_id: None,
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
    };
    match explain_source(&request) {
        Ok(response) => {
            if parsed.json {
                println!(
                    "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"evidence_id\":\"{}\",\"property_id\":\"{}\",\"failure_step_index\":{},\"involved_fields\":[{}],\"candidate_causes\":[{}],\"repair_hints\":[{}],\"confidence\":{},\"best_practices\":[{}]}}",
                    response.schema_version,
                    response.request_id,
                    response.status,
                    response.evidence_id,
                    response.property_id,
                    response.failure_step_index,
                    response.involved_fields.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
                    response.candidate_causes.iter().map(|c| format!("{{\"kind\":\"{}\",\"message\":\"{}\"}}", c.kind, c.message)).collect::<Vec<_>>().join(","),
                    response.repair_hints.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
                    response.confidence,
                    response.best_practices.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
                );
            } else {
                println!("property_id: {}", response.property_id);
                println!("evidence_id: {}", response.evidence_id);
                println!("failure_step_index: {}", response.failure_step_index);
                println!("involved_fields: {}", response.involved_fields.join(", "));
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
    let model = parsed.model.unwrap_or_else(|| usage_exit("usage: cargo valid coverage <model> [--json]"));
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
    let model = parsed.model.unwrap_or_else(|| usage_exit("usage: cargo valid orchestrate <model> [--json]"));
    let request = OrchestrateRequest {
        request_id: "cargo-valid-orchestrate".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
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
    let model = parsed.model.unwrap_or_else(|| usage_exit("usage: cargo valid testgen <model> [--json] [--strategy=<counterexample|transition|witness|guard|boundary|random>]"));
    let request = TestgenRequest {
        request_id: "cargo-valid-testgen".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
        strategy: parsed
            .strategy
            .clone()
            .unwrap_or_else(|| "counterexample".to_string()),
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
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

#[derive(Default)]
struct ParsedArgs {
    json: bool,
    model: Option<String>,
    strategy: Option<String>,
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
    json: bool,
}

fn parse_cli(args: Vec<String>) -> CliArgs {
    let mut parsed = CliArgs::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--manifest-path" => parsed.manifest_path = Some(next_arg(&mut iter, "--manifest-path")),
            "--example" => parsed.example = Some(next_arg(&mut iter, "--example")),
            "--bin" => parsed.bin = Some(next_arg(&mut iter, "--bin")),
            "--file" => parsed.file = Some(next_arg(&mut iter, "--file")),
            "--json" => parsed.json = true,
            _ if arg.starts_with("--strategy=") => {
                parsed.strategy = Some(arg.trim_start_matches("--strategy=").to_string())
            }
            _ if parsed.command.is_empty() => parsed.command = arg,
            _ if parsed.model.is_none() => parsed.model = Some(arg),
            _ => usage_exit("usage: cargo valid [--manifest-path <path>] [--file <path>|--example <name>|--bin <name>] <list|inspect|check|all|explain|coverage|orchestrate|testgen> [model] [--json] [--strategy=<counterexample|transition|witness|guard|boundary|random>]"),
        }
    }
    if parsed.command.is_empty() {
        usage_exit("usage: cargo valid [--manifest-path <path>] [--file <path>|--example <name>|--bin <name>] <list|inspect|check|all|explain|coverage|orchestrate|testgen> [model] [--json] [--strategy=<counterexample|transition|witness|guard|boundary|random>]");
    }
    if parsed.file.is_some() && (parsed.example.is_some() || parsed.bin.is_some()) {
        usage_exit("use either --file or --example/--bin, not both");
    }
    parsed
}

fn next_arg(iter: &mut impl Iterator<Item = String>, flag: &str) -> String {
    iter.next().unwrap_or_else(|| usage_exit(&format!("missing value for {flag}")))
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
        .unwrap_or_else(|| usage_exit("registry list output did not contain a closing models array"));
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
