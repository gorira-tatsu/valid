use std::{env, fs, process};

use crate::{
    benchmark::{
        benchmark_check_outcomes, compare_benchmark_to_baseline, parse_benchmark_summary_json,
        render_benchmark_comparison_json, render_benchmark_comparison_text, render_benchmark_json,
        render_benchmark_text,
    },
    coverage::{render_coverage_json, render_coverage_text, CoverageReport},
    engine::CheckOutcome,
    evidence::{render_diagnostics_json, render_outcome_json, render_outcome_text},
    modeling::{
        build_machine_test_vectors_for_strategy, check_machine_outcome,
        check_machine_outcome_for_property, check_machine_outcomes, check_machine_with_adapter,
        collect_machine_coverage, explain_machine, lower_machine_model, machine_capability_report,
        property_ids, replay_machine_actions, ActionSpec, StateSpec, VerifiedMachine,
    },
    reporter::{
        render_model_dot_with_view, render_model_mermaid_with_view, render_model_svg_with_view,
        GraphView,
    },
    solver::AdapterConfig,
    support::{artifact::benchmark_baseline_path, hash::stable_hash_hex, io::write_text_file},
    testgen::{render_replay_json, write_generated_test_files, ReplayTarget},
};

use crate::api::{
    explicit_analysis_warning, lint_from_inspect, migration_from_inspect, render_explain_json,
    render_explain_text, render_inspect_json, render_inspect_text, render_lint_json,
    render_lint_text, render_migration_json, render_migration_text, ExplainResponse, InspectAction,
    InspectCapabilities, InspectProperty, InspectResponse, InspectStateField, InspectTransition,
    InspectTransitionUpdate, OrchestrateResponse, OrchestratedRunSummary, TestgenResponse,
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
    let command = normalize_command(&args.next().unwrap_or_default());
    let remaining = args.collect::<Vec<_>>();

    match command.as_str() {
        "list" => cmd_list(&models, remaining),
        "inspect" => cmd_inspect(&models, remaining),
        "graph" => cmd_graph(&models, remaining),
        "lint" => cmd_lint(&models, remaining),
        "benchmark" => cmd_benchmark(&models, remaining),
        "migrate" => cmd_migrate(&models, remaining),
        "check" => cmd_check(&models, remaining),
        "explain" => cmd_explain(&models, remaining),
        "coverage" => cmd_coverage(&models, remaining),
        "orchestrate" => cmd_orchestrate(&models, remaining),
        "testgen" => cmd_testgen(&models, remaining),
        "replay" => cmd_replay(&models, remaining),
        "help" => usage_exit(),
        _ => usage_exit(),
    }
}

