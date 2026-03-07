use std::{
    env, fs,
    io::{self, Read},
    process::{self, Command},
};

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
    cli::{
        child_stream_to_json, detect_json_flag, detect_progress_json_flag, message_diagnostic,
        parse_batch_request, render_batch_response, render_cli_error_json, render_commands_json,
        render_commands_text, render_schema_json, usage_diagnostic, BatchResult, ExitCode,
        ProgressReporter, Surface,
    },
    contract::{
        build_lock_file, compare_snapshot, parse_lock_file, render_drift_json, render_lock_json,
        snapshot_model, write_lock_file,
    },
    coverage::{collect_coverage, render_coverage_json, render_coverage_text},
    engine::CheckOutcome,
    evidence::{render_outcome_json, render_outcome_text, write_outcome_artifacts},
    frontend::compile_model,
    reporter::{
        render_model_dot_with_view, render_model_mermaid_with_view, render_model_svg_with_view,
        render_trace_mermaid, render_trace_sequence_mermaid, GraphView,
    },
    selfcheck::{run_smoke_selfcheck, write_selfcheck_artifact},
    testgen::{render_replay_json, replay_path_for_model},
};

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let command = normalize_command(args.first().map(String::as_str).unwrap_or_default());
    let remaining = args.into_iter().skip(1).collect::<Vec<_>>();
    match command.as_str() {
        "check" => cmd_check(remaining),
        "inspect" => cmd_inspect(remaining),
        "graph" => cmd_graph(remaining),
        "lint" => cmd_lint(remaining),
        "capabilities" => cmd_capabilities(remaining),
        "explain" => cmd_explain(remaining),
        "minimize" => cmd_minimize(remaining),
        "contract" => cmd_contract(remaining),
        "trace" => cmd_trace(remaining),
        "orchestrate" => cmd_orchestrate(remaining),
        "testgen" => cmd_testgen(remaining),
        "replay" => cmd_replay(remaining),
        "coverage" => cmd_coverage(remaining),
        "clean" => cmd_clean(remaining),
        "selfcheck" => cmd_selfcheck(remaining),
        "commands" => cmd_commands(remaining),
        "schema" => cmd_schema(remaining),
        "batch" => cmd_batch(remaining),
        _ => {
            usage_exit(
                "valid",
                detect_json_flag(&remaining),
                "usage: valid <inspect|graph|readiness|verify|capabilities|explain|minimize|contract|trace|orchestrate|generate-tests|replay|coverage|clean|selfcheck|commands|schema|batch> ...",
            );
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
        "usage: valid check <model-file> [--json] [--progress=json] [--property=<id>] [--seed=<u64>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
    );
    let progress = ProgressReporter::new("check", parsed.progress_json);
    progress.start(None);
    let source = read_source(&parsed.path, "check", parsed.json);
    let request = CheckRequest {
        request_id: "req-local-0001".to_string(),
        source_name: parsed.path.clone(),
        source,
        property_id: parsed.property_id.clone(),
        seed: parsed.seed,
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
    };
    if let Err(message) = validate_check_request(&request) {
        message_exit("check", parsed.json, &message, None);
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
    let code = ExitCode::from_check_outcome(&outcome);
    progress.finish(code);
    process::exit(code.code());
}

fn cmd_explain(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid explain <model-file> [--json] [--progress=json] [--property=<id>] [--seed=<u64>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
    );
    let progress = ProgressReporter::new("explain", parsed.progress_json);
    progress.start(None);
    let source = read_source(&parsed.path, "explain", parsed.json);
    match explain_source(&CheckRequest {
        request_id: "req-local-explain".to_string(),
        source_name: parsed.path.clone(),
        source,
        property_id: parsed.property_id.clone(),
        seed: parsed.seed,
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
    }) {
        Ok(response) => {
            if let Err(message) = validate_explain_response(&response) {
                message_exit("explain", parsed.json, &message, None);
            }
            if parsed.json {
                println!("{}", render_explain_json(&response));
            } else {
                print!("{}", render_explain_text(&response));
            }
            progress.finish(ExitCode::Success);
        }
        Err(error) => {
            diagnostics_exit("explain", parsed.json, &error.diagnostics, None);
        }
    }
}

fn cmd_minimize(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid minimize <model-file> [--json] [--progress=json] [--property=<id>] [--seed=<u64>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
    );
    let progress = ProgressReporter::new("minimize", parsed.progress_json);
    progress.start(None);
    let source = read_source(&parsed.path, "minimize", parsed.json);
    match minimize_source(&MinimizeRequest {
        request_id: "req-local-minimize".to_string(),
        source_name: parsed.path.clone(),
        source,
        property_id: parsed.property_id.clone(),
        seed: parsed.seed,
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
    }) {
        Ok(response) => {
            if let Err(message) = validate_minimize_response(&response) {
                message_exit("minimize", parsed.json, &message, None);
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
            progress.finish(ExitCode::Success);
        }
        Err(error) => {
            diagnostics_exit("minimize", parsed.json, &error.diagnostics, None);
        }
    }
}

fn cmd_inspect(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid inspect <model-file> [--json] [--progress=json]",
    );
    let progress = ProgressReporter::new("inspect", parsed.progress_json);
    progress.start(None);
    let source = read_source(&parsed.path, "inspect", parsed.json);
    let request = InspectRequest {
        request_id: "req-local-inspect".to_string(),
        source_name: parsed.path.clone(),
        source,
    };
    if let Err(message) = validate_inspect_request(&request) {
        message_exit("inspect", parsed.json, &message, None);
    }
    match inspect_source(&request) {
        Ok(response) => {
            if let Err(message) = validate_inspect_response(&response) {
                message_exit("inspect", parsed.json, &message, None);
            }
            if parsed.json {
                println!("{}", render_inspect_json(&response));
            } else {
                print!("{}", render_inspect_text(&response));
            }
            progress.finish(ExitCode::Success);
        }
        Err(diagnostics) => {
            diagnostics_exit("inspect", parsed.json, &diagnostics, None);
        }
    }
}

fn cmd_graph(args: Vec<String>) {
    let parsed = parse_common_args_with(
        args,
        "usage: valid graph <model-file> [--format=mermaid|dot|svg|text|json] [--view=overview|logic] [--json] [--progress=json]",
        |_arg, _parsed| false,
    );
    let json_output = parsed.json || matches!(parsed.format.as_deref(), Some("json"));
    let progress = ProgressReporter::new("graph", parsed.progress_json);
    progress.start(None);
    let source = if is_bundled_model_ref(&parsed.path) {
        String::new()
    } else {
        read_source(&parsed.path, "graph", json_output)
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
    let view = GraphView::parse(parsed.view.as_deref());
    match inspect_source(&request) {
        Ok(response) => match render_format {
            "json" => println!("{}", render_inspect_json(&response)),
            "text" => print!("{}", render_inspect_text(&response)),
            "dot" => println!("{}", render_model_dot_with_view(&response, view)),
            "svg" => println!("{}", render_model_svg_with_view(&response, view)),
            _ => println!("{}", render_model_mermaid_with_view(&response, view)),
        },
        Err(diagnostics) => diagnostics_exit("graph", json_output, &diagnostics, None),
    }
    progress.finish(ExitCode::Success);
}

fn cmd_lint(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid lint <model-file> [--json] [--progress=json]",
    );
    let progress = ProgressReporter::new("lint", parsed.progress_json);
    progress.start(None);
    let source = read_source(&parsed.path, "lint", parsed.json);
    let request = InspectRequest {
        request_id: "req-local-lint".to_string(),
        source_name: parsed.path.clone(),
        source,
    };
    if let Err(message) = validate_inspect_request(&request) {
        message_exit("lint", parsed.json, &message, None);
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
            let exit_code = if has_findings {
                ExitCode::Fail
            } else {
                ExitCode::Success
            };
            progress.finish(exit_code);
            process::exit(exit_code.code());
        }
        Err(diagnostics) => {
            diagnostics_exit("lint", parsed.json, &diagnostics, None);
        }
    }
}

