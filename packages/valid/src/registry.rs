use std::{
    env, fs,
    io::{self, Read},
    process::{self, Command},
};

use crate::{
    benchmark::{
        benchmark_check_outcomes, compare_benchmark_to_baseline, parse_benchmark_summary_json,
        render_benchmark_comparison_json, render_benchmark_comparison_text, render_benchmark_json,
        render_benchmark_text,
    },
    cli::{
        child_stream_to_json, detect_json_flag, detect_progress_json_flag, message_diagnostic,
        parse_batch_request, render_batch_response, render_cli_error_json, render_cli_warning_json,
        render_commands_json, render_commands_text, render_schema_json, usage_diagnostic,
        BatchResult, ExitCode, ProgressReporter, Surface,
    },
    contract::{
        build_lock_file, compare_snapshot, parse_lock_file, render_drift_json, render_lock_json,
        snapshot_model, write_lock_file, ContractSnapshot,
    },
    coverage::{render_coverage_json, render_coverage_text, CoverageReport},
    engine::CheckOutcome,
    evidence::{render_outcome_json, render_outcome_text},
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

const REGISTRY_USAGE: &str =
    "usage: <registry-bin> <models|inspect|graph|readiness|migrate|benchmark|verify|explain|coverage|orchestrate|generate-tests|replay|contract|commands|schema|batch> [model] [--json] [--progress=json] [--format=<mermaid|dot|svg|text|json>] [--view=<overview|logic>] [--property=<id>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>] [--focus-action=<id>] [--actions=a,b,c] [--strategy=<counterexample|transition|witness|guard|boundary|path|random>] [--repeat=<n>] [--baseline[=compare|record|ignore]] [--threshold-percent=<n>] [--write[=<path>]] [--check]";
const LIST_USAGE: &str = "usage: <registry-bin> list [--json]";
const INSPECT_USAGE: &str = "usage: <registry-bin> inspect <model> [--json] [--progress=json]";
const GRAPH_USAGE: &str = "usage: <registry-bin> graph <model> [--format=mermaid|dot|svg|text|json] [--view=<overview|logic>] [--json] [--progress=json]";
const LINT_USAGE: &str = "usage: <registry-bin> lint <model> [--json] [--progress=json]";
const BENCHMARK_USAGE: &str = "usage: <registry-bin> benchmark <model> [--json] [--progress=json] [--property=<id>] [--repeat=<n>] [--baseline[=compare|record|ignore]] [--threshold-percent=<n>] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>]";
const MIGRATE_USAGE: &str =
    "usage: <registry-bin> migrate <model> [--json] [--progress=json] [--write[=<path>]] [--check]";
const CHECK_USAGE: &str = "usage: <registry-bin> check <model> [--json] [--progress=json] [--property=<id>] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>]";
const EXPLAIN_USAGE: &str = "usage: <registry-bin> explain <model> [--json] [--progress=json]";
const COVERAGE_USAGE: &str = "usage: <registry-bin> coverage <model> [--json] [--progress=json]";
const ORCHESTRATE_USAGE: &str = "usage: <registry-bin> orchestrate <model> [--json] [--progress=json] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>]";
const TESTGEN_USAGE: &str = "usage: <registry-bin> testgen <model> [--json] [--progress=json] [--property=<id>] [--strategy=<...>]";
const REPLAY_USAGE: &str = "usage: <registry-bin> replay <model> [--json] [--progress=json] [--property=<id>] [--focus-action=<id>] [--actions=a,b,c]";
const CONTRACT_USAGE: &str =
    "usage: <registry-bin> contract <snapshot|lock|drift|check> [lock-file] [--json] [--progress=json]";
const SCHEMA_USAGE: &str = "usage: <registry-bin> schema <command>";
const BATCH_USAGE: &str = "usage: <registry-bin> batch [--json] [--progress=json] < batch.json";

pub struct RegisteredModel {
    pub name: &'static str,
    pub inspect: fn(&str) -> InspectResponse,
    pub check: fn(&str, Option<&str>, Option<&AdapterConfig>) -> Result<CheckOutcome, String>,
    pub explain: fn(&str) -> Result<ExplainResponse, String>,
    pub coverage: fn() -> CoverageReport,
    pub orchestrate: fn(&str, Option<&AdapterConfig>) -> Result<OrchestrateResponse, String>,
    pub testgen: fn(Option<&str>, &str, bool) -> TestgenResponse,
    pub replay: fn(Option<&str>, &[String], Option<&str>) -> Result<String, String>,
    pub contract_snapshot: fn() -> Result<ContractSnapshot, String>,
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
            contract_snapshot: contract_snapshot_machine::<M>,
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
    let args = env::args().skip(1).collect::<Vec<_>>();
    let json = detect_json_flag(&args) || args.iter().any(|arg| arg == "--format=json");
    let command = normalize_command(args.first().map(String::as_str).unwrap_or_default());
    let remaining = args.into_iter().skip(1).collect::<Vec<_>>();

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
        "contract" => cmd_contract(&models, remaining),
        "commands" => cmd_commands(remaining),
        "schema" => cmd_schema(remaining),
        "batch" => cmd_batch(remaining),
        "help" => usage_exit("registry", json, REGISTRY_USAGE),
        _ => usage_exit("registry", json, REGISTRY_USAGE),
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
    let parsed = parse_args(args, "list", LIST_USAGE);
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
    let parsed = parse_args(args, "inspect", INSPECT_USAGE);
    let progress = ProgressReporter::new("inspect", parsed.progress_json);
    progress.start(None);
    let model = find_model(
        "inspect",
        parsed.json,
        INSPECT_USAGE,
        models,
        parsed.model.as_deref(),
    );
    let response = (model.inspect)("registry-inspect");
    if parsed.json {
        println!("{}", render_inspect_json(&response));
    } else {
        print!("{}", render_inspect_text(&response));
    }
    progress.finish(ExitCode::Success);
}

fn cmd_graph(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args, "graph", GRAPH_USAGE);
    let json_output = parsed.json || matches!(parsed.format.as_deref(), Some("json"));
    let progress = ProgressReporter::new("graph", parsed.progress_json);
    progress.start(None);
    let model = find_model(
        "graph",
        json_output,
        GRAPH_USAGE,
        models,
        parsed.model.as_deref(),
    );
    let response = (model.inspect)("registry-graph");
    let view = GraphView::parse(parsed.view.as_deref());
    match parsed.format.as_deref().unwrap_or("mermaid") {
        "json" => println!("{}", render_inspect_json(&response)),
        "text" => print!("{}", render_inspect_text(&response)),
        "dot" => println!("{}", render_model_dot_with_view(&response, view)),
        "svg" => println!("{}", render_model_svg_with_view(&response, view)),
        _ => println!("{}", render_model_mermaid_with_view(&response, view)),
    }
    progress.finish(ExitCode::Success);
}

