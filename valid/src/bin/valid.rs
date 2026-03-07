use std::{env, fs, process};

use valid::{
    api::{
        capabilities_response, check_source, explain_source, inspect_source, lint_source,
        minimize_source, orchestrate_source, render_explain_json, render_explain_text,
        render_inspect_json, render_inspect_text, render_lint_json, render_lint_text,
        testgen_source, validate_capabilities_request, validate_capabilities_response,
        validate_check_request, validate_explain_response, validate_inspect_request,
        validate_inspect_response, validate_minimize_response, validate_orchestrate_request,
        validate_orchestrate_response, validate_testgen_request, validate_testgen_response,
        CapabilitiesRequest, CapabilitiesResponse, CheckRequest, InspectRequest, MinimizeRequest,
        OrchestrateRequest, TestgenRequest,
    },
    bundled_models::{coverage_bundled_model, is_bundled_model_ref},
    contract::{
        build_lock_file, compare_snapshot, parse_lock_file, render_drift_json, render_lock_json,
        snapshot_model, write_lock_file,
    },
    coverage::{collect_coverage, render_coverage_json, render_coverage_text},
    engine::CheckOutcome,
    evidence::{
        render_diagnostics_json, render_outcome_json, render_outcome_text, write_outcome_artifacts,
    },
    frontend::compile_model,
    reporter::{
        render_model_dot, render_model_mermaid, render_model_svg, render_trace_mermaid,
        render_trace_sequence_mermaid,
    },
    selfcheck::{run_smoke_selfcheck, write_selfcheck_artifact},
    testgen::render_replay_json,
};

fn main() {
    let mut args = env::args().skip(1);
    let command = normalize_command(&args.next().unwrap_or_default());

    match command.as_str() {
        "check" => cmd_check(args.collect()),
        "inspect" => cmd_inspect(args.collect()),
        "graph" => cmd_graph(args.collect()),
        "lint" => cmd_lint(args.collect()),
        "capabilities" => cmd_capabilities(args.collect()),
        "explain" => cmd_explain(args.collect()),
        "minimize" => cmd_minimize(args.collect()),
        "contract" => cmd_contract(args.collect()),
        "trace" => cmd_trace(args.collect()),
        "orchestrate" => cmd_orchestrate(args.collect()),
        "testgen" => cmd_testgen(args.collect()),
        "replay" => cmd_replay(args.collect()),
        "coverage" => cmd_coverage(args.collect()),
        "clean" => cmd_clean(args.collect()),
        "selfcheck" => cmd_selfcheck(),
        _ => {
            eprintln!("usage: valid <inspect|graph|readiness|verify|capabilities|explain|minimize|contract|trace|orchestrate|generate-tests|replay|coverage|clean|selfcheck> ...");
            process::exit(3);
        }
    }
}

fn normalize_command(command: &str) -> String {
    match command {
        "diagram" => "graph",
        "readiness" => "lint",
        "verify" => "check",
        "generate-tests" => "testgen",
        other => other,
    }
    .to_string()
}

fn cmd_check(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid check <model-file> [--json] [--property=<id>] [--backend=<explicit|mock-bmc|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
    );
    let source = read_source(&parsed.path);
    let request = CheckRequest {
        request_id: "req-local-0001".to_string(),
        source_name: parsed.path.clone(),
        source,
        property_id: parsed.property_id.clone(),
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
    };
    if let Err(message) = validate_check_request(&request) {
        eprintln!("{message}");
        process::exit(3);
    }
    let outcome = check_source(&request);
    let _ = write_outcome_artifacts(
        &parsed.path,
        valid::engine::ArtifactPolicy::EmitOnFailure,
        &outcome,
    );
    if parsed.json {
        println!("{}", render_outcome_json(&parsed.path, &outcome));
    } else {
        print!("{}", render_outcome_text(&outcome));
        println!("model_ref: {}", parsed.path);
    }
    let code = match outcome {
        CheckOutcome::Completed(result) => match result.status {
            valid::engine::RunStatus::Pass => 0,
            valid::engine::RunStatus::Fail => 2,
            valid::engine::RunStatus::Unknown => 4,
        },
        CheckOutcome::Errored(_) => 3,
    };
    process::exit(code);
}