fn cmd_capabilities(args: Vec<String>) {
    let parsed = parse_common_args_with(
        args,
        "usage: valid capabilities [--json] [--progress=json] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
        |_arg, _parsed| false,
    );
    let progress = ProgressReporter::new("capabilities", parsed.progress_json);
    progress.start(None);
    let request = CapabilitiesRequest {
        request_id: "req-local-capabilities".to_string(),
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
    };
    if let Err(message) = validate_capabilities_request(&request) {
        message_exit("capabilities", parsed.json, &message, None);
    }
    match capabilities_response(&request) {
        Ok(response) => {
            if let Err(message) = validate_capabilities_response(&response) {
                message_exit("capabilities", parsed.json, &message, None);
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
            progress.finish(ExitCode::Success);
        }
        Err(message) => {
            message_exit("capabilities", parsed.json, &message, None);
        }
    }
}

fn cmd_contract(args: Vec<String>) {
    let json = detect_json_flag(&args);
    let progress = ProgressReporter::from_args("contract", &args);
    progress.start(None);
    let positional = args
        .into_iter()
        .filter(|arg| !arg.starts_with("--"))
        .collect::<Vec<_>>();
    let mut args = positional.into_iter();
    let sub = args.next().unwrap_or_else(|| "snapshot".to_string());
    let path = args.next().unwrap_or_else(|| {
        usage_exit(
            "contract",
            json,
            "usage: valid contract <snapshot|lock|drift> <model-file> [lock-file] [--json] [--progress=json]",
        )
    });
    let source = read_source(&path, "contract", json);
    let model = compile_model(&source)
        .unwrap_or_else(|diagnostics| diagnostics_exit("contract", json, &diagnostics, None));
    let snapshot = snapshot_model(&model);
    match sub.as_str() {
        "snapshot" => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "model_id": snapshot.model_id,
                        "contract_hash": snapshot.contract_hash,
                        "state_fields": snapshot.state_fields,
                    })
                );
            } else {
                println!("model_id: {}", snapshot.model_id);
                println!("contract_hash: {}", snapshot.contract_hash);
                println!("state_fields: {}", snapshot.state_fields.join(", "));
            }
            progress.finish(ExitCode::Success);
        }
        "lock" => {
            let lock = build_lock_file(vec![snapshot]);
            let output = args.next().unwrap_or_else(|| "valid.lock.json".to_string());
            write_lock_file(&output, &lock)
                .unwrap_or_else(|err| message_exit("contract", json, &err.to_string(), None));
            println!("{}", render_lock_json(&lock));
            progress.finish(ExitCode::Success);
        }
        "drift" => {
            let lock_path = args.next().unwrap_or_else(|| {
                usage_exit(
                    "contract",
                    json,
                    "usage: valid contract drift <model-file> <lock-file> [--json] [--progress=json]",
                )
            });
            let lock_body = read_source(&lock_path, "contract", json);
            let lock = parse_lock_file(&lock_body).unwrap_or_else(|err| {
                message_exit(
                    "contract",
                    json,
                    &format!("failed to parse lock file: {err}"),
                    None,
                )
            });
            let expected = lock
                .entries
                .into_iter()
                .find(|entry| entry.model_id == snapshot.model_id)
                .unwrap_or_else(|| {
                    message_exit(
                        "contract",
                        json,
                        &format!("model `{}` not found in lock file", snapshot.model_id),
                        None,
                    )
                });
            let drift = compare_snapshot(&expected, &snapshot);
            println!("{}", render_drift_json(&drift));
            let exit_code = if drift.status == "unchanged" {
                ExitCode::Success
            } else {
                ExitCode::Fail
            };
            progress.finish(exit_code);
            process::exit(exit_code.code());
        }
        _ => {
            usage_exit(
                "contract",
                json,
                "usage: valid contract <snapshot|lock|drift> <model-file> [lock-file] [--json] [--progress=json]",
            );
        }
    }
}

