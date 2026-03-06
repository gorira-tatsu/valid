use std::{env, fs, process};

use valid::{
    api::{check_source, inspect_source, CheckRequest},
    contract::snapshot_model,
    coverage::{collect_coverage, render_coverage_json},
    engine::CheckOutcome,
    evidence::{render_diagnostics_json, render_outcome_json, render_outcome_text},
    frontend::compile_model,
    selfcheck::run_smoke_selfcheck,
    testgen::{build_counterexample_vector, generated_test_output_path, render_rust_test},
};

fn main() {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_default();

    match command.as_str() {
        "check" => cmd_check(args.collect()),
        "inspect" => cmd_inspect(args.collect()),
        "contract" => cmd_contract(args.collect()),
        "testgen" => cmd_testgen(args.collect()),
        "coverage" => cmd_coverage(args.collect()),
        "selfcheck" => cmd_selfcheck(),
        _ => {
            eprintln!("usage: valid <check|inspect|contract|testgen|coverage|selfcheck> ...");
            process::exit(3);
        }
    }
}

fn cmd_check(args: Vec<String>) {
    let (json, path) = parse_json_and_path(args, "usage: valid check <model-file> [--json]");
    let source = read_source(&path);
    let outcome = check_source(&CheckRequest {
        request_id: "req-local-0001".to_string(),
        source_name: path.clone(),
        source,
        property_id: None,
    });
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
            if json { println!("{}", render_diagnostics_json(&diagnostics)); } else { print_diagnostics(&diagnostics); }
            process::exit(3);
        }
    }
}

fn cmd_contract(args: Vec<String>) {
    let (_, path) = parse_json_and_path(args, "usage: valid contract <model-file>");
    let source = read_source(&path);
    let model = compile_model(&source).unwrap_or_else(|diagnostics| {
        print_diagnostics(&diagnostics);
        process::exit(3);
    });
    let snapshot = snapshot_model(&model);
    println!("model_id: {}", snapshot.model_id);
    println!("contract_hash: {}", snapshot.contract_hash);
    println!("state_fields: {}", snapshot.state_fields.join(", "));
}

fn cmd_testgen(args: Vec<String>) {
    let (_, path) = parse_json_and_path(args, "usage: valid testgen <model-file>");
    let source = read_source(&path);
    let outcome = check_source(&CheckRequest {
        request_id: "req-local-testgen".to_string(),
        source_name: path.clone(),
        source,
        property_id: None,
    });
    match outcome {
        CheckOutcome::Completed(result) => {
            let trace = match result.trace {
                Some(trace) => trace,
                None => {
                    eprintln!("no evidence trace available for test generation");
                    process::exit(3);
                }
            };
            let vector = build_counterexample_vector(&trace).unwrap_or_else(|err| {
                eprintln!("{err}");
                process::exit(3);
            });
            println!("vector_id: {}", vector.vector_id);
            println!("output_path: {}", generated_test_output_path(&vector));
            println!("{}", render_rust_test(&vector));
        }
        CheckOutcome::Errored(error) => {
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
    let report = run_smoke_selfcheck();
    println!("suite_id: {}", report.suite_id);
    println!("run_id: {}", report.run_id);
    println!("status: {}", report.status);
    for case in report.cases {
        println!("case {}: {}", case.case_id, case.status);
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