fn cmd_explain(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid explain <model-file> [--json] [--property=<id>] [--backend=<explicit|mock-bmc|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
    );
    let source = read_source(&parsed.path);
    match explain_source(&CheckRequest {
        request_id: "req-local-explain".to_string(),
        source_name: parsed.path.clone(),
        source,
        property_id: parsed.property_id.clone(),
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
    }) {
        Ok(response) => {
            if let Err(message) = validate_explain_response(&response) {
                eprintln!("{message}");
                process::exit(3);
            }
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
                print_diagnostics(&error.diagnostics);
            }
            process::exit(3);
        }
    }
}

fn cmd_minimize(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid minimize <model-file> [--json] [--property=<id>] [--backend=<explicit|mock-bmc|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
    );
    let source = read_source(&parsed.path);
    match minimize_source(&MinimizeRequest {
        request_id: "req-local-minimize".to_string(),
        source_name: parsed.path.clone(),
        source,
        property_id: parsed.property_id.clone(),
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
    }) {
        Ok(response) => {
            if let Err(message) = validate_minimize_response(&response) {
                eprintln!("{message}");
                process::exit(3);
            }
            if parsed.json {
                println!(
                    "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"original_steps\":{},\"minimized_steps\":{},\"vector_id\":\"{}\"}}",
                    response.schema_version,
                    response.request_id,
                    response.status,
                    response.original_steps,
                    response.minimized_steps,
                    response.vector_id
                );
            } else {
                println!("vector_id: {}", response.vector_id);
                println!("original_steps: {}", response.original_steps);
                println!("minimized_steps: {}", response.minimized_steps);
            }
        }
        Err(error) => {
            if parsed.json {
                println!("{}", render_diagnostics_json(&error.diagnostics));
            } else {
                print_diagnostics(&error.diagnostics);
            }
            process::exit(3);
        }
    }
}

fn cmd_inspect(args: Vec<String>) {
    let parsed = parse_common_args(args, "usage: valid inspect <model-file> [--json]");
    let source = read_source(&parsed.path);
    let request = InspectRequest {
        request_id: "req-local-inspect".to_string(),
        source_name: parsed.path.clone(),
        source,
    };
    if let Err(message) = validate_inspect_request(&request) {
        eprintln!("{message}");
        process::exit(3);
    }
    match inspect_source(&request) {
        Ok(response) => {
            if let Err(message) = validate_inspect_response(&response) {
                eprintln!("{message}");
                process::exit(3);
            }
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
                print_diagnostics(&diagnostics);
            }
            process::exit(3);
        }
    }
}

fn cmd_graph(args: Vec<String>) {
    let parsed = parse_common_args_with(
        args,
        "usage: valid graph <model-file> [--format=mermaid|dot|svg|text|json]",
        |_arg, _parsed| false,
    );
    let source = if is_bundled_model_ref(&parsed.path) {
        String::new()
    } else {
        read_source(&parsed.path)
    };
    let request = InspectRequest {
        request_id: "req-local-graph".to_string(),
        source_name: parsed.path.clone(),
        source,
    };
    let env_default_format = std::env::var("VALID_DEFAULT_GRAPH_FORMAT").ok();
    let render_format = parsed
        .format
        .as_deref()
        .or(env_default_format.as_deref())
        .unwrap_or("mermaid");
    match inspect_source(&request) {
        Ok(response) => match render_format {
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
                print_diagnostics(&diagnostics);
            }
            process::exit(3);
        }
    }
}

fn cmd_lint(args: Vec<String>) {
    let parsed = parse_common_args(args, "usage: valid lint <model-file> [--json]");
    let source = read_source(&parsed.path);
    let request = InspectRequest {
        request_id: "req-local-lint".to_string(),
        source_name: parsed.path.clone(),
        source,
    };
    if let Err(message) = validate_inspect_request(&request) {
        eprintln!("{message}");
        process::exit(3);
    }
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
                print_diagnostics(&diagnostics);
            }
            process::exit(3);
        }
    }
}

