use std::{
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{self, Command},
};

use clap::Parser;
use serde_json::{json, Value};
use valid::{
    api::{
        check_source, explain_source, explicit_analysis_warning, inspect_source, lint_source,
        migration_from_inspect, orchestrate_source, render_explain_json, render_explain_text,
        render_inspect_json, render_inspect_text, render_lint_json, render_lint_text,
        render_migration_json, render_migration_text, testgen_source, CheckRequest, InspectRequest,
        OrchestrateRequest, TestgenRequest,
    },
    benchmark::{
        benchmark_check_outcomes, compare_benchmark_to_baseline, parse_benchmark_summary_json,
        render_benchmark_comparison_json, render_benchmark_comparison_text, render_benchmark_json,
        render_benchmark_text,
    },
    bundled_models::{coverage_bundled_model, list_bundled_models},
    cli::{
        child_stream_to_json, detect_json_flag, detect_progress_json_flag, message_diagnostic,
        parse_batch_request, render_batch_response, render_cli_error_json, render_commands_json,
        render_commands_text, render_schema_json, usage_diagnostic, BatchResult, ExitCode,
        ProgressReporter, Surface,
    },
    coverage::{render_coverage_json, render_coverage_text},
    doc::{
        check_doc, default_doc_path, generate_doc, render_doc_check_json, render_doc_check_text,
        render_doc_json, render_doc_text, write_doc,
    },
    evidence::{render_outcome_json, render_outcome_text},
    external_registry::{
        discover_external_project as discover_external_registry_project,
        project_root as external_project_root,
        resolve_external_target as resolve_external_registry_target, ExternalTarget,
        ExternalTargetKind, ExternalTargetOptions,
    },
    handoff::{
        check_handoff, default_handoff_path, generate_handoff, render_handoff_check_json,
        render_handoff_check_text, render_handoff_json, render_handoff_text, write_handoff,
        HandoffInputs,
    },
    project::{
        render_bootstrap_ai_readme, render_bootstrap_claude_code_config,
        render_bootstrap_claude_desktop_config, render_bootstrap_codex_config,
        render_project_config_template, render_registry_source_template, verification_policy,
        ProjectConfig,
    },
    reporter::{
        build_failure_graph_slice, render_model_dot_failure, render_model_dot_with_view,
        render_model_mermaid_failure, render_model_mermaid_with_view, render_model_svg_failure,
        render_model_svg_with_view, render_model_text_failure, render_model_text_with_view,
        GraphView,
    },
    support::{
        artifact::{benchmark_baseline_path, benchmark_report_path},
        artifact_index::{
            load_artifact_index, load_run_history, record_artifact, render_artifact_inventory_json,
            render_artifact_inventory_text, ArtifactRecord,
        },
        hash::stable_hash_hex,
        io::write_text_file,
    },
};

#[derive(Parser, Debug)]
#[command(
    name = "cargo valid",
    disable_help_flag = true,
    disable_version_flag = true,
    trailing_var_arg = true
)]
struct CargoValidCli {
    #[arg(long = "manifest-path")]
    manifest_path: Option<String>,
    #[arg(long)]
    registry: Option<String>,
    #[arg(long)]
    file: Option<String>,
    #[arg(long)]
    example: Option<String>,
    #[arg(long)]
    bin: Option<String>,
    command: Option<String>,
    #[arg(allow_hyphen_values = true)]
    rest: Vec<String>,
}

fn main() {
    let raw_args = env::args().skip(1).collect::<Vec<_>>();
    let parsed = parse_cli(raw_args);
    match parsed.command.as_str() {
        "commands" => cmd_commands(parsed.json),
        "schema" => cmd_schema(&parsed),
        "batch" => cmd_batch(&parsed),
        _ => {}
    }
    let parsed = maybe_auto_discover_external(parsed);
    if parsed.command == "init" {
        cmd_init(&parsed);
    }
    if parsed.command == "artifacts" {
        cmd_artifacts(&parsed);
    }
    if parsed.command == "clean" {
        cmd_clean(&parsed);
    }
    if parsed.manifest_path.is_none()
        && parsed.example.is_none()
        && parsed.bin.is_none()
        && parsed.file.is_none()
        && !parsed.extra_positionals.is_empty()
    {
        usage_exit(&primary_usage());
    }
    if parsed.manifest_path.is_some()
        || parsed.example.is_some()
        || parsed.bin.is_some()
        || parsed.file.is_some()
    {
        run_external_registry(parsed);
    }
    if !internal_bundled_mode_enabled() {
        usage_exit(
            "project-first mode expects valid.toml or --registry/--file/--example/--bin; bundled models are internal fixtures",
        );
    }

    let local_args = ParsedArgs {
        json: parsed.json,
        progress_json: parsed.progress_json,
        model: parsed.model,
        seed: parsed.seed,
        repeat: parsed.repeat,
        baseline_mode: parsed.baseline_mode,
        threshold_percent: parsed.threshold_percent,
        strategy: parsed.strategy,
        format: parsed.format,
        view: parsed.view,
        property_id: parsed.property_id,
        backend: parsed.backend,
        solver_executable: parsed.solver_executable,
        solver_args: parsed.solver_args,
        actions: parsed.actions,
        focus_action_id: parsed.focus_action_id,
        suite_models: parsed.suite_models,
        critical: parsed.critical,
        suite_name: parsed.suite_name,
        project_config: parsed.project_config,
        write_path: parsed.write_path,
        check: parsed.check,
    };

    match parsed.command.as_str() {
        "list" => cmd_list(local_args),
        "inspect" => cmd_inspect(local_args),
        "graph" => cmd_graph(local_args),
        "doc" => cmd_doc(local_args),
        "handoff" => cmd_handoff(local_args),
        "lint" => cmd_lint(local_args),
        "benchmark" => cmd_benchmark(local_args),
        "migrate" => cmd_migrate(local_args),
        "check" => cmd_check(local_args),
        "all" => cmd_all(local_args),
        "explain" => cmd_explain(local_args),
        "coverage" => cmd_coverage(local_args),
        "orchestrate" => cmd_orchestrate(local_args),
        "testgen" => cmd_testgen(local_args),
        "replay" => cmd_replay(local_args),
        "init" => usage_exit(&primary_usage()),
        "help" => usage_exit(&primary_usage()),
        _ => {
            usage_exit(&primary_usage());
        }
    }
}

fn primary_usage() -> String {
    "usage: cargo valid [--manifest-path <path>] [--registry <path>|--file <path>|--example <name>|--bin <name>] <init|models|inspect|graph|doc|handoff|readiness|migrate|benchmark|verify|suite|explain|coverage|orchestrate|generate-tests|replay|contract|artifacts|clean|commands|schema|batch> [extra args] [model] [--json] [--progress=json] [--format=<mermaid|dot|svg|text|json>] [--view=<overview|logic|failure|deadlock|scc>] [--property=<id>] [--critical] [--suite=<name>] [--seed=<u64>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>] [--focus-action=<id>] [--actions=a,b,c] [--strategy=<counterexample|transition|witness|guard|boundary|path|random|deadlock|enablement>] [--repeat=<n>] [--baseline[=compare|record|ignore]] [--threshold-percent=<n>] [--write[=<path>]] [--check]".to_string()
}

fn internal_bundled_mode_enabled() -> bool {
    matches!(
        env::var("VALID_INTERNAL_BUNDLED_MODELS").as_deref(),
        Ok("1")
    )
}

fn run_external_registry(parsed: CliArgs) -> ! {
    if parsed.command == "all" {
        run_external_all(parsed);
    }
    if parsed.command == "benchmark" && parsed.model.is_none() {
        run_external_benchmark(parsed);
    }

    let status = build_external_command(&parsed)
        .status()
        .unwrap_or_else(|err| {
            message_exit(
                &parsed.command,
                parsed.json,
                &format!("failed to execute target registry: {err}"),
                None,
            );
        });
    process::exit(to_exit_code(status.code().unwrap_or(ExitCode::Error.code())).code());
}

fn run_external_benchmark(parsed: CliArgs) -> ! {
    let progress = ProgressReporter::new("benchmark", parsed.progress_json);
    let models = if parsed.benchmark_models.is_empty() {
        if parsed.suite_models.is_empty() {
            fetch_external_models(&parsed)
        } else {
            parsed.suite_models.clone()
        }
    } else {
        parsed.benchmark_models.clone()
    };
    let total = models.len();
    let mut aggregate_status = ExitCode::Success;
    let mut json_runs = Vec::new();
    progress.start(Some(total));

    for (index, model) in models.into_iter().enumerate() {
        progress.item_start(index, total, &model);
        let output = build_external_command(&CliArgs {
            command: "benchmark".to_string(),
            model: Some(model.clone()),
            seed: parsed.seed,
            repeat: parsed.repeat,
            baseline_mode: parsed.baseline_mode.clone(),
            threshold_percent: parsed.threshold_percent,
            strategy: None,
            format: parsed.format.clone(),
            view: parsed.view.clone(),
            property_id: parsed.property_id.clone(),
            backend: parsed.backend.clone(),
            solver_executable: parsed.solver_executable.clone(),
            solver_args: parsed.solver_args.clone(),
            actions: Vec::new(),
            focus_action_id: None,
            json: parsed.json,
            progress_json: false,
            manifest_path: parsed.manifest_path.clone(),
            example: parsed.example.clone(),
            bin: parsed.bin.clone(),
            file: parsed.file.clone(),
            suite_models: Vec::new(),
            critical: parsed.critical,
            suite_name: parsed.suite_name.clone(),
            project_config: parsed.project_config.clone(),
            benchmark_models: Vec::new(),
            write_path: None,
            check: false,
            extra_positionals: Vec::new(),
        })
        .output()
        .unwrap_or_else(|err| {
            message_exit(
                "benchmark",
                parsed.json,
                &format!("failed to execute target registry: {err}"),
                None,
            );
        });
        let code = to_exit_code(output.status.code().unwrap_or(ExitCode::Error.code()));
        aggregate_status = aggregate_status.aggregate(code);
        if parsed.json {
            json_runs.push(preferred_child_json(&output));
        } else {
            println!("== {model} ==");
            print!("{}", String::from_utf8_lossy(&output.stdout));
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.trim().is_empty() {
                eprint!("{stderr}");
            }
        }
        progress.item_complete(index, total, &model, code.code());
    }

    if parsed.json {
        let mut body = json!({ "runs": json_runs });
        let body_string = serde_json::to_string(&body).expect("benchmark suite json");
        if let Some(path) = write_benchmark_artifact("project-benchmark-suite", &body_string) {
            body["artifact_path"] = Value::String(path);
        }
        println!(
            "{}",
            serde_json::to_string(&body).expect("benchmark suite output")
        );
    }
    progress.finish(aggregate_status);
    process::exit(aggregate_status.code());
}