fn cmd_lint(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args, "lint", LINT_USAGE);
    let progress = ProgressReporter::new("lint", parsed.progress_json);
    progress.start(None);
    let model = find_model(
        "lint",
        parsed.json,
        LINT_USAGE,
        models,
        parsed.model.as_deref(),
    );
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
    let exit_code = if has_findings {
        ExitCode::Fail
    } else {
        ExitCode::Success
    };
    progress.finish(exit_code);
    process::exit(exit_code.code());
}

fn cmd_benchmark(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args, "benchmark", BENCHMARK_USAGE);
    let progress = ProgressReporter::new("benchmark", parsed.progress_json);
    let repeat_total = parsed.repeat.max(1);
    progress.start(Some(repeat_total));
    let model = find_model(
        "benchmark",
        parsed.json,
        BENCHMARK_USAGE,
        models,
        parsed.model.as_deref(),
    );
    let adapter = adapter_from_parsed_args(&parsed).unwrap_or_else(|message| {
        message_exit("benchmark", parsed.json, &message, Some(BENCHMARK_USAGE))
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
        |iteration| {
            progress.item_start(iteration, repeat_total, model.name);
            let outcome = (model.check)(
                "registry-benchmark",
                parsed.property_id.as_deref(),
                adapter.as_ref(),
            )
            .unwrap_or_else(|message| {
                message_exit("benchmark", parsed.json, &message, Some(BENCHMARK_USAGE))
            });
            progress.item_complete(
                iteration,
                repeat_total,
                model.name,
                ExitCode::from_check_outcome(&outcome).code(),
            );
            outcome
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
            parsed.json,
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
            ExitCode::Error
        } else if summary.fail_count > 0 || regression_detected {
            ExitCode::Fail
        } else if summary.unknown_count > 0 {
            ExitCode::Unknown
        } else {
            ExitCode::Success
        }
    } else if summary.error_count > 0 {
        ExitCode::Error
    } else if regression_detected {
        ExitCode::Fail
    } else {
        ExitCode::Success
    };
    progress.finish(exit_code);
    process::exit(exit_code.code());
}