fn cmd_capabilities(args: Vec<String>) {
    let parsed = parse_common_args_with(
        args,
        "usage: valid capabilities [--json] [--backend=<explicit|mock-bmc|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
        |_arg, _parsed| false,
    );
    let request = CapabilitiesRequest {
        request_id: "req-local-capabilities".to_string(),
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
    };
    if let Err(message) = validate_capabilities_request(&request) {
        eprintln!("{message}");
        process::exit(3);
    }
    match capabilities_response(&request) {
        Ok(response) => {
            if let Err(message) = validate_capabilities_response(&response) {
                eprintln!("{message}");
                process::exit(3);
            }
            if parsed.json {
                print_capabilities_json(&response);
            } else {
                println!("backend: {}", response.backend);
                println!(
                    "supports_explicit: {}",
                    response.capabilities.supports_explicit
                );
                println!("supports_bmc: {}", response.capabilities.supports_bmc);
                println!(
                    "supports_certificate: {}",
                    response.capabilities.supports_certificate
                );
                println!("supports_trace: {}", response.capabilities.supports_trace);
                println!(
                    "supports_witness: {}",
                    response.capabilities.supports_witness
                );
                println!(
                    "selfcheck_compatible: {}",
                    response.capabilities.selfcheck_compatible
                );
            }
        }
        Err(message) => {
            eprintln!("{message}");
            process::exit(3);
        }
    }
}

fn cmd_contract(args: Vec<String>) {
    let mut args = args.into_iter();
    let sub = args.next().unwrap_or_else(|| "snapshot".to_string());
    let path = args.next().unwrap_or_else(|| {
        eprintln!("usage: valid contract <snapshot|lock|drift> <model-file> [lock-file]");
        process::exit(3);
    });
    let source = read_source(&path);
    let model = compile_model(&source).unwrap_or_else(|diagnostics| {
        print_diagnostics(&diagnostics);
        process::exit(3);
    });
    let snapshot = snapshot_model(&model);
    match sub.as_str() {
        "snapshot" => {
            println!("model_id: {}", snapshot.model_id);
            println!("contract_hash: {}", snapshot.contract_hash);
            println!("state_fields: {}", snapshot.state_fields.join(", "));
        }
        "lock" => {
            let lock = build_lock_file(vec![snapshot]);
            let output = args.next().unwrap_or_else(|| "valid.lock.json".to_string());
            write_lock_file(&output, &lock).unwrap_or_else(|err| {
                eprintln!("{err}");
                process::exit(3);
            });
            println!("{}", render_lock_json(&lock));
        }
        "drift" => {
            let lock_path = args.next().unwrap_or_else(|| {
                eprintln!("usage: valid contract drift <model-file> <lock-file>");
                process::exit(3);
            });
            let lock_body = read_source(&lock_path);
            let lock = parse_lock_file(&lock_body).unwrap_or_else(|err| {
                eprintln!("failed to parse lock file: {err}");
                process::exit(3);
            });
            let expected = lock
                .entries
                .into_iter()
                .find(|entry| entry.model_id == snapshot.model_id)
                .unwrap_or_else(|| {
                    eprintln!("model `{}` not found in lock file", snapshot.model_id);
                    process::exit(3);
                });
            let drift = compare_snapshot(&expected, &snapshot);
            println!("{}", render_drift_json(&drift));
        }
        _ => {
            eprintln!("usage: valid contract <snapshot|lock|drift> <model-file> [lock-file]");
            process::exit(3);
        }
    }
}