fn run_external_all(parsed: CliArgs) -> ! {
    let progress = ProgressReporter::new("all", parsed.progress_json);
    let runs = build_suite_runs_for_external(&parsed);
    let total = runs.len();
    let mut aggregate_status = ExitCode::Success;
    let mut json_runs = Vec::new();
    progress.start(Some(total));

    for (index, run) in runs.into_iter().enumerate() {
        progress.item_start(index, total, &run.model_id);
        let mut command = build_external_command(&CliArgs {
            command: "check".to_string(),
            model: Some(run.model_id.clone()),
            seed: parsed.seed,
            repeat: 0,
            baseline_mode: None,
            threshold_percent: None,
            strategy: None,
            format: parsed.format.clone(),
            view: parsed.view.clone(),
            property_id: run.property_id.clone(),
            backend: parsed.backend.clone(),
            solver_executable: parsed.solver_executable.clone(),
            solver_args: parsed.solver_args.clone(),
            actions: Vec::new(),
            focus_action_id: None,
            json: parsed.json,
            progress_json: false,
            manifest_path: parsed.manifest_path.clone(),
            example: parsed.example.clone(),
            bin: parsed.bin.clone(),
            file: parsed.file.clone(),
            suite_models: Vec::new(),
            critical: parsed.critical,
            suite_name: parsed.suite_name.clone(),
            project_config: parsed.project_config.clone(),
            benchmark_models: Vec::new(),
            write_path: None,
            check: false,
            extra_positionals: Vec::new(),
        });
        let output = command.output().unwrap_or_else(|err| {
            message_exit(
                "all",
                parsed.json,
                &format!("failed to execute target registry: {err}"),
                None,
            );
        });
        let code = to_exit_code(output.status.code().unwrap_or(ExitCode::Error.code()));
        aggregate_status = aggregate_status.aggregate(code);
        if parsed.json {
            let mut item = preferred_child_json(&output);
            if let Some(object) = item.as_object_mut() {
                object.insert("model_id".to_string(), Value::String(run.model_id.clone()));
                object.insert(
                    "property_id".to_string(),
                    run.property_id
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                );
            }
            json_runs.push(item);
        } else {
            println!("== {} ==", run.model_id);
            print!("{}", String::from_utf8_lossy(&output.stdout));
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.trim().is_empty() {
                eprint!("{stderr}");
            }
        }
        progress.item_complete(index, total, &run.model_id, code.code());
    }

    if parsed.json {
        let effective_suite_name = effective_suite_name(
            parsed.project_config.as_ref(),
            parsed.critical,
            parsed.suite_name.as_deref(),
        );
        let mode = match suite_selection_mode(parsed.critical, effective_suite_name.as_deref()) {
            SuiteSelectionMode::All => "all",
            SuiteSelectionMode::Critical => "critical",
            SuiteSelectionMode::Named => "named_suite",
        };
        let mut body = json!({
            "selection_mode": mode,
            "suite_name": effective_suite_name,
            "runs": json_runs
        });
        if let Some(config) = &parsed.project_config {
            body["verification_policy"] = json!(verification_policy(config));
        }
        println!("{}", serde_json::to_string(&body).expect("suite json"));
    }
    progress.finish(aggregate_status);
    process::exit(aggregate_status.code());
}

fn build_external_command(parsed: &CliArgs) -> Command {
    let target = resolve_external_target(parsed);
    let mut command = Command::new("cargo");
    command.arg("run");
    let mut features = Vec::new();
    if external_target_requires_runtime_feature(parsed, &target) {
        features.push("verification-runtime");
    }
    if matches!(parsed.backend.as_deref(), Some("sat-varisat")) {
        features.push("varisat-backend");
    }
    if !features.is_empty() {
        command.arg("--features").arg(features.join(","));
    }
    if let Some(manifest_path) = target.manifest_path {
        command.arg("--manifest-path").arg(manifest_path);
    }
    command.arg(target.kind.cargo_flag()).arg(target.name);
    if let Some(manifest_path) = &parsed.manifest_path {
        command.env("VALID_REGISTRY_MANIFEST_PATH", manifest_path);
    }
    if let Some(file) = &parsed.file {
        command.env("VALID_REGISTRY_FILE", file);
    }
    if let Some(model) = &parsed.model {
        command.env("VALID_REGISTRY_MODEL_NAME", model);
    }
    command.arg("--");
    command.arg(&parsed.command);
    if let Some(model) = &parsed.model {
        command.arg(model);
    }
    for extra in &parsed.extra_positionals {
        command.arg(extra);
    }
    if parsed.command == "benchmark" && parsed.repeat > 0 {
        command.arg(format!("--repeat={}", parsed.repeat));
    }
    if parsed.command == "benchmark" {
        if let Some(baseline_mode) = &parsed.baseline_mode {
            command.arg(format!("--baseline={baseline_mode}"));
        }
        if let Some(threshold_percent) = parsed.threshold_percent {
            command.arg(format!("--threshold-percent={threshold_percent}"));
        }
    }
    if command_supports_strategy(&parsed.command) {
        if let Some(strategy) = &parsed.strategy {
            command.arg(format!("--strategy={strategy}"));
        }
    }
    if command_supports_format(&parsed.command) {
        if let Some(format) = &parsed.format {
            command.arg(format!("--format={format}"));
        }
    }
    if command_supports_view(&parsed.command) {
        if let Some(view) = &parsed.view {
            command.arg(format!("--view={view}"));
        }
    }
    if command_supports_property(&parsed.command) {
        if let Some(property_id) = &parsed.property_id {
            command.arg(format!("--property={property_id}"));
        }
    }
    if command_supports_seed(&parsed.command) {
        if let Some(seed) = parsed.seed {
            command.arg(format!("--seed={seed}"));
        }
    }
    if command_supports_backend(&parsed.command) {
        if let Some(backend) = &parsed.backend {
            command.arg(format!("--backend={backend}"));
        }
    }
    if command_supports_solver(&parsed.command) {
        if let Some(solver_executable) = &parsed.solver_executable {
            command.arg("--solver-exec").arg(solver_executable);
        }
        for solver_arg in &parsed.solver_args {
            command.arg("--solver-arg").arg(solver_arg);
        }
    }
    if command_supports_focus_action(&parsed.command) {
        if let Some(focus_action_id) = &parsed.focus_action_id {
            command.arg(format!("--focus-action={focus_action_id}"));
        }
    }
    if command_supports_actions(&parsed.command) && !parsed.actions.is_empty() {
        command.arg(format!("--actions={}", parsed.actions.join(",")));
    }
    if command_supports_write_path(&parsed.command) {
        if let Some(write_path) = &parsed.write_path {
            if write_path.is_empty() {
                command.arg("--write");
            } else {
                command.arg(format!("--write={write_path}"));
            }
        }
    }
    if command_supports_check_flag(&parsed.command) && parsed.check {
        command.arg("--check");
    }
    if parsed.json {
        command.arg("--json");
    }
    if parsed.progress_json {
        command.arg("--progress=json");
    }
    command
}

fn command_supports_backend(command: &str) -> bool {
    matches!(
        command,
        "check" | "verify" | "benchmark" | "orchestrate" | "testgen" | "trace" | "coverage"
    )
}

fn command_supports_solver(command: &str) -> bool {
    command_supports_backend(command)
}

fn command_supports_property(command: &str) -> bool {
    matches!(
        command,
        "check"
            | "verify"
            | "benchmark"
            | "graph"
            | "testgen"
            | "trace"
            | "coverage"
            | "replay"
            | "all"
    )
}

fn command_supports_seed(command: &str) -> bool {
    matches!(
        command,
        "check" | "verify" | "testgen" | "trace" | "coverage" | "orchestrate" | "all"
    )
}

fn command_supports_strategy(command: &str) -> bool {
    matches!(command, "testgen" | "generate-tests")
}

fn command_supports_format(command: &str) -> bool {
    matches!(command, "graph")
}

fn command_supports_view(command: &str) -> bool {
    matches!(command, "graph")
}

fn command_supports_focus_action(command: &str) -> bool {
    matches!(command, "replay" | "testgen" | "generate-tests")
}

fn command_supports_actions(command: &str) -> bool {
    matches!(command, "replay")
}

fn command_supports_write_path(command: &str) -> bool {
    matches!(command, "migrate" | "doc" | "handoff")
}

fn command_supports_check_flag(command: &str) -> bool {
    matches!(command, "migrate" | "doc" | "handoff")
}

fn external_target_requires_runtime_feature(parsed: &CliArgs, target: &ExternalTarget) -> bool {
    let built_in_manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    external_project_root(parsed.manifest_path.as_deref()) == built_in_manifest_dir
        && matches!(
            target.kind,
            ExternalTargetKind::Example | ExternalTargetKind::Bin
        )
}

fn fetch_external_models(parsed: &CliArgs) -> Vec<String> {
    let output = build_external_command(&CliArgs {
        command: "list".to_string(),
        model: None,
        seed: None,
        repeat: 0,
        baseline_mode: None,
        threshold_percent: None,
        strategy: None,
        format: None,
        view: None,
        property_id: None,
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
        actions: Vec::new(),
        focus_action_id: None,
        json: true,
        progress_json: false,
        manifest_path: parsed.manifest_path.clone(),
        example: parsed.example.clone(),
        bin: parsed.bin.clone(),
        file: parsed.file.clone(),
        suite_models: Vec::new(),
        critical: false,
        suite_name: None,
        project_config: parsed.project_config.clone(),
        benchmark_models: Vec::new(),
        write_path: None,
        check: false,
        extra_positionals: Vec::new(),
    })
    .output()
    .unwrap_or_else(|err| {
        message_exit(
            "list",
            true,
            &format!("failed to execute target registry: {err}"),
            None,
        );
    });
    if !output.status.success() {
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        process::exit(to_exit_code(output.status.code().unwrap_or(ExitCode::Error.code())).code());
    }
    parse_models_json(&String::from_utf8_lossy(&output.stdout))
}

