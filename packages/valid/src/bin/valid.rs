use std::{
    env, fs,
    io::{self, Read, Write},
    path::PathBuf,
    process::{self, Command},
};

use clap::{ArgAction, Args, Parser, Subcommand};
use serde::Serialize;
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
    bundled_models::{coverage_bundled_model, is_bundled_model_ref, list_bundled_models},
    cli::{
        child_stream_to_json, detect_json_flag, detect_progress_json_flag, install_completion,
        message_diagnostic, parse_batch_request, render_batch_response, render_cli_error_json,
        render_command_help, render_commands_json, render_commands_text, render_completion,
        render_schema_json, render_surface_help, set_plain_text_output, text_bullet, text_command,
        text_header, text_hint, text_kv, text_section, text_status_badge, usage_diagnostic,
        BatchResult, ExitCode, ProgressReporter, Surface,
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
    project::{
        check_project_init, load_project_config, repair_project_init, rerun_recommendations,
        scaffold_project_init,
    },
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
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    plain: bool,
    #[command(subcommand)]
    command: Option<ValidCommand>,
}

#[derive(Subcommand, Debug)]
enum ValidCommand {
    Init(InitArgs),
    Onboarding(OnboardingArgs),
    Doctor(JsonProgressArgs),
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
    Completion(CompletionArgs),
    Schema(SchemaArgs),
    Batch(JsonProgressArgs),
}

#[derive(Args, Debug, Clone)]
struct CompletionArgs {
    #[arg(allow_hyphen_values = true)]
    args: Vec<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    shell_config: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    stdout: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

#[derive(Args, Debug, Clone)]
struct JsonProgressArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long = "progress", default_value = None)]
    progress: Option<String>,
}

#[derive(Args, Debug, Clone)]
struct InitArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long = "progress", default_value = None)]
    progress: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    check: bool,
    #[arg(long, action = ArgAction::SetTrue, conflicts_with = "check")]
    repair: bool,
}

#[derive(Args, Debug, Clone)]
struct OnboardingArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
    #[arg(long = "progress", default_value = None)]
    progress: Option<String>,
    #[arg(long = "non-interactive", action = ArgAction::SetTrue)]
    non_interactive: bool,
}

#[derive(Args, Debug, Clone)]
struct JsonOnlyArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

#[derive(Debug, Clone, Serialize)]
struct OnboardingStageGuide {
    stage_id: String,
    title: String,
    command: String,
    purpose: String,
    effect_kind: String,
    effect_summary: String,
    key_paths: Vec<String>,
    expected_result: String,
    writes_repo_state: bool,
}

#[derive(Debug, Serialize)]
struct OnboardingStageReport {
    #[serde(flatten)]
    guide: OnboardingStageGuide,
    status: String,
    summary: String,
    stdout_excerpt: Option<String>,
    repair_hint: Option<String>,
}

#[derive(Debug, Serialize)]
struct OnboardingReport {
    status: String,
    root: String,
    interactive: bool,
    cargo_project_detected: bool,
    valid_project_detected: bool,
    overview: Vec<OnboardingStageGuide>,
    stages: Vec<OnboardingStageReport>,
    next_paths: Vec<String>,
    next_path_summaries: Vec<OnboardingNextPathSummary>,
}

#[derive(Debug, Serialize)]
struct OnboardingNextPathSummary {
    path_id: String,
    summary: String,
}

#[derive(Debug, Serialize)]
struct DoctorCheckReport {
    check_id: String,
    status: String,
    summary: String,
    details: Option<String>,
    repair_hint: Option<String>,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    status: String,
    root: String,
    active_shell: Option<String>,
    active_shell_source: String,
    checks: Vec<DoctorCheckReport>,
}

#[derive(Debug, Clone)]
struct ActiveShellResolution {
    active_shell: Option<String>,
    source: String,
    login_shell: Option<String>,
}

#[derive(Clone, Copy)]
enum OnboardingProgram {
    Synthetic,
    CurrentExe,
    Cargo,
}

#[derive(Clone, Copy)]
struct OnboardingStageDescriptor {
    stage_id: &'static str,
    title: &'static str,
    command: &'static str,
    purpose: &'static str,
    effect_kind: &'static str,
    effect_summary: &'static str,
    key_paths: &'static [&'static str],
    expected_result: &'static str,
    writes_repo_state: bool,
    program: OnboardingProgram,
    args: &'static [&'static str],
    repair_hint: Option<&'static str>,
}

