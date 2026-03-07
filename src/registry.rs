use std::{env, process};

use crate::{
    coverage::{render_coverage_json, render_coverage_text, CoverageReport},
    engine::CheckOutcome,
    evidence::{render_diagnostics_json, render_outcome_json, render_outcome_text},
    modeling::{
        build_machine_test_vectors_for_strategy, check_machine_outcome,
        check_machine_outcome_for_property, check_machine_outcomes, check_machine_with_adapter,
        collect_machine_coverage, explain_machine, lower_machine_model,
        machine_capability_report, property_ids, replay_machine_actions, ActionSpec, StateSpec,
        VerifiedMachine,
    },
    solver::AdapterConfig,
    testgen::{render_replay_json, write_generated_test_files, ReplayTarget},
};

use crate::api::{
    lint_from_inspect, render_inspect_json, render_inspect_text, render_lint_json,
    render_lint_text, ExplainResponse, InspectAction, InspectCapabilities, InspectProperty,
    InspectResponse, InspectStateField, InspectTransition, OrchestrateResponse,
    OrchestratedRunSummary, TestgenResponse,
};

pub struct RegisteredModel {
    pub name: &'static str,
    pub inspect: fn(&str) -> InspectResponse,
    pub check: fn(&str, Option<&str>, Option<&AdapterConfig>) -> Result<CheckOutcome, String>,
    pub explain: fn(&str) -> Result<ExplainResponse, String>,
    pub coverage: fn() -> CoverageReport,
    pub orchestrate: fn(&str, Option<&AdapterConfig>) -> Result<OrchestrateResponse, String>,
    pub testgen: fn(Option<&str>, &str) -> TestgenResponse,
    pub replay: fn(Option<&str>, &[String], Option<&str>) -> Result<String, String>,
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
            replay: replay_machine::<M>,
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
        "lint" => cmd_lint(&models, remaining),
        "check" => cmd_check(&models, remaining),
        "explain" => cmd_explain(&models, remaining),
        "coverage" => cmd_coverage(&models, remaining),
        "orchestrate" => cmd_orchestrate(&models, remaining),
        "testgen" => cmd_testgen(&models, remaining),
        "replay" => cmd_replay(&models, remaining),
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
        println!("{}", render_inspect_json(&response));
    } else {
        print!("{}", render_inspect_text(&response));
    }
}

fn cmd_lint(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args);
    let model = find_model(models, parsed.model.as_deref());
    let response = lint_from_inspect(&(model.inspect)("registry-lint"));
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

fn cmd_check(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args);
    let model = find_model(models, parsed.model.as_deref());
    let adapter = adapter_from_parsed_args(&parsed).unwrap_or_else(|message| {
        eprintln!("{message}");
        process::exit(3);
    });
    let outcome = (model.check)("registry-check", parsed.property_id.as_deref(), adapter.as_ref())
        .unwrap_or_else(|message| {
            eprintln!("{message}");
            process::exit(3);
        });
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
    let adapter = adapter_from_parsed_args(&parsed).unwrap_or_else(|message| {
        eprintln!("{message}");
        process::exit(3);
    });
    let response = (model.orchestrate)("registry-orchestrate", adapter.as_ref()).unwrap_or_else(
        |message| {
            eprintln!("{message}");
            process::exit(3);
        },
    );
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
    let response = (model.testgen)(
        parsed.property_id.as_deref(),
        parsed
            .strategy
            .as_deref()
            .unwrap_or("counterexample"),
    );
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

fn cmd_replay(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args);
    let model = find_model(models, parsed.model.as_deref());
    let response = (model.replay)(
        parsed.property_id.as_deref(),
        &parsed.actions,
        parsed.focus_action_id.as_deref(),
    )
    .unwrap_or_else(|message| {
        eprintln!("{message}");
        process::exit(3);
    });
    println!("{response}");
}