fn normalize_command(command: &str) -> String {
    match command {
        "models" => "list",
        "diagram" => "graph",
        "readiness" => "lint",
        "verify" => "check",
        "bench" => "benchmark",
        "generate-tests" => "testgen",
        other => other,
    }
    .to_string()
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

fn cmd_graph(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args);
    let model = find_model(models, parsed.model.as_deref());
    let response = (model.inspect)("registry-graph");
    let view = GraphView::parse(parsed.view.as_deref());
    match parsed.format.as_deref().unwrap_or("mermaid") {
        "json" => println!("{}", render_inspect_json(&response)),
        "text" => print!("{}", render_inspect_text(&response)),
        "dot" => println!("{}", render_model_dot_with_view(&response, view)),
        "svg" => println!("{}", render_model_svg_with_view(&response, view)),
        _ => println!("{}", render_model_mermaid_with_view(&response, view)),
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

fn cmd_benchmark(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args);
    let model = find_model(models, parsed.model.as_deref());
    let adapter = adapter_from_parsed_args(&parsed).unwrap_or_else(|message| {
        eprintln!("{message}");
        process::exit(3);
    });
    let backend_label = parsed
        .backend
        .clone()
        .unwrap_or_else(|| "explicit".to_string());
    let summary = benchmark_check_outcomes(
        "registry-benchmark",
        model.name,
        &backend_label,
        parsed.property_id.as_deref(),
        parsed.repeat,
        |_| {
            (model.check)(
                "registry-benchmark",
                parsed.property_id.as_deref(),
                adapter.as_ref(),
            )
            .unwrap_or_else(|message| {
                eprintln!("{message}");
                process::exit(3);
            })
        },
    );
    let summary_json = render_benchmark_json(&summary);
    let baseline_id = format!(
        "baseline-{}",
        stable_hash_hex(&format!(
            "{}:{}:{}",
            model.name,
            backend_label,
            parsed.property_id.as_deref().unwrap_or("")
        ))
        .replace("sha256:", "")
    );
    let (comparison_json, comparison_text, regression_detected) =
        registry_benchmark_baseline_outputs(
            &summary_json,
            &baseline_id,
            parsed.baseline_mode.as_deref().unwrap_or("compare"),
            parsed.threshold_percent.unwrap_or(25),
        );
    if parsed.json {
        println!(
            "{{\"summary\":{},\"baseline\":{}}}",
            summary_json,
            comparison_json.unwrap_or_else(|| "null".to_string())
        );
    } else {
        print!("{}", render_benchmark_text(&summary));
        if let Some(text) = comparison_text {
            print!("{text}");
        }
    }
    let baseline_mode = parsed.baseline_mode.as_deref().unwrap_or("compare");
    let exit_code = if baseline_mode == "ignore" {
        if summary.error_count > 0 {
            3
        } else if summary.fail_count > 0 {
            2
        } else if summary.unknown_count > 0 {
            4
        } else if regression_detected {
            5
        } else {
            0
        }
    } else if summary.error_count > 0 {
        3
    } else if regression_detected {
        5
    } else {
        0
    };
    process::exit(exit_code);
}

fn cmd_migrate(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args);
    let model = find_model(models, parsed.model.as_deref());
    let inspect = (model.inspect)("registry-migrate");
    let lint = lint_from_inspect(&inspect);
    let migration = migration_from_inspect(&inspect, &lint, parsed.check);
    let json_body = render_migration_json(&migration);
    let text_body = render_migration_text(&migration);
    let written_path = parsed
        .write_path
        .as_ref()
        .map(|value| registry_migration_output_path(model.name, value))
        .and_then(|path| write_text_file(&path, &text_body).ok().map(|_| path));
    if parsed.json {
        if let Some(path) = written_path {
            println!(
                "{{\"written\":\"{}\",\"migration\":{}}}",
                path.replace('\\', "\\\\"),
                json_body
            );
        } else {
            println!("{json_body}");
        }
    } else {
        print!("{text_body}");
        if let Some(path) = written_path {
            println!("written: {path}");
        }
    }
    let exit_code = if parsed.check {
        match migration.check.as_ref().map(|check| check.status.as_str()) {
            Some("already-declarative") => 0,
            Some("no-candidates") => 2,
            Some("candidate-complete") | Some("partial") => 6,
            _ => 3,
        }
    } else if migration.snippets.is_empty() {
        2
    } else {
        0
    };
    process::exit(exit_code);
}