const ONBOARDING_STAGE_DESCRIPTORS: &[OnboardingStageDescriptor] = &[
    OnboardingStageDescriptor {
        stage_id: "detect_environment",
        title: "Detect Project Context",
        command: "detect environment",
        purpose: "Check whether this directory already looks like a Cargo/valid project so onboarding can decide whether to bootstrap or skip straight to review.",
        effect_kind: "context_detection",
        effect_summary: "Reads the current directory layout and decides whether scaffold creation is needed.",
        key_paths: &["Cargo.toml", "valid.toml"],
        expected_result: "Onboarding chooses between creating the starter scaffold or reusing the current project.",
        writes_repo_state: false,
        program: OnboardingProgram::Synthetic,
        args: &[],
        repair_hint: None,
    },
    OnboardingStageDescriptor {
        stage_id: "bootstrap_project",
        title: "Bootstrap Project",
        command: "valid init",
        purpose: "Create the starter valid project scaffold, including the registry entrypoint, starter model, local MCP snippets, and artifact directories.",
        effect_kind: "scaffold_write",
        effect_summary: "Creates starter project files and directories in this repository without asking you to author model logic yet.",
        key_paths: &[
            "valid.toml",
            "valid/registry.rs",
            "valid/models/approval.rs",
            "src/main.rs",
            ".mcp/codex.toml",
            "docs/ai/bootstrap.md",
            "docs/rdd/README.md",
            "generated-tests/",
            "artifacts/",
            "benchmarks/baselines/",
        ],
        expected_result: "This directory becomes a runnable starter valid project with one reviewable model.",
        writes_repo_state: true,
        program: OnboardingProgram::CurrentExe,
        args: &["init", "--json"],
        repair_hint: None,
    },
    OnboardingStageDescriptor {
        stage_id: "check_scaffold",
        title: "Check Scaffold",
        command: "valid init --check --json",
        purpose: "Validate that the expected starter files still exist and that the supported layout is intact before review commands run.",
        effect_kind: "scaffold_validation",
        effect_summary: "Performs a read-only health check over the scaffolded layout.",
        key_paths: &[
            "valid.toml",
            "valid/registry.rs",
            "valid/models/approval.rs",
            ".mcp/codex.toml",
            "docs/ai/bootstrap.md",
            "docs/rdd/README.md",
        ],
        expected_result: "You learn whether the scaffold is healthy or needs repair before continuing.",
        writes_repo_state: false,
        program: OnboardingProgram::CurrentExe,
        args: &["init", "--check", "--json"],
        repair_hint: Some("Run `valid doctor` to inspect the scaffold state, then use `valid init --repair` for any safe missing files."),
    },
    OnboardingStageDescriptor {
        stage_id: "warm_project_build",
        title: "Warm Project Build",
        command: "cargo build --quiet",
        purpose: "Warm the local Cargo build so the first review commands focus on model output instead of dependency resolution and compile noise.",
        effect_kind: "build_warmup",
        effect_summary: "Builds the starter project, may create `Cargo.lock`, and populates local Cargo/target artifacts for faster follow-up commands.",
        key_paths: &["Cargo.toml", "Cargo.lock", "target/"],
        expected_result: "Later `cargo valid ...` steps run with much less build noise and fail earlier if the toolchain is unhealthy.",
        writes_repo_state: true,
        program: OnboardingProgram::Cargo,
        args: &["build", "--quiet"],
        repair_hint: Some("Run `valid doctor` to check Cargo and PATH, then rerun `cargo build` once the project toolchain is healthy."),
    },
    OnboardingStageDescriptor {
        stage_id: "list_models",
        title: "List Starter Models",
        command: "cargo valid models",
        purpose: "Confirm that the registry loads and exposes the scaffolded starter model before deeper inspection.",
        effect_kind: "registry_review",
        effect_summary: "Runs a read-only registry listing to prove the starter model is discoverable.",
        key_paths: &["valid/registry.rs", "valid/models/mod.rs", "valid/models/approval.rs"],
        expected_result: "You see `approval-model` in the registry output.",
        writes_repo_state: false,
        program: OnboardingProgram::Cargo,
        args: &["valid", "models"],
        repair_hint: Some("Run `valid doctor` to check Cargo and PATH, then rerun `cargo valid models` once project-first loading is healthy."),
    },
    OnboardingStageDescriptor {
        stage_id: "inspect_starter_model",
        title: "Inspect Starter Model",
        command: "cargo valid inspect approval-model",
        purpose: "Show the starter model's state fields, actions, and properties so the user can read the contract before authoring anything.",
        effect_kind: "model_inspection",
        effect_summary: "Runs a read-only model inspection and prints the starter model structure.",
        key_paths: &["valid/models/approval.rs", "valid/registry.rs"],
        expected_result: "You can map the CLI output back to the starter model file and understand what the model currently expresses.",
        writes_repo_state: false,
        program: OnboardingProgram::Cargo,
        args: &["valid", "inspect", "approval-model"],
        repair_hint: Some("Run `valid doctor` first, then `cargo valid inspect approval-model` after the registry and Cargo setup are healthy."),
    },
    OnboardingStageDescriptor {
        stage_id: "graph_starter_model",
        title: "Render Starter Graph",
        command: "cargo valid graph approval-model --view=overview",
        purpose: "Render the first graph view so the starter model is visible as a quick overview rather than only as Rust source.",
        effect_kind: "graph_render",
        effect_summary: "Runs a read-only graph render of the starter model overview.",
        key_paths: &["valid/models/approval.rs", "valid/registry.rs"],
        expected_result: "You get an overview graph that matches the starter model's fields, actions, and property.",
        writes_repo_state: false,
        program: OnboardingProgram::Cargo,
        args: &["valid", "graph", "approval-model", "--view=overview"],
        repair_hint: Some("Run `valid doctor` and, if needed, `valid init --repair` before retrying `cargo valid graph approval-model --view=overview`."),
    },
    OnboardingStageDescriptor {
        stage_id: "handoff_starter_model",
        title: "Generate Starter Handoff",
        command: "cargo valid handoff approval-model",
        purpose: "Produce the implementation-facing handoff summary so the user can see the artifact that downstream engineers or AI tools would consume.",
        effect_kind: "artifact_generation",
        effect_summary: "Generates the starter handoff artifact and points at the file you can read next.",
        key_paths: &[
            "valid/models/approval.rs",
            "artifacts/handoff/ApprovalModel.md",
            "generated-tests/",
        ],
        expected_result: "You have a concrete handoff artifact on disk for the starter model.",
        writes_repo_state: true,
        program: OnboardingProgram::Cargo,
        args: &["valid", "handoff", "approval-model"],
        repair_hint: Some("Run `valid doctor` first, then retry `cargo valid handoff approval-model` after inspect and graph are healthy."),
    },
];

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
    #[arg(long = "focus-action")]
    focus_action: Option<String>,
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
    if handle_builtin_help_or_version(&raw_args) {
        return;
    }
    let json = detect_json_flag(&raw_args);
    let cli = match ValidCli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            message_exit("valid", json, &error.to_string(), None);
        }
    };
    set_plain_text_output(cli.plain);
    match cli.command {
        Some(ValidCommand::Init(args)) => cmd_init_from_parsed(args),
        Some(ValidCommand::Onboarding(args)) => cmd_onboarding_from_parsed(args),
        Some(ValidCommand::Doctor(args)) => cmd_doctor_from_parsed(args),
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
        Some(ValidCommand::Completion(args)) => cmd_completion(args),
        Some(ValidCommand::Schema(args)) => cmd_schema_from_parsed(args),
        Some(ValidCommand::Batch(args)) => cmd_batch_from_parsed(args),
        None => {
            print!("{}", render_surface_help(Surface::Valid, "valid"));
            process::exit(0);
        }
    }
}

fn handle_builtin_help_or_version(raw_args: &[String]) -> bool {
    let args = raw_args
        .iter()
        .skip(1)
        .filter(|arg| arg.as_str() != "--plain")
        .cloned()
        .collect::<Vec<_>>();
    match args.as_slice() {
        [] => {
            print!("{}", render_surface_help(Surface::Valid, "valid"));
            process::exit(0);
        }
        [arg] if matches!(arg.as_str(), "-h" | "--help" | "help") => {
            print!("{}", render_surface_help(Surface::Valid, "valid"));
            process::exit(0);
        }
        [arg] if matches!(arg.as_str(), "-v" | "--version" | "version") => {
            println!("valid {}", env!("CARGO_PKG_VERSION"));
            process::exit(0);
        }
        [command, flag] if matches!(flag.as_str(), "-h" | "--help") => {
            let command = normalize_command(command);
            print!(
                "{}",
                render_command_help(Surface::Valid, &command).unwrap_or_else(|message| {
                    message_exit("valid", false, &message, None);
                })
            );
            process::exit(0);
        }
        [help, command] if help == "help" => {
            let command = normalize_command(command);
            print!(
                "{}",
                render_command_help(Surface::Valid, &command).unwrap_or_else(|message| {
                    message_exit("valid", false, &message, None);
                })
            );
            process::exit(0);
        }
        _ => false,
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
        focus_action_id: args.focus_action,
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

fn command_is_available(name: &str, arg: &str) -> bool {
    Command::new(name)
        .arg(arg)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn current_exe_dir_on_path() -> bool {
    let Ok(exe) = env::current_exe() else {
        return false;
    };
    let Some(parent) = exe.parent() else {
        return false;
    };
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|path| path == parent))
        .unwrap_or(false)
}

fn current_exe_dir() -> Option<PathBuf> {
    env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(PathBuf::from))
}

fn env_shell_name() -> Option<String> {
    env::var("SHELL").ok().and_then(|shell| {
        PathBuf::from(shell)
            .file_name()
            .and_then(|name| normalize_shell_name(name.to_string_lossy().as_ref()))
    })
}

fn normalize_shell_name(name: &str) -> Option<String> {
    match name.trim_start_matches('-') {
        "bash" | "fish" | "zsh" => Some(name.trim_start_matches('-').to_string()),
        _ => None,
    }
}

fn resolve_active_shell() -> ActiveShellResolution {
    let login_shell = env_shell_name();
    if let Some(shell) = env::var("VALID_ACTIVE_SHELL")
        .ok()
        .and_then(|value| normalize_shell_name(value.as_str()))
    {
        return ActiveShellResolution {
            active_shell: Some(shell),
            source: env::var("VALID_ACTIVE_SHELL_SOURCE")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "override".to_string()),
            login_shell,
        };
    }
    if let Some(shell) = parent_process_shell_name() {
        return ActiveShellResolution {
            active_shell: Some(shell),
            source: "parent_process".to_string(),
            login_shell,
        };
    }
    if login_shell.is_some() {
        return ActiveShellResolution {
            active_shell: login_shell.clone(),
            source: "env_shell".to_string(),
            login_shell,
        };
    }
    ActiveShellResolution {
        active_shell: None,
        source: "unknown".to_string(),
        login_shell: None,
    }
}

fn parent_process_shell_name() -> Option<String> {
    let mut pid = process::id();
    for _ in 0..8 {
        let parent = parent_process_id(pid)?;
        if parent <= 1 || parent == pid {
            break;
        }
        if let Some(shell) = process_command_name(parent)
            .as_deref()
            .and_then(normalize_shell_name)
        {
            return Some(shell);
        }
        pid = parent;
    }
    None
}