fn cmd_testgen(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid testgen <model-file> [--json] [--property=<id>] [--strategy=<counterexample|transition|witness|guard|boundary|path|random>] [--backend=<explicit|mock-bmc|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
    );
    let strategy = parsed
        .extra
        .clone()
        .unwrap_or_else(|| "counterexample".to_string());
    let source = read_source(&parsed.path);
    let request = TestgenRequest {
        request_id: "req-local-testgen".to_string(),
        source_name: parsed.path.clone(),
        source: source.clone(),
        property_id: parsed.property_id.clone(),
        strategy,
        backend: parsed.backend.clone(),
        solver_executable: parsed.solver_executable.clone(),
        solver_args: parsed.solver_args.clone(),
    };
    if let Err(message) = validate_testgen_request(&request) {
        eprintln!("{message}");
        process::exit(3);
    }
    match testgen_source(&request) {
        Ok(response) => {
            if let Err(message) = validate_testgen_response(&response) {
                eprintln!("{message}");
                process::exit(3);
            }
            if parsed.json {
                println!(
                    "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"vector_ids\":[{}],\"generated_files\":[{}]}}",
                    response.schema_version,
                    response.request_id,
                    response.status,
                    response
                        .vector_ids
                        .iter()
                        .map(|s| format!("\"{}\"", s))
                        .collect::<Vec<_>>()
                        .join(","),
                    response
                        .generated_files
                        .iter()
                        .map(|s| format!("\"{}\"", s))
                        .collect::<Vec<_>>()
                        .join(",")
                );
            } else {
                println!("generated {} vector(s)", response.vector_ids.len());
                for path in &response.generated_files {
                    println!("  {path}");
                }
            }
        }
        Err(error) => {
            print_diagnostics(&error.diagnostics);
            process::exit(3);
        }
    }
}

fn cmd_trace(args: Vec<String>) {
    let parsed = parse_common_args_with(
        args,
        "usage: valid trace <model-file> [--format=mermaid-state|mermaid-sequence|json] [--property=<id>] [--backend=<explicit|mock-bmc|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
        |arg, options| {
            let _ = (arg, options);
            false
        },
    );
    let format = parsed
        .format
        .clone()
        .unwrap_or_else(|| "mermaid-state".to_string());
    let source = read_source(&parsed.path);
    let outcome = check_source(&CheckRequest {
        request_id: "req-local-trace".to_string(),
        source_name: parsed.path.clone(),
        source,
        property_id: parsed.property_id.clone(),
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
    });
    let trace = match outcome {
        CheckOutcome::Completed(result) => result.trace,
        CheckOutcome::Errored(error) => {
            print_diagnostics(&error.diagnostics);
            process::exit(3);
        }
    }
    .unwrap_or_else(|| {
        eprintln!("no trace available");
        process::exit(3);
    });
    match format.as_str() {
        "json" => println!("{}", valid::evidence::render_trace_json(&trace)),
        "mermaid-sequence" => println!("{}", render_trace_sequence_mermaid(&trace)),
        _ => println!("{}", render_trace_mermaid(&trace)),
    }
}

fn cmd_replay(args: Vec<String>) {
    let parsed = parse_common_args_with(
        args,
        "usage: valid replay <model-file> [--json] [--property=<id>] [--focus-action=<id>] [--actions=a,b,c]",
        |arg, parsed| {
            if let Some(value) = arg.strip_prefix("--actions=") {
                parsed.actions = value
                    .split(',')
                    .filter(|item| !item.is_empty())
                    .map(|item| item.to_string())
                    .collect();
                true
            } else if let Some(value) = arg.strip_prefix("--focus-action=") {
                parsed.focus_action_id = Some(value.to_string());
                true
            } else {
                false
            }
        },
    );
    let output = if is_bundled_model_ref(&parsed.path) {
        valid::bundled_models::replay_bundled_model(
            &parsed.path,
            parsed.property_id.as_deref(),
            &parsed.actions,
            parsed.focus_action_id.as_deref(),
        )
    } else {
        let source = read_source(&parsed.path);
        let model = compile_model(&source).unwrap_or_else(|diagnostics| {
            print_diagnostics(&diagnostics);
            process::exit(3);
        });
        let property_id = parsed
            .property_id
            .as_deref()
            .or_else(|| {
                model
                    .properties
                    .first()
                    .map(|property| property.property_id.as_str())
            })
            .unwrap_or("P_SAFE");
        let terminal = valid::kernel::replay::replay_actions(&model, &parsed.actions)
            .unwrap_or_else(|error| {
                print_diagnostics(&[error]);
                process::exit(3);
            });
        let focus_enabled = parsed.focus_action_id.as_deref().map(|action_id| {
            valid::kernel::transition::apply_action(&model, &terminal, action_id)
                .ok()
                .flatten()
                .is_some()
        });
        Ok(render_replay_json(
            property_id,
            &parsed.actions,
            &terminal.as_named_map(&model),
            parsed.focus_action_id.as_deref(),
            focus_enabled,
        ))
    }
    .unwrap_or_else(|message| {
        eprintln!("{message}");
        process::exit(3);
    });
    println!("{output}");
}