fn cmd_migrate(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args, "migrate", MIGRATE_USAGE);
    let progress = ProgressReporter::new("migrate", parsed.progress_json);
    progress.start(None);
    let model = find_model(
        "migrate",
        parsed.json,
        MIGRATE_USAGE,
        models,
        parsed.model.as_deref(),
    );
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
            Some("already-declarative") => ExitCode::Success,
            Some("no-candidates") => ExitCode::Unknown,
            Some("candidate-complete") | Some("partial") => ExitCode::Fail,
            _ => ExitCode::Error,
        }
    } else if migration.snippets.is_empty() {
        ExitCode::Unknown
    } else {
        ExitCode::Success
    };
    progress.finish(exit_code);
    process::exit(exit_code.code());
}

fn cmd_check(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args, "check", CHECK_USAGE);
    let progress = ProgressReporter::new("check", parsed.progress_json);
    progress.start(None);
    let model = find_model(
        "check",
        parsed.json,
        CHECK_USAGE,
        models,
        parsed.model.as_deref(),
    );
    let inspect = (model.inspect)("registry-check-preflight");
    let adapter = adapter_from_parsed_args(&parsed)
        .unwrap_or_else(|message| message_exit("check", parsed.json, &message, Some(CHECK_USAGE)));
    if matches!(adapter, Some(AdapterConfig::Explicit) | None) {
        if let Some(warning) = explicit_analysis_warning(&inspect) {
            if parsed.json || parsed.progress_json {
                eprintln!("{}", render_cli_warning_json("check", &warning));
            } else {
                eprintln!("{warning}");
            }
        }
    }
    let outcome = (model.check)(
        "registry-check",
        parsed.property_id.as_deref(),
        adapter.as_ref(),
    )
    .unwrap_or_else(|message| message_exit("check", parsed.json, &message, Some(CHECK_USAGE)));
    if parsed.json {
        println!("{}", render_outcome_json(model.name, &outcome));
    } else {
        print!("{}", render_outcome_text(&outcome));
    }
    let exit_code = ExitCode::from_check_outcome(&outcome);
    progress.finish(exit_code);
    process::exit(exit_code.code());
}

fn cmd_explain(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args, "explain", EXPLAIN_USAGE);
    let progress = ProgressReporter::new("explain", parsed.progress_json);
    progress.start(None);
    let model = find_model(
        "explain",
        parsed.json,
        EXPLAIN_USAGE,
        models,
        parsed.model.as_deref(),
    );
    match (model.explain)("registry-explain") {
        Ok(response) => {
            if parsed.json {
                println!("{}", render_explain_json(&response));
            } else {
                print!("{}", render_explain_text(&response));
            }
            progress.finish(ExitCode::Success);
        }
        Err(message) => {
            message_exit("explain", parsed.json, &message, Some(EXPLAIN_USAGE));
        }
    }
}

fn cmd_coverage(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args, "coverage", COVERAGE_USAGE);
    let progress = ProgressReporter::new("coverage", parsed.progress_json);
    progress.start(None);
    let model = find_model(
        "coverage",
        parsed.json,
        COVERAGE_USAGE,
        models,
        parsed.model.as_deref(),
    );
    let report = (model.coverage)();
    if parsed.json {
        println!("{}", render_coverage_json(&report));
    } else {
        println!("{}", render_coverage_text(&report));
    }
    progress.finish(ExitCode::Success);
}

