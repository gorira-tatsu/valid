use std::{env, fs, process};

use valid::{
    engine::{check_explicit, CheckOutcome, PropertySelection, RunPlan},
    evidence::{render_diagnostics_json, render_outcome_json, render_outcome_text},
    support::diagnostics::Diagnostic,
};

fn main() {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_default();

    match command.as_str() {
        "check" => {
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
                    eprintln!("usage: valid check <model-file> [--json]");
                    process::exit(3);
                }
            };

            let source = match fs::read_to_string(&path) {
                Ok(source) => source,
                Err(err) => {
                    eprintln!("error [frontend.parse]: failed to read `{path}`: {err}");
                    process::exit(3);
                }
            };

            let model = match valid::frontend::compile_model(&source) {
                Ok(model) => model,
                Err(diagnostics) => {
                    if json {
                        println!("{}", render_diagnostics_json(&diagnostics));
                    } else {
                        print_diagnostics(&diagnostics);
                    }
                    process::exit(3);
                }
            };

            let mut plan = RunPlan::default();
            let property_id = model
                .properties
                .first()
                .map(|property| property.property_id.clone())
                .unwrap_or_else(|| "P_SAFE".to_string());
            plan.property_selection = PropertySelection::ExactlyOne(property_id);
            plan.reporter_options.json = json;

            let outcome = check_explicit(&model, &plan);
            if json {
                println!("{}", render_outcome_json(&model.model_id, &outcome));
            } else {
                print!("{}", render_outcome_text(&outcome));
                println!("model_id: {}", model.model_id);
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
        _ => {
            eprintln!("usage: valid check <model-file> [--json]");
            process::exit(3);
        }
    }
}

fn print_diagnostics(diagnostics: &[Diagnostic]) {
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