fn cmd_orchestrate(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid orchestrate <model-file> [--json] [--backend=<explicit|mock-bmc|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
    );
    let source = read_source(&parsed.path);
    let request = OrchestrateRequest {
        request_id: "req-local-orchestrate".to_string(),
        source_name: parsed.path.clone(),
        source,
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
    };
    if let Err(message) = validate_orchestrate_request(&request) {
        eprintln!("{message}");
        process::exit(3);
    }
    match orchestrate_source(&request) {
        Ok(response) => {
            if let Err(message) = validate_orchestrate_response(&response) {
                eprintln!("{message}");
                process::exit(3);
            }
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
                for run in &response.runs {
                    println!("property_id: {} status: {}", run.property_id, run.status);
                }
                if let Some(report) = &response.aggregate_coverage {
                    println!();
                    println!("{}", render_coverage_text(report));
                }
            }
        }
        Err(error) => {
            if parsed.json {
                println!("{}", render_diagnostics_json(&error.diagnostics));
            } else {
                print_diagnostics(&error.diagnostics);
            }
            process::exit(3);
        }
    }
}

fn cmd_coverage(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid coverage <model-file> [--property=<id>] [--backend=<explicit|mock-bmc|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
    );
    if is_bundled_model_ref(&parsed.path) {
        let report = coverage_bundled_model(&parsed.path).unwrap_or_else(|message| {
            eprintln!("{message}");
            process::exit(3);
        });
        if parsed.json {
            println!("{}", render_coverage_json(&report));
        } else {
            println!("{}", render_coverage_text(&report));
        }
        return;
    }
    let source = read_source(&parsed.path);
    let model = compile_model(&source).unwrap_or_else(|diagnostics| {
        print_diagnostics(&diagnostics);
        process::exit(3);
    });
    let outcome = check_source(&CheckRequest {
        request_id: "req-local-coverage".to_string(),
        source_name: parsed.path.clone(),
        source,
        property_id: parsed.property_id.clone(),
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
    });
    match outcome {
        CheckOutcome::Completed(result) => {
            let traces = result.trace.into_iter().collect::<Vec<_>>();
            let report = collect_coverage(&model, &traces);
            if parsed.json {
                println!("{}", render_coverage_json(&report));
            } else {
                println!("{}", render_coverage_text(&report));
            }
        }
        CheckOutcome::Errored(error) => {
            print_diagnostics(&error.diagnostics);
            process::exit(3);
        }
    }
}

#[derive(Default)]
struct ParsedArgs {
    json: bool,
    path: String,
    backend: Option<String>,
    solver_executable: Option<String>,
    solver_args: Vec<String>,
    format: Option<String>,
    property_id: Option<String>,
    actions: Vec<String>,
    focus_action_id: Option<String>,
    extra: Option<String>,
}

fn parse_common_args(args: Vec<String>, usage: &str) -> ParsedArgs {
    parse_common_args_with(args, usage, |arg, parsed| {
        if let Some(value) = arg.strip_prefix("--strategy=") {
            parsed.extra = Some(value.to_string());
            true
        } else {
            false
        }
    })
}

fn parse_common_args_with<F>(args: Vec<String>, usage: &str, mut extra_handler: F) -> ParsedArgs
where
    F: FnMut(&str, &mut ParsedArgs) -> bool,
{
    let mut parsed = ParsedArgs::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--json" {
            parsed.json = true;
        } else if let Some(value) = arg.strip_prefix("--format=") {
            parsed.format = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--backend=") {
            parsed.backend = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--property=") {
            parsed.property_id = Some(value.to_string());
        } else if arg == "--solver-exec" {
            parsed.solver_executable = Some(iter.next().unwrap_or_else(|| {
                eprintln!("{usage}");
                process::exit(3);
            }));
        } else if arg == "--solver-arg" {
            parsed.solver_args.push(iter.next().unwrap_or_else(|| {
                eprintln!("{usage}");
                process::exit(3);
            }));
        } else if extra_handler(&arg, &mut parsed) {
            continue;
        } else if parsed.path.is_empty() {
            parsed.path = arg;
        } else {
            eprintln!("{usage}");
            process::exit(3);
        }
    }
    if parsed.path.is_empty() && !usage.contains("valid capabilities") {
        eprintln!("{usage}");
        process::exit(3);
    }
    parsed
}