fn cmd_orchestrate(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args, "orchestrate", ORCHESTRATE_USAGE);
    let progress = ProgressReporter::new("orchestrate", parsed.progress_json);
    progress.start(None);
    let model = find_model(
        "orchestrate",
        parsed.json,
        ORCHESTRATE_USAGE,
        models,
        parsed.model.as_deref(),
    );
    let adapter = adapter_from_parsed_args(&parsed).unwrap_or_else(|message| {
        message_exit(
            "orchestrate",
            parsed.json,
            &message,
            Some(ORCHESTRATE_USAGE),
        )
    });
    let response =
        (model.orchestrate)("registry-orchestrate", adapter.as_ref()).unwrap_or_else(|message| {
            message_exit(
                "orchestrate",
                parsed.json,
                &message,
                Some(ORCHESTRATE_USAGE),
            )
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
    progress.finish(ExitCode::Success);
}

fn cmd_testgen(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args, "testgen", TESTGEN_USAGE);
    let progress = ProgressReporter::new("testgen", parsed.progress_json);
    progress.start(None);
    let model = find_model(
        "testgen",
        parsed.json,
        TESTGEN_USAGE,
        models,
        parsed.model.as_deref(),
    );
    let response = (model.testgen)(
        parsed.property_id.as_deref(),
        parsed.strategy.as_deref().unwrap_or("counterexample"),
        parsed.json,
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
    progress.finish(ExitCode::Success);
}

fn cmd_replay(models: &[RegisteredModel], args: Vec<String>) {
    let parsed = parse_args(args, "replay", REPLAY_USAGE);
    let progress = ProgressReporter::new("replay", parsed.progress_json);
    progress.start(None);
    let model = find_model(
        "replay",
        parsed.json,
        REPLAY_USAGE,
        models,
        parsed.model.as_deref(),
    );
    let response = (model.replay)(
        parsed.property_id.as_deref(),
        &parsed.actions,
        parsed.focus_action_id.as_deref(),
    )
    .unwrap_or_else(|message| message_exit("replay", parsed.json, &message, Some(REPLAY_USAGE)));
    println!("{response}");
    progress.finish(ExitCode::Success);
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
    json: bool,
) -> TestgenResponse {
    let mut vectors = build_machine_test_vectors_for_strategy::<M>(property_id, request_id);
    annotate_registry_replay_targets::<M>(property_id, &mut vectors);
    let generated_files = write_generated_test_files(&vectors)
        .unwrap_or_else(|message| message_exit("testgen", json, &message, Some(TESTGEN_USAGE)));
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

fn contract_snapshot_machine<M: VerifiedMachine>() -> Result<ContractSnapshot, String> {
    let model = lower_machine_model::<M>()?;
    Ok(snapshot_model(&model))
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
    progress_json: bool,
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

fn parse_args(args: Vec<String>, command: &str, usage: &str) -> ParsedArgs {
    let mut parsed = ParsedArgs::default();
    parsed.repeat = 3;
    parsed.json = detect_json_flag(&args) || args.iter().any(|arg| arg == "--format=json");
    parsed.progress_json = detect_progress_json_flag(&args);
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--json" {
            parsed.json = true;
        } else if arg == "--progress=json" {
            parsed.progress_json = true;
        } else if arg.starts_with("--progress=") {
            message_exit(
                command,
                parsed.json,
                "unsupported progress mode",
                Some(usage),
            );
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
            parsed.threshold_percent = Some(
                value
                    .parse()
                    .unwrap_or_else(|_| usage_exit(command, parsed.json, usage)),
            );
        } else if let Some(value) = arg.strip_prefix("--repeat=") {
            parsed.repeat = value
                .parse()
                .unwrap_or_else(|_| usage_exit(command, parsed.json, usage));
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
            parsed.solver_executable = Some(
                iter.next()
                    .unwrap_or_else(|| usage_exit(command, parsed.json, usage)),
            );
        } else if arg == "--solver-arg" {
            parsed.solver_args.push(
                iter.next()
                    .unwrap_or_else(|| usage_exit(command, parsed.json, usage)),
            );
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
            usage_exit(command, parsed.json, usage);
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

fn find_model<'a>(
    command: &str,
    json: bool,
    usage: &str,
    models: &'a [RegisteredModel],
    model_name: Option<&str>,
) -> &'a RegisteredModel {
    let Some(model_name) = model_name else {
        usage_exit(command, json, usage);
    };
    models
        .iter()
        .find(|model| model.name == model_name)
        .unwrap_or_else(|| {
            message_exit(
                command,
                json,
                &format!("unknown model `{model_name}`"),
                Some(usage),
            )
        })
}

fn registry_benchmark_baseline_outputs(
    summary_json: &str,
    baseline_id: &str,
    baseline_mode: &str,
    threshold_percent: u32,
    json: bool,
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
            message_exit(
                "benchmark",
                json,
                &format!("unsupported benchmark baseline mode `{other}`"),
                Some(BENCHMARK_USAGE),
            );
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

fn cmd_contract(models: &[RegisteredModel], args: Vec<String>) {
    let json = detect_json_flag(&args);
    reject_unsupported_progress_mode("contract", json, &args, CONTRACT_USAGE);
    let progress = ProgressReporter::from_args("contract", &args);
    let total = models.len();
    progress.start(Some(total));
    let positional: Vec<&str> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .map(|a| a.as_str())
        .collect();
    let sub = positional.first().copied().unwrap_or("snapshot");
    let lock_path = positional.get(1).map(|s| s.to_string());

    match sub {
        "snapshot" => {
            let mut snapshots = Vec::new();
            for (index, model) in models.iter().enumerate() {
                progress.item_start(index, total, model.name);
                let snapshot = (model.contract_snapshot)().unwrap_or_else(|message| {
                    message_exit(
                        "contract",
                        json,
                        &format!("contract snapshot failed for `{}`: {message}", model.name),
                        Some(CONTRACT_USAGE),
                    )
                });
                progress.item_complete(index, total, model.name, ExitCode::Success.code());
                snapshots.push(snapshot);
            }
            if json {
                let body = snapshots
                    .iter()
                    .map(|s| {
                        format!(
                            "{{\"model_id\":\"{}\",\"contract_hash\":\"{}\"}}",
                            s.model_id, s.contract_hash
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                println!("{{\"snapshots\":[{body}]}}");
            } else {
                for snapshot in &snapshots {
                    println!("{}: {}", snapshot.model_id, snapshot.contract_hash);
                }
            }
            progress.finish(ExitCode::Success);
        }
        "lock" => {
            let output = lock_path.unwrap_or_else(|| "valid.lock.json".to_string());
            let mut snapshots = Vec::new();
            for (index, model) in models.iter().enumerate() {
                progress.item_start(index, total, model.name);
                let snapshot = (model.contract_snapshot)().unwrap_or_else(|message| {
                    message_exit(
                        "contract",
                        json,
                        &format!("contract snapshot failed for `{}`: {message}", model.name),
                        Some(CONTRACT_USAGE),
                    )
                });
                progress.item_complete(index, total, model.name, ExitCode::Success.code());
                snapshots.push(snapshot);
            }
            let lock = build_lock_file(snapshots);
            write_lock_file(&output, &lock).unwrap_or_else(|err| {
                message_exit("contract", json, &err.to_string(), Some(CONTRACT_USAGE))
            });
            if json {
                println!("{}", render_lock_json(&lock));
            } else {
                println!("lock written: {output}");
            }
            progress.finish(ExitCode::Success);
        }
        "drift" | "check" => {
            let lock_file = lock_path.unwrap_or_else(|| "valid.lock.json".to_string());
            let lock_body = fs::read_to_string(&lock_file).unwrap_or_else(|err| {
                message_exit(
                    "contract",
                    json,
                    &format!("failed to read lock file `{lock_file}`: {err}"),
                    Some(CONTRACT_USAGE),
                )
            });
            let lock = parse_lock_file(&lock_body).unwrap_or_else(|err| {
                message_exit(
                    "contract",
                    json,
                    &format!("failed to parse lock file: {err}"),
                    Some(CONTRACT_USAGE),
                )
            });
            let mut has_drift = false;
            let mut reports = Vec::new();
            for (index, model) in models.iter().enumerate() {
                progress.item_start(index, total, model.name);
                let snapshot = (model.contract_snapshot)().unwrap_or_else(|message| {
                    message_exit(
                        "contract",
                        json,
                        &format!("contract snapshot failed for `{}`: {message}", model.name),
                        Some(CONTRACT_USAGE),
                    )
                });
                let expected = lock
                    .entries
                    .iter()
                    .find(|entry| entry.model_id == snapshot.model_id);
                let Some(expected) = expected else {
                    let report = format!(
                        "{{\"status\":\"missing\",\"contract_id\":\"{}\"}}",
                        snapshot.model_id
                    );
                    if !json {
                        println!("{}: missing from lock file", snapshot.model_id);
                    }
                    reports.push(report);
                    has_drift = true;
                    progress.item_complete(index, total, model.name, ExitCode::Fail.code());
                    continue;
                };
                let drift = compare_snapshot(expected, &snapshot);
                let item_exit = if drift.status != "unchanged" {
                    has_drift = true;
                    ExitCode::Fail
                } else {
                    ExitCode::Success
                };
                if json {
                    reports.push(render_drift_json(&drift));
                } else {
                    println!("{}: {}", drift.contract_id, drift.status);
                    for change in &drift.changes {
                        println!("  changed: {change}");
                    }
                }
                progress.item_complete(index, total, model.name, item_exit.code());
            }
            if json {
                println!("{{\"reports\":[{}]}}", reports.join(","));
            }
            let exit_code = if has_drift {
                ExitCode::Fail
            } else {
                ExitCode::Success
            };
            progress.finish(exit_code);
            process::exit(exit_code.code());
        }
        _ => {
            usage_exit("contract", json, CONTRACT_USAGE);
        }
    }
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

fn reject_unsupported_progress_mode(command: &str, json: bool, args: &[String], usage: &str) {
    if args
        .iter()
        .any(|arg| arg.starts_with("--progress=") && arg != "--progress=json")
    {
        message_exit(command, json, "unsupported progress mode", Some(usage));
    }
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

fn cmd_commands(args: Vec<String>) {
    let json = detect_json_flag(&args);
    reject_unsupported_progress_mode("commands", json, &args, REGISTRY_USAGE);
    if detect_json_flag(&args) {
        println!("{}", render_commands_json(Surface::Registry));
    } else {
        println!("{}", render_commands_text(Surface::Registry));
    }
}

fn cmd_schema(args: Vec<String>) {
    let json = detect_json_flag(&args);
    reject_unsupported_progress_mode("schema", json, &args, SCHEMA_USAGE);
    let command = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| usage_exit("schema", true, SCHEMA_USAGE));
    match render_schema_json(Surface::Registry, &normalize_command(&command)) {
        Ok(body) => println!("{body}"),
        Err(message) => message_exit("schema", true, &message, Some(SCHEMA_USAGE)),
    }
}

fn cmd_batch(args: Vec<String>) {
    let json = detect_json_flag(&args);
    reject_unsupported_progress_mode("batch", json, &args, BATCH_USAGE);
    let progress = ProgressReporter::from_args("batch", &args);
    let mut stdin = String::new();
    io::stdin()
        .read_to_string(&mut stdin)
        .unwrap_or_else(|err| {
            message_exit(
                "batch",
                json,
                &format!("failed to read stdin: {err}"),
                Some(BATCH_USAGE),
            )
        });
    let request = parse_batch_request(&stdin)
        .unwrap_or_else(|message| message_exit("batch", true, &message, Some(BATCH_USAGE)));
    let total = request.operations.len();
    progress.start(Some(total));
    let current_exe = env::current_exe().unwrap_or_else(|err| {
        message_exit(
            "batch",
            json,
            &format!("failed to resolve current executable: {err}"),
            Some(BATCH_USAGE),
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
            if normalize_command(&operation.command) == "graph" {
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
                    Some(BATCH_USAGE),
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