#[derive(Debug, Clone)]
struct SuiteRun {
    model_id: String,
    property_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SuiteSelectionMode {
    All,
    Critical,
    Named,
}

fn build_suite_runs_for_external(parsed: &CliArgs) -> Vec<SuiteRun> {
    let effective_suite_name = effective_suite_name(
        parsed.project_config.as_ref(),
        parsed.critical,
        parsed.suite_name.as_deref(),
    );
    match suite_selection_mode(parsed.critical, effective_suite_name.as_deref()) {
        SuiteSelectionMode::All => {
            let models = if parsed.suite_models.is_empty() {
                fetch_external_models(parsed)
            } else {
                parsed.suite_models.clone()
            };
            models
                .into_iter()
                .map(|model_id| SuiteRun {
                    model_id,
                    property_id: parsed.property_id.clone(),
                })
                .collect()
        }
        SuiteSelectionMode::Critical => {
            let config = parsed.project_config.as_ref().unwrap_or_else(|| {
                usage_exit("`cargo valid suite --critical` requires valid.toml critical_properties")
            });
            let ordered_models = if parsed.suite_models.is_empty() {
                config
                    .critical_properties
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
            } else {
                parsed
                    .suite_models
                    .iter()
                    .filter(|model| config.critical_properties.contains_key(model.as_str()))
                    .cloned()
                    .collect::<Vec<_>>()
            };
            ordered_models
                .into_iter()
                .flat_map(|model_id| {
                    config
                        .critical_properties
                        .get(&model_id)
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .map(move |property_id| SuiteRun {
                            model_id: model_id.clone(),
                            property_id: Some(property_id),
                        })
                })
                .collect()
        }
        SuiteSelectionMode::Named => {
            let config = parsed.project_config.as_ref().unwrap_or_else(|| {
                usage_exit("`cargo valid suite --suite=<name>` requires valid.toml property_suites")
            });
            let suite_name = effective_suite_name.as_deref().expect("named suite");
            let entries = config.property_suites.get(suite_name).unwrap_or_else(|| {
                usage_exit(&format!("unknown property suite `{suite_name}`"));
            });
            entries
                .iter()
                .flat_map(|entry| {
                    entry
                        .properties
                        .iter()
                        .cloned()
                        .map(|property_id| SuiteRun {
                            model_id: entry.model.clone(),
                            property_id: Some(property_id),
                        })
                })
                .collect()
        }
    }
}

fn build_suite_runs_for_bundled(parsed: &ParsedArgs) -> Vec<SuiteRun> {
    let model_catalog = bundled_model_property_catalog();
    build_suite_runs_from_catalog(
        parsed.project_config.as_ref(),
        &model_catalog,
        &parsed.suite_models,
        parsed.property_id.as_ref(),
        parsed.critical,
        effective_suite_name(
            parsed.project_config.as_ref(),
            parsed.critical,
            parsed.suite_name.as_deref(),
        )
        .as_deref(),
    )
}

fn build_suite_runs_from_catalog(
    config: Option<&ProjectConfig>,
    model_catalog: &std::collections::BTreeMap<String, Vec<String>>,
    suite_models: &[String],
    property_id: Option<&String>,
    critical: bool,
    suite_name: Option<&str>,
) -> Vec<SuiteRun> {
    let selection = suite_selection_mode(critical, suite_name);
    match selection {
        SuiteSelectionMode::All => {
            let models = if suite_models.is_empty() {
                model_catalog.keys().cloned().collect::<Vec<_>>()
            } else {
                suite_models.to_vec()
            };
            models
                .into_iter()
                .map(|model_id| SuiteRun {
                    model_id,
                    property_id: property_id.cloned(),
                })
                .collect()
        }
        SuiteSelectionMode::Critical => {
            let config = config.unwrap_or_else(|| {
                usage_exit("`cargo valid suite --critical` requires valid.toml critical_properties")
            });
            let ordered_models = if suite_models.is_empty() {
                config
                    .critical_properties
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
            } else {
                suite_models
                    .iter()
                    .filter(|model| config.critical_properties.contains_key(model.as_str()))
                    .cloned()
                    .collect::<Vec<_>>()
            };
            if ordered_models.is_empty() {
                usage_exit("no critical properties matched the selected suite models");
            }
            expand_property_targets(
                model_catalog,
                ordered_models
                    .into_iter()
                    .flat_map(|model| {
                        config
                            .critical_properties
                            .get(&model)
                            .cloned()
                            .unwrap_or_default()
                            .into_iter()
                            .map(move |property| (model.clone(), property))
                    })
                    .collect(),
            )
        }
        SuiteSelectionMode::Named => {
            let config = config.unwrap_or_else(|| {
                usage_exit("`cargo valid suite --suite=<name>` requires valid.toml property_suites")
            });
            let suite_name = suite_name.expect("named suite");
            let entries = config.property_suites.get(suite_name).unwrap_or_else(|| {
                usage_exit(&format!("unknown property suite `{suite_name}`"));
            });
            if entries.is_empty() {
                usage_exit(&format!("property suite `{suite_name}` has no entries"));
            }
            let allowed_models = if suite_models.is_empty() {
                None
            } else {
                Some(
                    suite_models
                        .iter()
                        .cloned()
                        .collect::<std::collections::BTreeSet<_>>(),
                )
            };
            expand_property_targets(
                model_catalog,
                entries
                    .iter()
                    .filter(|entry| {
                        allowed_models
                            .as_ref()
                            .map(|models| models.contains(&entry.model))
                            .unwrap_or(true)
                    })
                    .flat_map(|entry| {
                        entry
                            .properties
                            .iter()
                            .cloned()
                            .map(|property| (entry.model.clone(), property))
                    })
                    .collect(),
            )
        }
    }
}

fn suite_selection_mode(critical: bool, suite_name: Option<&str>) -> SuiteSelectionMode {
    if critical {
        SuiteSelectionMode::Critical
    } else if suite_name.is_some() {
        SuiteSelectionMode::Named
    } else {
        SuiteSelectionMode::All
    }
}

fn effective_suite_name(
    config: Option<&ProjectConfig>,
    critical: bool,
    explicit_suite_name: Option<&str>,
) -> Option<String> {
    if critical {
        return None;
    }
    explicit_suite_name
        .map(str::to_string)
        .or_else(|| config.and_then(|config| config.default_suite.clone()))
}

fn expand_property_targets(
    model_catalog: &std::collections::BTreeMap<String, Vec<String>>,
    requested: Vec<(String, String)>,
) -> Vec<SuiteRun> {
    let mut seen = std::collections::BTreeSet::new();
    let mut runs = Vec::new();
    for (model_id, property_id) in requested {
        let properties = model_catalog
            .get(&model_id)
            .unwrap_or_else(|| usage_exit(&format!("unknown model `{model_id}` in valid.toml")));
        if property_id.trim().is_empty() {
            usage_exit(&format!(
                "empty property id configured for model `{model_id}`"
            ));
        }
        if !properties.contains(&property_id) {
            usage_exit(&format!(
                "unknown property `{property_id}` configured for model `{model_id}`"
            ));
        }
        if seen.insert((model_id.clone(), property_id.clone())) {
            runs.push(SuiteRun {
                model_id,
                property_id: Some(property_id),
            });
        }
    }
    runs
}

fn bundled_model_property_catalog() -> std::collections::BTreeMap<String, Vec<String>> {
    list_bundled_models()
        .into_iter()
        .map(|model| {
            let request = InspectRequest {
                request_id: format!("cargo-valid-suite-catalog-{model}"),
                source_name: normalized_model_ref(model),
                source: String::new(),
            };
            let inspect = inspect_source(&request).unwrap_or_else(|_| {
                usage_exit(&format!("failed to inspect bundled model `{model}`"))
            });
            (model.to_string(), inspect.properties)
        })
        .collect()
}

fn cmd_all(parsed: ParsedArgs) {
    let progress = ProgressReporter::new("all", parsed.progress_json);
    let runs = build_suite_runs_for_bundled(&parsed);
    let total = runs.len();
    let mut aggregate_status = ExitCode::Success;
    let mut json_runs = Vec::new();
    progress.start(Some(total));

    for (index, run) in runs.into_iter().enumerate() {
        progress.item_start(index, total, &run.model_id);
        let request = CheckRequest {
            request_id: format!("cargo-valid-check-{}", run.model_id),
            source_name: normalized_model_ref(&run.model_id),
            source: String::new(),
            property_id: run.property_id.clone(),
            scenario_id: None,
            seed: parsed.seed,
            backend: parsed.backend.clone(),
            solver_executable: parsed.solver_executable.clone(),
            solver_args: parsed.solver_args.clone(),
        };
        let outcome = check_source(&request);
        let exit_code = ExitCode::from_check_outcome(&outcome);
        aggregate_status = aggregate_status.aggregate(exit_code);
        if parsed.json {
            let mut item: Value =
                serde_json::from_str(&render_outcome_json(&run.model_id, &outcome))
                    .expect("check outcome json");
            if let Some(object) = item.as_object_mut() {
                object.insert("model_id".to_string(), Value::String(run.model_id.clone()));
                object.insert(
                    "property_id".to_string(),
                    run.property_id
                        .clone()
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                );
            }
            json_runs.push(item);
        } else {
            println!("== {} ==", run.model_id);
            print!("{}", render_outcome_text(&outcome));
        }
        progress.item_complete(index, total, &run.model_id, exit_code.code());
    }

    if parsed.json {
        let effective_suite_name = effective_suite_name(
            parsed.project_config.as_ref(),
            parsed.critical,
            parsed.suite_name.as_deref(),
        );
        let mode = match suite_selection_mode(parsed.critical, effective_suite_name.as_deref()) {
            SuiteSelectionMode::All => "all",
            SuiteSelectionMode::Critical => "critical",
            SuiteSelectionMode::Named => "named_suite",
        };
        let mut body = json!({
            "selection_mode": mode,
            "suite_name": effective_suite_name,
            "runs": json_runs
        });
        if let Some(config) = &parsed.project_config {
            body["verification_policy"] = json!(verification_policy(config));
        }
        println!("{}", serde_json::to_string(&body).expect("suite json"));
    }
    progress.finish(aggregate_status);
    process::exit(aggregate_status.code());
}

fn cmd_list(parsed: ParsedArgs) {
    let models = list_bundled_models();
    if parsed.json {
        let mut body = json!({
            "models": models
        });
        if let Some(config) = &parsed.project_config {
            body["critical_properties"] = json!(config.critical_properties);
            body["property_suites"] = json!(config.property_suites);
            body["verification_policy"] = json!(verification_policy(config));
        }
        println!("{}", serde_json::to_string(&body).expect("list json"));
    } else {
        for model in models {
            println!("{model}");
        }
    }
}

fn cmd_inspect(parsed: ParsedArgs) {
    let progress = ProgressReporter::new("inspect", parsed.progress_json);
    progress.start(None);
    let model = parsed
        .model
        .unwrap_or_else(|| usage_exit("usage: cargo valid inspect <model> [--json]"));
    let request = InspectRequest {
        request_id: "cargo-valid-inspect".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
    };
    match inspect_source(&request) {
        Ok(response) => {
            if parsed.json {
                println!("{}", render_inspect_json(&response));
            } else {
                print!("{}", render_inspect_text(&response));
            }
            progress.finish(ExitCode::Success);
        }
        Err(diagnostics) => diagnostics_exit("inspect", parsed.json, &diagnostics),
    }
}

fn cmd_graph(parsed: ParsedArgs) {
    let progress = ProgressReporter::new("graph", parsed.progress_json);
    progress.start(None);
    let model = parsed.model.clone().unwrap_or_else(|| {
        usage_exit(
            "usage: cargo valid graph <model> [--format=mermaid|dot|svg|text|json] [--view=overview|logic|failure|deadlock|scc] [--property=<id>]",
        )
    });
    let request = InspectRequest {
        request_id: "cargo-valid-graph".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
    };
    let env_default_format = env::var("VALID_DEFAULT_GRAPH_FORMAT").ok();
    let default_format = parsed
        .format
        .as_deref()
        .or(env_default_format.as_deref())
        .unwrap_or("mermaid");
    let json_output = parsed.json || default_format == "json";
    let view = GraphView::parse(parsed.view.as_deref());
    match inspect_source(&request) {
        Ok(response) => {
            match render_cargo_graph_output(&response, &request, &parsed, default_format, view) {
                Ok(body) => print!("{body}"),
                Err(message) => message_exit("cargo-valid", json_output, &message, None),
            }
        }
        Err(diagnostics) => diagnostics_exit("graph", json_output, &diagnostics),
    }
    progress.finish(ExitCode::Success);
}

fn render_cargo_graph_output(
    response: &valid::api::InspectResponse,
    request: &InspectRequest,
    parsed: &ParsedArgs,
    render_format: &str,
    view: GraphView,
) -> Result<String, String> {
    if view != GraphView::Failure {
        return Ok(match render_format {
            "json" => format!("{}\n", render_inspect_json(response)),
            "text" => render_model_text_with_view(response, view),
            "dot" => format!("{}\n", render_model_dot_with_view(response, view)),
            "svg" => format!("{}\n", render_model_svg_with_view(response, view)),
            _ => format!("{}\n", render_model_mermaid_with_view(response, view)),
        });
    }

    let property_id = parsed
        .property_id
        .as_ref()
        .ok_or_else(|| "failure graph view requires --property=<id>".to_string())?;
    let outcome = check_source(&CheckRequest {
        request_id: "cargo-valid-graph-failure".to_string(),
        source_name: request.source_name.clone(),
        source: request.source.clone(),
        property_id: Some(property_id.clone()),
        scenario_id: None,
        seed: parsed.seed,
        backend: parsed.backend.clone(),
        solver_executable: parsed.solver_executable.clone(),
        solver_args: parsed.solver_args.clone(),
    });
    let result = match outcome {
        valid::engine::CheckOutcome::Completed(result) => result,
        valid::engine::CheckOutcome::Errored(error) => {
            return Err(error
                .diagnostics
                .first()
                .map(|diagnostic| diagnostic.message.clone())
                .unwrap_or_else(|| "failure graph check failed".to_string()))
        }
    };
    let trace = result
        .trace
        .as_ref()
        .ok_or_else(|| format!("property `{property_id}` did not produce evidence trace"))?;
    let slice = build_failure_graph_slice(response, trace, property_id)?;

    Ok(match render_format {
        "json" => {
            let mut body: Value = serde_json::from_str(&render_inspect_json(response))
                .map_err(|err| format!("failed to prepare graph json: {err}"))?;
            body["graph_view"] = Value::String("failure".to_string());
            body["graph_slice"] = json!({
                "property_id": slice.property_id,
                "failing_action_id": slice.failing_action_id,
                "failing_step_index": slice.failing_step_index,
                "focused_fields": slice.focused_fields,
                "focused_reads": slice.focused_reads,
                "focused_writes": slice.focused_writes,
                "focused_path_tags": slice.focused_path_tags,
                "focused_transition_indexes": slice.focused_transition_indexes,
                "summary": slice.summary,
            });
            format!(
                "{}\n",
                serde_json::to_string(&body).map_err(|err| err.to_string())?
            )
        }
        "text" => render_model_text_failure(response, &slice),
        "dot" => format!("{}\n", render_model_dot_failure(response, &slice)),
        "svg" => format!("{}\n", render_model_svg_failure(response, &slice)),
        _ => format!("{}\n", render_model_mermaid_failure(response, &slice)),
    })
}

fn cmd_doc(parsed: ParsedArgs) {
    let progress = ProgressReporter::new("doc", parsed.progress_json);
    progress.start(None);
    let model = parsed.model.unwrap_or_else(|| {
        usage_exit("usage: cargo valid doc <model> [--json] [--write[=<path>]] [--check]")
    });
    let request = InspectRequest {
        request_id: "cargo-valid-doc".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
    };
    match inspect_source(&request) {
        Ok(response) => {
            let mermaid = render_model_mermaid_with_view(&response, GraphView::Overview);
            let source_hash = stable_hash_hex(&model);
            let contract_hash = stable_hash_hex(&format!(
                "{}|{}|{}|{}",
                response.model_id,
                response.state_fields.join(","),
                response.actions.join(","),
                response.properties.join(",")
            ));
            let generated = generate_doc(&response, mermaid, source_hash, contract_hash);
            let output_path = parsed
                .write_path
                .clone()
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| default_doc_path(&generated.model_id));
            if parsed.check {
                let existing = fs::read_to_string(&output_path).ok();
                let report = check_doc(output_path.clone(), existing.as_deref(), &generated);
                let code = if report.status == "unchanged" {
                    ExitCode::Success
                } else {
                    ExitCode::Unknown
                };
                if parsed.json {
                    println!("{}", render_doc_check_json(&report));
                } else {
                    print!("{}", render_doc_check_text(&report));
                }
                progress.finish(code);
                process::exit(code.code());
            }
            if parsed.write_path.is_some() {
                if let Err(message) = write_doc(&output_path, &generated) {
                    message_exit("doc", parsed.json, &message, None);
                }
            }
            if parsed.json {
                println!(
                    "{}",
                    render_doc_json(
                        &generated,
                        parsed.write_path.as_ref().map(|_| output_path.as_str())
                    )
                );
            } else {
                print!(
                    "{}",
                    render_doc_text(
                        &generated,
                        parsed.write_path.as_ref().map(|_| output_path.as_str())
                    )
                );
            }
            progress.finish(ExitCode::Success);
        }
        Err(diagnostics) => diagnostics_exit("doc", parsed.json, &diagnostics),
    }
}