fn cmd_selfcheck() {
    let args = env::args().skip(2).collect::<Vec<_>>();
    let json = args.iter().any(|arg| arg == "--json");
    let report = run_smoke_selfcheck();
    let _ = write_selfcheck_artifact(&report);
    if json {
        println!("{}", valid::selfcheck::render_selfcheck_json(&report));
    } else {
        println!("suite_id: {}", report.suite_id);
        println!("run_id: {}", report.run_id);
        println!("status: {}", report.status);
        for case in report.cases {
            println!("case {}: {}", case.case_id, case.status);
        }
    }
}

fn cmd_clean(args: Vec<String>) {
    let json = args.iter().any(|arg| arg == "--json");
    let scope = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .map(String::as_str)
        .unwrap_or("all");
    let root = env::current_dir().unwrap_or_else(|_| ".".into());
    let mut removed = Vec::new();
    match scope {
        "all" => {
            removed.extend(clean_generated_tests(&root));
            removed.extend(clean_artifacts(&root));
        }
        "generated" | "generated-tests" => removed.extend(clean_generated_tests(&root)),
        "artifacts" => removed.extend(clean_artifacts(&root)),
        other => {
            eprintln!("usage: valid clean [generated|artifacts|all] [--json]\nunknown clean scope `{other}`");
            process::exit(3);
        }
    }
    if json {
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
}

fn read_source(path: &str) -> String {
    if is_bundled_model_ref(path) {
        return String::new();
    }
    fs::read_to_string(path).unwrap_or_else(|err| {
        eprintln!("error [frontend.parse]: failed to read `{path}`: {err}");
        process::exit(3);
    })
}

fn print_diagnostics(diagnostics: &[valid::support::diagnostics::Diagnostic]) {
    for diagnostic in diagnostics {
        eprintln!("error: {}", diagnostic.message);
        eprintln!("  segment: {}", diagnostic.segment.as_str());
        eprintln!("  code: {}", diagnostic.error_code.as_str());
        if let Some(span) = &diagnostic.primary_span {
            eprintln!("  --> {}:{}:{}", span.source, span.line, span.column);
        }
        if !diagnostic.help.is_empty() {
            eprintln!("help:");
            for item in &diagnostic.help {
                eprintln!("  - {item}");
            }
        }
        if !diagnostic.best_practices.is_empty() {
            eprintln!("best practice:");
            for item in &diagnostic.best_practices {
                eprintln!("  - {item}");
            }
        }
    }
}

fn clean_generated_tests(root: &std::path::Path) -> Vec<String> {
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

fn clean_artifacts(root: &std::path::Path) -> Vec<String> {
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

fn resolve_project_dir(
    root: &std::path::Path,
    env_key: &str,
    default_rel: &str,
) -> std::path::PathBuf {
    std::env::var(env_key)
        .ok()
        .map(std::path::PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        })
        .unwrap_or_else(|| root.join(default_rel))
}

fn print_capabilities_json(response: &CapabilitiesResponse) {
    println!(
        "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"backend\":\"{}\",\"capabilities\":{{\"backend_name\":\"{}\",\"supports_explicit\":{},\"supports_bmc\":{},\"supports_certificate\":{},\"supports_trace\":{},\"supports_witness\":{},\"selfcheck_compatible\":{}}}}}",
        response.schema_version,
        response.request_id,
        response.backend,
        response.capabilities.backend_name,
        response.capabilities.supports_explicit,
        response.capabilities.supports_bmc,
        response.capabilities.supports_certificate,
        response.capabilities.supports_trace,
        response.capabilities.supports_witness,
        response.capabilities.selfcheck_compatible
    );
}