fn parent_process_id(pid: u32) -> Option<u32> {
    let output = Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .ok()
}

fn process_command_name(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let command = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if command.is_empty() {
        return None;
    }
    let basename = PathBuf::from(command)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    if basename.is_empty() {
        None
    } else {
        Some(basename)
    }
}

fn shell_resolution_details(resolution: &ActiveShellResolution) -> String {
    let mut details = vec![
        format!(
            "active_shell={}",
            resolution.active_shell.as_deref().unwrap_or("unknown")
        ),
        format!("source={}", resolution.source),
    ];
    if let Some(login_shell) = &resolution.login_shell {
        details.push(format!("login_shell={login_shell}"));
    }
    details.join(", ")
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn shell_path_repair_hint(shell: Option<&str>, bin_dir: Option<&PathBuf>) -> Option<String> {
    let dir = bin_dir?.display().to_string();
    Some(match shell {
        Some("fish") => format!(
            "Run `fish_add_path {dir}` now and add the same path in `~/.config/fish/config.fish` for future sessions."
        ),
        Some("zsh") => format!(
            "Run `export PATH=\"{dir}:$PATH\"` now and add the same export to `~/.zshrc`."
        ),
        Some("bash") => format!(
            "Run `export PATH=\"{dir}:$PATH\"` now and add the same export to `~/.bashrc` or `~/.bash_profile`."
        ),
        _ => format!(
            "Add `{dir}` to PATH in your current shell session and your shell startup config."
        ),
    })
}

fn completion_path_for_shell(shell: Option<&str>) -> Option<PathBuf> {
    let home = home_dir()?;
    match shell {
        Some("fish") => Some(home.join(".config/fish/completions/valid.fish")),
        Some("zsh") => Some(home.join(".zsh/completions/_valid")),
        Some("bash") => Some(home.join(".local/share/bash-completion/completions/valid")),
        _ => None,
    }
}

fn shell_completion_repair_hint(shell: Option<&str>) -> Option<String> {
    Some(match shell {
        Some("fish") => "Run `valid completion install fish` to install Fish completions.".to_string(),
        Some("zsh") => {
            "Run `valid completion install zsh --shell-config` to install completions and update `~/.zshrc`."
                .to_string()
        }
        Some("bash") => {
            "Run `valid completion install bash` to install Bash completions into the standard completion directory."
                .to_string()
        }
        _ => "Run `valid completion install <bash|fish|zsh>` for your shell to install completions."
            .to_string(),
    })
}

fn cargo_toml_has_nonempty_field(manifest: &str, field: &str) -> bool {
    let pattern = format!(
        r#"(?m)^\s*{}\s*=\s*"(?:[^"\n]|\\")+"\s*$"#,
        regex::escape(field)
    );
    regex::Regex::new(&pattern)
        .expect("manifest field regex should compile")
        .is_match(manifest)
}

fn cargo_toml_has_path_and_version_dependency(manifest: &str, dep: &str) -> bool {
    let pattern = format!(
        r#"(?m)^\s*{}\s*=\s*\{{[^}}]*\bpath\s*=\s*"[^"]+"[^}}]*\bversion\s*=\s*"[^"]+"[^}}]*\}}\s*$"#,
        regex::escape(dep)
    );
    regex::Regex::new(&pattern)
        .expect("dependency regex should compile")
        .is_match(manifest)
}

fn cargo_package_name(manifest: &str) -> Option<String> {
    let capture = regex::Regex::new(r#"(?m)^\s*name\s*=\s*"([^"\n]+)"\s*$"#)
        .expect("package name regex should compile")
        .captures(manifest)?;
    Some(capture.get(1)?.as_str().to_string())
}

fn build_doctor_report(root: &PathBuf) -> DoctorReport {
    let mut checks = Vec::new();
    let active_shell = resolve_active_shell();
    let exe_dir = current_exe_dir();
    let exe_on_path = current_exe_dir_on_path();
    checks.push(DoctorCheckReport {
        check_id: "shell_path".to_string(),
        status: if exe_on_path { "ok" } else { "warn" }.to_string(),
        summary: if exe_on_path {
            "`valid` appears to be discoverable on PATH.".to_string()
        } else {
            "`valid` is running, but the current executable directory is not on PATH.".to_string()
        },
        details: env::current_exe().ok().map(|path| {
            format!(
                "current_executable={}, {}",
                path.display(),
                shell_resolution_details(&active_shell)
            )
        }),
        repair_hint: if exe_on_path {
            None
        } else {
            shell_path_repair_hint(active_shell.active_shell.as_deref(), exe_dir.as_ref())
        },
    });

    let completion_path = completion_path_for_shell(active_shell.active_shell.as_deref());
    let completion_installed = completion_path.as_ref().is_some_and(|path| path.exists());
    checks.push(DoctorCheckReport {
        check_id: "shell_completion".to_string(),
        status: if completion_installed {
            "ok"
        } else if completion_path.is_some() {
            "warn"
        } else {
            "warn"
        }
        .to_string(),
        summary: if completion_installed {
            "Shell completions for `valid` appear to be installed.".to_string()
        } else if completion_path.is_some() {
            "Shell completions for `valid` are not installed.".to_string()
        } else {
            "The active shell could not be detected for completion installation hints.".to_string()
        },
        details: completion_path
            .as_ref()
            .map(|path| {
                format!(
                    "{}, expected_completion_path={}",
                    shell_resolution_details(&active_shell),
                    path.display()
                )
            })
            .or_else(|| Some(shell_resolution_details(&active_shell))),
        repair_hint: if completion_installed {
            None
        } else {
            shell_completion_repair_hint(active_shell.active_shell.as_deref())
        },
    });

    let cargo_available = command_is_available("cargo", "--version");
    let project_requires_cargo =
        root.join("Cargo.toml").exists() || root.join("valid.toml").exists();
    checks.push(DoctorCheckReport {
        check_id: "cargo_available".to_string(),
        status: if cargo_available {
            "ok"
        } else if project_requires_cargo {
            "error"
        } else {
            "warn"
        }
        .to_string(),
        summary: if cargo_available {
            "`cargo` is available.".to_string()
        } else if project_requires_cargo {
            "`cargo` is not available on PATH, and this project flow requires Cargo.".to_string()
        } else {
            "`cargo` is not available on PATH, so Cargo-first model loading and authoring are currently unavailable.".to_string()
        },
        details: None,
        repair_hint: if cargo_available {
            None
        } else {
            Some("Install Rust/Cargo and make sure `cargo` is available on PATH before using project-first model loading workflows.".to_string())
        },
    });

    let cargo_valid_available = command_is_available("cargo-valid", "--version");
    checks.push(DoctorCheckReport {
        check_id: "cargo_valid_available".to_string(),
        status: if cargo_valid_available { "ok" } else { "warn" }.to_string(),
        summary: if cargo_valid_available {
            "`cargo-valid` is available.".to_string()
        } else {
            "`cargo-valid` is not available on PATH.".to_string()
        },
        details: None,
        repair_hint: if cargo_valid_available {
            None
        } else {
            Some("Reinstall `valid` so the Cargo subcommand is available, or add the install directory to PATH.".to_string())
        },
    });

    let init_report = check_project_init(root, "valid/registry.rs");
    checks.push(DoctorCheckReport {
        check_id: "project_scaffold".to_string(),
        status: init_report.status.clone(),
        summary: match init_report.status.as_str() {
            "ok" => "The current directory matches the expected `valid init` scaffold.".to_string(),
            "warn" => "The current directory is a valid project, but some scaffold assets are missing.".to_string(),
            _ => "The current directory has blocking scaffold/config issues.".to_string(),
        },
        details: Some(format!(
            "cargo_project_detected={}, valid_toml_detected={}, missing_paths={}, mismatched_paths={}",
            init_report.cargo_project_detected,
            init_report.valid_toml_detected,
            init_report.missing_paths.join(", "),
            init_report.mismatched_paths.join(", ")
        )),
        repair_hint: match init_report.status.as_str() {
            "warn" => Some("Run `valid init --repair` to restore the safe scaffold files and directories.".to_string()),
            "error" => init_report.recommended_repairs.first().cloned(),
            _ => None,
        },
    });

    let mcp_dir = root.join(".mcp");
    checks.push(DoctorCheckReport {
        check_id: "mcp_snippets".to_string(),
        status: if mcp_dir.exists() { "ok" } else { "warn" }.to_string(),
        summary: if mcp_dir.exists() {
            "Local MCP snippets are present.".to_string()
        } else {
            "Local MCP snippets are missing.".to_string()
        },
        details: Some(mcp_dir.display().to_string()),
        repair_hint: if mcp_dir.exists() {
            None
        } else {
            Some("Run `valid init --repair` to restore the `.mcp/` snippet files.".to_string())
        },
    });

    let registry_ready = init_report
        .registry
        .as_ref()
        .is_some_and(|registry| root.join(registry).exists());
    let mcp_ready_status = if init_report.status == "error" || !registry_ready {
        "error"
    } else if !mcp_dir.exists() {
        "warn"
    } else {
        "ok"
    };
    checks.push(DoctorCheckReport {
        check_id: "mcp_project_readiness".to_string(),
        status: mcp_ready_status.to_string(),
        summary: match mcp_ready_status {
            "ok" => "The current project has the minimum files needed for `valid mcp --project .`.".to_string(),
            "warn" => "The project is mostly ready for `valid mcp --project .`, but local MCP snippets are missing.".to_string(),
            _ => "The current project is not ready for `valid mcp --project .`.".to_string(),
        },
        details: Some(format!(
            "registry_path={}, registry_exists={}, mcp_dir_exists={}",
            init_report.registry.as_deref().unwrap_or("<missing>"),
            registry_ready,
            mcp_dir.exists()
        )),
        repair_hint: match mcp_ready_status {
            "warn" => Some("Run `valid init --repair` to restore the `.mcp/` snippet files before wiring MCP clients.".to_string()),
            "error" => Some("Run `valid init --check` first and fix the reported scaffold or registry-path issues before using `valid mcp --project .`.".to_string()),
            _ => None,
        },
    });

    let manifest_path = root.join("Cargo.toml");
    let publish_check = if !manifest_path.exists() {
        DoctorCheckReport {
            check_id: "publish_readiness".to_string(),
            status: "ok".to_string(),
            summary: "Publish-readiness checks apply only when the current directory is a Cargo package root.".to_string(),
            details: Some(manifest_path.display().to_string()),
            repair_hint: None,
        }
    } else {
        let manifest = fs::read_to_string(&manifest_path).unwrap_or_default();
        let package_name = cargo_package_name(&manifest);
        if package_name.as_deref() != Some("valid") {
            DoctorCheckReport {
                check_id: "publish_readiness".to_string(),
                status: "ok".to_string(),
                summary: "Publish-readiness checks are maintainer-focused and are skipped for application projects.".to_string(),
                details: Some(format!(
                    "manifest={}, package_name={}",
                    manifest_path.display(),
                    package_name.as_deref().unwrap_or("<unknown>")
                )),
                repair_hint: None,
            }
        } else {
            let mut missing_fields = Vec::new();
            for field in [
                "description",
                "license",
                "readme",
                "repository",
                "homepage",
                "documentation",
            ] {
                if !cargo_toml_has_nonempty_field(&manifest, field) {
                    missing_fields.push(field.to_string());
                }
            }
            let has_keywords = manifest.contains("keywords = [");
            let has_categories = manifest.contains("categories = [");
            if !has_keywords {
                missing_fields.push("keywords".to_string());
            }
            if !has_categories {
                missing_fields.push("categories".to_string());
            }
            let derive_dependency_ready =
                cargo_toml_has_path_and_version_dependency(&manifest, "valid_derive");
            let publish_ready = missing_fields.is_empty() && derive_dependency_ready;
            let publish_order_blocker = derive_dependency_ready;
            DoctorCheckReport {
                check_id: "publish_readiness".to_string(),
                status: if publish_ready && !publish_order_blocker {
                    "ok"
                } else {
                    "warn"
                }
                .to_string(),
                summary: if publish_ready && !publish_order_blocker {
                    format!(
                        "The package metadata for `{}` looks ready for `cargo publish --dry-run`.",
                        package_name.as_deref().unwrap_or("valid")
                    )
                } else if publish_ready {
                    "The root package metadata looks ready, but the `valid_derive` dependency must be published first.".to_string()
                } else {
                    "The current Cargo package still has publish-readiness gaps.".to_string()
                },
                details: Some(format!(
                    "manifest={}, package_name={}, missing_fields={}, valid_derive_versioned_path_dependency={}, publish_order={}",
                    manifest_path.display(),
                    package_name.as_deref().unwrap_or("<unknown>"),
                    if missing_fields.is_empty() {
                        "<none>".to_string()
                    } else {
                        missing_fields.join(", ")
                    },
                    derive_dependency_ready,
                    if publish_order_blocker {
                        "publish valid_derive before valid"
                    } else {
                        "<none>"
                    }
                )),
                repair_hint: if publish_ready && !publish_order_blocker {
                    Some("Run `cargo publish --dry-run` from this directory to verify packaging before a real release.".to_string())
                } else if publish_ready {
                    Some("Publish `valid_derive` first, then rerun `cargo publish --dry-run` for the root `valid` package.".to_string())
                } else {
                    Some("Fill in the missing Cargo package metadata, keep local path dependencies versioned, then rerun `cargo publish --dry-run`.".to_string())
                },
            }
        }
    };
    checks.push(publish_check);

    let status = if checks.iter().any(|check| check.status == "error") {
        "error"
    } else if checks.iter().any(|check| check.status == "warn") {
        "warn"
    } else {
        "ok"
    };
    DoctorReport {
        status: status.to_string(),
        root: root.display().to_string(),
        active_shell: active_shell.active_shell,
        active_shell_source: active_shell.source,
        checks,
    }
}

fn cmd_doctor_from_parsed(args: JsonProgressArgs) {
    let progress = ProgressReporter::new("doctor", args.progress.as_deref() == Some("json"));
    progress.start(None);
    let root = env::current_dir().unwrap_or_else(|error| {
        message_exit(
            "doctor",
            args.json,
            &format!("failed to resolve current directory: {error}"),
            None,
        )
    });
    let report = build_doctor_report(&root);
    if args.json {
        println!(
            "{}",
            serde_json::to_string(&report).expect("doctor json should serialize")
        );
    } else {
        print!("{}", text_header("valid doctor"));
        println!(
            "{} {}",
            text_status_badge(report.status.as_str()),
            text_kv("root", report.root.as_str())
        );
        println!(
            "{}",
            text_kv(
                "active_shell",
                &format!(
                    "{} ({})",
                    report.active_shell.as_deref().unwrap_or("unknown"),
                    report.active_shell_source
                )
            )
        );
        print!("{}", text_section("Checks"));
        for check in &report.checks {
            println!(
                "{} {} {}",
                text_bullet(check.check_id.as_str()),
                text_status_badge(check.status.as_str()),
                check.summary
            );
            if let Some(details) = &check.details {
                if !details.trim().is_empty() {
                    println!("  {}", text_kv("details", details));
                }
            }
            if let Some(repair_hint) = doctor_hint(check) {
                println!("  {}", text_hint(repair_hint));
            }
        }
        let hinted_checks = report
            .checks
            .iter()
            .filter_map(|check| doctor_hint(check).map(|hint| (check.check_id.as_str(), hint)))
            .collect::<Vec<_>>();
        if !hinted_checks.is_empty() {
            print!("{}", text_section("Hints"));
            for (check_id, hint) in hinted_checks {
                println!("{}", text_bullet(&format!("{check_id}: {hint}")));
            }
        }
        if report.status != "ok" {
            print!("{}", text_section("Recovery"));
            if report
                .checks
                .iter()
                .any(|check| check.check_id == "project_scaffold" && check.status == "warn")
            {
                println!(
                    "{}",
                    text_bullet("Run `valid init --repair` for safe scaffold gaps.")
                );
                println!(
                    "{}",
                    text_bullet("Rerun `valid onboarding` once the repair completes.")
                );
            } else {
                println!(
                    "{}",
                    text_bullet("Fix the first error-level check, then rerun `valid doctor`.")
                );
            }
        }
    }
    let exit = if report.status == "error" {
        ExitCode::Error
    } else {
        ExitCode::Success
    };
    progress.finish(exit);
    process::exit(exit.code());
}

fn cmd_init_from_parsed(args: InitArgs) {
    let progress = ProgressReporter::new("init", args.progress.as_deref() == Some("json"));
    progress.start(None);
    let root = env::current_dir().unwrap_or_else(|error| {
        message_exit(
            "init",
            args.json,
            &format!("failed to resolve current directory: {error}"),
            None,
        )
    });
    if args.check {
        let report = check_project_init(&root, "valid/registry.rs");
        if args.json {
            println!(
                "{}",
                serde_json::to_string(&report).expect("init check json should serialize")
            );
        } else {
            print!("{}", text_header("valid init --check"));
            println!(
                "{} {}",
                text_status_badge(report.status.as_str()),
                text_kv("root", report.root.as_str())
            );
            println!(
                "{}",
                text_kv(
                    "cargo_project_detected",
                    if report.cargo_project_detected {
                        "true"
                    } else {
                        "false"
                    }
                )
            );
            println!(
                "{}",
                text_kv(
                    "valid_toml_detected",
                    if report.valid_toml_detected {
                        "true"
                    } else {
                        "false"
                    }
                )
            );
            println!(
                "{}",
                text_kv(
                    "registry",
                    report.registry.as_deref().unwrap_or("<missing>")
                )
            );
            if !report.missing_paths.is_empty() {
                print!("{}", text_section("Missing Paths"));
                for path in &report.missing_paths {
                    println!("{}", text_bullet(path));
                }
            }
            if !report.mismatched_paths.is_empty() {
                print!("{}", text_section("Mismatched Paths"));
                for path in &report.mismatched_paths {
                    println!("{}", text_bullet(path));
                }
            }
            if !report.recommended_repairs.is_empty() {
                print!("{}", text_section("Recommended Repairs"));
                for repair in &report.recommended_repairs {
                    println!("{}", text_bullet(repair));
                }
            }
        }
        let exit = match report.status.as_str() {
            "error" => ExitCode::Error,
            _ => ExitCode::Success,
        };
        progress.finish(exit);
        process::exit(exit.code());
    }
    if args.repair {
        let result = repair_project_init(&root, "valid/registry.rs")
            .unwrap_or_else(|message| message_exit("init", args.json, &message, None));
        if args.json {
            println!(
                "{}",
                serde_json::to_string(&result).expect("init repair json should serialize")
            );
        } else {
            print!("{}", text_header("valid init --repair"));
            println!(
                "{} {}",
                text_status_badge(result.status.as_str()),
                text_kv("root", result.root.as_str())
            );
            if !result.repaired_files.is_empty() {
                print!("{}", text_section("Repaired Files"));
                for file in &result.repaired_files {
                    println!("{}", text_bullet(file));
                }
            }
            if !result.repaired_directories.is_empty() {
                print!("{}", text_section("Repaired Directories"));
                for dir in &result.repaired_directories {
                    println!("{}", text_bullet(dir));
                }
            }
            if !result.skipped_existing.is_empty() {
                print!("{}", text_section("Skipped Existing"));
                for path in &result.skipped_existing {
                    println!("{}", text_bullet(path));
                }
            }
            if !result.remaining_warnings.is_empty() {
                print!("{}", text_section("Remaining Warnings"));
                for warning in &result.remaining_warnings {
                    println!("{}", text_bullet(warning));
                }
            }
        }
        let exit = if result.status == "warn" {
            ExitCode::Success
        } else {
            ExitCode::Success
        };
        progress.finish(exit);
        process::exit(exit.code());
    }
    let cargo_toml = root.join("Cargo.toml");
    let cargo_init_ran = if cargo_toml.exists() {
        false
    } else {
        let output = Command::new("cargo")
            .arg("init")
            .arg("--bin")
            .arg(".")
            .current_dir(&root)
            .output()
            .unwrap_or_else(|error| {
                message_exit(
                    "init",
                    args.json,
                    &format!("failed to run `cargo init --bin .`: {error}"),
                    None,
                )
            });
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let detail = stderr.trim();
            let fallback = stdout.trim();
            let message = if !detail.is_empty() { detail } else { fallback };
            message_exit(
                "init",
                args.json,
                &format!("`cargo init --bin .` failed: {message}"),
                None,
            );
        }
        true
    };
    let result = scaffold_project_init(&root, "valid/registry.rs", cargo_init_ran)
        .unwrap_or_else(|message| message_exit("init", args.json, &message, None));
    if args.json {
        println!(
            "{}",
            serde_json::to_string(&result).expect("init result json should serialize")
        );
    } else {
        print!("{}", text_header("valid init"));
        println!(
            "{} {}",
            text_status_badge("ok"),
            text_kv("root", result.root.as_str())
        );
        println!(
            "{}",
            text_kv(
                "cargo_init_ran",
                if result.cargo_init_ran {
                    "true"
                } else {
                    "false"
                }
            )
        );
        println!("{}", text_kv("created", result.created.as_str()));
        println!(
            "{}",
            text_kv("registry", result.scaffolded_registry.as_str())
        );
        println!(
            "{}",
            text_kv("generated_tests_dir", result.generated_tests_dir.as_str())
        );
        println!(
            "{}",
            text_kv("artifacts_dir", result.artifacts_dir.as_str())
        );
        println!(
            "{}",
            text_kv(
                "benchmarks_baseline_dir",
                result.benchmarks_baseline_dir.as_str()
            )
        );
        println!("{}", text_kv("docs_rdd", result.rdd_guide.as_str()));
        println!(
            "{}",
            text_kv("ai_bootstrap_guide", result.ai_bootstrap_guide.as_str())
        );
        if !result.skipped_existing.is_empty() {
            print!("{}", text_section("Skipped Existing"));
            for path in &result.skipped_existing {
                println!("{}", text_bullet(path));
            }
        }
        print!("{}", text_section("Next Steps"));
        println!("{}", text_bullet(&text_command("valid init --check")));
        println!("{}", text_bullet(&text_command("cargo valid models")));
        println!(
            "{}",
            text_bullet(&text_command("cargo valid inspect approval-model"))
        );
        println!(
            "{}",
            text_bullet(&text_command("cargo valid handoff approval-model"))
        );
    }
    progress.finish(ExitCode::Success);
    process::exit(ExitCode::Success.code());
}

fn cmd_onboarding_from_parsed(args: OnboardingArgs) {
    let progress = ProgressReporter::new("onboarding", args.progress.as_deref() == Some("json"));
    progress.start(None);
    let interactive = !args.non_interactive && !args.json;
    let root = env::current_dir().unwrap_or_else(|error| {
        message_exit(
            "onboarding",
            args.json,
            &format!("failed to resolve current directory: {error}"),
            None,
        )
    });
    let cargo_project_detected = root.join("Cargo.toml").exists();
    let valid_project_detected = root.join("valid.toml").exists();
    let overview = onboarding_overview();
    let mut stages = Vec::new();

    if !args.json {
        print_onboarding_overview(&overview);
    }

    let detect_stage = onboarding_stage_descriptor("detect_environment");
    stages.push(onboarding_stage_report(
        detect_stage,
        "success",
        if cargo_project_detected || valid_project_detected {
            "Detected an existing project context and selected the review-first walkthrough."
                .to_string()
        } else {
            "No scaffold detected; onboarding will bootstrap a new project first.".to_string()
        },
        None,
        None,
    ));

    if !cargo_project_detected || !valid_project_detected {
        let bootstrap_stage = onboarding_stage_descriptor("bootstrap_project");
        if let Some(stage) = run_onboarding_stage(interactive, args.json, bootstrap_stage, &root) {
            let failed = stage.status == "error";
            stages.push(stage);
            if failed {
                finish_onboarding(
                    args.json,
                    progress,
                    root,
                    interactive,
                    cargo_project_detected,
                    valid_project_detected,
                    overview.clone(),
                    stages,
                );
            }
        }
    } else {
        let bootstrap_stage = onboarding_stage_descriptor("bootstrap_project");
        stages.push(onboarding_stage_report(
            bootstrap_stage,
            "skipped",
            "Skipped scaffold creation because the project already looks initialized.".to_string(),
            None,
            None,
        ));
    }

    for stage_id in [
        "check_scaffold",
        "warm_project_build",
        "list_models",
        "inspect_starter_model",
        "graph_starter_model",
        "handoff_starter_model",
    ] {
        let descriptor = onboarding_stage_descriptor(stage_id);
        if let Some(stage) = run_onboarding_stage(interactive, args.json, descriptor, &root) {
            let failed = stage.status == "error";
            stages.push(stage);
            if failed {
                finish_onboarding(
                    args.json,
                    progress,
                    root,
                    interactive,
                    cargo_project_detected,
                    valid_project_detected,
                    overview.clone(),
                    stages,
                );
            }
        }
    }

    finish_onboarding(
        args.json,
        progress,
        root,
        interactive,
        cargo_project_detected,
        valid_project_detected,
        overview,
        stages,
    );
}

fn current_exe_or_exit() -> PathBuf {
    env::current_exe().unwrap_or_else(|error| {
        message_exit(
            "onboarding",
            false,
            &format!("failed to resolve current executable: {error}"),
            None,
        )
    })
}

fn run_onboarding_stage(
    interactive: bool,
    json: bool,
    descriptor: &OnboardingStageDescriptor,
    root: &PathBuf,
) -> Option<OnboardingStageReport> {
    let guide = onboarding_stage_guide(descriptor);
    if interactive {
        print_onboarding_stage_context(&guide);
    }
    let (program, args) = onboarding_stage_command(descriptor);
    let output = Command::new(&program)
        .current_dir(root)
        .args(&args)
        .output()
        .unwrap_or_else(|error| {
            message_exit(
                "onboarding",
                false,
                &format!("failed to run `{}`: {error}", descriptor.command),
                None,
            )
        });
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let excerpt = onboarding_excerpt(if stdout.is_empty() { &stderr } else { &stdout });
    let exit_code = output.status.code().map_or_else(
        || "terminated by signal".to_string(),
        |code| code.to_string(),
    );
    let stage_success = if descriptor.stage_id == "check_scaffold" && output.status.success() {
        serde_json::from_str::<Value>(&stdout)
            .ok()
            .and_then(|value| {
                value
                    .get("status")
                    .and_then(Value::as_str)
                    .map(|s| s == "ok")
            })
            .unwrap_or(false)
    } else {
        output.status.success()
    };
    let status = if stage_success { "success" } else { "error" };
    let repair_hint = if stage_success {
        None
    } else {
        descriptor.repair_hint.map(|hint| hint.to_string())
    };
    if interactive {
        println!(
            "{} {}",
            text_status_badge(status),
            text_kv("command", descriptor.command)
        );
        println!("  {}", text_kv("exit_code", &exit_code));
        print_onboarding_command_output("stdout", &stdout, None);
        print_onboarding_command_output("stderr", &stderr, None);
        if let Some(repair_hint) = &repair_hint {
            println!("  {}", text_hint(repair_hint));
        }
        if stage_success {
            print!("{}", onboarding_continue_prompt());
            let _ = io::stdout().flush();
            let mut line = String::new();
            let _ = io::stdin().read_line(&mut line);
        }
    } else if !json {
        println!(
            "{} {}",
            text_status_badge(status),
            text_kv("stage", guide.title.as_str())
        );
        println!("  {}", text_kv("command", descriptor.command));
        println!("  {}", text_kv("purpose", guide.purpose.as_str()));
        println!("  {}", text_kv("effect", guide.effect_summary.as_str()));
        if !guide.key_paths.is_empty() {
            println!("  {}", text_kv("look_at", &guide.key_paths.join(", ")));
        }
        println!("  {}", text_kv("expect", guide.expected_result.as_str()));
        println!("  {}", text_kv("exit_code", &exit_code));
        if let Some(excerpt) = &excerpt {
            println!("  {}", text_kv("output", excerpt));
        }
        if let Some(repair_hint) = &repair_hint {
            println!("  {}", text_hint(repair_hint));
        }
    }
    Some(onboarding_stage_report(
        descriptor,
        status,
        descriptor.purpose.to_string(),
        excerpt,
        repair_hint,
    ))
}

fn onboarding_stage_descriptor(stage_id: &str) -> &'static OnboardingStageDescriptor {
    ONBOARDING_STAGE_DESCRIPTORS
        .iter()
        .find(|descriptor| descriptor.stage_id == stage_id)
        .unwrap_or_else(|| panic!("missing onboarding stage descriptor: {stage_id}"))
}