fn cmd_handoff(parsed: ParsedArgs) {
    let progress = ProgressReporter::new("handoff", parsed.progress_json);
    progress.start(None);
    let model = parsed.model.unwrap_or_else(|| {
        usage_exit(
            "usage: cargo valid handoff <model> [--json] [--property=<id>] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>] [--write[=<path>]] [--check]",
        )
    });
    let source_name = normalized_model_ref(&model);
    let inspect_request = InspectRequest {
        request_id: "cargo-valid-handoff".to_string(),
        source_name: source_name.clone(),
        source: String::new(),
    };
    match inspect_source(&inspect_request) {
        Ok(response) => {
            let orchestrated = orchestrate_source(&OrchestrateRequest {
                request_id: "cargo-valid-handoff-orchestrate".to_string(),
                source_name: source_name.clone(),
                source: String::new(),
                seed: None,
                backend: parsed.backend.clone(),
                solver_executable: parsed.solver_executable.clone(),
                solver_args: parsed.solver_args.clone(),
            })
            .unwrap_or_else(|error| diagnostics_exit("handoff", parsed.json, &error.diagnostics));
            let explanations = response
                .properties
                .iter()
                .filter(|property_id| {
                    parsed
                        .property_id
                        .as_deref()
                        .map(|candidate| candidate == property_id.as_str())
                        .unwrap_or(true)
                })
                .filter_map(|property_id| {
                    explain_source(&CheckRequest {
                        request_id: format!("cargo-valid-handoff-explain-{property_id}"),
                        source_name: source_name.clone(),
                        source: String::new(),
                        property_id: Some(property_id.clone()),
                        scenario_id: None,
                        seed: None,
                        backend: parsed.backend.clone(),
                        solver_executable: parsed.solver_executable.clone(),
                        solver_args: parsed.solver_args.clone(),
                    })
                    .ok()
                })
                .collect::<Vec<_>>();
            let coverage = orchestrated.aggregate_coverage.clone().unwrap_or_else(|| {
                coverage_bundled_model(&source_name)
                    .unwrap_or_else(|message| message_exit("handoff", parsed.json, &message, None))
            });
            let source_hash = stable_hash_hex(&model);
            let contract_hash = stable_hash_hex(&format!(
                "{}|{}|{}|{}",
                response.model_id,
                response.state_fields.join(","),
                response.actions.join(","),
                response.properties.join(",")
            ));
            let generated = generate_handoff(HandoffInputs {
                inspect: &response,
                runs: &orchestrated.runs,
                coverage: &coverage,
                explanations: &explanations,
                property_id: parsed.property_id.as_deref(),
                source_hash: &source_hash,
                contract_hash: &contract_hash,
            });
            let output_path = parsed
                .write_path
                .clone()
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| default_handoff_path(&generated.model_id));
            if parsed.check {
                let existing = fs::read_to_string(&output_path).ok();
                let report = check_handoff(output_path.clone(), existing.as_deref(), &generated);
                let code = if report.status == "unchanged" {
                    ExitCode::Success
                } else {
                    ExitCode::Unknown
                };
                if parsed.json {
                    println!("{}", render_handoff_check_json(&report));
                } else {
                    print!("{}", render_handoff_check_text(&report));
                }
                progress.finish(code);
                process::exit(code.code());
            }
            if parsed.write_path.is_some() {
                if let Err(message) = write_handoff(&output_path, &generated) {
                    message_exit("handoff", parsed.json, &message, None);
                }
            }
            if parsed.json {
                println!(
                    "{}",
                    render_handoff_json(&generated, Some(output_path.as_str()))
                );
            } else {
                print!(
                    "{}",
                    render_handoff_text(&generated, Some(output_path.as_str()))
                );
            }
            progress.finish(ExitCode::Success);
        }
        Err(diagnostics) => diagnostics_exit("handoff", parsed.json, &diagnostics),
    }
}

