use std::{
    env, fs,
    io::{self, Read},
    path::PathBuf,
    process::{self, Command},
};

use clap::{ArgAction, Args, Parser, Subcommand};
use serde_json::Value;
use valid::{
    api::{
        capabilities_response, check_source, distinguish_source, explain_source, inspect_source,
        lint_source, minimize_source, orchestrate_source, render_distinguish_json,
        render_distinguish_text, render_explain_json, render_explain_text, render_inspect_json,
        render_inspect_text, render_lint_json, render_lint_text, testgen_source,
        validate_capabilities_request, validate_capabilities_response, validate_check_request,
        validate_distinguish_request, validate_distinguish_response, validate_explain_response,
        validate_inspect_request, validate_inspect_response, validate_minimize_response,
        validate_orchestrate_request, validate_orchestrate_response, validate_testgen_request,
        validate_testgen_response, CapabilitiesRequest, CapabilitiesResponse, CheckRequest,
        DistinguishRequest, InspectRequest, MinimizeRequest, OrchestrateRequest, TestgenRequest,
    },
    bundled_models::{coverage_bundled_model, is_bundled_model_ref},
    cli::{
        child_stream_to_json, detect_json_flag, detect_progress_json_flag, message_diagnostic,
        parse_batch_request, render_batch_response, render_cli_error_json, render_commands_json,
        render_commands_text, render_schema_json, usage_diagnostic, BatchResult, ExitCode,
        ProgressReporter, Surface,
    },
    conformance::{build_vector_from_actions, render_conformance_report_json, run_conformance},
    contract::{
        build_lock_file, compare_snapshot, parse_lock_file, render_drift_json, render_drift_text,
        render_lock_json, snapshot_model, write_lock_file,
    },
    coverage::{collect_coverage, render_coverage_json, render_coverage_text},
    doc::{
        check_doc, default_doc_path, generate_doc, render_doc_check_json, render_doc_check_text,
        render_doc_json, render_doc_text, write_doc,
    },
    engine::CheckOutcome,
    evidence::{render_outcome_json, render_outcome_text, write_outcome_artifacts},
    external_registry::{
        build_registry_binary, discover_external_project, resolve_external_target,
        ExternalTargetOptions, RegistryBuildOptions,
    },
    frontend::compile_model,
    handoff::{
        check_handoff, default_handoff_path, generate_handoff, render_handoff_check_json,
        render_handoff_check_text, render_handoff_json, render_handoff_text, write_handoff,
        HandoffInputs,
    },
    mcp::{serve_stdio, ServerConfig},
    project::{load_project_config, rerun_recommendations},
    reporter::{
        build_failure_graph_slice, render_model_dot_failure, render_model_dot_with_view,
        render_model_mermaid_failure, render_model_mermaid_with_view, render_model_svg_failure,
        render_model_svg_with_view, render_model_text_failure, render_model_text_with_view,
        render_trace_mermaid, render_trace_sequence_mermaid, GraphView,
    },
    selfcheck::{run_smoke_selfcheck, write_selfcheck_artifact},
    support::artifact_index::{
        load_artifact_index, load_run_history, render_artifact_inventory_json,
        render_artifact_inventory_text,
    },
    testgen::{render_replay_json, replay_path_for_model},
};

#[derive(Parser, Debug)]
#[command(name = "valid", disable_help_flag = true, disable_version_flag = true)]
struct ValidCli {
    #[command(subcommand)]
    command: Option<ValidCommand>,
}

#[derive(Subcommand, Debug)]
enum ValidCommand {
    #[command(alias = "verify")]
    Check(CommonModelArgs),
    Inspect(ModelPathArgs),
    #[command(alias = "diagram")]
    Graph(GraphArgs),
    Doc(DocArgs),
    Handoff(HandoffArgs),
    #[command(alias = "readiness")]
    Lint(ModelPathArgs),
    Capabilities(CapabilitiesArgs),
    Explain(CommonModelArgs),
    Minimize(CommonModelArgs),
    Contract(ContractArgs),
    Trace(TraceArgs),
    Orchestrate(CommonModelArgs),
    #[command(alias = "generate-tests")]
    Testgen(TestgenArgs),
    Distinguish(DistinguishArgs),
    Replay(ReplayArgs),
    Coverage(CommonModelArgs),
    Conformance(ConformanceArgs),
    Artifacts(JsonOnlyArgs),
    Clean(CleanArgs),
    Selfcheck(JsonProgressArgs),
    Mcp(McpArgs),
    Commands(JsonOnlyArgs),
    Schema(SchemaArgs),
    Batch(JsonProgressArgs),
}

#[derive(Args, Debug, Clone)]
struct JsonProgressArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long = "progress", default_value = None)]
    progress: Option<String>,
}

#[derive(Args, Debug, Clone)]
struct JsonOnlyArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

#[derive(Args, Debug, Clone)]
struct ModelPathArgs {
    path: String,
    #[command(flatten)]
    json_progress: JsonProgressArgs,
}

#[derive(Args, Debug, Clone)]
struct CommonModelArgs {
    path: String,
    #[arg(long)]
    property: Option<String>,
    #[arg(long)]
    scenario: Option<String>,
    #[arg(long)]
    seed: Option<u64>,
    #[arg(long)]
    backend: Option<String>,
    #[arg(long = "solver-exec")]
    solver_exec: Option<String>,
    #[arg(long = "solver-arg", allow_hyphen_values = true)]
    solver_args: Vec<String>,
    #[command(flatten)]
    json_progress: JsonProgressArgs,
}

#[derive(Args, Debug, Clone)]
struct GraphArgs {
    path: String,
    #[arg(long)]
    format: Option<String>,
    #[arg(long)]
    view: Option<String>,
    #[arg(long)]
    property: Option<String>,
    #[command(flatten)]
    json_progress: JsonProgressArgs,
}