fn onboarding_stage_guide(descriptor: &OnboardingStageDescriptor) -> OnboardingStageGuide {
    OnboardingStageGuide {
        stage_id: descriptor.stage_id.to_string(),
        title: descriptor.title.to_string(),
        command: descriptor.command.to_string(),
        purpose: descriptor.purpose.to_string(),
        effect_kind: descriptor.effect_kind.to_string(),
        effect_summary: descriptor.effect_summary.to_string(),
        key_paths: descriptor
            .key_paths
            .iter()
            .map(|path| path.to_string())
            .collect(),
        expected_result: descriptor.expected_result.to_string(),
        writes_repo_state: descriptor.writes_repo_state,
    }
}

fn onboarding_overview() -> Vec<OnboardingStageGuide> {
    ONBOARDING_STAGE_DESCRIPTORS
        .iter()
        .map(onboarding_stage_guide)
        .collect()
}

fn onboarding_stage_report(
    descriptor: &OnboardingStageDescriptor,
    status: &str,
    summary: String,
    stdout_excerpt: Option<String>,
    repair_hint: Option<String>,
) -> OnboardingStageReport {
    OnboardingStageReport {
        guide: onboarding_stage_guide(descriptor),
        status: status.to_string(),
        summary,
        stdout_excerpt,
        repair_hint,
    }
}

