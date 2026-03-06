use std::{env, fs, process};

use valid::{
    api::{
        check_source, explain_source, inspect_source, minimize_source, testgen_source,
        validate_check_request, CheckRequest, MinimizeRequest, TestgenRequest,
    },
    contract::{
        build_lock_file, compare_snapshot, parse_lock_file, render_drift_json, render_lock_json,
        snapshot_model, write_lock_file,
    },
    coverage::{collect_coverage, render_coverage_json},
    engine::CheckOutcome,
    evidence::{
        render_diagnostics_json, render_outcome_json, render_outcome_text, write_outcome_artifacts,
        write_vector_artifact,
    },
    frontend::compile_model,
    selfcheck::{run_smoke_selfcheck, write_selfcheck_artifact},
    testgen::{
        build_counterexample_vector, build_transition_coverage_vectors, generated_test_output_path,
        render_rust_test,
    },
};

fn main() {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_default();

    match command.as_str() {
        "check" => cmd_check(args.collect()),
        "inspect" => cmd_inspect(args.collect()),
        "explain" => cmd_explain(args.collect()),
        "minimize" => cmd_minimize(args.collect()),
        "contract" => cmd_contract(args.collect()),
        "testgen" => cmd_testgen(args.collect()),
        "coverage" => cmd_coverage(args.collect()),
        "selfcheck" => cmd_selfcheck(),
        _ => {
            eprintln!("usage: valid <check|inspect|explain|minimize|contract|testgen|coverage|selfcheck> ...");
            process::exit(3);
        }
    }
}