fn cmd_lint(parsed: ParsedArgs) {
    let progress = ProgressReporter::new("lint", parsed.progress_json);
    progress.start(None);
    let model = parsed
        .model
        .unwrap_or_else(|| usage_exit("usage: cargo valid lint <model> [--json]"));
    let request = InspectRequest {
        request_id: "cargo-valid-lint".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
    };
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
        Err(diagnostics) => diagnostics_exit("lint", parsed.json, &diagnostics),
    }
}

fn cmd_benchmark(parsed: ParsedArgs) {
    let progress = ProgressReporter::new("benchmark", parsed.progress_json);
    progress.start(None);
    let model = parsed.model.unwrap_or_else(|| {
        usage_exit("usage: cargo valid benchmark <model> [--json] [--property=<id>] [--repeat=<n>] [--baseline[=compare|record|ignore]] [--threshold-percent=<n>]")
    });
    let backend_label = parsed
        .backend
        .clone()
        .unwrap_or_else(|| "explicit".to_string());
    let summary = benchmark_check_outcomes(
        "cargo-valid-benchmark",
        &model,
        &backend_label,
        parsed.property_id.as_deref(),
        parsed.repeat,
        |_| {
            let request = CheckRequest {
                request_id: "cargo-valid-benchmark".to_string(),
                source_name: normalized_model_ref(&model),
                source: String::new(),
                property_id: parsed.property_id.clone(),
                scenario_id: None,
                seed: parsed.seed,
                backend: parsed.backend.clone(),
                solver_executable: parsed.solver_executable.clone(),
                solver_args: parsed.solver_args.clone(),
            };
            check_source(&request)
        },
    );
    let json = render_benchmark_json(&summary);
    let artifact_id = format!(
        "bench-{}",
        stable_hash_hex(&format!(
            "{}:{}:{}:{}",
            model,
            backend_label,
            parsed.property_id.as_deref().unwrap_or(""),
            parsed.repeat
        ))
        .replace("sha256:", "")
    );
    let baseline_id =
        benchmark_baseline_report_id(&model, &backend_label, parsed.property_id.as_deref());
    let artifact_path = write_benchmark_artifact(&artifact_id, &json);
    let baseline_mode = parsed.baseline_mode.as_deref().unwrap_or("compare");
    let threshold_percent = parsed.threshold_percent.unwrap_or(25);
    let (comparison_json, comparison_text, regression_detected) =
        benchmark_baseline_outputs(&json, &baseline_id, baseline_mode, threshold_percent);
    if parsed.json {
        if let Some(path) = artifact_path {
            println!(
                "{{\"artifact_path\":\"{}\",\"summary\":{},\"baseline\":{}}}",
                path.replace('\\', "\\\\"),
                json,
                comparison_json.unwrap_or_else(|| "null".to_string())
            );
        } else {
            println!(
                "{{\"summary\":{},\"baseline\":{}}}",
                json,
                comparison_json.unwrap_or_else(|| "null".to_string())
            );
        }
    } else {
        print!("{}", render_benchmark_text(&summary));
        if let Some(path) = artifact_path {
            println!("artifact_path: {path}");
        }
        if let Some(text) = comparison_text {
            print!("{text}");
        }
    }
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

fn cmd_migrate(parsed: ParsedArgs) {
    let progress = ProgressReporter::new("migrate", parsed.progress_json);
    progress.start(None);
    let model = parsed.model.unwrap_or_else(|| {
        usage_exit("usage: cargo valid migrate <model> [--json] [--write[=<path>]] [--check]")
    });
    let request = InspectRequest {
        request_id: "cargo-valid-migrate".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
    };
    match inspect_source(&request) {
        Ok(inspect) => {
            let lint = valid::api::lint_from_inspect(&inspect);
            let migration = migration_from_inspect(&inspect, &lint, parsed.check);
            let json_body = render_migration_json(&migration);
            let text_body = render_migration_text(&migration);
            let written_path = parsed
                .write_path
                .as_ref()
                .map(|value| migration_output_path(&model, value))
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
        Err(diagnostics) => diagnostics_exit("migrate", parsed.json, &diagnostics),
    }
}

fn cmd_check(parsed: ParsedArgs) {
    let progress = ProgressReporter::new("check", parsed.progress_json);
    progress.start(None);
    let model = parsed.model.unwrap_or_else(|| usage_exit("usage: cargo valid check <model> [--json] [--property=<id>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]"));
    let inspect_request = InspectRequest {
        request_id: "cargo-valid-check-preflight".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
    };
    if matches!(parsed.backend.as_deref(), None | Some("explicit")) {
        if let Ok(inspect) = inspect_source(&inspect_request) {
            if let Some(warning) = explicit_analysis_warning(&inspect) {
                if parsed.json {
                    eprintln!("{}", valid::cli::render_cli_warning_json("check", &warning));
                } else {
                    eprintln!("{warning}");
                }
            }
        }
    }
    let request = CheckRequest {
        request_id: "cargo-valid-check".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
        property_id: parsed.property_id.clone(),
        scenario_id: None,
        seed: parsed.seed,
        backend: parsed.backend.clone(),
        solver_executable: parsed.solver_executable.clone(),
        solver_args: parsed.solver_args.clone(),
    };
    let outcome = check_source(&request);
    if parsed.json {
        println!("{}", render_outcome_json(&model, &outcome));
    } else {
        print!("{}", render_outcome_text(&outcome));
    }
    let exit_code = ExitCode::from_check_outcome(&outcome);
    progress.finish(exit_code);
    process::exit(exit_code.code());
}

fn cmd_explain(parsed: ParsedArgs) {
    let progress = ProgressReporter::new("explain", parsed.progress_json);
    progress.start(None);
    let model = parsed
        .model
        .unwrap_or_else(|| usage_exit("usage: cargo valid explain <model> [--json]"));
    let request = CheckRequest {
        request_id: "cargo-valid-explain".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
        property_id: parsed.property_id.clone(),
        scenario_id: None,
        seed: parsed.seed,
        backend: parsed.backend.clone(),
        solver_executable: parsed.solver_executable.clone(),
        solver_args: parsed.solver_args.clone(),
    };
    match explain_source(&request) {
        Ok(response) => {
            if parsed.json {
                println!("{}", render_explain_json(&response));
            } else {
                print!("{}", render_explain_text(&response));
            }
            progress.finish(ExitCode::Success);
        }
        Err(error) => diagnostics_exit("explain", parsed.json, &error.diagnostics),
    }
}

fn cmd_coverage(parsed: ParsedArgs) {
    let progress = ProgressReporter::new("coverage", parsed.progress_json);
    progress.start(None);
    let model = parsed
        .model
        .unwrap_or_else(|| usage_exit("usage: cargo valid coverage <model> [--json]"));
    let report = coverage_bundled_model(&normalized_model_ref(&model))
        .unwrap_or_else(|message| message_exit("coverage", parsed.json, &message, None));
    if parsed.json {
        println!("{}", render_coverage_json(&report));
    } else {
        println!("{}", render_coverage_text(&report));
    }
    progress.finish(ExitCode::Success);
}

fn cmd_orchestrate(parsed: ParsedArgs) {
    let progress = ProgressReporter::new("orchestrate", parsed.progress_json);
    progress.start(None);
    let model = parsed.model.unwrap_or_else(|| usage_exit("usage: cargo valid orchestrate <model> [--json] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]"));
    let request = OrchestrateRequest {
        request_id: "cargo-valid-orchestrate".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
        seed: parsed.seed,
        backend: parsed.backend.clone(),
        solver_executable: parsed.solver_executable.clone(),
        solver_args: parsed.solver_args.clone(),
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
            progress.finish(ExitCode::Success);
        }
        Err(error) => diagnostics_exit("orchestrate", parsed.json, &error.diagnostics),
    }
}

fn cmd_testgen(parsed: ParsedArgs) {
    let progress = ProgressReporter::new("testgen", parsed.progress_json);
    progress.start(None);
    let model = parsed.model.unwrap_or_else(|| usage_exit("usage: cargo valid testgen <model> [--json] [--strategy=<counterexample|transition|witness|guard|boundary|path|random|deadlock|enablement>] [--focus-action=<id>]"));
    let request = TestgenRequest {
        request_id: "cargo-valid-testgen".to_string(),
        source_name: normalized_model_ref(&model),
        source: String::new(),
        property_id: parsed.property_id.clone(),
        strategy: parsed
            .strategy
            .clone()
            .unwrap_or_else(|| "counterexample".to_string()),
        focus_action_id: parsed.focus_action_id.clone(),
        seed: parsed.seed,
        backend: parsed.backend.clone(),
        solver_executable: parsed.solver_executable.clone(),
        solver_args: parsed.solver_args.clone(),
    };
    match testgen_source(&request) {
        Ok(response) => {
            if parsed.json {
                println!(
                    "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"vector_ids\":[{}],\"vectors\":[{}],\"vector_groups\":[{}],\"generated_files\":[{}]}}",
                    response.schema_version,
                    response.request_id,
                    response.status,
                    response.vector_ids.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
                    response
                        .vectors
                        .iter()
                        .map(|vector| format!(
                            "{{\"vector_id\":\"{}\",\"run_id\":\"{}\",\"strictness\":\"{}\",\"derivation\":\"{}\",\"source_kind\":\"{}\",\"strategy\":\"{}\",\"requirement_clusters\":[{}],\"risk_clusters\":[{}],\"focus_action_id\":{},\"expected_guard_enabled\":{},\"notes\":[{}]}}",
                            vector.vector_id,
                            vector.run_id,
                            vector.strictness,
                            vector.derivation,
                            vector.source_kind,
                            vector.strategy,
                            vector.requirement_clusters.iter().map(|cluster| format!("\"{}\"", cluster)).collect::<Vec<_>>().join(","),
                            vector.risk_clusters.iter().map(|cluster| format!("\"{}\"", cluster)).collect::<Vec<_>>().join(","),
                            vector.focus_action_id.as_ref().map(|id| format!("\"{}\"", id)).unwrap_or_else(|| "null".to_string()),
                            vector.expected_guard_enabled.map(|value| value.to_string()).unwrap_or_else(|| "null".to_string()),
                            vector.notes.iter().map(|note| format!("\"{}\"", note)).collect::<Vec<_>>().join(",")
                        ))
                        .collect::<Vec<_>>()
                        .join(","),
                    response
                        .vector_groups
                        .iter()
                        .map(|group| format!(
                            "{{\"group_kind\":\"{}\",\"group_id\":\"{}\",\"vector_ids\":[{}]}}",
                            group.group_kind,
                            group.group_id,
                            group.vector_ids.iter().map(|id| format!("\"{}\"", id)).collect::<Vec<_>>().join(","),
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
                            "- {} run_id={} strictness={} derivation={} source={} strategy={} requirements={} risks={} focus_action={} guard_enabled={} notes={}",
                            vector.vector_id,
                            vector.run_id,
                            vector.strictness,
                            vector.derivation,
                            vector.source_kind,
                            vector.strategy,
                            if vector.requirement_clusters.is_empty() {
                                "-".to_string()
                            } else {
                                vector.requirement_clusters.join(",")
                            },
                            if vector.risk_clusters.is_empty() {
                                "-".to_string()
                            } else {
                                vector.risk_clusters.join(",")
                            },
                            vector.focus_action_id.as_deref().unwrap_or("-"),
                            vector
                                .expected_guard_enabled
                                .map(|value| value.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            if vector.notes.is_empty() {
                                "-".to_string()
                            } else {
                                vector.notes.join(",")
                            }
                        );
                    }
                }
                if !response.vector_groups.is_empty() {
                    println!("grouped output:");
                    for group in &response.vector_groups {
                        println!(
                            "- {}:{} -> {}",
                            group.group_kind,
                            group.group_id,
                            group.vector_ids.join(",")
                        );
                    }
                }
            }
            progress.finish(ExitCode::Success);
        }
        Err(error) => diagnostics_exit("testgen", parsed.json, &error.diagnostics),
    }
}

fn cmd_replay(parsed: ParsedArgs) {
    let progress = ProgressReporter::new("replay", parsed.progress_json);
    progress.start(None);
    let model = parsed
        .model
        .unwrap_or_else(|| usage_exit("usage: cargo valid replay <model> [--json] [--property=<id>] [--focus-action=<id>] [--actions=a,b,c]"));
    let output = valid::bundled_models::replay_bundled_model(
        &normalized_model_ref(&model),
        parsed.property_id.as_deref(),
        &parsed.actions,
        parsed.focus_action_id.as_deref(),
    )
    .unwrap_or_else(|message| message_exit("replay", parsed.json, &message, None));
    println!("{output}");
    progress.finish(ExitCode::Success);
}

fn cmd_clean(parsed: &CliArgs) -> ! {
    let progress = ProgressReporter::new("clean", parsed.progress_json);
    progress.start(None);
    let scope = parsed.model.as_deref().unwrap_or("all");
    let root = clean_root(parsed);
    let mut removed = Vec::new();
    match scope {
        "all" => {
            removed.extend(clean_generated_tests(&root));
            removed.extend(clean_artifacts(&root));
        }
        "generated" | "generated-tests" => {
            removed.extend(clean_generated_tests(&root));
        }
        "artifacts" => {
            removed.extend(clean_artifacts(&root));
        }
        other => usage_exit(&format!(
            "usage: cargo valid clean [generated|artifacts|all] [--json] [--progress=json]\nunknown clean scope `{other}`"
        )),
    }
    if parsed.json {
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
    progress.finish(ExitCode::Success);
    process::exit(ExitCode::Success.code());
}

fn cmd_artifacts(parsed: &CliArgs) -> ! {
    let index = load_artifact_index().unwrap_or_else(|message| {
        message_exit("artifacts", parsed.json, &message, None);
    });
    let history = load_run_history().unwrap_or_else(|message| {
        message_exit("artifacts", parsed.json, &message, None);
    });
    if parsed.json {
        println!(
            "{}",
            render_artifact_inventory_json(&index, &history).unwrap_or_else(|message| {
                message_exit("artifacts", true, &message, None);
            })
        );
    } else {
        print!("{}", render_artifact_inventory_text(&index, &history));
    }
    process::exit(ExitCode::Success.code());
}

#[derive(Default)]
struct ParsedArgs {
    json: bool,
    progress_json: bool,
    model: Option<String>,
    seed: Option<u64>,
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
    suite_models: Vec<String>,
    critical: bool,
    suite_name: Option<String>,
    project_config: Option<ProjectConfig>,
    write_path: Option<String>,
    check: bool,
}

#[derive(Default)]
struct CliArgs {
    manifest_path: Option<String>,
    example: Option<String>,
    bin: Option<String>,
    file: Option<String>,
    command: String,
    model: Option<String>,
    extra_positionals: Vec<String>,
    seed: Option<u64>,
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
    json: bool,
    progress_json: bool,
    suite_models: Vec<String>,
    critical: bool,
    suite_name: Option<String>,
    project_config: Option<ProjectConfig>,
    benchmark_models: Vec<String>,
    write_path: Option<String>,
    check: bool,
}

fn parse_cli(args: Vec<String>) -> CliArgs {
    let json = detect_json_flag(&args);
    let cli = CargoValidCli::try_parse_from(
        std::iter::once("cargo-valid".to_string()).chain(args.clone()),
    )
    .unwrap_or_else(|error| {
        message_exit(
            "cargo-valid",
            json,
            &error.to_string(),
            Some(&primary_usage()),
        )
    });
    let mut parsed = CliArgs::default();
    parsed.json = json;
    parsed.manifest_path = cli.manifest_path;
    parsed.file = cli.registry.or(cli.file);
    parsed.example = cli.example;
    parsed.bin = cli.bin;
    let mut command = cli.command.unwrap_or_else(|| usage_exit(&primary_usage()));
    let mut rest = cli.rest;
    if command == "valid" {
        command = rest
            .first()
            .cloned()
            .unwrap_or_else(|| usage_exit(&primary_usage()));
        rest.remove(0);
    }
    parsed.progress_json = detect_progress_json_flag(&rest);
    parsed.command = normalize_command(&command);
    let mut iter = rest.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--manifest-path" => {
                parsed.manifest_path = Some(next_arg(&mut iter, "--manifest-path"))
            }
            "--registry" => parsed.file = Some(next_arg(&mut iter, "--registry")),
            "--example" => parsed.example = Some(next_arg(&mut iter, "--example")),
            "--bin" => parsed.bin = Some(next_arg(&mut iter, "--bin")),
            "--file" => parsed.file = Some(next_arg(&mut iter, "--file")),
            "--solver-exec" => {
                parsed.solver_executable = Some(next_arg(&mut iter, "--solver-exec"))
            }
            "--solver-arg" => parsed.solver_args.push(next_arg(&mut iter, "--solver-arg")),
            "--json" => parsed.json = true,
            "--progress=json" => parsed.progress_json = true,
            _ if arg.starts_with("--progress=") => usage_exit("`--progress` only supports `json`"),
            "--check" => parsed.check = true,
            "--write" => parsed.write_path = Some(String::new()),
            _ if arg.starts_with("--write=") => {
                parsed.write_path = Some(arg.trim_start_matches("--write=").to_string())
            }
            "--baseline" => parsed.baseline_mode = Some("compare".to_string()),
            _ if arg.starts_with("--baseline=") => {
                parsed.baseline_mode = Some(arg.trim_start_matches("--baseline=").to_string())
            }
            _ if arg.starts_with("--threshold-percent=") => {
                parsed.threshold_percent = Some(
                    arg.trim_start_matches("--threshold-percent=")
                        .parse()
                        .unwrap_or_else(|_| {
                            usage_exit("`--threshold-percent` expects a non-negative integer")
                        }),
                )
            }
            _ if arg.starts_with("--repeat=") => {
                parsed.repeat = arg
                    .trim_start_matches("--repeat=")
                    .parse()
                    .unwrap_or_else(|_| usage_exit("`--repeat` expects a positive integer"))
            }
            _ if arg.starts_with("--strategy=") => {
                parsed.strategy = Some(arg.trim_start_matches("--strategy=").to_string())
            }
            _ if arg.starts_with("--format=") => {
                parsed.format = Some(arg.trim_start_matches("--format=").to_string())
            }
            _ if arg.starts_with("--view=") => {
                parsed.view = Some(arg.trim_start_matches("--view=").to_string())
            }
            _ if arg.starts_with("--property=") => {
                parsed.property_id = Some(arg.trim_start_matches("--property=").to_string())
            }
            "--critical" => parsed.critical = true,
            "--suite" => parsed.suite_name = Some(next_arg(&mut iter, "--suite")),
            _ if arg.starts_with("--suite=") => {
                parsed.suite_name = Some(arg.trim_start_matches("--suite=").to_string())
            }
            _ if arg.starts_with("--seed=") => {
                parsed.seed = Some(
                    arg.trim_start_matches("--seed=")
                        .parse()
                        .unwrap_or_else(|_| usage_exit("`--seed` expects an unsigned integer")),
                )
            }
            "--seed" => {
                parsed.seed = Some(
                    next_arg(&mut iter, "--seed")
                        .parse()
                        .unwrap_or_else(|_| usage_exit("`--seed` expects an unsigned integer")),
                )
            }
            _ if arg.starts_with("--backend=") => {
                parsed.backend = Some(arg.trim_start_matches("--backend=").to_string())
            }
            _ if arg.starts_with("--actions=") => {
                parsed.actions = arg
                    .trim_start_matches("--actions=")
                    .split(',')
                    .filter(|item| !item.is_empty())
                    .map(|item| item.to_string())
                    .collect()
            }
            _ if arg.starts_with("--focus-action=") => {
                parsed.focus_action_id = Some(arg.trim_start_matches("--focus-action=").to_string())
            }
            _ if parsed.model.is_none() => parsed.model = Some(arg),
            _ => parsed.extra_positionals.push(arg),
        }
    }
    if parsed.file.is_some() && (parsed.example.is_some() || parsed.bin.is_some()) {
        usage_exit("use either --file or --example/--bin, not both");
    }
    if parsed.critical && parsed.suite_name.is_some() {
        usage_exit("use either --critical or --suite=<name>, not both");
    }
    parsed
}