fn inspect_machine<M: VerifiedMachine>(request_id: &str) -> InspectResponse
{
    let state_field_details = M::State::state_fields()
        .into_iter()
        .map(|field| InspectStateField {
            name: field.name.to_string(),
            rust_type: field.rust_type.to_string(),
            range: field.range.map(str::to_string),
        })
        .collect::<Vec<_>>();
    let action_details = M::Action::action_descriptors()
        .into_iter()
        .map(|action| InspectAction {
            action_id: action.action_id.to_string(),
            reads: action.reads.iter().map(|item| item.to_string()).collect(),
            writes: action.writes.iter().map(|item| item.to_string()).collect(),
        })
        .collect::<Vec<_>>();
    let transition_details = crate::modeling::machine_transition_ir::<M>()
        .into_iter()
        .map(|transition| InspectTransition {
            action_id: transition.action_id.to_string(),
            guard: transition.guard.map(str::to_string),
            effect: transition.effect.map(str::to_string),
            reads: transition.reads.iter().map(|item| item.to_string()).collect(),
            writes: transition.writes.iter().map(|item| item.to_string()).collect(),
            path_tags: crate::modeling::decision_path_tags(
                &transition.path_tags,
                transition.action_id,
                transition.reads.iter().copied(),
                transition.writes.iter().copied(),
                transition.guard,
                transition.effect,
            ),
        })
        .collect::<Vec<_>>();
    let property_details = M::properties()
        .into_iter()
        .map(|property| InspectProperty {
            property_id: property.property_id.to_string(),
            kind: format!("{:?}", property.property_kind),
        })
        .collect::<Vec<_>>();
    let capabilities = machine_capability_report::<M>();
    InspectResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        status: "ok".to_string(),
        model_id: M::model_id().to_string(),
        machine_ir_ready: capabilities.ir_ready,
        machine_ir_error: capabilities.machine_ir_error.clone(),
        capabilities: InspectCapabilities {
            parse_ready: capabilities.parse_ready,
            explicit_ready: capabilities.explicit_ready,
            ir_ready: capabilities.ir_ready,
            solver_ready: capabilities.solver_ready,
            coverage_ready: capabilities.coverage_ready,
            explain_ready: capabilities.explain_ready,
            testgen_ready: capabilities.testgen_ready,
            reasons: capabilities.reasons.clone(),
        },
        state_fields: state_field_details.iter().map(|field| field.name.clone()).collect(),
        actions: action_details
            .iter()
            .map(|action| action.action_id.clone())
            .collect(),
        properties: property_ids::<M>()
            .into_iter()
            .map(str::to_string)
            .collect(),
        state_field_details,
        action_details,
        transition_details,
        property_details,
    }
}

fn check_machine<M: VerifiedMachine>(
    request_id: &str,
    property_id: Option<&str>,
    adapter: Option<&AdapterConfig>,
) -> Result<CheckOutcome, String> {
    match adapter {
        Some(adapter) => check_machine_with_adapter::<M>(request_id, property_id, adapter),
        None => Ok(match property_id {
            Some(property_id) => check_machine_outcome_for_property::<M>(request_id, property_id),
            None => check_machine_outcome::<M>(request_id),
        }),
    }
}

fn explain_machine_entry<M: VerifiedMachine>(request_id: &str) -> Result<ExplainResponse, String>
{
    explain_machine::<M>(request_id)
}

fn coverage_machine<M: VerifiedMachine>() -> CoverageReport {
    collect_machine_coverage::<M>()
}

fn orchestrate_machine<M: VerifiedMachine>(
    request_id: &str,
    adapter: Option<&AdapterConfig>,
) -> Result<OrchestrateResponse, String> {
    if let Some(adapter) = adapter {
        if !matches!(adapter, AdapterConfig::Explicit) {
            let model = lower_machine_model::<M>()?;
            let base_plan = valid::engine::RunPlan::default();
            let mut traces = Vec::new();
            let runs = valid::orchestrator::run_all_properties_with_backend(&model, &base_plan, adapter)
                .into_iter()
                .map(|run| match run.outcome {
                    CheckOutcome::Completed(result) => {
                        if let Some(trace) = result.trace.clone() {
                            traces.push(trace);
                        }
                        OrchestratedRunSummary {
                            property_id: run.property_id,
                            status: format!("{:?}", result.status),
                            assurance_level: format!("{:?}", result.assurance_level),
                            run_id: result.manifest.run_id,
                        }
                    }
                    CheckOutcome::Errored(error) => OrchestratedRunSummary {
                        property_id: run.property_id,
                        status: "ERROR".to_string(),
                        assurance_level: format!("{:?}", error.assurance_level),
                        run_id: error.manifest.run_id,
                    },
                })
                .collect();
            let aggregate_coverage = if traces.is_empty() {
                None
            } else {
                Some(valid::coverage::collect_coverage(&model, &traces))
            };
            return Ok(OrchestrateResponse {
                schema_version: "1.0.0".to_string(),
                request_id: request_id.to_string(),
                runs,
                aggregate_coverage,
            });
        }
    }
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
    Ok(OrchestrateResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        runs,
        aggregate_coverage: Some(coverage),
    })
}

fn testgen_machine<M: VerifiedMachine>(property_id: Option<&str>, request_id: &str) -> TestgenResponse {
    let mut vectors = build_machine_test_vectors_for_strategy::<M>(property_id, request_id);
    annotate_registry_replay_targets::<M>(property_id, &mut vectors);
    let generated_files = write_generated_test_files(&vectors).unwrap_or_else(|message| {
        eprintln!("{message}");
        process::exit(3);
    });
    TestgenResponse {
        schema_version: "1.0.0".to_string(),
        request_id: "registry-testgen".to_string(),
        status: "ok".to_string(),
        vector_ids: vectors.iter().map(|vector| vector.vector_id.clone()).collect(),
        generated_files,
    }
}