fn onboarding_stage_command(
    descriptor: &OnboardingStageDescriptor,
) -> (PathBuf, Vec<&'static str>) {
    match descriptor.program {
        OnboardingProgram::CurrentExe => (current_exe_or_exit(), descriptor.args.to_vec()),
        OnboardingProgram::Cargo => (PathBuf::from("cargo"), descriptor.args.to_vec()),
        OnboardingProgram::Synthetic => (PathBuf::new(), Vec::new()),
    }
}

fn print_onboarding_overview(overview: &[OnboardingStageGuide]) {
    print!("{}", text_section("Onboarding Overview"));
    println!(
        "{}",
        text_bullet(
            "This walkthrough explains what each step will do, which files matter, and what result to expect before deeper authoring work begins."
        )
    );
    for guide in overview {
        println!("{}", text_bullet(guide.title.as_str()));
        println!("  {}", text_kv("command", guide.command.as_str()));
        println!("  {}", text_kv("purpose", guide.purpose.as_str()));
        println!("  {}", text_kv("effect", guide.effect_summary.as_str()));
        if !guide.key_paths.is_empty() {
            println!("  {}", text_kv("look_at", &guide.key_paths.join(", ")));
        }
        println!("  {}", text_kv("expect", guide.expected_result.as_str()));
    }
}