#[derive(Args, Debug, Clone)]
struct DocArgs {
    path: String,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    write: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    check: bool,
    #[command(flatten)]
    json_progress: JsonProgressArgs,
}

#[derive(Args, Debug, Clone)]
struct HandoffArgs {
    path: String,
    #[arg(long)]
    property: Option<String>,
    #[arg(long)]
    backend: Option<String>,
    #[arg(long = "solver-exec")]
    solver_exec: Option<String>,
    #[arg(long = "solver-arg", allow_hyphen_values = true)]
    solver_args: Vec<String>,
    #[arg(long, num_args = 0..=1, default_missing_value = "")]
    write: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    check: bool,
    #[command(flatten)]
    json_progress: JsonProgressArgs,
}

#[derive(Args, Debug, Clone)]
struct CapabilitiesArgs {
    #[arg(long)]
    backend: Option<String>,
    #[arg(long = "solver-exec")]
    solver_exec: Option<String>,
    #[arg(long = "solver-arg", allow_hyphen_values = true)]
    solver_args: Vec<String>,
    #[command(flatten)]
    json_progress: JsonProgressArgs,
}

#[derive(Args, Debug, Clone)]
struct ContractArgs {
    subcommand: String,
    path: String,
    lock_file: Option<String>,
    #[command(flatten)]
    json_progress: JsonProgressArgs,
}

#[derive(Args, Debug, Clone)]
struct TraceArgs {
    path: String,
    #[arg(long)]
    format: Option<String>,
    #[arg(long)]
    property: Option<String>,
    #[arg(long)]
    scenario: Option<String>,
    #[arg(long)]
    seed: Option<u64>,
    #[arg(long)]
    backend: Option<String>,
    #[arg(long = "solver-exec")]
    solver_exec: Option<String>,
    #[arg(long = "solver-arg", allow_hyphen_values = true)]
    solver_args: Vec<String>,
    #[command(flatten)]
    json_progress: JsonProgressArgs,
}

#[derive(Args, Debug, Clone)]
struct TestgenArgs {
    path: String,
    #[arg(long)]
    property: Option<String>,
    #[arg(long)]
    strategy: Option<String>,
    #[arg(long)]
    seed: Option<u64>,
    #[arg(long)]
    backend: Option<String>,
    #[arg(long = "solver-exec")]
    solver_exec: Option<String>,
    #[arg(long = "solver-arg", allow_hyphen_values = true)]
    solver_args: Vec<String>,
    #[command(flatten)]
    json_progress: JsonProgressArgs,
}

#[derive(Args, Debug, Clone)]
struct DistinguishArgs {
    path: String,
    #[arg(long = "compare")]
    compare_path: Option<String>,
    #[arg(long)]
    property: Option<String>,
    #[arg(long = "compare-property")]
    compare_property: Option<String>,
    #[arg(long = "max-depth")]
    max_depth: Option<usize>,
    #[command(flatten)]
    json_progress: JsonProgressArgs,
}

#[derive(Args, Debug, Clone)]
struct ReplayArgs {
    path: String,
    #[arg(long)]
    property: Option<String>,
    #[arg(long = "focus-action")]
    focus_action: Option<String>,
    #[arg(long, value_delimiter = ',')]
    actions: Vec<String>,
    #[command(flatten)]
    json_progress: JsonProgressArgs,
}

#[derive(Args, Debug, Clone)]
struct ConformanceArgs {
    path: String,
    #[arg(long)]
    runner: String,
    #[arg(long = "runner-arg", allow_hyphen_values = true)]
    runner_args: Vec<String>,
    #[arg(long)]
    property: Option<String>,
    #[arg(long, value_delimiter = ',')]
    actions: Vec<String>,
    #[command(flatten)]
    json_progress: JsonProgressArgs,
}

#[derive(Args, Debug, Clone)]
struct CleanArgs {
    scope: Option<String>,
    #[command(flatten)]
    json_progress: JsonProgressArgs,
}

#[derive(Args, Debug, Clone)]
struct SchemaArgs {
    command: String,
}

#[derive(Args, Debug, Clone)]
struct McpArgs {
    #[arg(long)]
    project: Option<String>,
    #[arg(long = "manifest-path")]
    manifest_path: Option<String>,
    #[arg(long = "registry", alias = "file")]
    registry: Option<String>,
    #[arg(long)]
    example: Option<String>,
    #[arg(long)]
    bin: Option<String>,
    #[arg(long = "model-file")]
    model_file: Option<String>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    locked: bool,
    #[arg(long)]
    offline: bool,
    #[arg(long = "feature")]
    features: Vec<String>,
    #[arg(long = "print-config")]
    print_config: Option<String>,
}

