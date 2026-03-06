use std::{env, process};

use crate::{
    coverage::{render_coverage_json, render_coverage_text, CoverageReport},
    engine::CheckOutcome,
    evidence::{render_diagnostics_json, render_outcome_json, render_outcome_text},
    modeling::{
        build_machine_test_vectors, check_machine_outcome, check_machine_outcomes,
        collect_machine_coverage, explain_machine, property_ids, Finite, ModelingAction,
        ModelingState, VerifiedMachine,
    },
};

use crate::api::{
    ExplainResponse, InspectResponse, OrchestrateResponse, OrchestratedRunSummary, TestgenResponse,
};

pub struct RegisteredModel {
    pub name: &'static str,
    pub inspect: fn(&str) -> InspectResponse,
    pub check: fn(&str) -> CheckOutcome,
    pub explain: fn(&str) -> Result<ExplainResponse, String>,
    pub coverage: fn() -> CoverageReport,
    pub orchestrate: fn(&str) -> OrchestrateResponse,
    pub testgen: fn(&str) -> TestgenResponse,
}

impl RegisteredModel {
    pub fn for_machine<M: VerifiedMachine>(name: &'static str) -> Self {
        Self {
            name,
            inspect: inspect_machine::<M>,
            check: check_machine::<M>,
            explain: explain_machine_entry::<M>,
            coverage: coverage_machine::<M>,
            orchestrate: orchestrate_machine::<M>,
            testgen: testgen_machine::<M>,
        }
    }
}

#[macro_export]
macro_rules! valid_models {
    ($($name:literal => $machine:ty),+ $(,)?) => {{
        vec![
            $(
                $crate::registry::RegisteredModel::for_machine::<$machine>($name)
            ),+
        ]
    }};
}

pub fn run_registry_cli(models: Vec<RegisteredModel>) {
    let models = models;
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_default();
    let remaining = args.collect::<Vec<_>>();

    match command.as_str() {
        "list" => cmd_list(&models, remaining),
        "inspect" => cmd_inspect(&models, remaining),
        "check" => cmd_check(&models, remaining),
        "explain" => cmd_explain(&models, remaining),
        "coverage" => cmd_coverage(&models, remaining),
        "orchestrate" => cmd_orchestrate(&models, remaining),
        "testgen" => cmd_testgen(&models, remaining),
        _ => usage_exit(),
    }
}

fn cmd_list(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args);
    if parsed.json {
        println!(
            "{{\"models\":[{}]}}",
            models
                .iter()
                .map(|model| format!("\"{}\"", model.name))
                .collect::<Vec<_>>()
                .join(",")
        );
    } else {
        for model in models {
            println!("{}", model.name);
        }
    }
}

fn cmd_inspect(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args);
    let model = find_model(models, parsed.model.as_deref());
    let response = (model.inspect)("registry-inspect");
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

fn cmd_check(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args);
    let model = find_model(models, parsed.model.as_deref());
    let outcome = (model.check)("registry-check");
    if parsed.json {
        println!("{}", render_outcome_json(model.name, &outcome));
    } else {
        print!("{}", render_outcome_text(&outcome));
    }
    process::exit(match outcome {
        CheckOutcome::Completed(result) => match result.status {
            crate::engine::RunStatus::Pass => 0,
            crate::engine::RunStatus::Fail => 2,
            crate::engine::RunStatus::Unknown => 4,
        },
        CheckOutcome::Errored(_) => 3,
    });
}

fn cmd_explain(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args);
    let model = find_model(models, parsed.model.as_deref());
    match (model.explain)("registry-explain") {
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
            }
        }
        Err(message) => {
            if parsed.json {
                println!("{}", render_diagnostics_json(&[crate::support::diagnostics::Diagnostic::new(
                    crate::support::diagnostics::ErrorCode::SearchError,
                    crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                    message,
                )]));
            } else {
                eprintln!("{message}");
            }
            process::exit(3);
        }
    }
}

fn cmd_coverage(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args);
    let model = find_model(models, parsed.model.as_deref());
    let report = (model.coverage)();
    if parsed.json {
        println!("{}", render_coverage_json(&report));
    } else {
        println!("{}", render_coverage_text(&report));
    }
}

fn cmd_orchestrate(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args);
    let model = find_model(models, parsed.model.as_deref());
    let response = (model.orchestrate)("registry-orchestrate");
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

fn cmd_testgen(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args);
    let model = find_model(models, parsed.model.as_deref());
    let response = (model.testgen)("registry-testgen");
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

fn inspect_machine<M: VerifiedMachine>(request_id: &str) -> InspectResponse
{
    let state_fields = M::init_states()
        .first()
        .map(|state| state.snapshot().keys().cloned().collect())
        .unwrap_or_default();
    let actions = M::Action::all()
        .into_iter()
        .map(|action| action.action_id())
        .collect();
    InspectResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        status: "ok".to_string(),
        model_id: M::model_id().to_string(),
        state_fields,
        actions,
        properties: property_ids::<M>()
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

fn check_machine<M: VerifiedMachine>(request_id: &str) -> CheckOutcome {
    check_machine_outcome::<M>(request_id)
}

fn explain_machine_entry<M: VerifiedMachine>(request_id: &str) -> Result<ExplainResponse, String>
{
    explain_machine::<M>(request_id)
}

fn coverage_machine<M: VerifiedMachine>() -> CoverageReport {
    collect_machine_coverage::<M>()
}

fn orchestrate_machine<M: VerifiedMachine>(request_id: &str) -> OrchestrateResponse {
    let outcomes = check_machine_outcomes::<M>(request_id);
    let coverage = collect_machine_coverage::<M>();
    let runs = outcomes
        .into_iter()
        .map(|result| OrchestratedRunSummary {
            property_id: result.property_result.property_id,
            status: format!("{:?}", result.status),
            assurance_level: format!("{:?}", result.assurance_level),
            run_id: result.manifest.run_id,
        })
        .collect();
    OrchestrateResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        runs,
        aggregate_coverage: Some(coverage),
    }
}

fn testgen_machine<M: VerifiedMachine>(request_id: &str) -> TestgenResponse {
    let vectors = build_machine_test_vectors::<M>();
    TestgenResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        status: "ok".to_string(),
        vector_ids: vectors.iter().map(|vector| vector.vector_id.clone()).collect(),
        generated_files: vectors
            .iter()
            .map(crate::testgen::generated_test_output_path)
            .collect(),
    }
}

#[derive(Default)]
struct ParsedArgs {
    json: bool,
    model: Option<String>,
}

fn parse_args(args: Vec<String>) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    for arg in args {
        if arg == "--json" {
            parsed.json = true;
        } else if parsed.model.is_none() {
            parsed.model = Some(arg);
        } else {
            usage_exit();
        }
    }
    parsed
}

fn find_model<'a>(models: &'a [RegisteredModel], model_name: Option<&str>) -> &'a RegisteredModel {
    let Some(model_name) = model_name else {
        usage_exit();
    };
    models
        .iter()
        .find(|model| model.name == model_name)
        .unwrap_or_else(|| {
            eprintln!("unknown model `{model_name}`");
            process::exit(3);
        })
}

fn usage_exit() -> ! {
    eprintln!("usage: <registry-bin> <list|inspect|check|explain|coverage|orchestrate|testgen> [model] [--json]");
    process::exit(3);
}