fn cmd_check(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args);
    let model = find_model(models, parsed.model.as_deref());
    let inspect = (model.inspect)("registry-check-preflight");
    let adapter = adapter_from_parsed_args(&parsed).unwrap_or_else(|message| {
        eprintln!("{message}");
        process::exit(3);
    });
    if matches!(adapter, Some(AdapterConfig::Explicit) | None) {
        if let Some(warning) = explicit_analysis_warning(&inspect) {
            eprintln!("{warning}");
        }
    }
    let outcome = (model.check)(
        "registry-check",
        parsed.property_id.as_deref(),
        adapter.as_ref(),
    )
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
                println!("{}", render_explain_json(&response));
            } else {
                print!("{}", render_explain_text(&response));
            }
        }
        Err(message) => {
            if parsed.json {
                println!(
                    "{}",
                    render_diagnostics_json(&[crate::support::diagnostics::Diagnostic::new(
                        crate::support::diagnostics::ErrorCode::SearchError,
                        crate::support::diagnostics::DiagnosticSegment::EngineSearch,
                        message,
                    )])
                );
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
    let response =
        (model.orchestrate)("registry-orchestrate", adapter.as_ref()).unwrap_or_else(|message| {
            eprintln!("{message}");
            process::exit(3);
        });
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
        parsed.strategy.as_deref().unwrap_or("counterexample"),
    );
    if parsed.json {
        println!(
            "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"vector_ids\":[{}],\"vectors\":[{}],\"generated_files\":[{}]}}",
            response.schema_version,
            response.request_id,
            response.status,
            response.vector_ids.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
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
            response.generated_files.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(",")
        );
    } else {
        println!("vector_ids: {}", response.vector_ids.join(", "));
        if !response.vectors.is_empty() {
            println!("vectors:");
            for vector in &response.vectors {
                println!(
                    "- {} strictness={} derivation={} source={} strategy={}",
                    vector.vector_id,
                    vector.strictness,
                    vector.derivation,
                    vector.source_kind,
                    vector.strategy
                );
            }
        }
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

fn inspect_machine<M: VerifiedMachine>(request_id: &str) -> InspectResponse {
    let state_field_details = M::State::state_fields()
        .into_iter()
        .map(|field| InspectStateField {
            name: field.name.to_string(),
            rust_type: field.rust_type.to_string(),
            range: field.range.map(str::to_string),
            variants: if let Some(variants) = field.variants {
                variants.into_iter().map(str::to_string).collect()
            } else if field.is_relation {
                vec![
                    format!(
                        "left:{}",
                        field
                            .relation_left_variants
                            .unwrap_or_default()
                            .into_iter()
                            .collect::<Vec<_>>()
                            .join("|")
                    ),
                    format!(
                        "right:{}",
                        field
                            .relation_right_variants
                            .unwrap_or_default()
                            .into_iter()
                            .collect::<Vec<_>>()
                            .join("|")
                    ),
                ]
            } else if field.is_map {
                vec![
                    format!(
                        "keys:{}",
                        field
                            .map_key_variants
                            .unwrap_or_default()
                            .into_iter()
                            .collect::<Vec<_>>()
                            .join("|")
                    ),
                    format!(
                        "values:{}",
                        field
                            .map_value_variants
                            .unwrap_or_default()
                            .into_iter()
                            .collect::<Vec<_>>()
                            .join("|")
                    ),
                ]
            } else {
                Vec::new()
            },
            is_set: field.is_set,
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
            reads: transition
                .reads
                .iter()
                .map(|item| item.to_string())
                .collect(),
            writes: transition
                .writes
                .iter()
                .map(|item| item.to_string())
                .collect(),
            path_tags: crate::modeling::decision_path_tags(
                &transition.path_tags,
                transition.action_id,
                transition.reads.iter().copied(),
                transition.writes.iter().copied(),
                transition.guard,
                transition.effect,
            ),
            updates: transition
                .updates
                .iter()
                .filter_map(|update| {
                    update.expr.map(|expr| InspectTransitionUpdate {
                        field: update.field.to_string(),
                        expr: expr.to_string(),
                    })
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let property_details = M::properties()
        .into_iter()
        .map(|property| InspectProperty {
            property_id: property.property_id.to_string(),
            kind: format!("{:?}", property.property_kind),
            expr: property.expr.map(str::to_string),
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
        state_fields: state_field_details
            .iter()
            .map(|field| field.name.clone())
            .collect(),
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

fn explain_machine_entry<M: VerifiedMachine>(request_id: &str) -> Result<ExplainResponse, String> {
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
            let runs =
                valid::orchestrator::run_all_properties_with_backend(&model, &base_plan, adapter)
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

fn testgen_machine<M: VerifiedMachine>(
    property_id: Option<&str>,
    request_id: &str,
) -> TestgenResponse {
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
        vector_ids: vectors
            .iter()
            .map(|vector| vector.vector_id.clone())
            .collect(),
        vectors: vectors
            .iter()
            .map(|vector| crate::api::TestgenVectorSummary {
                vector_id: vector.vector_id.clone(),
                strictness: vector.strictness.clone(),
                derivation: vector.derivation.clone(),
                source_kind: vector.source_kind.clone(),
                strategy: vector.strategy.clone(),
            })
            .collect(),
        generated_files,
    }
}

fn replay_machine<M: VerifiedMachine>(
    property_id: Option<&str>,
    action_ids: &[String],
    focus_action_id: Option<&str>,
) -> Result<String, String> {
    let (terminal_state, property_id, focus_action_enabled, property_holds, path_tags) =
        replay_machine_actions::<M>(property_id, action_ids, focus_action_id)?;
    Ok(render_replay_json(
        property_id,
        action_ids,
        &terminal_state,
        focus_action_id,
        focus_action_enabled,
        Some(property_holds),
        &path_tags,
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
        args.push(
            env::var("VALID_REGISTRY_MODEL_NAME").unwrap_or_else(|_| M::model_id().to_string()),
        );
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
    repeat: usize,
    baseline_mode: Option<String>,
    threshold_percent: Option<u32>,
    strategy: Option<String>,
    format: Option<String>,
    view: Option<String>,
    property_id: Option<String>,
    backend: Option<String>,
    solver_executable: Option<String>,
    solver_args: Vec<String>,
    actions: Vec<String>,
    focus_action_id: Option<String>,
    write_path: Option<String>,
    check: bool,
}

fn parse_args(args: Vec<String>) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    parsed.repeat = 3;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--json" {
            parsed.json = true;
        } else if arg == "--check" {
            parsed.check = true;
        } else if arg == "--write" {
            parsed.write_path = Some(String::new());
        } else if let Some(value) = arg.strip_prefix("--write=") {
            parsed.write_path = Some(value.to_string());
        } else if arg == "--baseline" {
            parsed.baseline_mode = Some("compare".to_string());
        } else if let Some(value) = arg.strip_prefix("--baseline=") {
            parsed.baseline_mode = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--threshold-percent=") {
            parsed.threshold_percent = Some(value.parse().unwrap_or_else(|_| usage_exit()));
        } else if let Some(value) = arg.strip_prefix("--repeat=") {
            parsed.repeat = value.parse().unwrap_or_else(|_| usage_exit());
        } else if let Some(value) = arg.strip_prefix("--format=") {
            parsed.format = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--view=") {
            parsed.view = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--strategy=") {
            parsed.strategy = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--property=") {
            parsed.property_id = Some(value.to_string());
        } else if let Some(value) = arg.strip_prefix("--backend=") {
            parsed.backend = Some(value.to_string());
        } else if arg == "--solver-exec" {
            parsed.solver_executable = Some(iter.next().unwrap_or_else(|| usage_exit()));
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
        Some("sat-varisat") => Ok(Some(AdapterConfig::SatVarisat)),
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

fn registry_benchmark_baseline_outputs(
    summary_json: &str,
    baseline_id: &str,
    baseline_mode: &str,
    threshold_percent: u32,
) -> (Option<String>, Option<String>, bool) {
    match baseline_mode {
        "ignore" => (None, None, false),
        "record" => {
            let path = benchmark_baseline_path(baseline_id);
            let comparison = if write_text_file(&path, summary_json).is_ok() {
                Some(format!(
                    "{{\"status\":\"recorded\",\"baseline_path\":\"{}\",\"threshold_percent\":{},\"regressions\":[]}}",
                    path.replace('\\', "\\\\"),
                    threshold_percent
                ))
            } else {
                None
            };
            let text = comparison.as_ref().map(|_| {
                format!(
                    "baseline_path: {}\nbaseline_threshold_percent: {}\nbaseline_status: recorded\nbaseline_regressions: none\n",
                    path, threshold_percent
                )
            });
            (comparison, text, false)
        }
        "compare" => {
            let path = benchmark_baseline_path(baseline_id);
            let Ok(body) = fs::read_to_string(&path) else {
                let json = format!(
                    "{{\"status\":\"missing\",\"baseline_path\":\"{}\",\"threshold_percent\":{},\"regressions\":[]}}",
                    path.replace('\\', "\\\\"),
                    threshold_percent
                );
                let text = format!(
                    "baseline_path: {}\nbaseline_threshold_percent: {}\nbaseline_status: missing\nbaseline_regressions: none\n",
                    path, threshold_percent
                );
                return (Some(json), Some(text), false);
            };
            let current = match parse_benchmark_summary_json(summary_json) {
                Ok(summary) => summary,
                Err(_) => return (None, None, false),
            };
            let baseline = match parse_benchmark_summary_json(&body) {
                Ok(summary) => summary,
                Err(message) => {
                    let json = format!(
                        "{{\"status\":\"invalid\",\"baseline_path\":\"{}\",\"threshold_percent\":{},\"regressions\":[\"{}\"]}}",
                        path.replace('\\', "\\\\"),
                        threshold_percent,
                        message.replace('\\', "\\\\").replace('"', "\\\"")
                    );
                    let text = format!(
                        "baseline_path: {}\nbaseline_threshold_percent: {}\nbaseline_status: invalid\nbaseline_regressions:\n- {}\n",
                        path, threshold_percent, message
                    );
                    return (Some(json), Some(text), false);
                }
            };
            let comparison =
                compare_benchmark_to_baseline(&current, &path, &baseline, threshold_percent);
            let regression = comparison.status == "regressed";
            (
                Some(render_benchmark_comparison_json(&comparison)),
                Some(render_benchmark_comparison_text(&comparison)),
                regression,
            )
        }
        other => {
            eprintln!("unsupported benchmark baseline mode `{other}`");
            process::exit(3);
        }
    }
}

fn registry_migration_output_path(model: &str, requested: &str) -> String {
    if !requested.trim().is_empty() {
        return requested.to_string();
    }
    let root = env::var("VALID_ARTIFACTS_DIR").unwrap_or_else(|_| "artifacts".to_string());
    format!(
        "{}/migrations/{}.snippet.rs",
        root.trim_end_matches('/'),
        model
    )
}

fn usage_exit() -> ! {
    eprintln!("usage: <registry-bin> <models|inspect|graph|readiness|migrate|benchmark|verify|explain|coverage|orchestrate|generate-tests|replay> [model] [--json] [--format=<mermaid|dot|svg|text|json>] [--view=<overview|logic>] [--property=<id>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>] [--focus-action=<id>] [--actions=a,b,c] [--strategy=<counterexample|transition|witness|guard|boundary|path|random>] [--repeat=<n>] [--baseline[=compare|record|ignore]] [--threshold-percent=<n>] [--write[=<path>]] [--check]");
    process::exit(3);
}