fn normalize_command(command: &str) -> String {
    match command {
        "models" => "list",
        "diagram" => "graph",
        "readiness" => "lint",
        "migrate" => "migrate",
        "benchmark" | "bench" => "benchmark",
        "verify" => "check",
        "suite" => "all",
        "generate-tests" => "testgen",
        other => other,
    }
    .to_string()
}

fn next_arg(iter: &mut impl Iterator<Item = String>, flag: &str) -> String {
    iter.next()
        .unwrap_or_else(|| usage_exit(&format!("missing value for {flag}")))
}

fn normalized_model_ref(model: &str) -> String {
    if model.starts_with("rust:") {
        model.to_string()
    } else {
        format!("rust:{model}")
    }
}

fn maybe_auto_discover_external(mut parsed: CliArgs) -> CliArgs {
    if matches!(
        parsed.command.as_str(),
        "clean" | "init" | "help" | "commands" | "schema" | "batch"
    ) {
        return parsed;
    }
    let target_options = ExternalTargetOptions {
        manifest_path: parsed.manifest_path.clone(),
        file: parsed.file.clone(),
        example: parsed.example.clone(),
        bin: parsed.bin.clone(),
    };
    match discover_external_registry_project(&target_options) {
        Ok(discovered) => {
            if let Some(config) = discovered.config {
                let explicit_registry_target =
                    parsed.file.is_some() || parsed.example.is_some() || parsed.bin.is_some();
                apply_project_runtime_config(&config);
                parsed.project_config = Some(config.clone());
                if parsed.backend.is_none() {
                    parsed.backend = config.default_backend.clone();
                }
                if parsed.property_id.is_none() {
                    parsed.property_id = config
                        .default_property
                        .clone()
                        .filter(|value| !value.trim().is_empty());
                }
                if parsed.solver_executable.is_none() {
                    parsed.solver_executable = config
                        .default_solver_executable
                        .clone()
                        .filter(|value| !value.trim().is_empty());
                }
                if parsed.solver_args.is_empty() && !config.default_solver_args.is_empty() {
                    parsed.solver_args = config.default_solver_args.clone();
                }
                if parsed.suite_models.is_empty() && !explicit_registry_target {
                    parsed.suite_models = config.suite_models.clone();
                }
                if parsed.command == "benchmark"
                    && parsed.benchmark_models.is_empty()
                    && !explicit_registry_target
                {
                    parsed.benchmark_models = if config.benchmark_models.is_empty() {
                        config.suite_models.clone()
                    } else {
                        config.benchmark_models.clone()
                    };
                }
                if parsed.command == "benchmark" && parsed.repeat == 0 {
                    parsed.repeat = config.benchmark_repeats.unwrap_or(3);
                }
                if parsed.command == "benchmark" && parsed.threshold_percent.is_none() {
                    parsed.threshold_percent = config.benchmark_regression_threshold_percent;
                }
                if parsed.command == "benchmark" && parsed.baseline_mode.is_none() {
                    parsed.baseline_mode = Some("compare".to_string());
                }
                if !explicit_registry_target {
                    parsed.file = discovered.options.file.clone();
                }
            }
            parsed.manifest_path = discovered.options.manifest_path.clone();
            if parsed.file.is_none() {
                parsed.file = discovered.options.file.clone();
            }
            if parsed.example.is_none() {
                parsed.example = discovered.options.example.clone();
            }
            if parsed.bin.is_none() {
                parsed.bin = discovered.options.bin.clone();
            }
        }
        Err(message) => {
            message_exit("cargo-valid", parsed.json, &message, None);
        }
    }
    if parsed.command == "benchmark" && parsed.repeat == 0 {
        parsed.repeat = 3;
    }
    if parsed.command == "benchmark" && parsed.baseline_mode.is_none() {
        parsed.baseline_mode = Some("compare".to_string());
    }
    parsed
}