fn replay_machine<M: VerifiedMachine>(
    property_id: Option<&str>,
    action_ids: &[String],
    focus_action_id: Option<&str>,
) -> Result<String, String> {
    let (terminal_state, property_id, focus_action_enabled) =
        replay_machine_actions::<M>(property_id, action_ids, focus_action_id)?;
    Ok(render_replay_json(
        property_id,
        action_ids,
        &terminal_state,
        focus_action_id,
        focus_action_enabled,
    ))
}

fn annotate_registry_replay_targets<M: VerifiedMachine>(
    property_id: Option<&str>,
    vectors: &mut [crate::testgen::TestVector],
) {
    let Some(file) = env::var("VALID_REGISTRY_FILE").ok() else {
        return;
    };
    let manifest_path = env::var("VALID_REGISTRY_MANIFEST_PATH").ok();
    for vector in vectors {
        let mut args = Vec::new();
        if let Some(manifest_path) = &manifest_path {
            args.push("--manifest-path".to_string());
            args.push(manifest_path.clone());
        }
        args.push("--file".to_string());
        args.push(file.clone());
        args.push("replay".to_string());
        args.push(env::var("VALID_REGISTRY_MODEL_NAME").unwrap_or_else(|_| M::model_id().to_string()));
        let property_id = property_id.unwrap_or(vector.property_id.as_str());
        args.push(format!("--property={property_id}"));
        if let Some(action_id) = &vector.focus_action_id {
            args.push(format!("--focus-action={action_id}"));
        }
        if !vector.actions.is_empty() {
            args.push(format!(
                "--actions={}",
                vector
                    .actions
                    .iter()
                    .map(|step| step.action_id.clone())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        args.push("--json".to_string());
        vector.replay_target = Some(ReplayTarget {
            runner: "cargo-valid".to_string(),
            args,
        });
    }
}

#[derive(Default)]
struct ParsedArgs {
    json: bool,
    model: Option<String>,
    strategy: Option<String>,
    property_id: Option<String>,
    backend: Option<String>,
    solver_executable: Option<String>,
    solver_args: Vec<String>,
    actions: Vec<String>,
    focus_action_id: Option<String>,
}

fn parse_args(args: Vec<String>) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--json" {
            parsed.json = true;
        } else if let Some(value) = arg.strip_prefix("--strategy=") {
            parsed.strategy = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--property=") {
            parsed.property_id = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--backend=") {
            parsed.backend = Some(value.to_string());
        } else if arg == "--solver-exec" {
            parsed.solver_executable = Some(
                iter.next()
                    .unwrap_or_else(|| usage_exit()),
            );
        } else if arg == "--solver-arg" {
            parsed
                .solver_args
                .push(iter.next().unwrap_or_else(|| usage_exit()));
        } else if let Some(value) = arg.strip_prefix("--actions=") {
            parsed.actions = value
                .split(',')
                .filter(|item| !item.is_empty())
                .map(|item| item.to_string())
                .collect();
        } else if let Some(value) = arg.strip_prefix("--focus-action=") {
            parsed.focus_action_id = Some(value.to_string());
        } else if parsed.model.is_none() {
            parsed.model = Some(arg);
        } else {
            usage_exit();
        }
    }
    parsed
}

fn adapter_from_parsed_args(parsed: &ParsedArgs) -> Result<Option<AdapterConfig>, String> {
    match parsed.backend.as_deref() {
        None | Some("explicit") => Ok(None),
        Some("mock-bmc") => Ok(Some(AdapterConfig::MockBmc)),
        Some("smt-cvc5") => Ok(Some(AdapterConfig::SmtCvc5 {
            executable: parsed
                .solver_executable
                .clone()
                .ok_or_else(|| "solver_executable is required when backend=smt-cvc5".to_string())?,
            args: parsed.solver_args.clone(),
        })),
        Some("command") => Ok(Some(AdapterConfig::Command {
            backend_name: "command".to_string(),
            executable: parsed
                .solver_executable
                .clone()
                .ok_or_else(|| "solver_executable is required when backend=command".to_string())?,
            args: parsed.solver_args.clone(),
        })),
        Some(other) => Err(format!("unsupported backend `{other}`")),
    }
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
    eprintln!("usage: <registry-bin> <list|inspect|lint|check|explain|coverage|orchestrate|testgen|replay> [model] [--json] [--property=<id>] [--backend=<explicit|mock-bmc|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>] [--focus-action=<id>] [--actions=a,b,c] [--strategy=<counterexample|transition|witness|guard|boundary|path|random>]");
    process::exit(3);
}