fn cmd_check(args: Vec<String>) {
    let (json, path) = parse_json_and_path(args, "usage: valid check <model-file> [--json]");
    let source = read_source(&path);
    let request = CheckRequest {
        request_id: "req-local-0001".to_string(),
        source_name: path.clone(),
        source,
        property_id: None,
    };
    if let Err(message) = validate_check_request(&request) {
        eprintln!("{message}");
        process::exit(3);
    }
    let outcome = check_source(&request);
    let _ = write_outcome_artifacts(
        &path,
        valid::engine::ArtifactPolicy::EmitOnFailure,
        &outcome,
    );
    if json {
        println!("{}", render_outcome_json(&path, &outcome));
    } else {
        print!("{}", render_outcome_text(&outcome));
        println!("model_ref: {}", path);
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
    let (json, path) = parse_json_and_path(args, "usage: valid explain <model-file> [--json]");
    let source = read_source(&path);
    match explain_source(&CheckRequest {
        request_id: "req-local-explain".to_string(),
        source_name: path.clone(),
        source,
        property_id: None,
    }) {
        Ok(response) => {
            if json {
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
                for cause in response.candidate_causes {
                    println!("cause[{}]: {}", cause.kind, cause.message);
                }
            }
        }
        Err(error) => {
            if json {
                println!("{}", render_diagnostics_json(&error.diagnostics));
            } else {
                print_diagnostics(&error.diagnostics);
            }
            process::exit(3);
        }
    }
}

fn cmd_minimize(args: Vec<String>) {
    let (json, path) = parse_json_and_path(args, "usage: valid minimize <model-file> [--json]");
    let source = read_source(&path);
    match minimize_source(&MinimizeRequest {
        request_id: "req-local-minimize".to_string(),
        source_name: path.clone(),
        source,
        property_id: None,
    }) {
        Ok(response) => {
            if json {
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
            if json {
                println!("{}", render_diagnostics_json(&error.diagnostics));
            } else {
                print_diagnostics(&error.diagnostics);
            }
            process::exit(3);
        }
    }
}

fn cmd_inspect(args: Vec<String>) {
    let (json, path) = parse_json_and_path(args, "usage: valid inspect <model-file> [--json]");
    let source = read_source(&path);
    match inspect_source("req-local-inspect", &source) {
        Ok(response) => {
            if json {
                println!("{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"model_id\":\"{}\",\"state_fields\":[{}],\"actions\":[{}],\"properties\":[{}]}}",
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
            if json {
                println!("{}", render_diagnostics_json(&diagnostics));
            } else {
                print_diagnostics(&diagnostics);
            }
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
    let (json, path) = parse_json_and_path(args, "usage: valid testgen <model-file> [--json]");
    let source = read_source(&path);
    let request = TestgenRequest {
        request_id: "req-local-testgen".to_string(),
        source_name: path.clone(),
        source: source.clone(),
        strategy: "counterexample".to_string(),
    };
    match testgen_source(&request) {
        Ok(_) => {
            let outcome = check_source(&CheckRequest {
                request_id: request.request_id.clone(),
                source_name: request.source_name.clone(),
                source,
                property_id: None,
            });
            let run_id = match &outcome {
                CheckOutcome::Completed(result) => result.manifest.run_id.clone(),
                CheckOutcome::Errored(error) => error.manifest.run_id.clone(),
            };
            let traces = match outcome {
                CheckOutcome::Completed(result) => result.trace.into_iter().collect::<Vec<_>>(),
                CheckOutcome::Errored(error) => {
                    print_diagnostics(&error.diagnostics);
                    process::exit(3);
                }
            };
            let vectors = if request.strategy == "transition" {
                build_transition_coverage_vectors(
                    &traces,
                    &compile_model(&read_source(&path))
                        .unwrap()
                        .actions
                        .iter()
                        .map(|a| a.action_id.clone())
                        .collect::<Vec<_>>(),
                )
            } else {
                traces
                    .iter()
                    .filter_map(|trace| build_counterexample_vector(trace).ok())
                    .collect::<Vec<_>>()
            };
            for vector in vectors {
                let rendered = render_rust_test(&vector);
                let _ = write_vector_artifact(&run_id, &vector.vector_id, &rendered);
                write_generated_test_file(&vector, &rendered);
                if json {
                    println!(
                        "{{\"vector_id\":\"{}\",\"output_path\":\"{}\"}}",
                        vector.vector_id,
                        generated_test_output_path(&vector)
                    );
                } else {
                    println!("vector_id: {}", vector.vector_id);
                    println!("output_path: {}", generated_test_output_path(&vector));
                    println!("{}", rendered);
                }
            }
        }
        Err(error) => {
            print_diagnostics(&error.diagnostics);
            process::exit(3);
        }
    }
}

fn cmd_coverage(args: Vec<String>) {
    let (_, path) = parse_json_and_path(args, "usage: valid coverage <model-file>");
    let source = read_source(&path);
    let model = compile_model(&source).unwrap_or_else(|diagnostics| {
        print_diagnostics(&diagnostics);
        process::exit(3);
    });
    let outcome = check_source(&CheckRequest {
        request_id: "req-local-coverage".to_string(),
        source_name: path.clone(),
        source,
        property_id: None,
    });
    match outcome {
        CheckOutcome::Completed(result) => {
            let traces = result.trace.into_iter().collect::<Vec<_>>();
            let report = collect_coverage(&model, &traces);
            println!("{}", render_coverage_json(&report));
        }
        CheckOutcome::Errored(error) => {
            print_diagnostics(&error.diagnostics);
            process::exit(3);
        }
    }
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

fn parse_json_and_path(args: Vec<String>, usage: &str) -> (bool, String) {
    let mut json = false;
    let mut path = None;
    for arg in args {
        if arg == "--json" {
            json = true;
        } else {
            path = Some(arg);
        }
    }
    let path = match path {
        Some(path) => path,
        None => {
            eprintln!("{usage}");
            process::exit(3);
        }
    };
    (json, path)
}

fn read_source(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|err| {
        eprintln!("error [frontend.parse]: failed to read `{path}`: {err}");
        process::exit(3);
    })
}

fn write_generated_test_file(vector: &valid::testgen::TestVector, body: &str) {
    let path = generated_test_output_path(vector);
    if let Err(err) = valid::support::io::write_text_file(&path, body) {
        eprintln!("failed to write generated test `{path}`: {err}");
        process::exit(3);
    }
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