fn print_onboarding_stage_context(guide: &OnboardingStageGuide) {
    print!("{}", text_section(guide.title.as_str()));
    println!("{}", text_bullet(guide.purpose.as_str()));
    println!("  {}", text_kv("command", guide.command.as_str()));
    println!("  {}", text_kv("effect", guide.effect_summary.as_str()));
    if !guide.key_paths.is_empty() {
        println!("  {}", text_kv("look_at", &guide.key_paths.join(", ")));
    }
    println!("  {}", text_kv("expect", guide.expected_result.as_str()));
}

fn onboarding_excerpt(output: &str) -> Option<String> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }
    let excerpt = trimmed.lines().take(8).collect::<Vec<_>>().join(" | ");
    Some(excerpt)
}

fn print_onboarding_command_output(label: &str, output: &str, line_limit: Option<usize>) {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return;
    }
    println!("  {}", text_kv(label, ""));
    let lines = trimmed.lines().collect::<Vec<_>>();
    let limit = line_limit.unwrap_or(lines.len());
    for line in lines.iter().take(limit) {
        println!("    {line}");
    }
    if lines.len() > limit {
        println!(
            "    {}",
            text_hint(&format!(
                "output truncated after {limit} lines; rerun the command directly for the full result."
            ))
        );
    }
}