fn main() {
    let raw_args = env::args().collect::<Vec<_>>();
    let json = detect_json_flag(&raw_args);
    let cli = match ValidCli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            message_exit("valid", json, &error.to_string(), None);
        }
    };
    match cli.command {
        Some(ValidCommand::Check(args)) => cmd_check_from_parsed(common_to_parsed(args)),
        Some(ValidCommand::Inspect(args)) => cmd_inspect_from_parsed(model_to_parsed(args)),
        Some(ValidCommand::Graph(args)) => cmd_graph_from_parsed(graph_to_parsed(args)),
        Some(ValidCommand::Doc(args)) => cmd_doc_from_parsed(doc_to_parsed(args)),
        Some(ValidCommand::Handoff(args)) => cmd_handoff_from_parsed(handoff_to_parsed(args)),
        Some(ValidCommand::Lint(args)) => cmd_lint_from_parsed(model_to_parsed(args)),
        Some(ValidCommand::Capabilities(args)) => {
            cmd_capabilities_from_parsed(capabilities_to_parsed(args))
        }
        Some(ValidCommand::Explain(args)) => cmd_explain_from_parsed(common_to_parsed(args)),
        Some(ValidCommand::Minimize(args)) => cmd_minimize_from_parsed(common_to_parsed(args)),
        Some(ValidCommand::Contract(args)) => cmd_contract_from_parsed(args),
        Some(ValidCommand::Trace(args)) => cmd_trace_from_parsed(trace_to_parsed(args)),
        Some(ValidCommand::Orchestrate(args)) => {
            cmd_orchestrate_from_parsed(common_to_parsed(args))
        }
        Some(ValidCommand::Testgen(args)) => cmd_testgen_from_parsed(testgen_to_parsed(args)),
        Some(ValidCommand::Distinguish(args)) => cmd_distinguish_from_args(args),
        Some(ValidCommand::Replay(args)) => cmd_replay_from_parsed(replay_to_parsed(args)),
        Some(ValidCommand::Coverage(args)) => cmd_coverage_from_parsed(common_to_parsed(args)),
        Some(ValidCommand::Conformance(args)) => {
            cmd_conformance_from_parsed(conformance_to_parsed(args))
        }
        Some(ValidCommand::Artifacts(args)) => cmd_artifacts_from_parsed(args),
        Some(ValidCommand::Clean(args)) => cmd_clean_from_parsed(args),
        Some(ValidCommand::Selfcheck(args)) => cmd_selfcheck_from_parsed(args),
        Some(ValidCommand::Mcp(args)) => cmd_mcp_from_parsed(args),
        Some(ValidCommand::Commands(args)) => cmd_commands_from_parsed(args),
        Some(ValidCommand::Schema(args)) => cmd_schema_from_parsed(args),
        Some(ValidCommand::Batch(args)) => cmd_batch_from_parsed(args),
        None => {
            usage_exit(
                "valid",
                json,
                "usage: valid <inspect|graph|doc|readiness|verify|capabilities|explain|minimize|contract|trace|orchestrate|generate-tests|distinguish|replay|coverage|conformance|artifacts|clean|selfcheck|mcp|commands|schema|batch> ...",
            );
        }
    }
}