fn cmd_testgen(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid testgen <model-file> [--json] [--progress=json] [--property=<id>] [--strategy=<counterexample|transition|witness|guard|boundary|path|random>] [--seed=<u64>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
    );
    let progress = ProgressReporter::new("testgen", parsed.progress_json);
    progress.start(None);
    let strategy = parsed
        .extra
        .clone()
        .unwrap_or_else(|| "counterexample".to_string());
    let source = read_source(&parsed.path, "testgen", parsed.json);
    let request = TestgenRequest {
        request_id: "req-local-testgen".to_string(),
        source_name: parsed.path.clone(),
        source: source.clone(),
        property_id: parsed.property_id.clone(),
        strategy,
        seed: parsed.seed,
        backend: parsed.backend.clone(),
        solver_executable: parsed.solver_executable.clone(),
        solver_args: parsed.solver_args.clone(),
    };
    if let Err(message) = validate_testgen_request(&request) {
        message_exit("testgen", parsed.json, &message, None);
    }
    match testgen_source(&request) {
        Ok(response) => {
            if let Err(message) = validate_testgen_response(&response) {
                message_exit("testgen", parsed.json, &message, None);
            }
            if parsed.json {
                println!(
                    "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"vector_ids\":[{}],\"vectors\":[{}],\"generated_files\":[{}]}}",
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
                        .vectors
                        .iter()
                        .map(|vector| format!(
                            "{{\"vector_id\":\"{}\",\"strictness\":\"{}\",\"derivation\":\"{}\",\"source_kind\":\"{}\",\"strategy\":\"{}\"}}",
                            vector.vector_id,
                            vector.strictness,
                            vector.derivation,
                            vector.source_kind,
                            vector.strategy
                        ))
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
                for vector in &response.vectors {
                    println!(
                        "  {} strictness={} derivation={} source={} strategy={}",
                        vector.vector_id,
                        vector.strictness,
                        vector.derivation,
                        vector.source_kind,
                        vector.strategy
                    );
                }
                for path in &response.generated_files {
                    println!("  {path}");
                }
            }
            progress.finish(ExitCode::Success);
        }
        Err(error) => {
            diagnostics_exit("testgen", parsed.json, &error.diagnostics, None);
        }
    }
}

fn cmd_trace(args: Vec<String>) {
    let parsed = parse_common_args_with(
        args,
        "usage: valid trace <model-file> [--format=mermaid-state|mermaid-sequence|json] [--property=<id>] [--seed=<u64>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>] [--json] [--progress=json]",
        |arg, options| {
            let _ = (arg, options);
            false
        },
    );
    let progress = ProgressReporter::new("trace", parsed.progress_json);
    progress.start(None);
    let format = parsed
        .format
        .clone()
        .unwrap_or_else(|| "mermaid-state".to_string());
    let json_output = parsed.json || format == "json";
    let source = read_source(&parsed.path, "trace", json_output);
    let outcome = check_source(&CheckRequest {
        request_id: "req-local-trace".to_string(),
        source_name: parsed.path.clone(),
        source,
        property_id: parsed.property_id.clone(),
        seed: parsed.seed,
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
    });
    let trace = match outcome {
        CheckOutcome::Completed(result) => result.trace,
        CheckOutcome::Errored(error) => {
            diagnostics_exit("trace", json_output, &error.diagnostics, None);
        }
    }
    .unwrap_or_else(|| {
        message_exit("trace", json_output, "no trace available", None);
    });
    match format.as_str() {
        "json" => println!("{}", valid::evidence::render_trace_json(&trace)),
        "mermaid-sequence" => println!("{}", render_trace_sequence_mermaid(&trace)),
        _ => println!("{}", render_trace_mermaid(&trace)),
    }
    progress.finish(ExitCode::Success);
}

fn cmd_replay(args: Vec<String>) {
    let parsed = parse_common_args_with(
        args,
        "usage: valid replay <model-file> [--json] [--progress=json] [--property=<id>] [--focus-action=<id>] [--actions=a,b,c]",
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
    let progress = ProgressReporter::new("replay", parsed.progress_json);
    progress.start(None);
    let output = if is_bundled_model_ref(&parsed.path) {
        valid::bundled_models::replay_bundled_model(
            &parsed.path,
            parsed.property_id.as_deref(),
            &parsed.actions,
            parsed.focus_action_id.as_deref(),
        )
    } else {
        let source = read_source(&parsed.path, "replay", true);
        let model = compile_model(&source)
            .unwrap_or_else(|diagnostics| diagnostics_exit("replay", true, &diagnostics, None));
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
            .unwrap_or_else(|error| diagnostics_exit("replay", true, &[error], None));
        let focus_enabled = parsed.focus_action_id.as_deref().map(|action_id| {
            valid::kernel::transition::apply_action(&model, &terminal, action_id)
                .ok()
                .flatten()
                .is_some()
        });
        let property = model
            .properties
            .iter()
            .find(|candidate| candidate.property_id == property_id)
            .unwrap_or_else(|| {
                message_exit(
                    "replay",
                    true,
                    &format!("unknown property `{property_id}`"),
                    None,
                )
            });
        let property_holds = matches!(
            valid::kernel::eval::eval_expr(&model, &terminal, &property.expr),
            Ok(valid::ir::Value::Bool(true))
        );
        let replay_path = replay_path_for_model(
            &model,
            &parsed.actions,
            parsed.focus_action_id.as_deref(),
            focus_enabled,
        );
        Ok(render_replay_json(
            property_id,
            &parsed.actions,
            &terminal.as_named_map(&model),
            parsed.focus_action_id.as_deref(),
            focus_enabled,
            Some(property_holds),
            &replay_path,
        ))
    }
    .unwrap_or_else(|message| message_exit("replay", true, &message, None));
    println!("{output}");
    progress.finish(ExitCode::Success);
}

fn cmd_orchestrate(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid orchestrate <model-file> [--json] [--progress=json] [--seed=<u64>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
    );
    let progress = ProgressReporter::new("orchestrate", parsed.progress_json);
    progress.start(None);
    let source = read_source(&parsed.path, "orchestrate", parsed.json);
    let request = OrchestrateRequest {
        request_id: "req-local-orchestrate".to_string(),
        source_name: parsed.path.clone(),
        source,
        seed: parsed.seed,
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
    };
    if let Err(message) = validate_orchestrate_request(&request) {
        message_exit("orchestrate", parsed.json, &message, None);
    }
    match orchestrate_source(&request) {
        Ok(response) => {
            if let Err(message) = validate_orchestrate_response(&response) {
                message_exit("orchestrate", parsed.json, &message, None);
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
            progress.finish(ExitCode::Success);
        }
        Err(error) => {
            diagnostics_exit("orchestrate", parsed.json, &error.diagnostics, None);
        }
    }
}

fn cmd_coverage(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid coverage <model-file> [--json] [--progress=json] [--property=<id>] [--seed=<u64>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
    );
    let progress = ProgressReporter::new("coverage", parsed.progress_json);
    progress.start(None);
    if is_bundled_model_ref(&parsed.path) {
        let report = coverage_bundled_model(&parsed.path)
            .unwrap_or_else(|message| message_exit("coverage", parsed.json, &message, None));
        if parsed.json {
            println!("{}", render_coverage_json(&report));
        } else {
            println!("{}", render_coverage_text(&report));
        }
        progress.finish(ExitCode::Success);
        return;
    }
    let source = read_source(&parsed.path, "coverage", parsed.json);
    let model = compile_model(&source).unwrap_or_else(|diagnostics| {
        diagnostics_exit("coverage", parsed.json, &diagnostics, None)
    });
    let outcome = check_source(&CheckRequest {
        request_id: "req-local-coverage".to_string(),
        source_name: parsed.path.clone(),
        source,
        property_id: parsed.property_id.clone(),
        seed: parsed.seed,
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
            progress.finish(ExitCode::Success);
        }
        CheckOutcome::Errored(error) => {
            diagnostics_exit("coverage", parsed.json, &error.diagnostics, None);
        }
    }
}

#[derive(Default)]
struct ParsedArgs {
    json: bool,
    progress_json: bool,
    path: String,
    seed: Option<u64>,
    backend: Option<String>,
    solver_executable: Option<String>,
    solver_args: Vec<String>,
    format: Option<String>,
    view: Option<String>,
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
    parsed.json = detect_json_flag(&args);
    parsed.progress_json = detect_progress_json_flag(&args);
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--json" {
            parsed.json = true;
        } else if arg == "--progress=json" {
            parsed.progress_json = true;
        } else if arg.starts_with("--progress=") {
            message_exit(
                "valid",
                parsed.json,
                "unsupported progress mode",
                Some(usage),
            );
        } else if let Some(value) = arg.strip_prefix("--format=") {
            parsed.format = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--view=") {
            parsed.view = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--backend=") {
            parsed.backend = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--seed=") {
            parsed.seed = Some(parse_seed_arg(value, usage));
        } else if let Some(value) = arg.strip_prefix("--property=") {
            parsed.property_id = Some(value.to_string());
        } else if arg == "--seed" {
            parsed.seed = Some(parse_seed_arg(
                &iter.next().unwrap_or_else(|| {
                    eprintln!("{usage}");
                    process::exit(3);
                }),
                usage,
            ));
        } else if arg == "--solver-exec" {
            parsed.solver_executable = Some(
                iter.next()
                    .unwrap_or_else(|| usage_exit("valid", parsed.json, usage)),
            );
        } else if arg == "--solver-arg" {
            parsed.solver_args.push(
                iter.next()
                    .unwrap_or_else(|| usage_exit("valid", parsed.json, usage)),
            );
        } else if extra_handler(&arg, &mut parsed) {
            continue;
        } else if parsed.path.is_empty() {
            parsed.path = arg;
        } else {
            usage_exit("valid", parsed.json, usage);
        }
    }
    if parsed.path.is_empty() && !usage.contains("valid capabilities") {
        usage_exit("valid", parsed.json, usage);
    }
    parsed
}

fn parse_seed_arg(value: &str, usage: &str) -> u64 {
    value.parse().unwrap_or_else(|_| {
        eprintln!("{usage}");
        process::exit(3);
    })
}

fn cmd_selfcheck(args: Vec<String>) {
    let json = detect_json_flag(&args);
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
    let json = detect_json_flag(&args);
    let scope = args
        .iter()
        .find(|arg| !arg.starts_with("--") && arg.as_str() != "clean")
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
            message_exit(
                "clean",
                json,
                &format!("unknown clean scope `{other}`"),
                Some("usage: valid clean [generated|artifacts|all] [--json] [--progress=json]"),
            );
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

fn read_source(path: &str, command: &str, json: bool) -> String {
    if is_bundled_model_ref(path) {
        return String::new();
    }
    fs::read_to_string(path).unwrap_or_else(|err| {
        message_exit(
            command,
            json,
            &format!("failed to read `{path}`: {err}"),
            None,
        )
    })
}

fn print_diagnostics(
    command: &str,
    json: bool,
    diagnostics: &[valid::support::diagnostics::Diagnostic],
) {
    if json {
        eprintln!("{}", render_cli_error_json(command, diagnostics, None));
        return;
    }
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
    let generated_dir = resolve_project_dir(root, "VALID_GENERATED_TESTS_DIR", "generated-tests");
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

fn usage_exit(command: &str, json: bool, usage: &str) -> ! {
    if json {
        eprintln!(
            "{}",
            render_cli_error_json(
                command,
                &[usage_diagnostic("invalid command arguments", usage)],
                Some(usage),
            )
        );
    } else {
        eprintln!("{usage}");
    }
    process::exit(ExitCode::Error.code());
}

fn message_exit(command: &str, json: bool, message: &str, usage: Option<&str>) -> ! {
    if json {
        eprintln!(
            "{}",
            render_cli_error_json(command, &[message_diagnostic(message)], usage)
        );
    } else {
        if let Some(usage) = usage {
            eprintln!("{usage}");
        }
        eprintln!("{message}");
    }
    process::exit(ExitCode::Error.code());
}

fn diagnostics_exit(
    command: &str,
    json: bool,
    diagnostics: &[valid::support::diagnostics::Diagnostic],
    usage: Option<&str>,
) -> ! {
    if json {
        eprintln!("{}", render_cli_error_json(command, diagnostics, usage));
    } else {
        print_diagnostics(command, false, diagnostics);
        if let Some(usage) = usage {
            eprintln!("{usage}");
        }
    }
    process::exit(ExitCode::Error.code());
}

fn cmd_commands(args: Vec<String>) {
    if detect_json_flag(&args) {
        println!("{}", render_commands_json(Surface::Valid));
    } else {
        println!("{}", render_commands_text(Surface::Valid));
    }
}

fn cmd_schema(args: Vec<String>) {
    let command = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| usage_exit("schema", true, "usage: valid schema <command>"));
    match render_schema_json(Surface::Valid, &normalize_command(&command)) {
        Ok(body) => println!("{body}"),
        Err(message) => message_exit(
            "schema",
            true,
            &message,
            Some("usage: valid schema <command>"),
        ),
    }
}

fn cmd_batch(args: Vec<String>) {
    let json = detect_json_flag(&args);
    let progress = ProgressReporter::from_args("batch", &args);
    let mut stdin = String::new();
    io::stdin()
        .read_to_string(&mut stdin)
        .unwrap_or_else(|err| {
            message_exit("batch", json, &format!("failed to read stdin: {err}"), None)
        });
    let request = parse_batch_request(&stdin).unwrap_or_else(|message| {
        message_exit(
            "batch",
            true,
            &message,
            Some("usage: valid batch [--json] [--progress=json] < batch.json"),
        )
    });
    let total = request.operations.len();
    progress.start(Some(total));
    let current_exe = env::current_exe().unwrap_or_else(|err| {
        message_exit(
            "batch",
            json,
            &format!("failed to resolve current executable: {err}"),
            None,
        )
    });
    let mut aggregate = ExitCode::Success;
    let mut results = Vec::new();
    for (index, operation) in request.operations.into_iter().enumerate() {
        progress.item_start(index, total, &operation.command);
        let mut child_args = vec![operation.command.clone()];
        child_args.extend(operation.args.clone());
        if operation.json
            && !child_args
                .iter()
                .any(|arg| arg == "--json" || arg.starts_with("--format="))
        {
            if matches!(operation.command.as_str(), "graph" | "trace") {
                child_args.push("--format=json".to_string());
            } else {
                child_args.push("--json".to_string());
            }
        }
        let output = Command::new(&current_exe)
            .args(&child_args)
            .output()
            .unwrap_or_else(|err| {
                message_exit(
                    "batch",
                    true,
                    &format!(
                        "failed to execute batch operation `{}`: {err}",
                        operation.command
                    ),
                    None,
                )
            });
        let exit_code = output.status.code().unwrap_or(ExitCode::Error.code());
        aggregate = aggregate.aggregate(match exit_code {
            0 => ExitCode::Success,
            1 => ExitCode::Fail,
            2 => ExitCode::Unknown,
            _ => ExitCode::Error,
        });
        results.push(BatchResult {
            index,
            command: operation.command.clone(),
            args: child_args.into_iter().skip(1).collect(),
            exit_code,
            stdout: child_stream_to_json(&output.stdout),
            stderr: child_stream_to_json(&output.stderr),
        });
        progress.item_complete(index, total, &operation.command, exit_code);
        if exit_code != 0 && !request.continue_on_error {
            break;
        }
    }
    progress.finish(aggregate);
    println!("{}", render_batch_response(aggregate, results));
    process::exit(aggregate.code());
}