fn onboarding_continue_prompt() -> String {
    format!(
        "{}\n",
        text_hint("Press Enter for the next step, or use Ctrl-C to stop.")
    )
}

fn doctor_hint(check: &DoctorCheckReport) -> Option<&str> {
    if matches!(check.status.as_str(), "ok" | "success" | "skipped") {
        return None;
    }
    check
        .repair_hint
        .as_deref()
        .filter(|hint| !hint.trim().is_empty())
        .or(Some("Resolve this check and rerun `valid doctor`."))
}

fn finish_onboarding(
    json: bool,
    progress: ProgressReporter,
    root: PathBuf,
    interactive: bool,
    cargo_project_detected: bool,
    valid_project_detected: bool,
    overview: Vec<OnboardingStageGuide>,
    stages: Vec<OnboardingStageReport>,
) -> ! {
    let has_error = stages.iter().any(|stage| stage.status == "error");
    let has_success = stages.iter().any(|stage| stage.status == "success");
    let report = OnboardingReport {
        status: if has_error && has_success {
            "partial".to_string()
        } else if has_error {
            "error".to_string()
        } else {
            "ok".to_string()
        },
        root: root.display().to_string(),
        interactive,
        cargo_project_detected,
        valid_project_detected,
        overview,
        stages,
        next_paths: vec![
            "review_models".to_string(),
            "generate_test_specs".to_string(),
            "connect_mcp".to_string(),
            "start_authoring".to_string(),
        ],
        next_path_summaries: vec![
            OnboardingNextPathSummary {
                path_id: "review_models".to_string(),
                summary: "Inspect additional models, read their properties, and use graph or explain output for review.".to_string(),
            },
            OnboardingNextPathSummary {
                path_id: "generate_test_specs".to_string(),
                summary: "Use `cargo valid testgen <model>` to produce language-agnostic test specs for implementation handoff.".to_string(),
            },
            OnboardingNextPathSummary {
                path_id: "connect_mcp".to_string(),
                summary: "Use the local `.mcp/` snippets or `valid mcp --project .` to connect docs, handoff, and verification workflows to AI tools.".to_string(),
            },
            OnboardingNextPathSummary {
                path_id: "start_authoring".to_string(),
                summary: "Edit `valid/models/` and `valid/registry.rs` when you are ready to move from review to authoring.".to_string(),
            },
        ],
    };
    if json {
        println!(
            "{}",
            serde_json::to_string(&report).expect("onboarding report json should serialize")
        );
    } else {
        print!("{}", text_header("valid onboarding"));
        println!(
            "{} {}",
            text_status_badge(report.status.as_str()),
            text_kv("root", report.root.as_str())
        );
        println!(
            "{}",
            text_kv(
                "interactive",
                if report.interactive { "true" } else { "false" }
            )
        );
        print!("{}", text_section("You Now Have"));
        println!("{}", text_bullet("a scaffolded Cargo-first valid project"));
        println!(
            "{}",
            text_bullet("a warmed local Cargo build for fast starter-model review")
        );
        println!(
            "{}",
            text_bullet("an inspectable starter model named approval-model")
        );
        println!("{}", text_bullet("a first overview graph you can review"));
        println!(
            "{}",
            text_bullet("an implementation-facing handoff summary")
        );
        println!(
            "{}",
            text_bullet(
                "artifact directories at `artifacts/`, `generated-tests/`, and `benchmarks/baselines/`"
            )
        );
        print!("{}", text_section("Recap Commands"));
        println!("{}", text_bullet(&text_command("cargo build --quiet")));
        println!("{}", text_bullet(&text_command("cargo valid models")));
        println!(
            "{}",
            text_bullet(&text_command("cargo valid inspect approval-model"))
        );
        println!(
            "{}",
            text_bullet(&text_command("cargo valid handoff approval-model"))
        );
        print!("{}", text_section("Where To Look Next"));
        println!(
            "{}",
            text_bullet("valid/models/approval.rs: read the starter model itself")
        );
        println!(
            "{}",
            text_bullet(
                "valid/registry.rs: see how the starter model is exported to `cargo valid`"
            )
        );
        println!(
            "{}",
            text_bullet(
                "docs/rdd/README.md: capture the requirement or rule family you want to model next"
            )
        );
        println!(
            "{}",
            text_bullet(
                ".mcp/codex.toml: wire the local project into AI tooling from the scaffold"
            )
        );
        println!(
            "{}",
            text_bullet("artifacts/handoff/ApprovalModel.md: inspect the generated implementation-facing starter artifact")
        );
        print!("{}", text_section("Next Paths"));
        for next_path in &report.next_path_summaries {
            println!(
                "{}",
                text_bullet(&format!("{}: {}", next_path.path_id, next_path.summary))
            );
        }
        if report.status != "ok" {
            print!("{}", text_section("Recovery"));
            println!(
                "{}",
                text_bullet(
                    "Run `valid doctor` to diagnose the current environment or project state."
                )
            );
            println!("{}", text_bullet("Run `valid init --repair` for safe scaffold repair, then rerun `valid onboarding`."));
        }
    }
    let exit = if report.status == "error" {
        ExitCode::Error
    } else {
        ExitCode::Success
    };
    progress.finish(exit);
    process::exit(exit.code());
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
    let testgen_request = TestgenRequest {
        request_id: "req-local-handoff-testgen".to_string(),
        source_name: parsed.path.clone(),
        source: source.clone(),
        property_id: parsed.property_id.clone(),
        focus_action_id: None,
        strategy: "counterexample".to_string(),
        seed: None,
        backend: parsed.backend.clone(),
        solver_executable: parsed.solver_executable.clone(),
        solver_args: parsed.solver_args.clone(),
    };
    let testgen = testgen_source(&testgen_request);
    let testgen_ref = testgen.as_ref().ok();
    let testgen_error = testgen
        .as_ref()
        .err()
        .and_then(|error| error.diagnostics.first())
        .map(|diagnostic| diagnostic.message.as_str());
    let generated = generate_handoff(HandoffInputs {
        inspect: &inspect,
        runs: &orchestrated.runs,
        coverage: &coverage,
        explanations: &explanations,
        testgen: testgen_ref,
        testgen_error,
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
        "usage: valid testgen <model-file> [--json] [--progress=json] [--property=<id>] [--strategy=<counterexample|transition|witness|guard|boundary|path|random|deadlock|enablement>] [--focus-action=<id>] [--seed=<u64>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
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
        focus_action_id: parsed.focus_action_id.clone(),
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
                    "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"status\":\"{}\",\"vector_ids\":[{}],\"vectors\":[{}],\"vector_groups\":[{}],\"generated_files\":[{}]}}",
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
                            "{{\"vector_id\":\"{}\",\"run_id\":\"{}\",\"property_id\":\"{}\",\"strictness\":\"{}\",\"derivation\":\"{}\",\"source_kind\":\"{}\",\"strategy\":\"{}\",\"requirement_clusters\":[{}],\"risk_clusters\":[{}],\"observation_mode\":\"{}\",\"observation_layers\":[{}],\"oracle_targets\":[{}],\"suggested_surface\":\"{}\",\"state_visibility\":\"{}\",\"focus_action_id\":{},\"expected_guard_enabled\":{},\"notes\":[{}]}}",
                            vector.vector_id,
                            vector.run_id,
                            vector.property_id,
                            vector.strictness,
                            vector.derivation,
                            vector.source_kind,
                            vector.strategy,
                            vector
                                .requirement_clusters
                                .iter()
                                .map(|s| format!("\"{}\"", s))
                                .collect::<Vec<_>>()
                                .join(","),
                            vector
                                .risk_clusters
                                .iter()
                                .map(|s| format!("\"{}\"", s))
                                .collect::<Vec<_>>()
                                .join(","),
                            vector.observation_mode,
                            vector.observation_layers.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
                            vector.oracle_targets.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(","),
                            vector.suggested_surface,
                            vector.state_visibility,
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
                            group
                                .vector_ids
                                .iter()
                                .map(|s| format!("\"{}\"", s))
                                .collect::<Vec<_>>()
                                .join(",")
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
                        "  {} run_id={} property_id={} strictness={} derivation={} source={} strategy={} requirements={} risks={} observation_mode={} layers={} targets={} surface={} state_visibility={} focus_action={} guard_enabled={} notes={}",
                        vector.vector_id,
                        vector.run_id,
                        vector.property_id,
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
                        vector.observation_mode,
                        if vector.observation_layers.is_empty() {
                            "-".to_string()
                        } else {
                            vector.observation_layers.join(",")
                        },
                        if vector.oracle_targets.is_empty() {
                            "-".to_string()
                        } else {
                            vector.oracle_targets.join(",")
                        },
                        vector.suggested_surface,
                        vector.state_visibility,
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
                if !response.vector_groups.is_empty() {
                    println!("grouped output:");
                    for group in &response.vector_groups {
                        println!(
                            "  {}:{} -> {}",
                            group.group_kind,
                            group.group_id,
                            group.vector_ids.join(",")
                        );
                    }
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
        if let Some(evidence_id) = &report.evidence_id {
            println!("evidence_id: {}", evidence_id);
        }
        if let Some(property_id) = &report.property_id {
            println!("property_id: {}", property_id);
        }
        println!("runner: {}", report.runner);
        println!("status: {}", report.status);
        println!("mismatch_count: {}", report.mismatch_count);
        if !report.mismatch_categories.is_empty() {
            println!(
                "mismatch_categories: {}",
                report.mismatch_categories.join(",")
            );
        }
        if let Some(traceback) = &report.traceback {
            println!("traceback.breakpoint_kind: {}", traceback.breakpoint_kind);
            println!(
                "traceback.failure_step_index: {}",
                traceback.failure_step_index
            );
            if let Some(action_id) = &traceback.failing_action_id {
                println!("traceback.failing_action_id: {}", action_id);
            }
            if !traceback.changed_fields.is_empty() {
                println!(
                    "traceback.changed_fields: {}",
                    traceback.changed_fields.join(",")
                );
            }
            if !traceback.involved_fields.is_empty() {
                println!(
                    "traceback.involved_fields: {}",
                    traceback.involved_fields.join(",")
                );
            }
        }
        if !report.candidate_causes.is_empty() {
            println!("candidate_causes:");
            for cause in &report.candidate_causes {
                println!("  - {}: {}", cause.kind, cause.message);
            }
        }
        if !report.repair_targets.is_empty() {
            println!("repair_targets:");
            for target in &report.repair_targets {
                println!(
                    "  - {} [{}] {}",
                    target.target, target.priority, target.reason
                );
            }
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
        println!(
            "review_summary.headline: {}",
            report.review_summary.headline
        );
        if !report.review_summary.next_steps.is_empty() {
            println!("review_summary.next_steps:");
            for step in &report.review_summary.next_steps {
                println!("  - {}", step);
            }
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
        } else if arg == "--focus-action" {
            parsed.focus_action_id = Some(
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
        } else if let Some(value) = arg.strip_prefix("--focus-action=") {
            parsed.focus_action_id = Some(value.to_string());
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

fn cmd_completion(args: CompletionArgs) {
    let usage = "usage: valid completion <bash|fish|zsh>\n       valid completion install <bash|fish|zsh> [--shell-config] [--stdout] [--json]\n       valid completion candidates <models|properties|actions|views> [target]";
    let json = args.json || args.args.iter().any(|arg| arg == "--json");
    let shell_config = args.shell_config || args.args.iter().any(|arg| arg == "--shell-config");
    let stdout = args.stdout || args.args.iter().any(|arg| arg == "--stdout");
    let positional = args
        .args
        .iter()
        .filter(|arg| !matches!(arg.as_str(), "--json" | "--shell-config" | "--stdout"))
        .cloned()
        .collect::<Vec<_>>();
    let Some(first) = positional.first().map(String::as_str) else {
        usage_exit("completion", json, usage);
    };
    match first {
        "install" => {
            let shell = positional
                .get(1)
                .map(String::as_str)
                .unwrap_or_else(|| usage_exit("completion", json, usage));
            match install_completion(Surface::Valid, shell, shell_config, stdout) {
                Ok(result) => {
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string(&result)
                                .expect("completion install json should serialize")
                        );
                    } else {
                        println!("status: {}", result.status);
                        println!("command: {}", result.command);
                        println!("shell: {}", result.shell);
                        println!("written_files: {}", result.written_files.join(", "));
                        if !result.updated_shell_configs.is_empty() {
                            println!(
                                "updated_shell_configs: {}",
                                result.updated_shell_configs.join(", ")
                            );
                        }
                    }
                }
                Err(message) => message_exit("completion", json, &message, Some(usage)),
            }
        }
        "candidates" => {
            let kind = positional
                .get(1)
                .map(String::as_str)
                .unwrap_or_else(|| usage_exit("completion", json, usage));
            let target = positional.get(2).map(String::as_str);
            for value in completion_candidates_valid(kind, target) {
                println!("{value}");
            }
        }
        shell => match render_completion(Surface::Valid, shell) {
            Ok(script) => print!("{script}"),
            Err(message) => message_exit("completion", json, &message, Some(usage)),
        },
    }
}

fn completion_candidates_valid(kind: &str, target: Option<&str>) -> Vec<String> {
    match kind {
        "models" => list_bundled_models()
            .into_iter()
            .map(|model| format!("rust:{model}"))
            .collect(),
        "properties" => inspect_for_completion(target)
            .map(|inspect| inspect.properties)
            .unwrap_or_default(),
        "actions" => inspect_for_completion(target)
            .map(|inspect| inspect.actions)
            .unwrap_or_default(),
        "views" => ["overview", "logic", "failure", "deadlock", "scc"]
            .into_iter()
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn inspect_for_completion(target: Option<&str>) -> Option<valid::api::InspectResponse> {
    let target = target?;
    let source = if is_bundled_model_ref(target) {
        String::new()
    } else {
        fs::read_to_string(target).ok()?
    };
    inspect_source(&InspectRequest {
        request_id: "req-completion-candidates".to_string(),
        source_name: target.to_string(),
        source,
    })
    .ok()
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