fn cmd_artifacts_from_parsed(args: JsonOnlyArgs) {
    let index = load_artifact_index().unwrap_or_else(|message| {
        message_exit("artifacts", args.json, &message, None);
    });
    let history = load_run_history().unwrap_or_else(|message| {
        message_exit("artifacts", args.json, &message, None);
    });
    if args.json {
        println!(
            "{}",
            render_artifact_inventory_json(&index, &history).unwrap_or_else(|message| {
                message_exit("artifacts", true, &message, None);
            })
        );
    } else {
        print!("{}", render_artifact_inventory_text(&index, &history));
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

fn common_to_parsed(args: CommonModelArgs) -> ParsedArgs {
    ParsedArgs {
        json: args.json_progress.json,
        progress_json: progress_flag(args.json_progress.progress.as_deref()),
        path: args.path,
        seed: args.seed,
        backend: args.backend,
        solver_executable: args.solver_exec,
        solver_args: args.solver_args,
        property_id: args.property,
        scenario_id: args.scenario,
        ..ParsedArgs::default()
    }
}

fn model_to_parsed(args: ModelPathArgs) -> ParsedArgs {
    ParsedArgs {
        json: args.json_progress.json,
        progress_json: progress_flag(args.json_progress.progress.as_deref()),
        path: args.path,
        ..ParsedArgs::default()
    }
}

fn graph_to_parsed(args: GraphArgs) -> ParsedArgs {
    ParsedArgs {
        json: args.json_progress.json,
        progress_json: progress_flag(args.json_progress.progress.as_deref()),
        path: args.path,
        format: args.format,
        view: args.view,
        property_id: args.property,
        ..ParsedArgs::default()
    }
}

fn doc_to_parsed(args: DocArgs) -> ParsedArgs {
    ParsedArgs {
        json: args.json_progress.json,
        progress_json: progress_flag(args.json_progress.progress.as_deref()),
        path: args.path,
        write_path: args.write,
        check: args.check,
        ..ParsedArgs::default()
    }
}

fn handoff_to_parsed(args: HandoffArgs) -> ParsedArgs {
    ParsedArgs {
        json: args.json_progress.json,
        progress_json: progress_flag(args.json_progress.progress.as_deref()),
        path: args.path,
        backend: args.backend,
        solver_executable: args.solver_exec,
        solver_args: args.solver_args,
        property_id: args.property,
        write_path: args.write,
        check: args.check,
        ..ParsedArgs::default()
    }
}

fn capabilities_to_parsed(args: CapabilitiesArgs) -> ParsedArgs {
    ParsedArgs {
        json: args.json_progress.json,
        progress_json: progress_flag(args.json_progress.progress.as_deref()),
        backend: args.backend,
        solver_executable: args.solver_exec,
        solver_args: args.solver_args,
        ..ParsedArgs::default()
    }
}

fn trace_to_parsed(args: TraceArgs) -> ParsedArgs {
    ParsedArgs {
        json: args.json_progress.json,
        progress_json: progress_flag(args.json_progress.progress.as_deref()),
        path: args.path,
        seed: args.seed,
        backend: args.backend,
        solver_executable: args.solver_exec,
        solver_args: args.solver_args,
        format: args.format,
        property_id: args.property,
        scenario_id: args.scenario,
        ..ParsedArgs::default()
    }
}

fn testgen_to_parsed(args: TestgenArgs) -> ParsedArgs {
    ParsedArgs {
        json: args.json_progress.json,
        progress_json: progress_flag(args.json_progress.progress.as_deref()),
        path: args.path,
        seed: args.seed,
        backend: args.backend,
        solver_executable: args.solver_exec,
        solver_args: args.solver_args,
        property_id: args.property,
        extra: args.strategy,
        ..ParsedArgs::default()
    }
}

fn replay_to_parsed(args: ReplayArgs) -> ParsedArgs {
    ParsedArgs {
        json: args.json_progress.json,
        progress_json: progress_flag(args.json_progress.progress.as_deref()),
        path: args.path,
        property_id: args.property,
        actions: args.actions,
        focus_action_id: args.focus_action,
        ..ParsedArgs::default()
    }
}

fn conformance_to_parsed(args: ConformanceArgs) -> ParsedArgs {
    ParsedArgs {
        json: args.json_progress.json,
        progress_json: progress_flag(args.json_progress.progress.as_deref()),
        path: args.path,
        property_id: args.property,
        actions: args.actions,
        runner: Some(args.runner),
        runner_args: args.runner_args,
        ..ParsedArgs::default()
    }
}

fn progress_flag(progress: Option<&str>) -> bool {
    matches!(progress, Some("json"))
}

fn cmd_check_from_parsed(parsed: ParsedArgs) {
    cmd_check(args_from_parsed(&parsed));
}
fn cmd_explain_from_parsed(parsed: ParsedArgs) {
    cmd_explain(args_from_parsed(&parsed));
}
fn cmd_minimize_from_parsed(parsed: ParsedArgs) {
    cmd_minimize(args_from_parsed(&parsed));
}
fn cmd_inspect_from_parsed(parsed: ParsedArgs) {
    cmd_inspect(args_from_parsed(&parsed));
}
fn cmd_graph_from_parsed(parsed: ParsedArgs) {
    cmd_graph(args_from_parsed(&parsed));
}
fn cmd_doc_from_parsed(parsed: ParsedArgs) {
    cmd_doc(args_from_parsed(&parsed));
}

fn cmd_handoff_from_parsed(parsed: ParsedArgs) {
    cmd_handoff(args_from_parsed(&parsed));
}
fn cmd_lint_from_parsed(parsed: ParsedArgs) {
    cmd_lint(args_from_parsed(&parsed));
}
fn cmd_capabilities_from_parsed(parsed: ParsedArgs) {
    cmd_capabilities(args_from_parsed(&parsed));
}
fn cmd_trace_from_parsed(parsed: ParsedArgs) {
    cmd_trace(args_from_parsed(&parsed));
}
fn cmd_testgen_from_parsed(parsed: ParsedArgs) {
    cmd_testgen(args_from_parsed(&parsed));
}
fn cmd_distinguish_from_args(args: DistinguishArgs) {
    cmd_distinguish(args);
}
fn cmd_replay_from_parsed(parsed: ParsedArgs) {
    cmd_replay(args_from_parsed(&parsed));
}
fn cmd_conformance_from_parsed(parsed: ParsedArgs) {
    cmd_conformance(args_from_parsed(&parsed));
}
fn cmd_orchestrate_from_parsed(parsed: ParsedArgs) {
    cmd_orchestrate(args_from_parsed(&parsed));
}
fn cmd_coverage_from_parsed(parsed: ParsedArgs) {
    cmd_coverage(args_from_parsed(&parsed));
}
fn cmd_selfcheck_from_parsed(args: JsonProgressArgs) {
    cmd_selfcheck(flags_to_args(args));
}
fn cmd_commands_from_parsed(args: JsonOnlyArgs) {
    cmd_commands(if args.json {
        vec!["--json".to_string()]
    } else {
        Vec::new()
    });
}
fn cmd_schema_from_parsed(args: SchemaArgs) {
    cmd_schema(vec![args.command]);
}
fn cmd_batch_from_parsed(args: JsonProgressArgs) {
    cmd_batch(flags_to_args(args));
}

fn cmd_mcp_from_parsed(args: McpArgs) {
    let args = normalized_mcp_args(args).unwrap_or_else(|message| {
        message_exit("mcp", false, &message, None);
    });
    if let Some(client) = &args.print_config {
        println!(
            "{}",
            render_mcp_config(client, &args).unwrap_or_else(|message| {
                message_exit("mcp", false, &message, None);
            })
        );
        process::exit(0);
    }
    let mut config = ServerConfig::default();
    if let Some(name) = args.name.clone() {
        config.server_name = name;
    }
    let build_options = RegistryBuildOptions {
        locked: args.locked,
        offline: args.offline,
        extra_features: args.features.clone(),
    };
    if let Some(model_file) = args.model_file.clone() {
        if args.registry.is_some() || args.example.is_some() || args.bin.is_some() {
            message_exit(
                "mcp",
                false,
                "use either --model-file for DSL mode or --registry/--example/--bin for registry mode",
                None,
            );
        }
        config.default_model_file = Some(model_file);
        if let Some(root) = args
            .model_file
            .as_deref()
            .and_then(|path| std::path::Path::new(path).parent())
        {
            config.project_config = load_project_config(root)
                .unwrap_or_else(|message| message_exit("mcp", false, &message, None));
        }
    } else {
        let discovered = discover_external_project(&ExternalTargetOptions {
            manifest_path: args.manifest_path.clone(),
            file: args.registry.clone(),
            example: args.example.clone(),
            bin: args.bin.clone(),
        })
        .unwrap_or_else(|message| message_exit("mcp", false, &message, None));
        if let Some(manifest_path) = &discovered.options.manifest_path {
            env::set_var("VALID_MCP_MANIFEST_PATH", manifest_path);
        }
        config.project_config = discovered.config.clone();
        let target = resolve_external_target(&discovered.options)
            .unwrap_or_else(|message| message_exit("mcp", false, &message, None));
        let registry_binary = build_registry_binary(&target, &build_options)
            .unwrap_or_else(|message| message_exit("mcp", false, &message, None));
        config.default_registry_binary = Some(registry_binary);
    }
    if let Err(message) = serve_stdio(config) {
        eprintln!("{message}");
        process::exit(1);
    }
    process::exit(0);
}

fn normalized_mcp_args(mut args: McpArgs) -> Result<McpArgs, String> {
    if args.project.is_some() && args.manifest_path.is_some() {
        return Err("use either --project or --manifest-path, not both".to_string());
    }
    if let Some(project) = args.project.take() {
        args.manifest_path = Some(
            PathBuf::from(project)
                .join("Cargo.toml")
                .to_string_lossy()
                .to_string(),
        );
    }
    Ok(args)
}

fn render_mcp_config(client: &str, args: &McpArgs) -> Result<String, String> {
    let server_name = args.name.as_deref().unwrap_or("valid");
    let command = "valid";
    let mut command_args = vec!["mcp".to_string()];
    if let Some(name) = &args.name {
        command_args.push("--name".to_string());
        command_args.push(name.clone());
    }
    if args.locked {
        command_args.push("--locked".to_string());
    }
    if args.offline {
        command_args.push("--offline".to_string());
    }
    for feature in &args.features {
        command_args.push("--feature".to_string());
        command_args.push(feature.clone());
    }
    if let Some(model_file) = &args.model_file {
        command_args.push("--model-file".to_string());
        command_args.push(model_file.clone());
    } else {
        if let Some(manifest_path) = &args.manifest_path {
            command_args.push("--manifest-path".to_string());
            command_args.push(manifest_path.clone());
        }
        if let Some(registry) = &args.registry {
            command_args.push("--registry".to_string());
            command_args.push(registry.clone());
        } else if let Some(example) = &args.example {
            command_args.push("--example".to_string());
            command_args.push(example.clone());
        } else if let Some(bin) = &args.bin {
            command_args.push("--bin".to_string());
            command_args.push(bin.clone());
        }
    }
    if command_args.len() == 1 {
        return Err(
            "print-config requires either --model-file or a registry project target via --project/--manifest-path/--registry/--example/--bin"
                .to_string(),
        );
    }

    match client {
        "codex" => Ok(format!(
            "[mcp_servers.{server_name}]\ncommand = {command:?}\nargs = [{}]\n",
            command_args
                .iter()
                .map(|arg| format!("{arg:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        "claude-code" | "claude" => Ok(format!(
            "{{\n  \"mcpServers\": {{\n    {server_name:?}: {{\n      \"command\": {command:?},\n      \"args\": [{}]\n    }}\n  }}\n}}",
            command_args
                .iter()
                .map(|arg| format!("{arg:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        "claude-desktop" => Ok(format!(
            "{{\n  \"mcpServers\": {{\n    {server_name:?}: {{\n      \"command\": {command:?},\n      \"args\": [{}],\n      \"env\": {{}}\n    }}\n  }}\n}}",
            command_args
                .iter()
                .map(|arg| format!("{arg:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        _ => Err(
            "unknown client for --print-config; expected codex, claude-code, or claude-desktop"
                .to_string(),
        ),
    }
}
fn cmd_clean_from_parsed(args: CleanArgs) {
    let mut values = flags_to_args(args.json_progress);
    if let Some(scope) = args.scope {
        values.insert(0, scope);
    }
    cmd_clean(values);
}
fn cmd_contract_from_parsed(args: ContractArgs) {
    let mut values = vec![args.subcommand, args.path];
    if let Some(lock_file) = args.lock_file {
        values.push(lock_file);
    }
    values.extend(flags_to_args(args.json_progress));
    cmd_contract(values);
}

fn flags_to_args(args: JsonProgressArgs) -> Vec<String> {
    let mut values = Vec::new();
    if args.json {
        values.push("--json".to_string());
    }
    if progress_flag(args.progress.as_deref()) {
        values.push("--progress=json".to_string());
    }
    values
}

fn args_from_parsed(parsed: &ParsedArgs) -> Vec<String> {
    let mut args = Vec::new();
    if !parsed.path.is_empty() {
        args.push(parsed.path.clone());
    }
    if let Some(property_id) = &parsed.property_id {
        args.push(format!("--property={property_id}"));
    }
    if let Some(scenario_id) = &parsed.scenario_id {
        args.push(format!("--scenario={scenario_id}"));
    }
    if let Some(seed) = parsed.seed {
        args.push(format!("--seed={seed}"));
    }
    if let Some(backend) = &parsed.backend {
        args.push(format!("--backend={backend}"));
    }
    if let Some(solver_exec) = &parsed.solver_executable {
        args.push("--solver-exec".to_string());
        args.push(solver_exec.clone());
    }
    for solver_arg in &parsed.solver_args {
        args.push("--solver-arg".to_string());
        args.push(solver_arg.clone());
    }
    if let Some(format) = &parsed.format {
        args.push(format!("--format={format}"));
    }
    if let Some(view) = &parsed.view {
        args.push(format!("--view={view}"));
    }
    if let Some(focus_action_id) = &parsed.focus_action_id {
        args.push(format!("--focus-action={focus_action_id}"));
    }
    if !parsed.actions.is_empty() {
        args.push(format!("--actions={}", parsed.actions.join(",")));
    }
    if let Some(runner) = &parsed.runner {
        args.push(format!("--runner={runner}"));
    }
    for runner_arg in &parsed.runner_args {
        args.push("--runner-arg".to_string());
        args.push(runner_arg.clone());
    }
    if let Some(extra) = &parsed.extra {
        args.push(format!("--strategy={extra}"));
    }
    if let Some(write_path) = &parsed.write_path {
        if write_path.is_empty() {
            args.push("--write".to_string());
        } else {
            args.push(format!("--write={write_path}"));
        }
    }
    if parsed.check {
        args.push("--check".to_string());
    }
    if parsed.json {
        args.push("--json".to_string());
    }
    if parsed.progress_json {
        args.push("--progress=json".to_string());
    }
    args
}

fn cmd_check(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid check <model-file> [--json] [--progress=json] [--property=<id>] [--scenario=<id>] [--seed=<u64>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
    );
    let progress = ProgressReporter::new("check", parsed.progress_json);
    progress.start(None);
    let source = read_source(&parsed.path, "check", parsed.json);
    let request = CheckRequest {
        request_id: "req-local-0001".to_string(),
        source_name: parsed.path.clone(),
        source,
        property_id: parsed.property_id.clone(),
        scenario_id: parsed.scenario_id.clone(),
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
        "usage: valid explain <model-file> [--json] [--progress=json] [--property=<id>] [--scenario=<id>] [--seed=<u64>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
    );
    let progress = ProgressReporter::new("explain", parsed.progress_json);
    progress.start(None);
    let source = read_source(&parsed.path, "explain", parsed.json);
    match explain_source(&CheckRequest {
        request_id: "req-local-explain".to_string(),
        source_name: parsed.path.clone(),
        source,
        property_id: parsed.property_id.clone(),
        scenario_id: parsed.scenario_id.clone(),
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
        "usage: valid graph <model-file> [--format=mermaid|dot|svg|text|json] [--view=overview|logic|failure|deadlock|scc] [--property=<id>] [--json] [--progress=json]",
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
        Ok(response) => {
            match render_graph_output(&response, &request, &parsed, render_format, view, "graph") {
                Ok(body) => print!("{body}"),
                Err(message) => message_exit("graph", json_output, &message, None),
            }
        }
        Err(diagnostics) => diagnostics_exit("graph", json_output, &diagnostics, None),
    }
    progress.finish(ExitCode::Success);
}

fn render_graph_output(
    response: &valid::api::InspectResponse,
    request: &InspectRequest,
    parsed: &ParsedArgs,
    render_format: &str,
    view: GraphView,
    command: &str,
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
        request_id: format!("req-{command}-failure"),
        source_name: request.source_name.clone(),
        source: request.source.clone(),
        property_id: Some(property_id.clone()),
        scenario_id: parsed.scenario_id.clone(),
        seed: parsed.seed,
        backend: parsed.backend.clone(),
        solver_executable: parsed.solver_executable.clone(),
        solver_args: parsed.solver_args.clone(),
    });
    let result = match outcome {
        CheckOutcome::Completed(result) => result,
        CheckOutcome::Errored(error) => {
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
            body["graph_slice"] = serde_json::json!({
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
                serde_json::to_string(&body)
                    .map_err(|err| format!("failed to render graph json: {err}"))?
            )
        }
        "text" => render_model_text_failure(response, &slice),
        "dot" => format!("{}\n", render_model_dot_failure(response, &slice)),
        "svg" => format!("{}\n", render_model_svg_failure(response, &slice)),
        _ => format!("{}\n", render_model_mermaid_failure(response, &slice)),
    })
}

fn cmd_doc(args: Vec<String>) {
    let parsed = parse_common_args(
        args,
        "usage: valid doc <model-file> [--json] [--progress=json] [--write[=<path>]] [--check]",
    );
    let progress = ProgressReporter::new("doc", parsed.progress_json);
    progress.start(None);
    let source = read_source(&parsed.path, "doc", parsed.json);
    let request = InspectRequest {
        request_id: "req-local-doc".to_string(),
        source_name: parsed.path.clone(),
        source: source.clone(),
    };
    let inspect = inspect_source(&request).unwrap_or_else(|diagnostics| {
        diagnostics_exit("doc", parsed.json, &diagnostics, None);
    });
    let mermaid = render_model_mermaid_with_view(&inspect, GraphView::Overview);
    let source_hash = if source.is_empty() {
        valid::support::hash::stable_hash_hex(&inspect.model_id)
    } else {
        valid::support::hash::stable_hash_hex(&source)
    };
    let contract_hash = if source.is_empty() {
        valid::support::hash::stable_hash_hex(&format!(
            "{}|{}|{}|{}",
            inspect.model_id,
            inspect.state_fields.join(","),
            inspect.actions.join(","),
            inspect.properties.join(",")
        ))
    } else {
        match compile_model(&source) {
            Ok(model) => snapshot_model(&model).contract_hash,
            Err(diagnostics) => diagnostics_exit("doc", parsed.json, &diagnostics, None),
        }
    };
    let generated = generate_doc(&inspect, mermaid, source_hash, contract_hash);
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

fn cmd_handoff(args: Vec<String>) {
    let parsed = parse_common_args_with(
        args,
        "usage: valid handoff <model-file> [--json] [--progress=json] [--property=<id>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>] [--write[=<path>]] [--check]",
        |_arg, _parsed| false,
    );
    let progress = ProgressReporter::new("handoff", parsed.progress_json);
    progress.start(None);
    let source = read_source(&parsed.path, "handoff", parsed.json);
    let request = InspectRequest {
        request_id: "req-local-handoff".to_string(),
        source_name: parsed.path.clone(),
        source: source.clone(),
    };
    let inspect = inspect_source(&request).unwrap_or_else(|diagnostics| {
        diagnostics_exit("handoff", parsed.json, &diagnostics, None);
    });
    let orchestrated = orchestrate_source(&OrchestrateRequest {
        request_id: "req-local-handoff-orchestrate".to_string(),
        source_name: parsed.path.clone(),
        source: source.clone(),
        seed: None,
        backend: parsed.backend.clone(),
        solver_executable: parsed.solver_executable.clone(),
        solver_args: parsed.solver_args.clone(),
    })
    .unwrap_or_else(|error| diagnostics_exit("handoff", parsed.json, &error.diagnostics, None));
    let explanations = inspect
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
                request_id: format!("req-local-handoff-explain-{property_id}"),
                source_name: parsed.path.clone(),
                source: source.clone(),
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
    let coverage = if let Some(report) = orchestrated.aggregate_coverage.clone() {
        report
    } else if is_bundled_model_ref(&parsed.path) {
        coverage_bundled_model(&parsed.path)
            .unwrap_or_else(|message| message_exit("handoff", parsed.json, &message, None))
    } else {
        let model = compile_model(&source).unwrap_or_else(|diagnostics| {
            diagnostics_exit("handoff", parsed.json, &diagnostics, None)
        });
        let check = check_source(&CheckRequest {
            request_id: "req-local-handoff-coverage".to_string(),
            source_name: parsed.path.clone(),
            source: source.clone(),
            property_id: parsed.property_id.clone(),
            scenario_id: None,
            seed: None,
            backend: parsed.backend.clone(),
            solver_executable: parsed.solver_executable.clone(),
            solver_args: parsed.solver_args.clone(),
        });
        match check {
            CheckOutcome::Completed(result) => {
                let traces = result.trace.into_iter().collect::<Vec<_>>();
                collect_coverage(&model, &traces)
            }
            CheckOutcome::Errored(error) => {
                diagnostics_exit("handoff", parsed.json, &error.diagnostics, None)
            }
        }
    };
    let source_hash = if source.is_empty() {
        valid::support::hash::stable_hash_hex(&inspect.model_id)
    } else {
        valid::support::hash::stable_hash_hex(&source)
    };
    let contract_hash = if source.is_empty() {
        valid::support::hash::stable_hash_hex(&format!(
            "{}|{}|{}|{}",
            inspect.model_id,
            inspect.state_fields.join(","),
            inspect.actions.join(","),
            inspect.properties.join(",")
        ))
    } else {
        match compile_model(&source) {
            Ok(model) => snapshot_model(&model).contract_hash,
            Err(diagnostics) => diagnostics_exit("handoff", parsed.json, &diagnostics, None),
        }
    };
    let generated = generate_handoff(HandoffInputs {
        inspect: &inspect,
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
                println!("builtin: {}", response.capabilities.builtin);
                println!("compiled_in: {}", response.capabilities.compiled_in);
                println!("available: {}", response.capabilities.available);
                if let Some(reason) = &response.capabilities.availability_reason {
                    println!("availability_reason: {reason}");
                }
                if let Some(remediation) = &response.capabilities.remediation {
                    println!("remediation: {remediation}");
                }
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
                println!("temporal.status: {}", response.capabilities.temporal.status);
                println!(
                    "temporal.semantics: {}",
                    response.capabilities.temporal.semantics
                );
                println!(
                    "temporal.assurance_levels: {}",
                    response.capabilities.temporal.assurance_levels.join(", ")
                );
                println!(
                    "temporal.supported_operators: {}",
                    response
                        .capabilities
                        .temporal
                        .supported_operators
                        .join(", ")
                );
                println!(
                    "temporal.unsupported_operators: {}",
                    response
                        .capabilities
                        .temporal
                        .unsupported_operators
                        .join(", ")
                );
                if !response.capabilities.temporal.notes.is_empty() {
                    println!(
                        "temporal.notes: {}",
                        response.capabilities.temporal.notes.join(" | ")
                    );
                }
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
            let mut drift = compare_snapshot(&expected, &snapshot);
            let project_config = std::path::Path::new(&path)
                .parent()
                .and_then(|root| load_project_config(root).ok().flatten());
            let recommendations = project_config
                .as_ref()
                .map(|config| rerun_recommendations(config, &snapshot.model_id))
                .unwrap_or_default();
            drift.affected_critical_properties = recommendations.affected_critical_properties;
            drift.affected_property_suites = recommendations.affected_property_suites;
            drift.affected_artifacts = recommendations.affected_artifacts;
            drift.repair_surfaces = recommendations.repair_surfaces;
            drift.suggested_reruns = recommendations.suggested_reruns;
            if json {
                println!("{}", render_drift_json(&drift));
            } else {
                print!("{}", render_drift_text(&drift));
            }
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
                            "{{\"vector_id\":\"{}\",\"run_id\":\"{}\",\"strictness\":\"{}\",\"derivation\":\"{}\",\"source_kind\":\"{}\",\"strategy\":\"{}\"}}",
                            vector.vector_id,
                            vector.run_id,
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
                        "  {} run_id={} strictness={} derivation={} source={} strategy={}",
                        vector.vector_id,
                        vector.run_id,
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

fn cmd_distinguish(args: DistinguishArgs) {
    let progress = ProgressReporter::new(
        "distinguish",
        progress_flag(args.json_progress.progress.as_deref()),
    );
    progress.start(None);
    let source = read_source(&args.path, "distinguish", args.json_progress.json);
    let compare_source_name = args
        .compare_path
        .clone()
        .unwrap_or_else(|| args.path.clone());
    let compare_source = args
        .compare_path
        .as_deref()
        .map(|path| read_source(path, "distinguish", args.json_progress.json));
    let request = DistinguishRequest {
        request_id: "req-local-distinguish".to_string(),
        source_name: args.path.clone(),
        source,
        compare_source_name: Some(compare_source_name),
        compare_source,
        property_id: args.property.clone(),
        compare_property_id: args.compare_property.clone(),
        max_depth: args.max_depth,
    };
    if let Err(message) = validate_distinguish_request(&request) {
        message_exit("distinguish", args.json_progress.json, &message, None);
    }
    match distinguish_source(&request) {
        Ok(response) => {
            if let Err(message) = validate_distinguish_response(&response) {
                message_exit("distinguish", args.json_progress.json, &message, None);
            }
            if args.json_progress.json {
                println!("{}", render_distinguish_json(&response));
            } else {
                println!("{}", render_distinguish_text(&response));
            }
            progress.finish(ExitCode::Success);
        }
        Err(error) => diagnostics_exit(
            "distinguish",
            args.json_progress.json,
            &error.diagnostics,
            None,
        ),
    }
}

fn cmd_trace(args: Vec<String>) {
    let parsed = parse_common_args_with(
        args,
        "usage: valid trace <model-file> [--format=mermaid-state|mermaid-sequence|json] [--property=<id>] [--scenario=<id>] [--seed=<u64>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>] [--json] [--progress=json]",
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
        scenario_id: parsed.scenario_id.clone(),
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

fn cmd_conformance(args: Vec<String>) {
    let parsed = parse_common_args_with(
        args,
        "usage: valid conformance <model-file> --runner <path> [--runner-arg <arg>] [--json] [--progress=json] [--property=<id>] [--actions=a,b,c]",
        |arg, parsed| {
            if let Some(value) = arg.strip_prefix("--actions=") {
                parsed.actions = value
                    .split(',')
                    .filter(|item| !item.is_empty())
                    .map(|item| item.to_string())
                    .collect();
                true
            } else if let Some(value) = arg.strip_prefix("--runner=") {
                parsed.runner = Some(value.to_string());
                true
            } else {
                false
            }
        },
    );
    let progress = ProgressReporter::new("conformance", parsed.progress_json);
    progress.start(None);
    let runner = parsed.runner.clone().unwrap_or_else(|| {
        usage_exit(
            "conformance",
            parsed.json,
            "usage: valid conformance <model-file> --runner <path> [--runner-arg <arg>] [--json] [--progress=json] [--property=<id>] [--actions=a,b,c]",
        )
    });
    let source = read_source(&parsed.path, "conformance", parsed.json);
    let model = compile_model(&source).unwrap_or_else(|diagnostics| {
        diagnostics_exit("conformance", parsed.json, &diagnostics, None)
    });
    let vector = build_vector_from_actions(&model, parsed.property_id.as_deref(), &parsed.actions)
        .unwrap_or_else(|message| message_exit("conformance", parsed.json, &message, None));
    let report = run_conformance(&vector, &runner, &parsed.runner_args)
        .unwrap_or_else(|message| message_exit("conformance", parsed.json, &message, None));
    if parsed.json {
        println!(
            "{}",
            render_conformance_report_json(&report).unwrap_or_else(|message| message_exit(
                "conformance",
                true,
                &message,
                None
            ))
        );
    } else {
        println!("vector_id: {}", report.vector_id);
        println!("runner: {}", report.runner);
        println!("status: {}", report.status);
        println!("mismatch_count: {}", report.mismatch_count);
        if !report.mismatch_categories.is_empty() {
            println!(
                "mismatch_categories: {}",
                report.mismatch_categories.join(",")
            );
        }
        for mismatch in &report.mismatches {
            println!(
                "mismatch {} fix_surface={}{}",
                mismatch.kind.as_str(),
                mismatch.likely_fix_surface,
                mismatch
                    .index
                    .map(|index| format!(" step={index}"))
                    .unwrap_or_default()
            );
            println!("  {}", mismatch.summary);
        }
        for mismatch in &report.observation_mismatches {
            println!(
                "step {} expected {:?} actual {:?}",
                mismatch.index, mismatch.expected, mismatch.actual
            );
        }
        if report.expected_property_holds != report.actual_property_holds {
            println!(
                "property_holds expected {:?} actual {:?}",
                report.expected_property_holds, report.actual_property_holds
            );
        }
    }
    let exit_code = if report.status == "PASS" {
        ExitCode::Success
    } else {
        ExitCode::Fail
    };
    progress.finish(exit_code);
    process::exit(exit_code.code());
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
        "usage: valid coverage <model-file> [--json] [--progress=json] [--property=<id>] [--scenario=<id>] [--seed=<u64>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
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
        scenario_id: parsed.scenario_id.clone(),
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
    scenario_id: Option<String>,
    actions: Vec<String>,
    focus_action_id: Option<String>,
    runner: Option<String>,
    runner_args: Vec<String>,
    extra: Option<String>,
    write_path: Option<String>,
    check: bool,
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
        } else if let Some(value) = arg.strip_prefix("--scenario=") {
            parsed.scenario_id = Some(value.to_string());
        } else if arg == "--write" {
            parsed.write_path = Some(String::new());
        } else if let Some(value) = arg.strip_prefix("--write=") {
            parsed.write_path = Some(value.to_string());
        } else if arg == "--check" {
            parsed.check = true;
        } else if arg == "--seed" {
            parsed.seed = Some(parse_seed_arg(
                &iter.next().unwrap_or_else(|| {
                    eprintln!("{usage}");
                    process::exit(3);
                }),
                usage,
            ));
        } else if arg == "--scenario" {
            parsed.scenario_id = Some(
                iter.next()
                    .unwrap_or_else(|| usage_exit("valid", parsed.json, usage)),
            );
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
        } else if arg == "--runner-arg" {
            parsed.runner_args.push(
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
        "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"backend\":\"{}\",\"capabilities\":{{\"backend_name\":\"{}\",\"builtin\":{},\"compiled_in\":{},\"available\":{},\"availability_reason\":{},\"remediation\":{},\"supports_explicit\":{},\"supports_bmc\":{},\"supports_certificate\":{},\"supports_trace\":{},\"supports_witness\":{},\"selfcheck_compatible\":{},\"temporal\":{{\"status\":\"{}\",\"semantics\":\"{}\",\"assurance_levels\":{},\"supported_operators\":{},\"unsupported_operators\":{},\"notes\":{}}}}}}}",
        response.schema_version,
        response.request_id,
        response.backend,
        response.capabilities.backend_name,
        response.capabilities.builtin,
        response.capabilities.compiled_in,
        response.capabilities.available,
        render_optional_string(response.capabilities.availability_reason.as_deref()),
        render_optional_string(response.capabilities.remediation.as_deref()),
        response.capabilities.supports_explicit,
        response.capabilities.supports_bmc,
        response.capabilities.supports_certificate,
        response.capabilities.supports_trace,
        response.capabilities.supports_witness,
        response.capabilities.selfcheck_compatible,
        response.capabilities.temporal.status,
        response.capabilities.temporal.semantics,
        render_string_array(&response.capabilities.temporal.assurance_levels),
        render_string_array(&response.capabilities.temporal.supported_operators),
        render_string_array(&response.capabilities.temporal.unsupported_operators),
        render_string_array(&response.capabilities.temporal.notes),
    );
}

fn render_optional_string(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")),
        None => "null".to_string(),
    }
}

fn render_string_array(values: &[String]) -> String {
    let body = values
        .iter()
        .map(|value| format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{}]", body)
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