fn resolve_external_target(parsed: &CliArgs) -> ExternalTarget {
    resolve_external_registry_target(&ExternalTargetOptions {
        manifest_path: parsed.manifest_path.clone(),
        file: parsed.file.clone(),
        example: parsed.example.clone(),
        bin: parsed.bin.clone(),
    })
    .unwrap_or_else(|message| usage_exit(&message))
}

fn parse_models_json(stdout: &str) -> Vec<String> {
    let start = stdout
        .find('[')
        .unwrap_or_else(|| usage_exit("registry list output did not contain a models array"));
    let end = stdout[start..]
        .find(']')
        .map(|offset| start + offset)
        .unwrap_or_else(|| {
            usage_exit("registry list output did not contain a closing models array")
        });
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

fn usage_exit(usage: &str) -> ! {
    if detect_json_flag(&env::args().skip(1).collect::<Vec<_>>()) {
        eprintln!(
            "{}",
            render_cli_error_json(
                "cargo-valid",
                &[usage_diagnostic("invalid command arguments", usage)],
                Some(usage),
            )
        );
    } else {
        eprintln!("{usage}");
    }
    process::exit(ExitCode::Error.code());
}

fn to_exit_code(code: i32) -> ExitCode {
    match code {
        0 => ExitCode::Success,
        1 => ExitCode::Fail,
        2 => ExitCode::Unknown,
        4 => ExitCode::Unknown,
        5 | 6 => ExitCode::Fail,
        _ => ExitCode::Error,
    }
}

fn preferred_child_json(output: &process::Output) -> Value {
    if !String::from_utf8_lossy(&output.stdout).trim().is_empty() {
        child_stream_to_json(&output.stdout)
    } else {
        child_stream_to_json(&output.stderr)
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

fn diagnostics_exit(
    command: &str,
    json: bool,
    diagnostics: &[valid::support::diagnostics::Diagnostic],
) -> ! {
    if json {
        eprintln!("{}", render_cli_error_json(command, diagnostics, None));
    } else {
        for diagnostic in diagnostics {
            eprintln!("{}", diagnostic.message);
        }
    }
    process::exit(ExitCode::Error.code());
}

fn cmd_commands(json: bool) {
    if json {
        println!("{}", render_commands_json(Surface::CargoValid));
    } else {
        println!("{}", render_commands_text(Surface::CargoValid));
    }
    process::exit(ExitCode::Success.code());
}

fn cmd_schema(parsed: &CliArgs) -> ! {
    let command = parsed.model.as_deref().unwrap_or_else(|| {
        message_exit(
            "schema",
            true,
            "missing command name",
            Some("usage: cargo valid schema <command>"),
        )
    });
    match render_schema_json(Surface::CargoValid, &normalize_command(command)) {
        Ok(body) => println!("{body}"),
        Err(message) => message_exit(
            "schema",
            true,
            &message,
            Some("usage: cargo valid schema <command>"),
        ),
    }
    process::exit(ExitCode::Success.code());
}

fn cmd_batch(parsed: &CliArgs) -> ! {
    let progress = ProgressReporter::new("batch", parsed.progress_json);
    let mut stdin = String::new();
    io::stdin()
        .read_to_string(&mut stdin)
        .unwrap_or_else(|err| {
            message_exit(
                "batch",
                parsed.json,
                &format!("failed to read stdin: {err}"),
                None,
            )
        });
    let request = parse_batch_request(&stdin).unwrap_or_else(|message| {
        message_exit(
            "batch",
            true,
            &message,
            Some("usage: cargo valid batch [--json] [--progress=json] < batch.json"),
        )
    });
    let total = request.operations.len();
    progress.start(Some(total));
    let current_exe = env::current_exe().unwrap_or_else(|err| {
        message_exit(
            "batch",
            parsed.json,
            &format!("failed to resolve current executable: {err}"),
            None,
        )
    });
    let mut aggregate = ExitCode::Success;
    let mut results = Vec::new();
    for (index, operation) in request.operations.into_iter().enumerate() {
        progress.item_start(index, total, &operation.command);
        let mut command_args = Vec::new();
        if let Some(manifest_path) = &parsed.manifest_path {
            command_args.push("--manifest-path".to_string());
            command_args.push(manifest_path.clone());
        }
        if let Some(file) = &parsed.file {
            command_args.push("--file".to_string());
            command_args.push(file.clone());
        }
        if let Some(example) = &parsed.example {
            command_args.push("--example".to_string());
            command_args.push(example.clone());
        }
        if let Some(bin) = &parsed.bin {
            command_args.push("--bin".to_string());
            command_args.push(bin.clone());
        }
        command_args.push(operation.command.clone());
        let mut operation_args = operation.args.clone();
        if operation.json
            && !operation_args
                .iter()
                .any(|arg| arg == "--json" || arg.starts_with("--format="))
        {
            if operation.command == "graph" {
                operation_args.push("--format=json".to_string());
            } else {
                operation_args.push("--json".to_string());
            }
        }
        command_args.extend(operation_args.clone());
        let output = Command::new(&current_exe)
            .args(&command_args)
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
        aggregate = aggregate.aggregate(to_exit_code(exit_code));
        results.push(BatchResult {
            index,
            command: operation.command.clone(),
            args: operation_args,
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

fn clean_root(parsed: &CliArgs) -> PathBuf {
    external_project_root(parsed.manifest_path.as_deref())
}

fn cmd_init(parsed: &CliArgs) -> ! {
    let progress = ProgressReporter::new("init", parsed.progress_json);
    progress.start(None);
    let root = external_project_root(parsed.manifest_path.as_deref());
    let cargo_toml = root.join("Cargo.toml");
    if !cargo_toml.exists() {
        message_exit(
            "init",
            parsed.json,
            &format!("expected Cargo.toml in {}", root.display()),
            None,
        );
    }
    let config_path = root.join("valid.toml");
    if config_path.exists() {
        message_exit(
            "init",
            parsed.json,
            &format!("`{}` already exists", config_path.display()),
            None,
        );
    }
    let registry = parsed.file.as_deref().unwrap_or("examples/valid_models.rs");
    let body = render_project_config_template(registry);
    let registry_path = root.join(registry);
    if let Some(parent) = registry_path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|err| {
            message_exit(
                "init",
                parsed.json,
                &format!("failed to create `{}`: {err}", parent.display()),
                None,
            )
        });
    }
    if !registry_path.exists() {
        fs::write(&registry_path, render_registry_source_template()).unwrap_or_else(|err| {
            message_exit(
                "init",
                parsed.json,
                &format!("failed to write `{}`: {err}", registry_path.display()),
                None,
            )
        });
    }
    let generated_dir = root.join("generated-tests");
    fs::create_dir_all(&generated_dir).unwrap_or_else(|err| {
        message_exit(
            "init",
            parsed.json,
            &format!("failed to create `{}`: {err}", generated_dir.display()),
            None,
        )
    });
    let gitkeep = generated_dir.join(".gitkeep");
    if !gitkeep.exists() {
        fs::write(&gitkeep, "").unwrap_or_else(|err| {
            message_exit(
                "init",
                parsed.json,
                &format!("failed to write `{}`: {err}", gitkeep.display()),
                None,
            )
        });
    }
    let artifacts_dir = root.join("artifacts");
    fs::create_dir_all(&artifacts_dir).unwrap_or_else(|err| {
        message_exit(
            "init",
            parsed.json,
            &format!("failed to create `{}`: {err}", artifacts_dir.display()),
            None,
        )
    });
    let artifacts_gitkeep = artifacts_dir.join(".gitkeep");
    if !artifacts_gitkeep.exists() {
        fs::write(&artifacts_gitkeep, "").unwrap_or_else(|err| {
            message_exit(
                "init",
                parsed.json,
                &format!("failed to write `{}`: {err}", artifacts_gitkeep.display()),
                None,
            )
        });
    }
    let benchmark_baseline_dir = root.join("benchmarks").join("baselines");
    fs::create_dir_all(&benchmark_baseline_dir).unwrap_or_else(|err| {
        message_exit(
            "init",
            parsed.json,
            &format!(
                "failed to create `{}`: {err}",
                benchmark_baseline_dir.display()
            ),
            None,
        )
    });
    let baseline_gitkeep = benchmark_baseline_dir.join(".gitkeep");
    if !baseline_gitkeep.exists() {
        fs::write(&baseline_gitkeep, "").unwrap_or_else(|err| {
            message_exit(
                "init",
                parsed.json,
                &format!("failed to write `{}`: {err}", baseline_gitkeep.display()),
                None,
            )
        });
    }
    let mcp_dir = root.join(".mcp");
    fs::create_dir_all(&mcp_dir).unwrap_or_else(|err| {
        message_exit(
            "init",
            parsed.json,
            &format!("failed to create `{}`: {err}", mcp_dir.display()),
            None,
        )
    });
    let codex_config = mcp_dir.join("codex.toml");
    fs::write(&codex_config, render_bootstrap_codex_config()).unwrap_or_else(|err| {
        message_exit(
            "init",
            parsed.json,
            &format!("failed to write `{}`: {err}", codex_config.display()),
            None,
        )
    });
    let claude_code_config = mcp_dir.join("claude-code.json");
    fs::write(&claude_code_config, render_bootstrap_claude_code_config()).unwrap_or_else(|err| {
        message_exit(
            "init",
            parsed.json,
            &format!("failed to write `{}`: {err}", claude_code_config.display()),
            None,
        )
    });
    let claude_desktop_config = mcp_dir.join("claude-desktop.json");
    fs::write(
        &claude_desktop_config,
        render_bootstrap_claude_desktop_config(),
    )
    .unwrap_or_else(|err| {
        message_exit(
            "init",
            parsed.json,
            &format!(
                "failed to write `{}`: {err}",
                claude_desktop_config.display()
            ),
            None,
        )
    });
    let docs_ai_dir = root.join("docs").join("ai");
    fs::create_dir_all(&docs_ai_dir).unwrap_or_else(|err| {
        message_exit(
            "init",
            parsed.json,
            &format!("failed to create `{}`: {err}", docs_ai_dir.display()),
            None,
        )
    });
    let bootstrap_readme = docs_ai_dir.join("bootstrap.md");
    fs::write(&bootstrap_readme, render_bootstrap_ai_readme()).unwrap_or_else(|err| {
        message_exit(
            "init",
            parsed.json,
            &format!("failed to write `{}`: {err}", bootstrap_readme.display()),
            None,
        )
    });
    fs::write(&config_path, body).unwrap_or_else(|err| {
        message_exit(
            "init",
            parsed.json,
            &format!("failed to write `{}`: {err}", config_path.display()),
            None,
        )
    });
    if parsed.json {
        println!(
            "{{\"status\":\"ok\",\"created\":\"{}\",\"registry\":\"{}\",\"scaffolded_registry\":\"{}\",\"generated_tests_dir\":\"{}\",\"artifacts_dir\":\"{}\",\"benchmarks_baseline_dir\":\"{}\",\"mcp_configs\":[\"{}\",\"{}\",\"{}\"],\"ai_bootstrap_guide\":\"{}\"}}",
            config_path.display(),
            registry,
            registry_path.display(),
            generated_dir.display(),
            artifacts_dir.display(),
            benchmark_baseline_dir.display(),
            codex_config.display(),
            claude_code_config.display(),
            claude_desktop_config.display(),
            bootstrap_readme.display(),
        );
    } else {
        println!("created: {}", config_path.display());
        println!("registry: {registry}");
        println!("scaffolded_registry: {}", registry_path.display());
        println!("generated_tests_dir: {}", generated_dir.display());
        println!("artifacts_dir: {}", artifacts_dir.display());
        println!(
            "benchmarks_baseline_dir: {}",
            benchmark_baseline_dir.display()
        );
        println!("mcp_config_codex: {}", codex_config.display());
        println!("mcp_config_claude_code: {}", claude_code_config.display());
        println!(
            "mcp_config_claude_desktop: {}",
            claude_desktop_config.display()
        );
        println!("ai_bootstrap_guide: {}", bootstrap_readme.display());
    }
    progress.finish(ExitCode::Success);
    process::exit(ExitCode::Success.code());
}

fn clean_generated_tests(root: &Path) -> Vec<String> {
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

fn clean_artifacts(root: &Path) -> Vec<String> {
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

fn write_benchmark_artifact(report_id: &str, body: &str) -> Option<String> {
    let path = benchmark_report_path(report_id);
    write_text_file(&path, body).ok()?;
    let _ = record_artifact(ArtifactRecord {
        artifact_kind: "benchmark_report".to_string(),
        path: path.clone(),
        run_id: report_id.to_string(),
        model_id: None,
        property_id: None,
        evidence_id: None,
        vector_id: None,
        suite_id: None,
    });
    Some(path)
}

fn migration_output_path(model: &str, requested: &str) -> String {
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

fn benchmark_baseline_report_id(model: &str, backend: &str, property_id: Option<&str>) -> String {
    format!(
        "baseline-{}",
        stable_hash_hex(&format!(
            "{}:{}:{}",
            model,
            backend,
            property_id.unwrap_or("")
        ))
        .replace("sha256:", "")
    )
}

fn benchmark_baseline_outputs(
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
        other => usage_exit(&format!(
            "unsupported benchmark baseline mode `{other}`; expected compare, record, or ignore"
        )),
    }
}

fn resolve_project_dir(root: &Path, env_key: &str, default_rel: &str) -> PathBuf {
    env::var(env_key)
        .ok()
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        })
        .unwrap_or_else(|| root.join(default_rel))
}

fn apply_project_runtime_config(config: &ProjectConfig) {
    if let Some(generated_tests_dir) = &config.generated_tests_dir {
        env::set_var("VALID_GENERATED_TESTS_DIR", generated_tests_dir);
    }
    if let Some(artifacts_dir) = &config.artifacts_dir {
        env::set_var("VALID_ARTIFACTS_DIR", artifacts_dir);
    }
    if let Some(benchmarks_dir) = &config.benchmarks_dir {
        env::set_var("VALID_BENCHMARKS_DIR", benchmarks_dir);
    }
    if let Some(benchmark_baseline_dir) = &config.benchmark_baseline_dir {
        env::set_var("VALID_BENCHMARK_BASELINES_DIR", benchmark_baseline_dir);
    }
    if let Some(default_graph_format) = &config.default_graph_format {
        env::set_var("VALID_DEFAULT_GRAPH_FORMAT", default_graph_format);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_keeps_extra_positionals_for_external_commands() {
        let parsed = parse_cli(vec![
            "contract".to_string(),
            "check".to_string(),
            "valid.lock.json".to_string(),
            "--json".to_string(),
        ]);

        assert_eq!(parsed.command, "contract");
        assert_eq!(parsed.model.as_deref(), Some("check"));
        assert_eq!(parsed.extra_positionals, vec!["valid.lock.json"]);
        assert!(parsed.json);
    }

    #[test]
    fn build_external_command_appends_extra_positionals_after_model() {
        let command = build_external_command(&CliArgs {
            manifest_path: Some("Cargo.toml".to_string()),
            file: Some("examples/valid_models.rs".to_string()),
            command: "contract".to_string(),
            model: Some("check".to_string()),
            extra_positionals: vec!["valid.lock.json".to_string()],
            json: true,
            ..CliArgs::default()
        });
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(args.ends_with(&[
            "--".to_string(),
            "contract".to_string(),
            "check".to_string(),
            "valid.lock.json".to_string(),
            "--json".to_string(),
        ]));
    }
}
