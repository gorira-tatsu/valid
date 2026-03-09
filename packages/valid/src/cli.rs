use std::{
    env, fs,
    path::{Path, PathBuf},
};

use clap::{Arg, ArgAction, Command};
use clap_complete::{
    generate,
    shells::{Bash, Fish, Zsh},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{
    engine::{CheckOutcome, RunStatus},
    support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode},
};

pub const CLI_SCHEMA_VERSION: &str = "1.0.0";

const RUN_RESULT_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/run_result.schema.json"
));
const CLI_COMPLETED_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/cli_completed.schema.json"
));
const CLI_ERROR_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/cli_error.schema.json"
));
const EVIDENCE_TRACE_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/evidence_trace.schema.json"
));
const COVERAGE_REPORT_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/coverage_report.schema.json"
));
const CONTRACT_SNAPSHOT_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/contract_snapshot.schema.json"
));
const CONTRACT_LOCK_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/contract_lock.schema.json"
));
const CONTRACT_DRIFT_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/contract_drift.schema.json"
));
const SELF_CHECK_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/selfcheck_report.schema.json"
));
const INSPECT_REQUEST_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/ai_inspect_request.schema.json"
));
const INSPECT_RESPONSE_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/ai_inspect_response.schema.json"
));
const CHECK_REQUEST_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/ai_check_request.schema.json"
));
const EXPLAIN_RESPONSE_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/ai_explain_response.schema.json"
));
const MINIMIZE_RESPONSE_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/ai_minimize_response.schema.json"
));
const TESTGEN_REQUEST_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/ai_testgen_request.schema.json"
));
const TESTGEN_RESPONSE_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/ai_testgen_response.schema.json"
));
const ORCHESTRATE_REQUEST_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/ai_orchestrate_request.schema.json"
));
const ORCHESTRATE_RESPONSE_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/ai_orchestrate_response.schema.json"
));
const CAPABILITIES_REQUEST_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/ai_capabilities_request.schema.json"
));
const CAPABILITIES_RESPONSE_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/rdd/09_reference/schemas/ai_capabilities_response.schema.json"
));

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExitCode {
    Success,
    Fail,
    Unknown,
    Error,
}

impl ExitCode {
    pub fn code(self) -> i32 {
        match self {
            Self::Success => 0,
            Self::Fail => 1,
            Self::Unknown => 2,
            Self::Error => 3,
        }
    }

    pub fn status_label(self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Fail => "FAIL",
            Self::Unknown => "UNKNOWN",
            Self::Error => "ERROR",
        }
    }

    pub fn aggregate(self, next: Self) -> Self {
        match (self, next) {
            (Self::Error, _) | (_, Self::Error) => Self::Error,
            (Self::Fail, _) | (_, Self::Fail) => Self::Fail,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            _ => Self::Success,
        }
    }

    pub fn from_check_outcome(outcome: &CheckOutcome) -> Self {
        match outcome {
            CheckOutcome::Completed(result) => match result.status {
                RunStatus::Pass => Self::Success,
                RunStatus::Fail => Self::Fail,
                RunStatus::Unknown => Self::Unknown,
            },
            CheckOutcome::Errored(_) => Self::Error,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Surface {
    Valid,
    CargoValid,
    Registry,
}

impl Surface {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::CargoValid => "cargo-valid",
            Self::Registry => "registry",
        }
    }
}

#[derive(Clone, Copy, Serialize)]
pub struct ArgSpec {
    pub name: &'static str,
    pub syntax: &'static str,
    pub value_type: &'static str,
    pub required: bool,
    pub multiple: bool,
    pub positional: bool,
    pub description: &'static str,
    pub values: &'static [&'static str],
}

#[derive(Clone, Copy)]
pub struct SchemaRef {
    pub id: &'static str,
    pub builder: fn() -> Value,
}

#[derive(Clone, Copy)]
pub struct CommandSpec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub usage: &'static str,
    pub positional: &'static [ArgSpec],
    pub options: &'static [ArgSpec],
    pub request_schema: Option<SchemaRef>,
    pub response_schema: Option<SchemaRef>,
    pub supports_json: bool,
    pub supports_progress: bool,
}

#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    #[serde(default = "schema_version_string")]
    pub schema_version: String,
    #[serde(default = "default_continue_on_error")]
    pub continue_on_error: bool,
    pub operations: Vec<BatchOperation>,
}

#[derive(Debug, Deserialize)]
pub struct BatchOperation {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_true")]
    pub json: bool,
}

#[derive(Debug, Serialize)]
pub struct BatchResponse {
    pub schema_version: String,
    pub status: &'static str,
    pub exit_code: i32,
    pub results: Vec<BatchResult>,
}

#[derive(Debug, Serialize)]
pub struct BatchResult {
    pub index: usize,
    pub command: String,
    pub args: Vec<String>,
    pub exit_code: i32,
    pub stdout: Value,
    pub stderr: Value,
}

pub struct ProgressReporter {
    command: String,
    enabled: bool,
}

impl ProgressReporter {
    pub fn new(command: impl Into<String>, enabled: bool) -> Self {
        Self {
            command: command.into(),
            enabled,
        }
    }

    pub fn from_args(command: impl Into<String>, args: &[String]) -> Self {
        Self::new(command, detect_progress_json_flag(args))
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn emit(&self, event: &str, extra: Value) {
        if !self.enabled {
            return;
        }
        let mut object = Map::new();
        object.insert(
            "schema_version".to_string(),
            Value::String(CLI_SCHEMA_VERSION.to_string()),
        );
        object.insert("kind".to_string(), Value::String("progress".to_string()));
        object.insert("command".to_string(), Value::String(self.command.clone()));
        object.insert("event".to_string(), Value::String(event.to_string()));
        if let Value::Object(extra_map) = extra {
            for (key, value) in extra_map {
                object.insert(key, value);
            }
        }
        eprintln!(
            "{}",
            serde_json::to_string(&Value::Object(object)).expect("progress json")
        );
    }

    pub fn start(&self, total: Option<usize>) {
        self.emit("start", json!({ "total": total }));
    }

    pub fn item_start(&self, index: usize, total: usize, target: &str) {
        self.emit(
            "item_start",
            json!({ "index": index, "total": total, "target": target }),
        );
    }

    pub fn item_complete(&self, index: usize, total: usize, target: &str, exit_code: i32) {
        self.emit(
            "item_complete",
            json!({
                "index": index,
                "total": total,
                "target": target,
                "exit_code": exit_code
            }),
        );
    }

    pub fn finish(&self, exit_code: ExitCode) {
        self.emit(
            "finish",
            json!({
                "status": exit_code.status_label(),
                "exit_code": exit_code.code()
            }),
        );
    }
}

const MODEL_FILE_ARG: ArgSpec = ArgSpec {
    name: "model_file",
    syntax: "<model-file>",
    value_type: "string",
    required: true,
    multiple: false,
    positional: true,
    description: "Path or model reference to inspect.",
    values: &[],
};
const MODEL_ARG: ArgSpec = ArgSpec {
    name: "model",
    syntax: "<model>",
    value_type: "string",
    required: true,
    multiple: false,
    positional: true,
    description: "Registered model name.",
    values: &[],
};
const JSON_ARG: ArgSpec = ArgSpec {
    name: "json",
    syntax: "--json",
    value_type: "boolean",
    required: false,
    multiple: false,
    positional: false,
    description: "Emit JSON on stdout and structured errors on stderr.",
    values: &[],
};
const PROGRESS_ARG: ArgSpec = ArgSpec {
    name: "progress",
    syntax: "--progress=json",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Emit structured progress events on stderr.",
    values: &["json"],
};
const PROPERTY_ARG: ArgSpec = ArgSpec {
    name: "property_id",
    syntax: "--property=<id>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Restrict execution to one property.",
    values: &[],
};
const BACKEND_ARG: ArgSpec = ArgSpec {
    name: "backend",
    syntax: "--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Select the verification backend.",
    values: &["explicit", "mock-bmc", "sat-varisat", "smt-cvc5", "command"],
};
const SOLVER_EXEC_ARG: ArgSpec = ArgSpec {
    name: "solver_executable",
    syntax: "--solver-exec <path>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Executable path for external solver backends.",
    values: &[],
};
const SOLVER_ARG_ARG: ArgSpec = ArgSpec {
    name: "solver_args",
    syntax: "--solver-arg <arg>",
    value_type: "string",
    required: false,
    multiple: true,
    positional: false,
    description: "Additional solver argument. Can be repeated.",
    values: &[],
};
const FORMAT_ARG: ArgSpec = ArgSpec {
    name: "format",
    syntax: "--format=<mermaid|dot|svg|text|json>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Graph or report output format.",
    values: &["mermaid", "dot", "svg", "text", "json"],
};
const TRACE_FORMAT_ARG: ArgSpec = ArgSpec {
    name: "format",
    syntax: "--format=<mermaid-state|mermaid-sequence|json>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Trace output format.",
    values: &["mermaid-state", "mermaid-sequence", "json"],
};
const VIEW_ARG: ArgSpec = ArgSpec {
    name: "view",
    syntax: "--view=<overview|logic|failure|deadlock|scc>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Graph view to render.",
    values: &["overview", "logic", "failure", "deadlock", "scc"],
};
const STRATEGY_ARG: ArgSpec = ArgSpec {
    name: "strategy",
    syntax: "--strategy=<counterexample|transition|witness|guard|boundary|path|random|deadlock|enablement>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Test generation strategy.",
    values: &[
        "counterexample",
        "transition",
        "witness",
        "guard",
        "boundary",
        "path",
        "random",
        "deadlock",
        "enablement",
    ],
};
const ACTIONS_ARG: ArgSpec = ArgSpec {
    name: "actions",
    syntax: "--actions=a,b,c",
    value_type: "array",
    required: false,
    multiple: false,
    positional: false,
    description: "Comma-separated action ids for replay.",
    values: &[],
};
const FOCUS_ACTION_ARG: ArgSpec = ArgSpec {
    name: "focus_action_id",
    syntax: "--focus-action=<id>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Optional focus action id for replay.",
    values: &[],
};
const REPEAT_ARG: ArgSpec = ArgSpec {
    name: "repeat",
    syntax: "--repeat=<n>",
    value_type: "integer",
    required: false,
    multiple: false,
    positional: false,
    description: "Benchmark iteration count.",
    values: &[],
};
const BASELINE_ARG: ArgSpec = ArgSpec {
    name: "baseline_mode",
    syntax: "--baseline[=compare|record|ignore]",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Benchmark baseline mode.",
    values: &["compare", "record", "ignore"],
};
const THRESHOLD_ARG: ArgSpec = ArgSpec {
    name: "threshold_percent",
    syntax: "--threshold-percent=<n>",
    value_type: "integer",
    required: false,
    multiple: false,
    positional: false,
    description: "Allowed benchmark regression threshold percentage.",
    values: &[],
};
const WRITE_ARG: ArgSpec = ArgSpec {
    name: "write_path",
    syntax: "--write[=<path>]",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Optional file path for generated output.",
    values: &[],
};
const CHECK_ARG: ArgSpec = ArgSpec {
    name: "check",
    syntax: "--check",
    value_type: "boolean",
    required: false,
    multiple: false,
    positional: false,
    description: "Enable migration check mode.",
    values: &[],
};
const MANIFEST_ARG: ArgSpec = ArgSpec {
    name: "manifest_path",
    syntax: "--manifest-path <path>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Cargo manifest path for project execution.",
    values: &[],
};
const PROJECT_ARG: ArgSpec = ArgSpec {
    name: "project",
    syntax: "--project <dir>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Project directory containing Cargo.toml for MCP startup.",
    values: &[],
};
const REGISTRY_ARG: ArgSpec = ArgSpec {
    name: "registry",
    syntax: "--registry <path>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Rust registry source file to execute.",
    values: &[],
};
const FILE_ARG: ArgSpec = ArgSpec {
    name: "file",
    syntax: "--file <path>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Rust registry source file to execute.",
    values: &[],
};
const EXAMPLE_ARG: ArgSpec = ArgSpec {
    name: "example",
    syntax: "--example <name>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Cargo example name to execute.",
    values: &[],
};
const BIN_ARG: ArgSpec = ArgSpec {
    name: "bin",
    syntax: "--bin <name>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Cargo binary name to execute.",
    values: &[],
};
const MODEL_FILE_OPTION_ARG: ArgSpec = ArgSpec {
    name: "model_file",
    syntax: "--model-file <path>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "DSL model file to pin at MCP startup.",
    values: &[],
};
const NAME_ARG: ArgSpec = ArgSpec {
    name: "name",
    syntax: "--name <server-name>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Override the MCP server name.",
    values: &[],
};
const LOCKED_ARG: ArgSpec = ArgSpec {
    name: "locked",
    syntax: "--locked",
    value_type: "boolean",
    required: false,
    multiple: false,
    positional: false,
    description: "Pass --locked through to cargo build for registry startup.",
    values: &[],
};
const OFFLINE_ARG: ArgSpec = ArgSpec {
    name: "offline",
    syntax: "--offline",
    value_type: "boolean",
    required: false,
    multiple: false,
    positional: false,
    description: "Pass --offline through to cargo build for registry startup.",
    values: &[],
};
const FEATURE_ARG: ArgSpec = ArgSpec {
    name: "feature",
    syntax: "--feature <name>",
    value_type: "string",
    required: false,
    multiple: true,
    positional: false,
    description: "Additional cargo feature to enable when building the registry target.",
    values: &[],
};
const PRINT_CONFIG_ARG: ArgSpec = ArgSpec {
    name: "print_config",
    syntax: "--print-config <codex|claude-code|claude-desktop>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: false,
    description: "Print a ready-to-paste MCP client config snippet instead of starting the server.",
    values: &["codex", "claude-code", "claude-desktop"],
};
const CLEAN_SCOPE_ARG: ArgSpec = ArgSpec {
    name: "scope",
    syntax: "[generated|artifacts|all]",
    value_type: "string",
    required: false,
    multiple: false,
    positional: true,
    description: "Clean scope.",
    values: &["generated", "generated-tests", "artifacts", "all"],
};
const CONTRACT_SUBCOMMAND_ARG: ArgSpec = ArgSpec {
    name: "mode",
    syntax: "<snapshot|lock|drift|check>",
    value_type: "string",
    required: false,
    multiple: false,
    positional: true,
    description: "Contract operation.",
    values: &["snapshot", "lock", "drift", "check"],
};
const LOCK_FILE_ARG: ArgSpec = ArgSpec {
    name: "lock_file",
    syntax: "[lock-file]",
    value_type: "string",
    required: false,
    multiple: false,
    positional: true,
    description: "Lock file path for contract operations.",
    values: &[],
};
const COMMAND_NAME_ARG: ArgSpec = ArgSpec {
    name: "command",
    syntax: "<command>",
    value_type: "string",
    required: true,
    multiple: false,
    positional: true,
    description: "Command name to describe.",
    values: &[],
};
const COMPLETION_SHELL_ARG: ArgSpec = ArgSpec {
    name: "shell",
    syntax: "<bash|fish|zsh>",
    value_type: "string",
    required: true,
    multiple: false,
    positional: true,
    description: "Target shell to generate completions for.",
    values: &["bash", "fish", "zsh"],
};

const CHECK_OPTIONS: &[ArgSpec] = &[
    JSON_ARG,
    PROGRESS_ARG,
    PROPERTY_ARG,
    BACKEND_ARG,
    SOLVER_EXEC_ARG,
    SOLVER_ARG_ARG,
];
const GRAPH_OPTIONS: &[ArgSpec] = &[FORMAT_ARG, VIEW_ARG, PROPERTY_ARG, JSON_ARG, PROGRESS_ARG];
const LINT_OPTIONS: &[ArgSpec] = &[JSON_ARG, PROGRESS_ARG];
const CAPABILITY_OPTIONS: &[ArgSpec] = &[
    JSON_ARG,
    PROGRESS_ARG,
    BACKEND_ARG,
    SOLVER_EXEC_ARG,
    SOLVER_ARG_ARG,
];
const EXPLAIN_OPTIONS: &[ArgSpec] = &[
    JSON_ARG,
    PROGRESS_ARG,
    PROPERTY_ARG,
    BACKEND_ARG,
    SOLVER_EXEC_ARG,
    SOLVER_ARG_ARG,
];
const HANDOFF_OPTIONS: &[ArgSpec] = &[
    JSON_ARG,
    PROGRESS_ARG,
    PROPERTY_ARG,
    BACKEND_ARG,
    SOLVER_EXEC_ARG,
    SOLVER_ARG_ARG,
    WRITE_ARG,
    CHECK_ARG,
];
const MINIMIZE_OPTIONS: &[ArgSpec] = &[
    JSON_ARG,
    PROGRESS_ARG,
    PROPERTY_ARG,
    BACKEND_ARG,
    SOLVER_EXEC_ARG,
    SOLVER_ARG_ARG,
];
const ORCHESTRATE_OPTIONS: &[ArgSpec] = &[
    JSON_ARG,
    PROGRESS_ARG,
    BACKEND_ARG,
    SOLVER_EXEC_ARG,
    SOLVER_ARG_ARG,
];
const TESTGEN_OPTIONS: &[ArgSpec] = &[
    JSON_ARG,
    PROGRESS_ARG,
    PROPERTY_ARG,
    STRATEGY_ARG,
    BACKEND_ARG,
    SOLVER_EXEC_ARG,
    SOLVER_ARG_ARG,
];
const REPLAY_OPTIONS: &[ArgSpec] = &[
    JSON_ARG,
    PROGRESS_ARG,
    PROPERTY_ARG,
    FOCUS_ACTION_ARG,
    ACTIONS_ARG,
];
const COVERAGE_OPTIONS: &[ArgSpec] = &[
    JSON_ARG,
    PROGRESS_ARG,
    PROPERTY_ARG,
    BACKEND_ARG,
    SOLVER_EXEC_ARG,
    SOLVER_ARG_ARG,
];
const TRACE_OPTIONS: &[ArgSpec] = &[
    TRACE_FORMAT_ARG,
    JSON_ARG,
    PROGRESS_ARG,
    PROPERTY_ARG,
    BACKEND_ARG,
    SOLVER_EXEC_ARG,
    SOLVER_ARG_ARG,
];
const CLEAN_OPTIONS: &[ArgSpec] = &[JSON_ARG, PROGRESS_ARG];
const ARTIFACTS_OPTIONS: &[ArgSpec] = &[JSON_ARG];
const SCHEMA_OPTIONS: &[ArgSpec] = &[JSON_ARG];
const COMMANDS_OPTIONS: &[ArgSpec] = &[JSON_ARG];
const BATCH_OPTIONS: &[ArgSpec] = &[JSON_ARG, PROGRESS_ARG];
const MCP_OPTIONS: &[ArgSpec] = &[
    PROJECT_ARG,
    MANIFEST_ARG,
    REGISTRY_ARG,
    FILE_ARG,
    EXAMPLE_ARG,
    BIN_ARG,
    MODEL_FILE_OPTION_ARG,
    NAME_ARG,
    LOCKED_ARG,
    OFFLINE_ARG,
    FEATURE_ARG,
    PRINT_CONFIG_ARG,
];
const BENCHMARK_OPTIONS: &[ArgSpec] = &[
    JSON_ARG,
    PROGRESS_ARG,
    PROPERTY_ARG,
    REPEAT_ARG,
    BASELINE_ARG,
    THRESHOLD_ARG,
    BACKEND_ARG,
    SOLVER_EXEC_ARG,
    SOLVER_ARG_ARG,
];
const MIGRATE_OPTIONS: &[ArgSpec] = &[JSON_ARG, PROGRESS_ARG, WRITE_ARG, CHECK_ARG];
const VALID_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "init",
        aliases: &[],
        description: "Create a Cargo project scaffold plus valid project layout.",
        usage: "valid init [--json] [--progress=json]",
        positional: &[],
        options: &[JSON_ARG, PROGRESS_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.init_response", builder: init_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "check",
        aliases: &["verify"],
        description: "Run model verification.",
        usage: "valid check <model-file> [--json] [--progress=json] [--property=<id>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
        positional: &[MODEL_FILE_ARG],
        options: CHECK_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.check_request", builder: check_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.cli.completed", builder: cli_completed_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "inspect",
        aliases: &[],
        description: "Inspect compiled model metadata.",
        usage: "valid inspect <model-file> [--json] [--progress=json]",
        positional: &[MODEL_FILE_ARG],
        options: &[JSON_ARG, PROGRESS_ARG],
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.inspect_response", builder: inspect_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "graph",
        aliases: &["diagram"],
        description: "Render the model graph.",
        usage: "valid graph <model-file> [--format=mermaid|dot|svg|text|json] [--view=overview|logic|failure|deadlock|scc] [--property=<id>] [--json] [--progress=json]",
        positional: &[MODEL_FILE_ARG],
        options: GRAPH_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.inspect_response", builder: inspect_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "doc",
        aliases: &[],
        description: "Generate model documentation and drift reports.",
        usage: "valid doc <model-file> [--json] [--progress=json] [--write[=<path>]] [--check]",
        positional: &[MODEL_FILE_ARG],
        options: MIGRATE_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: None,
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "handoff",
        aliases: &[],
        description: "Generate implementation-oriented handoff briefs and drift reports.",
        usage: "valid handoff <model-file> [--json] [--progress=json] [--property=<id>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>] [--write[=<path>]] [--check]",
        positional: &[MODEL_FILE_ARG],
        options: HANDOFF_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: None,
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "lint",
        aliases: &["readiness"],
        description: "Run model readiness and maintainability lint checks.",
        usage: "valid lint <model-file> [--json] [--progress=json]",
        positional: &[MODEL_FILE_ARG],
        options: LINT_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.cli.lint_response", builder: lint_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "capabilities",
        aliases: &[],
        description: "Report backend capabilities.",
        usage: "valid capabilities [--json] [--progress=json] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>]",
        positional: &[],
        options: CAPABILITY_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.capabilities_request", builder: capabilities_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.capabilities_response", builder: capabilities_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "explain",
        aliases: &[],
        description: "Explain a failing property.",
        usage: "valid explain <model-file> [--json] [--progress=json] [--property=<id>] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>]",
        positional: &[MODEL_FILE_ARG],
        options: EXPLAIN_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.check_request", builder: check_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.explain_response", builder: explain_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "minimize",
        aliases: &[],
        description: "Minimize a counterexample trace.",
        usage: "valid minimize <model-file> [--json] [--progress=json] [--property=<id>] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>]",
        positional: &[MODEL_FILE_ARG],
        options: MINIMIZE_OPTIONS,
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.ai.minimize_response", builder: minimize_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "contract",
        aliases: &[],
        description: "Manage contract snapshots and drift.",
        usage: "valid contract <snapshot|lock|drift> <model-file> [lock-file] [--json] [--progress=json]",
        positional: &[CONTRACT_SUBCOMMAND_ARG, MODEL_FILE_ARG, LOCK_FILE_ARG],
        options: &[JSON_ARG, PROGRESS_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.contract_response", builder: valid_contract_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "trace",
        aliases: &[],
        description: "Render a verification trace.",
        usage: "valid trace <model-file> [--format=mermaid-state|mermaid-sequence|json] [--property=<id>] [--json] [--progress=json] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>]",
        positional: &[MODEL_FILE_ARG],
        options: TRACE_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.check_request", builder: check_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.evidence_trace", builder: evidence_trace_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "orchestrate",
        aliases: &[],
        description: "Run all properties for one model.",
        usage: "valid orchestrate <model-file> [--json] [--progress=json] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>]",
        positional: &[MODEL_FILE_ARG],
        options: ORCHESTRATE_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.orchestrate_request", builder: orchestrate_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.orchestrate_response", builder: orchestrate_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "testgen",
        aliases: &["generate-tests"],
        description: "Generate replayable test vectors.",
        usage: "valid testgen <model-file> [--json] [--progress=json] [--property=<id>] [--strategy=<...>] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>]",
        positional: &[MODEL_FILE_ARG],
        options: TESTGEN_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.testgen_request", builder: testgen_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.testgen_response", builder: testgen_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "distinguish",
        aliases: &[],
        description: "Find a trace that separates two models or two property interpretations.",
        usage: "valid distinguish <model-file> [--compare=<other-model-file>] [--property=<id>] [--compare-property=<id>] [--max-depth=<n>] [--json] [--progress=json]",
        positional: &[MODEL_FILE_ARG],
        options: &[JSON_ARG, PROGRESS_ARG],
        request_schema: None,
        response_schema: None,
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "replay",
        aliases: &[],
        description: "Replay a sequence of actions.",
        usage: "valid replay <model-file> [--json] [--progress=json] [--property=<id>] [--focus-action=<id>] [--actions=a,b,c]",
        positional: &[MODEL_FILE_ARG],
        options: REPLAY_OPTIONS,
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.replay_response", builder: replay_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "coverage",
        aliases: &[],
        description: "Compute coverage from executed traces.",
        usage: "valid coverage <model-file> [--json] [--progress=json] [--property=<id>] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>]",
        positional: &[MODEL_FILE_ARG],
        options: COVERAGE_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.check_request", builder: check_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.coverage_report", builder: coverage_report_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "clean",
        aliases: &[],
        description: "Remove generated artifacts.",
        usage: "valid clean [generated|artifacts|all] [--json] [--progress=json]",
        positional: &[CLEAN_SCOPE_ARG],
        options: CLEAN_OPTIONS,
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.clean_response", builder: clean_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "artifacts",
        aliases: &[],
        description: "List artifact index and run history.",
        usage: "valid artifacts [--json]",
        positional: &[],
        options: ARTIFACTS_OPTIONS,
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.artifact_inventory", builder: artifact_inventory_schema }),
        supports_json: true,
        supports_progress: false,
    },
    CommandSpec {
        name: "selfcheck",
        aliases: &[],
        description: "Run built-in smoke selfcheck.",
        usage: "valid selfcheck [--json] [--progress=json]",
        positional: &[],
        options: &[JSON_ARG, PROGRESS_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.selfcheck_report", builder: selfcheck_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "commands",
        aliases: &[],
        description: "List command metadata.",
        usage: "valid commands --json",
        positional: &[],
        options: COMMANDS_OPTIONS,
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.commands_response", builder: commands_response_schema }),
        supports_json: true,
        supports_progress: false,
    },
    CommandSpec {
        name: "mcp",
        aliases: &[],
        description: "Start the MCP server with project-first target discovery.",
        usage: "valid mcp [--project <dir>|--manifest-path <path>] [--registry <path>|--file <path>|--example <name>|--bin <name>] [--model-file <path>] [--name <server-name>] [--locked] [--offline] [--feature <name>] [--print-config <client>]",
        positional: &[],
        options: MCP_OPTIONS,
        request_schema: None,
        response_schema: None,
        supports_json: false,
        supports_progress: false,
    },
    CommandSpec {
        name: "completion",
        aliases: &["completions"],
        description: "Generate shell completion scripts.",
        usage: "valid completion <bash|fish|zsh>",
        positional: &[COMPLETION_SHELL_ARG],
        options: &[],
        request_schema: None,
        response_schema: None,
        supports_json: false,
        supports_progress: false,
    },
    CommandSpec {
        name: "schema",
        aliases: &[],
        description: "Return machine-readable schemas for a command.",
        usage: "valid schema <command>",
        positional: &[COMMAND_NAME_ARG],
        options: SCHEMA_OPTIONS,
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.schema_response", builder: schema_response_schema }),
        supports_json: true,
        supports_progress: false,
    },
    CommandSpec {
        name: "batch",
        aliases: &[],
        description: "Execute multiple CLI operations from one JSON request on stdin.",
        usage: "valid batch [--json] [--progress=json] < batch.json",
        positional: &[],
        options: BATCH_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.cli.batch_request", builder: batch_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.cli.batch_response", builder: batch_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
];

const REGISTRY_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "list",
        aliases: &["models"],
        description: "List registered models.",
        usage: "<registry-bin> list [--json]",
        positional: &[],
        options: &[JSON_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.list_response", builder: list_response_schema }),
        supports_json: true,
        supports_progress: false,
    },
    CommandSpec {
        name: "inspect",
        aliases: &[],
        description: "Inspect a registered model.",
        usage: "<registry-bin> inspect <model> [--json] [--progress=json]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG],
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.inspect_response", builder: inspect_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "graph",
        aliases: &["diagram"],
        description: "Render a registered model graph.",
        usage: "<registry-bin> graph <model> [--format=mermaid|dot|svg|text|json] [--view=<overview|logic|failure|deadlock|scc>] [--property=<id>] [--json] [--progress=json]",
        positional: &[MODEL_ARG],
        options: GRAPH_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.inspect_response", builder: inspect_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "doc",
        aliases: &[],
        description: "Generate registered model documentation and drift reports.",
        usage: "<registry-bin> doc <model> [--json] [--progress=json] [--write[=<path>]] [--check]",
        positional: &[MODEL_ARG],
        options: MIGRATE_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: None,
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "handoff",
        aliases: &[],
        description: "Generate implementation-oriented handoff briefs and drift reports.",
        usage: "<registry-bin> handoff <model> [--json] [--progress=json] [--property=<id>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>] [--write[=<path>]] [--check]",
        positional: &[MODEL_ARG],
        options: HANDOFF_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: None,
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "lint",
        aliases: &["readiness"],
        description: "Run readiness and maintainability lint checks on a registered model.",
        usage: "<registry-bin> lint <model> [--json] [--progress=json]",
        positional: &[MODEL_ARG],
        options: LINT_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.cli.lint_response", builder: lint_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "benchmark",
        aliases: &["bench"],
        description: "Benchmark a registered model.",
        usage: "<registry-bin> benchmark <model> [--json] [--progress=json] [--property=<id>] [--repeat=<n>] [--baseline[=compare|record|ignore]] [--threshold-percent=<n>] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>]",
        positional: &[MODEL_ARG],
        options: BENCHMARK_OPTIONS,
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.benchmark_response", builder: benchmark_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "migrate",
        aliases: &[],
        description: "Generate declarative migration snippets.",
        usage: "<registry-bin> migrate <model> [--json] [--progress=json] [--write[=<path>]] [--check]",
        positional: &[MODEL_ARG],
        options: MIGRATE_OPTIONS,
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.migration_response", builder: migration_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "check",
        aliases: &["verify"],
        description: "Verify a registered model.",
        usage: "<registry-bin> check <model> [--json] [--progress=json] [--property=<id>] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>]",
        positional: &[MODEL_ARG],
        options: CHECK_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.check_request", builder: check_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.cli.completed", builder: cli_completed_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "explain",
        aliases: &[],
        description: "Explain a failing registered model property.",
        usage: "<registry-bin> explain <model> [--json] [--progress=json]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG],
        request_schema: Some(SchemaRef { id: "schema.ai.check_request", builder: check_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.explain_response", builder: explain_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "coverage",
        aliases: &[],
        description: "Return registered model coverage.",
        usage: "<registry-bin> coverage <model> [--json] [--progress=json]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.coverage_report", builder: coverage_report_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "orchestrate",
        aliases: &[],
        description: "Run all registered model properties.",
        usage: "<registry-bin> orchestrate <model> [--json] [--progress=json] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>]",
        positional: &[MODEL_ARG],
        options: ORCHESTRATE_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.ai.orchestrate_request", builder: orchestrate_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.orchestrate_response", builder: orchestrate_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "testgen",
        aliases: &["generate-tests"],
        description: "Generate test vectors for a registered model.",
        usage: "<registry-bin> testgen <model> [--json] [--progress=json] [--property=<id>] [--strategy=<...>]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG, PROPERTY_ARG, STRATEGY_ARG],
        request_schema: Some(SchemaRef { id: "schema.ai.testgen_request", builder: testgen_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.testgen_response", builder: testgen_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "replay",
        aliases: &[],
        description: "Replay registered model actions.",
        usage: "<registry-bin> replay <model> [--json] [--progress=json] [--property=<id>] [--focus-action=<id>] [--actions=a,b,c]",
        positional: &[MODEL_ARG],
        options: REPLAY_OPTIONS,
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.replay_response", builder: replay_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "contract",
        aliases: &[],
        description: "Check contracts for all registered models.",
        usage: "<registry-bin> contract <snapshot|lock|drift|check> [lock-file] [--json] [--progress=json]",
        positional: &[CONTRACT_SUBCOMMAND_ARG, LOCK_FILE_ARG],
        options: &[JSON_ARG, PROGRESS_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.contract_response", builder: registry_contract_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "commands",
        aliases: &[],
        description: "List command metadata.",
        usage: "<registry-bin> commands --json",
        positional: &[],
        options: COMMANDS_OPTIONS,
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.commands_response", builder: commands_response_schema }),
        supports_json: true,
        supports_progress: false,
    },
    CommandSpec {
        name: "completion",
        aliases: &["completions"],
        description: "Generate shell completion scripts.",
        usage: "<registry-bin> completion <bash|fish|zsh>",
        positional: &[COMPLETION_SHELL_ARG],
        options: &[],
        request_schema: None,
        response_schema: None,
        supports_json: false,
        supports_progress: false,
    },
    CommandSpec {
        name: "schema",
        aliases: &[],
        description: "Return machine-readable schemas for a command.",
        usage: "<registry-bin> schema <command>",
        positional: &[COMMAND_NAME_ARG],
        options: SCHEMA_OPTIONS,
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.schema_response", builder: schema_response_schema }),
        supports_json: true,
        supports_progress: false,
    },
    CommandSpec {
        name: "batch",
        aliases: &[],
        description: "Execute multiple registry commands from one JSON request on stdin.",
        usage: "<registry-bin> batch [--json] [--progress=json] < batch.json",
        positional: &[],
        options: BATCH_OPTIONS,
        request_schema: Some(SchemaRef { id: "schema.cli.batch_request", builder: batch_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.cli.batch_response", builder: batch_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
];

const CARGO_VALID_COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "artifacts",
        aliases: &[],
        description: "List project artifact index and run history.",
        usage: "cargo valid artifacts [--json]",
        positional: &[],
        options: ARTIFACTS_OPTIONS,
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.artifact_inventory", builder: artifact_inventory_schema }),
        supports_json: true,
        supports_progress: false,
    },
    CommandSpec {
        name: "init",
        aliases: &[],
        description: "Scaffold valid.toml and registry source.",
        usage: "cargo valid init [--json] [--progress=json]",
        positional: &[],
        options: &[JSON_ARG, PROGRESS_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.init_response", builder: init_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "clean",
        aliases: &[],
        description: "Remove generated artifacts in a project.",
        usage: "cargo valid clean [generated|artifacts|all] [--json] [--progress=json]",
        positional: &[CLEAN_SCOPE_ARG],
        options: &[JSON_ARG, PROGRESS_ARG, MANIFEST_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.clean_response", builder: clean_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "list",
        aliases: &["models"],
        description: "List bundled or project models.",
        usage: "cargo valid models [--json]",
        positional: &[],
        options: &[JSON_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.list_response", builder: list_response_schema }),
        supports_json: true,
        supports_progress: false,
    },
    CommandSpec {
        name: "inspect",
        aliases: &[],
        description: "Inspect a bundled or project model.",
        usage: "cargo valid inspect <model> [--json] [--progress=json]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.inspect_response", builder: inspect_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "graph",
        aliases: &["diagram"],
        description: "Render a model graph.",
        usage: "cargo valid graph <model> [--format=mermaid|dot|svg|text|json] [--view=<overview|logic|failure|deadlock|scc>] [--property=<id>] [--json] [--progress=json]",
        positional: &[MODEL_ARG],
        options: &[FORMAT_ARG, VIEW_ARG, PROPERTY_ARG, JSON_ARG, PROGRESS_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.inspect_response", builder: inspect_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "doc",
        aliases: &[],
        description: "Generate model documentation and drift reports.",
        usage: "cargo valid doc <model> [--json] [--progress=json] [--write[=<path>]] [--check]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG, WRITE_ARG, CHECK_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: None,
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "handoff",
        aliases: &[],
        description: "Generate implementation-oriented handoff briefs and drift reports.",
        usage: "cargo valid handoff <model> [--json] [--progress=json] [--property=<id>] [--backend=<explicit|mock-bmc|sat-varisat|smt-cvc5|command>] [--solver-exec <path>] [--solver-arg <arg>] [--write[=<path>]] [--check]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG, PROPERTY_ARG, BACKEND_ARG, SOLVER_EXEC_ARG, SOLVER_ARG_ARG, WRITE_ARG, CHECK_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: None,
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "lint",
        aliases: &["readiness"],
        description: "Run model readiness and maintainability lint checks.",
        usage: "cargo valid lint <model> [--json] [--progress=json]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: Some(SchemaRef { id: "schema.ai.inspect_request", builder: inspect_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.cli.lint_response", builder: lint_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "benchmark",
        aliases: &["bench"],
        description: "Benchmark one model or suite.",
        usage: "cargo valid benchmark <model> [--json] [--progress=json] [--property=<id>] [--repeat=<n>] [--baseline[=compare|record|ignore]] [--threshold-percent=<n>]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG, PROPERTY_ARG, REPEAT_ARG, BASELINE_ARG, THRESHOLD_ARG, BACKEND_ARG, SOLVER_EXEC_ARG, SOLVER_ARG_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.benchmark_response", builder: benchmark_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "migrate",
        aliases: &[],
        description: "Generate migration snippets for step models.",
        usage: "cargo valid migrate <model> [--json] [--progress=json] [--write[=<path>]] [--check]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG, WRITE_ARG, CHECK_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.migration_response", builder: migration_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "check",
        aliases: &["verify"],
        description: "Verify a model.",
        usage: "cargo valid verify <model> [--json] [--progress=json] [--property=<id>] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG, PROPERTY_ARG, BACKEND_ARG, SOLVER_EXEC_ARG, SOLVER_ARG_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: Some(SchemaRef { id: "schema.ai.check_request", builder: check_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.cli.completed", builder: cli_completed_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "all",
        aliases: &["suite"],
        description: "Run verification across multiple models.",
        usage: "cargo valid suite [--json] [--progress=json]",
        positional: &[],
        options: &[JSON_ARG, PROGRESS_ARG, PROPERTY_ARG, BACKEND_ARG, SOLVER_EXEC_ARG, SOLVER_ARG_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.batch_runs_response", builder: batch_runs_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "explain",
        aliases: &[],
        description: "Explain a failing model property.",
        usage: "cargo valid explain <model> [--json] [--progress=json]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: Some(SchemaRef { id: "schema.ai.check_request", builder: check_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.explain_response", builder: explain_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "coverage",
        aliases: &[],
        description: "Compute coverage for a model.",
        usage: "cargo valid coverage <model> [--json] [--progress=json]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.coverage_report", builder: coverage_report_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "orchestrate",
        aliases: &[],
        description: "Run all properties for one model.",
        usage: "cargo valid orchestrate <model> [--json] [--progress=json] [--backend=<...>] [--solver-exec <path>] [--solver-arg <arg>]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG, BACKEND_ARG, SOLVER_EXEC_ARG, SOLVER_ARG_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: Some(SchemaRef { id: "schema.ai.orchestrate_request", builder: orchestrate_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.orchestrate_response", builder: orchestrate_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "testgen",
        aliases: &["generate-tests"],
        description: "Generate test vectors for a model.",
        usage: "cargo valid testgen <model> [--json] [--progress=json] [--property=<id>] [--strategy=<...>]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG, PROPERTY_ARG, STRATEGY_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: Some(SchemaRef { id: "schema.ai.testgen_request", builder: testgen_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.ai.testgen_response", builder: testgen_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "replay",
        aliases: &[],
        description: "Replay model actions.",
        usage: "cargo valid replay <model> [--json] [--progress=json] [--property=<id>] [--focus-action=<id>] [--actions=a,b,c]",
        positional: &[MODEL_ARG],
        options: &[JSON_ARG, PROGRESS_ARG, PROPERTY_ARG, FOCUS_ACTION_ARG, ACTIONS_ARG, MANIFEST_ARG, REGISTRY_ARG, FILE_ARG, EXAMPLE_ARG, BIN_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.replay_response", builder: replay_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
    CommandSpec {
        name: "commands",
        aliases: &[],
        description: "List command metadata.",
        usage: "cargo valid commands --json",
        positional: &[],
        options: &[JSON_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.commands_response", builder: commands_response_schema }),
        supports_json: true,
        supports_progress: false,
    },
    CommandSpec {
        name: "completion",
        aliases: &["completions"],
        description: "Generate shell completion scripts.",
        usage: "cargo valid completion <bash|fish|zsh>",
        positional: &[COMPLETION_SHELL_ARG],
        options: &[],
        request_schema: None,
        response_schema: None,
        supports_json: false,
        supports_progress: false,
    },
    CommandSpec {
        name: "schema",
        aliases: &[],
        description: "Return machine-readable schemas for a command.",
        usage: "cargo valid schema <command>",
        positional: &[COMMAND_NAME_ARG],
        options: &[JSON_ARG],
        request_schema: None,
        response_schema: Some(SchemaRef { id: "schema.cli.schema_response", builder: schema_response_schema }),
        supports_json: true,
        supports_progress: false,
    },
    CommandSpec {
        name: "batch",
        aliases: &[],
        description: "Execute multiple cargo-valid operations from one JSON request on stdin.",
        usage: "cargo valid batch [--json] [--progress=json] < batch.json",
        positional: &[],
        options: &[JSON_ARG, PROGRESS_ARG],
        request_schema: Some(SchemaRef { id: "schema.cli.batch_request", builder: batch_request_schema }),
        response_schema: Some(SchemaRef { id: "schema.cli.batch_response", builder: batch_response_schema }),
        supports_json: true,
        supports_progress: true,
    },
];

pub fn command_specs(surface: Surface) -> &'static [CommandSpec] {
    match surface {
        Surface::Valid => VALID_COMMANDS,
        Surface::CargoValid => CARGO_VALID_COMMANDS,
        Surface::Registry => REGISTRY_COMMANDS,
    }
}

pub fn find_command_spec(surface: Surface, command: &str) -> Option<&'static CommandSpec> {
    command_specs(surface)
        .iter()
        .find(|spec| spec.name == command || spec.aliases.iter().any(|alias| *alias == command))
}

pub fn render_completion(surface: Surface, shell: &str) -> Result<String, String> {
    let mut command = completion_command(surface);
    let mut buffer = Vec::new();
    match shell {
        "bash" => generate(
            Bash,
            &mut command,
            completion_bin_name(surface),
            &mut buffer,
        ),
        "fish" => generate(
            Fish,
            &mut command,
            completion_bin_name(surface),
            &mut buffer,
        ),
        "zsh" => generate(Zsh, &mut command, completion_bin_name(surface), &mut buffer),
        other => {
            return Err(format!(
                "unsupported shell `{other}`; expected bash, fish, or zsh"
            ))
        }
    }
    let script = String::from_utf8(buffer)
        .map_err(|error| format!("completion output was not utf-8: {error}"))?;
    Ok(augment_completion(script, surface, shell))
}

#[derive(Debug, Clone, Serialize)]
pub struct CompletionInstallResult {
    pub status: String,
    pub shell: String,
    pub command: String,
    pub written_files: Vec<String>,
    pub updated_shell_configs: Vec<String>,
}

pub fn install_completion(
    surface: Surface,
    shell: &str,
    shell_config: bool,
    dry_run: bool,
) -> Result<CompletionInstallResult, String> {
    let script = render_completion(surface, shell)?;
    let install_path = completion_install_path(surface, shell)?;
    let mut written_files = Vec::new();
    let mut updated_shell_configs = Vec::new();
    if !dry_run {
        if let Some(parent) = install_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create completion directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
        fs::write(&install_path, script).map_err(|error| {
            format!(
                "failed to write completion file `{}`: {error}",
                install_path.display()
            )
        })?;
    }
    written_files.push(install_path.display().to_string());

    if shell == "zsh" && shell_config {
        let rc_path = zsh_rc_path()?;
        if !dry_run {
            ensure_zsh_shell_config(&rc_path)?;
        }
        updated_shell_configs.push(rc_path.display().to_string());
    }

    Ok(CompletionInstallResult {
        status: if dry_run {
            "planned".to_string()
        } else {
            "installed".to_string()
        },
        shell: shell.to_string(),
        command: completion_command_label(surface).to_string(),
        written_files,
        updated_shell_configs,
    })
}

fn completion_install_path(surface: Surface, shell: &str) -> Result<PathBuf, String> {
    let home = home_dir()?;
    let command = completion_file_stem(surface);
    let path = match shell {
        "bash" => home
            .join(".local/share/bash-completion/completions")
            .join(command),
        "fish" => home
            .join(".config/fish/completions")
            .join(format!("{command}.fish")),
        "zsh" => home.join(".zsh/completions").join(format!("_{command}")),
        other => {
            return Err(format!(
                "unsupported shell `{other}`; expected bash, fish, or zsh"
            ))
        }
    };
    Ok(path)
}

fn completion_command_label(surface: Surface) -> &'static str {
    match surface {
        Surface::Valid => "valid",
        Surface::CargoValid => "cargo valid",
        Surface::Registry => "registry",
    }
}

fn completion_file_stem(surface: Surface) -> &'static str {
    match surface {
        Surface::Valid => "valid",
        Surface::CargoValid => "cargo-valid",
        Surface::Registry => "registry",
    }
}

fn home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| "HOME is not set".to_string())
}

fn zsh_rc_path() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(".zshrc"))
}

fn ensure_zsh_shell_config(path: &Path) -> Result<(), String> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    let mut next = existing.clone();
    if !existing.contains("fpath=(~/.zsh/completions $fpath)") {
        if !next.is_empty() && !next.ends_with('\n') {
            next.push('\n');
        }
        next.push_str("fpath=(~/.zsh/completions $fpath)\n");
    }
    if !existing.contains("autoload -Uz compinit && compinit") {
        if !next.is_empty() && !next.ends_with('\n') {
            next.push('\n');
        }
        next.push_str("autoload -Uz compinit && compinit\n");
    }
    if next != existing {
        fs::write(path, next).map_err(|error| {
            format!("failed to update zsh config `{}`: {error}", path.display())
        })?;
    }
    Ok(())
}

fn augment_completion(script: String, surface: Surface, shell: &str) -> String {
    match (surface, shell) {
        (Surface::Valid, "fish") => format!("{script}\n{}", valid_fish_completion()),
        (Surface::Valid, "bash") => valid_bash_completion(script),
        (Surface::Valid, "zsh") => valid_zsh_completion(script),
        (Surface::CargoValid, "fish") => format!("{script}\n{}", cargo_valid_fish_completion()),
        (Surface::CargoValid, "bash") => cargo_valid_bash_completion(script),
        (Surface::CargoValid, "zsh") => cargo_valid_zsh_completion(script),
        (Surface::Registry, "fish") => format!("{script}\n{}", registry_fish_completion()),
        (Surface::Registry, "bash") => registry_bash_completion(script),
        (Surface::Registry, "zsh") => registry_zsh_completion(script),
        _ => script,
    }
}

fn cargo_valid_model_commands() -> &'static [&'static str] {
    &[
        "inspect",
        "graph",
        "diagram",
        "doc",
        "handoff",
        "lint",
        "readiness",
        "benchmark",
        "bench",
        "migrate",
        "check",
        "verify",
        "explain",
        "coverage",
        "orchestrate",
        "testgen",
        "generate-tests",
        "replay",
    ]
}

fn cargo_valid_property_commands() -> &'static [&'static str] {
    &[
        "graph",
        "handoff",
        "benchmark",
        "bench",
        "check",
        "verify",
        "explain",
        "coverage",
        "testgen",
        "replay",
    ]
}

fn cargo_valid_action_commands() -> &'static [&'static str] {
    &["testgen", "generate-tests", "replay"]
}

fn cargo_valid_fish_completion() -> String {
    let model_commands = cargo_valid_model_commands().join(" ");
    let property_commands = cargo_valid_property_commands().join(" ");
    let action_commands = cargo_valid_action_commands().join(" ");
    format!(
        r#"
function __fish_cargo_valid_model_names
    cargo valid completion candidates models 2>/dev/null
end

function __fish_cargo_valid_current_model
    set -l tokens (commandline -opc)
    set -l expecting 0
    set -l saw_valid 0
    for token in $tokens[2..-1]
        if test $expecting -eq 1
            set expecting 0
            continue
        end
        if test "$token" = "valid" -a $saw_valid -eq 0
            set saw_valid 1
            continue
        end
        if test $saw_valid -eq 0
            continue
        end
        switch $token
            case --manifest-path --registry --file --example --bin --property --focus-action --actions --format --view --backend --solver-exec --solver-arg --repeat --seed
                set expecting 1
            case '--*'
            case inspect graph diagram doc handoff lint readiness benchmark bench migrate check verify explain coverage orchestrate testgen generate-tests replay
            case '*'
                echo $token
                return 0
        end
    end
end

function __fish_cargo_valid_property_names
    set -l model (__fish_cargo_valid_current_model)
    test -n "$model"; and cargo valid completion candidates properties $model 2>/dev/null
end

function __fish_cargo_valid_action_names
    set -l model (__fish_cargo_valid_current_model)
    test -n "$model"; and cargo valid completion candidates actions $model 2>/dev/null
end

function __fish_cargo_valid_view_names
    cargo valid completion candidates views 2>/dev/null
end

complete -c cargo -n "__fish_seen_subcommand_from valid; and __fish_seen_subcommand_from {model_commands}" -a "(__fish_cargo_valid_model_names)"
complete -c cargo -n "__fish_seen_subcommand_from valid; and __fish_seen_subcommand_from {property_commands}" -l property -a "(__fish_cargo_valid_property_names)"
complete -c cargo -n "__fish_seen_subcommand_from valid; and __fish_seen_subcommand_from {action_commands}" -l focus-action -a "(__fish_cargo_valid_action_names)"
complete -c cargo -n "__fish_seen_subcommand_from valid; and __fish_seen_subcommand_from replay" -l actions -a "(__fish_cargo_valid_action_names)"
complete -c cargo -n "__fish_seen_subcommand_from valid; and __fish_seen_subcommand_from graph diagram" -l view -a "(__fish_cargo_valid_view_names)"
"#
    )
}

fn cargo_valid_bash_completion(script: String) -> String {
    let model_commands = cargo_valid_model_commands()
        .iter()
        .map(|command| format!(r#""{command}""#))
        .collect::<Vec<_>>()
        .join(" ");
    let base = script.replacen("_cargo() {", "__valid_base_cargo_completion() {", 1);
    format!(
        r#"{base}

__valid_cargo_models() {{
    cargo valid completion candidates models 2>/dev/null
}}

__valid_cargo_command() {{
    local i token expecting_value=0 saw_valid=0
    for ((i = 1; i < COMP_CWORD; i++)); do
        token="${{COMP_WORDS[i]}}"
        if [[ $expecting_value -eq 1 ]]; then
            expecting_value=0
            continue
        fi
        if [[ "$token" == "valid" && $saw_valid -eq 0 ]]; then
            saw_valid=1
            continue
        fi
        if [[ $saw_valid -eq 0 ]]; then
            continue
        fi
        case "$token" in
            --manifest-path|--registry|--file|--example|--bin|--property|--focus-action|--actions|--format|--view|--backend|--solver-exec|--solver-arg|--repeat|--seed)
                expecting_value=1
                ;;
            --*)
                ;;
            {model_commands})
                printf '%s\n' "$token"
                return 0
                ;;
        esac
    done
    return 1
}}

__valid_cargo_model() {{
    local i token command="" expecting_value=0 saw_valid=0
    for ((i = 1; i < COMP_CWORD; i++)); do
        token="${{COMP_WORDS[i]}}"
        if [[ $expecting_value -eq 1 ]]; then
            expecting_value=0
            continue
        fi
        if [[ "$token" == "valid" && $saw_valid -eq 0 ]]; then
            saw_valid=1
            continue
        fi
        if [[ $saw_valid -eq 0 ]]; then
            continue
        fi
        case "$token" in
            --manifest-path|--registry|--file|--example|--bin|--property|--focus-action|--actions|--format|--view|--backend|--solver-exec|--solver-arg|--repeat|--seed)
                expecting_value=1
                ;;
            --*)
                ;;
            {model_commands})
                if [[ -z "$command" ]]; then
                    command="$token"
                else
                    printf '%s\n' "$token"
                    return 0
                fi
                ;;
        esac
    done
    return 1
}}

_cargo() {{
    local cur prev command model completed_actions prefix
    cur="${{COMP_WORDS[COMP_CWORD]}}"
    prev="${{COMP_WORDS[COMP_CWORD-1]}}"
    command="$(__valid_cargo_command)" || true
    model="$(__valid_cargo_model)" || true
    case "$prev" in
        --property)
            [[ -n "$model" ]] && COMPREPLY=( $(compgen -W "$(cargo valid completion candidates properties "$model" 2>/dev/null)" -- "$cur") )
            [[ ${{#COMPREPLY[@]}} -gt 0 ]] && return 0
            ;;
        --focus-action)
            [[ -n "$model" ]] && COMPREPLY=( $(compgen -W "$(cargo valid completion candidates actions "$model" 2>/dev/null)" -- "$cur") )
            [[ ${{#COMPREPLY[@]}} -gt 0 ]] && return 0
            ;;
        --actions)
            if [[ -n "$model" ]]; then
                completed_actions="${{cur%,*}}"
                prefix="${{completed_actions:+${{completed_actions}},}}"
                cur="${{cur##*,}}"
                COMPREPLY=( $(compgen -W "$(cargo valid completion candidates actions "$model" 2>/dev/null)" -- "$cur") )
                COMPREPLY=( "${{COMPREPLY[@]/#/$prefix}}" )
            fi
            [[ ${{#COMPREPLY[@]}} -gt 0 ]] && return 0
            ;;
        --view)
            COMPREPLY=( $(compgen -W "$(cargo valid completion candidates views 2>/dev/null)" -- "$cur") )
            [[ ${{#COMPREPLY[@]}} -gt 0 ]] && return 0
            ;;
    esac
    if [[ "${{COMP_WORDS[1]}}" == "valid" && -n "$command" && -z "$model" && "$cur" != -* ]]; then
        COMPREPLY=( $(compgen -W "$(__valid_cargo_models)" -- "$cur") )
        if [[ ${{#COMPREPLY[@]}} -gt 0 ]]; then
            return 0
        fi
    fi
    __valid_base_cargo_completion "$@"
}}
"#
    )
}

fn cargo_valid_zsh_completion(script: String) -> String {
    let model_commands = cargo_valid_model_commands().join("|");
    let base = script.replacen("_cargo() {", "__valid_base_cargo_completion() {", 1);
    format!(
        r#"{base}

__valid_cargo_models() {{
    local -a models
    models=("${{(@f)$(cargo valid completion candidates models 2>/dev/null)}}")
    (( ${{#models[@]}} )) || return 1
    _describe -t models 'cargo valid models' models "$@"
}}

__valid_cargo_command() {{
    local i token expecting_value=0 saw_valid=0
    for ((i = 2; i < CURRENT; ++i)); do
        token="${{words[i]}}"
        if (( expecting_value )); then
            expecting_value=0
            continue
        fi
        if [[ "$token" == "valid" && $saw_valid -eq 0 ]]; then
            saw_valid=1
            continue
        fi
        (( saw_valid )) || continue
        case "$token" in
            --manifest-path|--registry|--file|--example|--bin|--property|--focus-action|--actions|--format|--view|--backend|--solver-exec|--solver-arg|--repeat|--seed)
                expecting_value=1
                ;;
            --*)
                ;;
            ({model_commands}))
                print -r -- "$token"
                ;;
        esac
    done
    return 1
}}

__valid_cargo_model() {{
    local i token command="" expecting_value=0 saw_valid=0
    for ((i = 2; i < CURRENT; ++i)); do
        token="${{words[i]}}"
        if (( expecting_value )); then
            expecting_value=0
            continue
        fi
        if [[ "$token" == "valid" && $saw_valid -eq 0 ]]; then
            saw_valid=1
            continue
        fi
        (( saw_valid )) || continue
        case "$token" in
            --manifest-path|--registry|--file|--example|--bin|--property|--focus-action|--actions|--format|--view|--backend|--solver-exec|--solver-arg|--repeat|--seed)
                expecting_value=1
                ;;
            --*)
                ;;
            ({model_commands}))
                if [[ -z "$command" ]]; then
                    command="$token"
                else
                    print -r -- "$token"
                    return 0
                fi
                ;;
        esac
    done
    return 1
}}

_cargo() {{
    local command model prev current_word prefix current_action
    command="$(__valid_cargo_command)" || true
    model="$(__valid_cargo_model)" || true
    prev="${{words[CURRENT-1]}}"
    case "$prev" in
        --property)
            [[ -n "$model" ]] && _describe -t properties 'cargo valid properties' "${{(@f)$(cargo valid completion candidates properties "$model" 2>/dev/null)}}" && return 0
            ;;
        --focus-action)
            [[ -n "$model" ]] && _describe -t actions 'cargo valid actions' "${{(@f)$(cargo valid completion candidates actions "$model" 2>/dev/null)}}" && return 0
            ;;
        --actions)
            if [[ -n "$model" ]]; then
                current_word="${{words[CURRENT]}}"
                prefix="${{current_word%,*}}"
                current_action="${{current_word##*,}}"
                compadd -Q -S '' -- "${{(@f)$(cargo valid completion candidates actions "$model" 2>/dev/null)/#/${{prefix:+$prefix,}}}}"
                return 0
            fi
            ;;
        --view)
            _describe -t views 'cargo valid graph views' "${{(@f)$(cargo valid completion candidates views 2>/dev/null)}}" && return 0
            ;;
    esac
    if [[ -n "$command" && -z "$model" && "${{words[CURRENT]}}" != -* ]]; then
        __valid_cargo_models && return 0
    fi
    __valid_base_cargo_completion "$@"
}}
"#
    )
}

fn valid_fish_completion() -> String {
    r#"
function __fish_valid_model_names
    valid completion candidates models 2>/dev/null
end

function __fish_valid_current_model
    set -l tokens (commandline -opc)
    for token in $tokens[3..-1]
        switch $token
            case --property --scenario --focus-action --actions --format --view --backend --solver-exec --solver-arg --seed --write --runner --runner-arg
                return 1
            case '--*'
            case '*'
                echo $token
                return 0
        end
    end
end

function __fish_valid_property_names
    set -l model (__fish_valid_current_model)
    test -n "$model"; and valid completion candidates properties $model 2>/dev/null
end

function __fish_valid_action_names
    set -l model (__fish_valid_current_model)
    test -n "$model"; and valid completion candidates actions $model 2>/dev/null
end

function __fish_valid_view_names
    valid completion candidates views 2>/dev/null
end

complete -c valid -n "__fish_seen_subcommand_from inspect graph doc handoff lint explain minimize trace orchestrate testgen distinguish replay coverage conformance check verify" -a "(__fish_valid_model_names)"
complete -c valid -n "__fish_seen_subcommand_from graph handoff explain trace replay coverage check verify" -l property -a "(__fish_valid_property_names)"
complete -c valid -n "__fish_seen_subcommand_from replay testgen" -l focus-action -a "(__fish_valid_action_names)"
complete -c valid -n "__fish_seen_subcommand_from replay" -l actions -a "(__fish_valid_action_names)"
complete -c valid -n "__fish_seen_subcommand_from graph" -l view -a "(__fish_valid_view_names)"
"#
    .to_string()
}

fn valid_bash_completion(script: String) -> String {
    let base = script.replacen("_valid() {", "__valid_base_completion() {", 1);
    format!(
        r#"{base}

__valid_completion_command() {{
    local i token
    for ((i = 1; i < COMP_CWORD; i++)); do
        token="${{COMP_WORDS[i]}}"
        case "$token" in
            --*) ;;
            *) printf '%s\n' "$token"; return 0 ;;
        esac
    done
    return 1
}}

__valid_completion_model() {{
    local i token command=""
    for ((i = 1; i < COMP_CWORD; i++)); do
        token="${{COMP_WORDS[i]}}"
        case "$token" in
            --property|--scenario|--focus-action|--actions|--format|--view|--backend|--solver-exec|--solver-arg|--seed|--write|--runner|--runner-arg)
                ((i++))
                ;;
            --*) ;;
            *)
                if [[ -z "$command" ]]; then
                    command="$token"
                else
                    printf '%s\n' "$token"
                    return 0
                fi
                ;;
        esac
    done
    return 1
}}

_valid() {{
    local cur prev command model
    cur="${{COMP_WORDS[COMP_CWORD]}}"
    prev="${{COMP_WORDS[COMP_CWORD-1]}}"
    command="$(__valid_completion_command)" || true
    model="$(__valid_completion_model)" || true
    case "$prev" in
        --property)
            [[ -n "$model" ]] && COMPREPLY=( $(compgen -W "$(valid completion candidates properties "$model" 2>/dev/null)" -- "$cur") )
            [[ ${{#COMPREPLY[@]}} -gt 0 ]] && return 0
            ;;
        --focus-action|--actions)
            [[ -n "$model" ]] && COMPREPLY=( $(compgen -W "$(valid completion candidates actions "$model" 2>/dev/null)" -- "$cur") )
            [[ ${{#COMPREPLY[@]}} -gt 0 ]] && return 0
            ;;
        --view)
            COMPREPLY=( $(compgen -W "$(valid completion candidates views 2>/dev/null)" -- "$cur") )
            [[ ${{#COMPREPLY[@]}} -gt 0 ]] && return 0
            ;;
    esac
    if [[ -n "$command" && -z "$model" && "$cur" != -* ]]; then
        COMPREPLY=( $(compgen -W "$(valid completion candidates models 2>/dev/null)" -- "$cur") )
        [[ ${{#COMPREPLY[@]}} -gt 0 ]] && return 0
    fi
    __valid_base_completion "$@"
}}
"#
    )
}

fn valid_zsh_completion(script: String) -> String {
    let base = script.replacen("_valid() {", "__valid_base_completion() {", 1);
    format!(
        r#"{base}

__valid_completion_command() {{
    local i token
    for ((i = 2; i < CURRENT; ++i)); do
        token="${{words[i]}}"
        [[ "$token" == --* ]] && continue
        print -r -- "$token"
        return 0
    done
    return 1
}}

__valid_completion_model() {{
    local i token command=""
    for ((i = 2; i < CURRENT; ++i)); do
        token="${{words[i]}}"
        case "$token" in
            --property|--scenario|--focus-action|--actions|--format|--view|--backend|--solver-exec|--solver-arg|--seed|--write|--runner|--runner-arg)
                ((i++))
                ;;
            --*)
                ;;
            *)
                if [[ -z "$command" ]]; then
                    command="$token"
                else
                    print -r -- "$token"
                    return 0
                fi
                ;;
        esac
    done
    return 1
}}

_valid() {{
    local curcontext="$curcontext" state line command model prev
    command="$(__valid_completion_command)" || true
    model="$(__valid_completion_model)" || true
    prev="${{words[CURRENT-1]}}"
    case "$prev" in
        --property)
            [[ -n "$model" ]] && _describe -t properties 'valid properties' "${{(@f)$(valid completion candidates properties "$model" 2>/dev/null)}}" && return 0
            ;;
        --focus-action|--actions)
            [[ -n "$model" ]] && _describe -t actions 'valid actions' "${{(@f)$(valid completion candidates actions "$model" 2>/dev/null)}}" && return 0
            ;;
        --view)
            _describe -t views 'valid graph views' "${{(@f)$(valid completion candidates views 2>/dev/null)}}" && return 0
            ;;
    esac
    if [[ -n "$command" && -z "$model" && "${{words[CURRENT]}}" != -* ]]; then
        _describe -t models 'valid models' "${{(@f)$(valid completion candidates models 2>/dev/null)}}" && return 0
    fi
    __valid_base_completion "$@"
}}
"#
    )
}

fn registry_fish_completion() -> String {
    r#"
function __fish_registry_command_name
    set -l tokens (commandline -opc)
    echo $tokens[1]
end

function __fish_registry_model_names
    set -l cmd (__fish_registry_command_name)
    test -n "$cmd"; and $cmd completion candidates models 2>/dev/null
end

function __fish_registry_current_model
    set -l tokens (commandline -opc)
    for token in $tokens[3..-1]
        switch $token
            case --property --focus-action --actions --format --view --backend --solver-exec --solver-arg --repeat --write
                return 1
            case '--*'
            case '*'
                echo $token
                return 0
        end
    end
end

function __fish_registry_property_names
    set -l cmd (__fish_registry_command_name)
    set -l model (__fish_registry_current_model)
    test -n "$cmd"; and test -n "$model"; and $cmd completion candidates properties $model 2>/dev/null
end

function __fish_registry_action_names
    set -l cmd (__fish_registry_command_name)
    set -l model (__fish_registry_current_model)
    test -n "$cmd"; and test -n "$model"; and $cmd completion candidates actions $model 2>/dev/null
end

function __fish_registry_view_names
    set -l cmd (__fish_registry_command_name)
    test -n "$cmd"; and $cmd completion candidates views 2>/dev/null
end

complete -c registry -n "__fish_seen_subcommand_from inspect graph doc handoff lint benchmark migrate check explain coverage orchestrate testgen replay" -a "(__fish_registry_model_names)"
complete -c registry -n "__fish_seen_subcommand_from graph handoff benchmark check replay" -l property -a "(__fish_registry_property_names)"
complete -c registry -n "__fish_seen_subcommand_from replay testgen" -l focus-action -a "(__fish_registry_action_names)"
complete -c registry -n "__fish_seen_subcommand_from replay" -l actions -a "(__fish_registry_action_names)"
complete -c registry -n "__fish_seen_subcommand_from graph" -l view -a "(__fish_registry_view_names)"
"#
    .to_string()
}

fn registry_bash_completion(script: String) -> String {
    let base = script.replacen("_registry() {", "__valid_base_registry_completion() {", 1);
    format!(
        r#"{base}

__valid_registry_command() {{
    printf '%s\n' "${{COMP_WORDS[0]}}"
}}

__valid_registry_subcommand() {{
    local i token
    for ((i = 1; i < COMP_CWORD; i++)); do
        token="${{COMP_WORDS[i]}}"
        [[ "$token" == --* ]] && continue
        printf '%s\n' "$token"
        return 0
    done
    return 1
}}

__valid_registry_model() {{
    local i token command=""
    for ((i = 1; i < COMP_CWORD; i++)); do
        token="${{COMP_WORDS[i]}}"
        case "$token" in
            --property|--focus-action|--actions|--format|--view|--backend|--solver-exec|--solver-arg|--repeat|--write)
                ((i++))
                ;;
            --*)
                ;;
            *)
                if [[ -z "$command" ]]; then
                    command="$token"
                else
                    printf '%s\n' "$token"
                    return 0
                fi
                ;;
        esac
    done
    return 1
}}

_registry() {{
    local cur prev cmd model registry_cmd
    cur="${{COMP_WORDS[COMP_CWORD]}}"
    prev="${{COMP_WORDS[COMP_CWORD-1]}}"
    cmd="$(__valid_registry_subcommand)" || true
    model="$(__valid_registry_model)" || true
    registry_cmd="$(__valid_registry_command)"
    case "$prev" in
        --property)
            [[ -n "$model" ]] && COMPREPLY=( $(compgen -W "$($registry_cmd completion candidates properties "$model" 2>/dev/null)" -- "$cur") )
            [[ ${{#COMPREPLY[@]}} -gt 0 ]] && return 0
            ;;
        --focus-action|--actions)
            [[ -n "$model" ]] && COMPREPLY=( $(compgen -W "$($registry_cmd completion candidates actions "$model" 2>/dev/null)" -- "$cur") )
            [[ ${{#COMPREPLY[@]}} -gt 0 ]] && return 0
            ;;
        --view)
            COMPREPLY=( $(compgen -W "$($registry_cmd completion candidates views 2>/dev/null)" -- "$cur") )
            [[ ${{#COMPREPLY[@]}} -gt 0 ]] && return 0
            ;;
    esac
    if [[ -n "$cmd" && -z "$model" && "$cur" != -* ]]; then
        COMPREPLY=( $(compgen -W "$($registry_cmd completion candidates models 2>/dev/null)" -- "$cur") )
        [[ ${{#COMPREPLY[@]}} -gt 0 ]] && return 0
    fi
    __valid_base_registry_completion "$@"
}}
"#
    )
}

fn registry_zsh_completion(script: String) -> String {
    let base = script.replacen("_registry() {", "__valid_base_registry_completion() {", 1);
    format!(
        r#"{base}

__valid_registry_command() {{
    print -r -- "${{words[1]}}"
}}

__valid_registry_subcommand() {{
    local i token
    for ((i = 2; i < CURRENT; ++i)); do
        token="${{words[i]}}"
        [[ "$token" == --* ]] && continue
        print -r -- "$token"
        return 0
    done
    return 1
}}

__valid_registry_model() {{
    local i token command=""
    for ((i = 2; i < CURRENT; ++i)); do
        token="${{words[i]}}"
        case "$token" in
            --property|--focus-action|--actions|--format|--view|--backend|--solver-exec|--solver-arg|--repeat|--write)
                ((i++))
                ;;
            --*)
                ;;
            *)
                if [[ -z "$command" ]]; then
                    command="$token"
                else
                    print -r -- "$token"
                    return 0
                fi
                ;;
        esac
    done
    return 1
}}

_registry() {{
    local prev cmd model registry_cmd
    prev="${{words[CURRENT-1]}}"
    cmd="$(__valid_registry_subcommand)" || true
    model="$(__valid_registry_model)" || true
    registry_cmd="$(__valid_registry_command)"
    case "$prev" in
        --property)
            [[ -n "$model" ]] && _describe -t properties 'registry properties' "${{(@f)$($registry_cmd completion candidates properties "$model" 2>/dev/null)}}" && return 0
            ;;
        --focus-action|--actions)
            [[ -n "$model" ]] && _describe -t actions 'registry actions' "${{(@f)$($registry_cmd completion candidates actions "$model" 2>/dev/null)}}" && return 0
            ;;
        --view)
            _describe -t views 'registry graph views' "${{(@f)$($registry_cmd completion candidates views 2>/dev/null)}}" && return 0
            ;;
    esac
    if [[ -n "$cmd" && -z "$model" && "${{words[CURRENT]}}" != -* ]]; then
        _describe -t models 'registry models' "${{(@f)$($registry_cmd completion candidates models 2>/dev/null)}}" && return 0
    fi
    __valid_base_registry_completion "$@"
}}
"#
    )
}

fn completion_bin_name(surface: Surface) -> &'static str {
    match surface {
        Surface::Valid => "valid",
        Surface::CargoValid => "cargo",
        Surface::Registry => "registry",
    }
}

fn completion_command(surface: Surface) -> Command {
    let mut root = match surface {
        Surface::Valid => Command::new("valid"),
        Surface::CargoValid => {
            let mut cargo = Command::new("cargo");
            cargo = cargo.subcommand(build_surface_command("valid", surface));
            return cargo;
        }
        Surface::Registry => Command::new("registry"),
    };

    root = root.subcommand_required(true).arg_required_else_help(true);
    for spec in command_specs(surface) {
        root = root.subcommand(build_command_spec(spec));
    }
    root
}

fn build_surface_command(name: &'static str, surface: Surface) -> Command {
    let mut command = Command::new(name)
        .subcommand_required(true)
        .arg_required_else_help(true);
    if matches!(surface, Surface::CargoValid) {
        command = command
            .arg(Arg::new("manifest-path").long("manifest-path").num_args(1))
            .arg(Arg::new("registry").long("registry").num_args(1))
            .arg(Arg::new("file").long("file").num_args(1))
            .arg(Arg::new("example").long("example").num_args(1))
            .arg(Arg::new("bin").long("bin").num_args(1));
    }
    for spec in command_specs(surface) {
        command = command.subcommand(build_command_spec(spec));
    }
    command
}

fn build_command_spec(spec: &CommandSpec) -> Command {
    let mut command = Command::new(spec.name).about(spec.description);
    for alias in spec.aliases {
        command = command.visible_alias(alias);
    }
    for (index, positional) in spec.positional.iter().enumerate() {
        command = command.arg(build_arg_spec(positional, Some(index + 1)));
    }
    for option in spec.options {
        command = command.arg(build_arg_spec(option, None));
    }
    command
}

fn build_arg_spec(spec: &ArgSpec, index: Option<usize>) -> Arg {
    let mut arg = Arg::new(spec.name).help(spec.description);
    if let Some(index) = index {
        arg = arg.index(index);
    } else if let Some(long_name) = long_name_from_syntax(spec.syntax) {
        arg = arg.long(long_name);
    }

    if spec.positional {
        if spec.multiple {
            arg = arg.num_args(1..);
        }
    } else if spec.value_type == "bool" {
        arg = arg.action(ArgAction::SetTrue);
    } else if spec.syntax.contains("[=<") {
        arg = arg.num_args(0..=1);
    } else if spec.multiple {
        arg = arg.action(ArgAction::Append).num_args(1);
    } else {
        arg = arg.num_args(1);
    }

    if !spec.values.is_empty() {
        arg = arg.value_parser(spec.values.to_vec());
    }
    arg
}

fn long_name_from_syntax(syntax: &str) -> Option<&str> {
    let trimmed = syntax.trim();
    let stripped = trimmed.strip_prefix("--")?;
    let end = stripped
        .find(|ch: char| ch == '[' || ch == '<' || ch == ' ' || ch == '=')
        .unwrap_or(stripped.len());
    Some(&stripped[..end])
}

pub fn render_commands_json(surface: Surface) -> String {
    let commands = command_specs(surface)
        .iter()
        .map(|spec| {
            json!({
                "name": spec.name,
                "aliases": spec.aliases,
                "description": spec.description,
                "usage": spec.usage,
                "supports_json": spec.supports_json,
                "supports_progress": spec.supports_progress,
                "positional": spec.positional,
                "options": spec.options,
                "schemas": {
                    "parameters": parameter_schema_id(surface, spec.name),
                    "request": spec.request_schema.map(|schema| schema.id),
                    "response": spec.response_schema.map(|schema| schema.id),
                    "error": "schema.cli.error",
                    "progress": if spec.supports_progress { Some("schema.cli.progress") } else { None },
                }
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&json!({
        "schema_version": CLI_SCHEMA_VERSION,
        "surface": surface.slug(),
        "commands": commands
    }))
    .expect("commands json")
}

pub fn render_commands_text(surface: Surface) -> String {
    let mut lines = Vec::new();
    for spec in command_specs(surface) {
        let alias = if spec.aliases.is_empty() {
            String::new()
        } else {
            format!(" (aliases: {})", spec.aliases.join(", "))
        };
        lines.push(format!("{}{} - {}", spec.name, alias, spec.description));
    }
    lines.join("\n")
}

pub fn render_schema_json(surface: Surface, command: &str) -> Result<String, String> {
    let Some(spec) = find_command_spec(surface, command) else {
        return Err(format!("unknown command `{command}`"));
    };
    let value = json!({
        "schema_version": CLI_SCHEMA_VERSION,
        "surface": surface.slug(),
        "command": spec.name,
        "aliases": spec.aliases,
        "description": spec.description,
        "usage": spec.usage,
        "parameter_schema_id": parameter_schema_id(surface, spec.name),
        "parameter_schema": parameter_schema(surface, spec),
        "request_schema_id": spec.request_schema.map(|schema| schema.id),
        "request_schema": spec.request_schema.map(|schema| (schema.builder)()),
        "response_schema_id": spec.response_schema.map(|schema| schema.id),
        "response_schema": spec.response_schema.map(|schema| (schema.builder)()),
        "error_schema_id": "schema.cli.error",
        "error_schema": cli_error_schema(),
        "progress_schema_id": if spec.supports_progress { Some("schema.cli.progress") } else { None },
        "progress_schema": if spec.supports_progress { Some(cli_progress_schema()) } else { None },
    });
    serde_json::to_string(&value).map_err(|err| err.to_string())
}

pub fn detect_json_flag(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--json")
}

pub fn detect_progress_json_flag(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--progress=json")
}

pub fn render_cli_error_json(
    command: &str,
    diagnostics: &[Diagnostic],
    usage: Option<&str>,
) -> String {
    let mut value = json!({
        "schema_version": CLI_SCHEMA_VERSION,
        "kind": "cli_error",
        "command": command,
        "status": ExitCode::Error.status_label(),
        "exit_code": ExitCode::Error.code(),
        "diagnostics": diagnostics.iter().map(diagnostic_to_value).collect::<Vec<_>>(),
    });
    if let Some(usage) = usage {
        value["usage"] = Value::String(usage.to_string());
    }
    serde_json::to_string(&value).expect("cli error json")
}

pub fn render_cli_warning_json(command: &str, message: &str) -> String {
    serde_json::to_string(&json!({
        "schema_version": CLI_SCHEMA_VERSION,
        "kind": "warning",
        "command": command,
        "message": message
    }))
    .expect("cli warning json")
}

pub fn message_diagnostic(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(
        ErrorCode::SearchError,
        DiagnosticSegment::EngineSearch,
        message.into(),
    )
}

pub fn usage_diagnostic(message: impl Into<String>, usage: &str) -> Diagnostic {
    message_diagnostic(message).with_help(usage)
}

pub fn parse_batch_request(body: &str) -> Result<BatchRequest, String> {
    let request: BatchRequest = serde_json::from_str(body).map_err(|err| err.to_string())?;
    if request.operations.is_empty() {
        return Err("batch request requires at least one operation".to_string());
    }
    if request.schema_version.trim().is_empty() {
        return Err("schema_version must be a non-empty string".to_string());
    }
    Ok(request)
}

pub fn render_batch_response(exit_code: ExitCode, results: Vec<BatchResult>) -> String {
    serde_json::to_string(&BatchResponse {
        schema_version: CLI_SCHEMA_VERSION.to_string(),
        status: exit_code.status_label(),
        exit_code: exit_code.code(),
        results,
    })
    .expect("batch response json")
}

pub fn child_stream_to_json(bytes: &[u8]) -> Value {
    if bytes.is_empty() {
        return Value::Null;
    }
    match serde_json::from_slice::<Value>(bytes) {
        Ok(value) => value,
        Err(_) => Value::String(String::from_utf8_lossy(bytes).trim().to_string()),
    }
}

fn diagnostic_to_value(diagnostic: &Diagnostic) -> Value {
    json!({
        "error_code": diagnostic.error_code.as_str(),
        "segment": diagnostic.segment.as_str(),
        "severity": match diagnostic.severity {
            crate::support::diagnostics::Severity::Info => "info",
            crate::support::diagnostics::Severity::Warning => "warning",
            crate::support::diagnostics::Severity::Error => "error",
        },
        "message": diagnostic.message,
        "primary_span": diagnostic.primary_span.as_ref().map(|span| json!({
            "source": span.source,
            "line": span.line,
            "column": span.column
        })),
        "conflicts": diagnostic.conflicts,
        "help": diagnostic.help,
        "best_practices": diagnostic.best_practices,
    })
}

fn parameter_schema_id(surface: Surface, command: &str) -> String {
    format!("schema.cli.{}.{}.parameters", surface.slug(), command)
}

fn parameter_schema(surface: Surface, spec: &CommandSpec) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for arg in spec.positional.iter().chain(spec.options.iter()) {
        properties.insert(arg.name.to_string(), arg_schema(arg));
        if arg.required {
            required.push(Value::String(arg.name.to_string()));
        }
    }
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": parameter_schema_id(surface, spec.name),
        "type": "object",
        "additionalProperties": false,
        "required": required,
        "properties": properties,
    })
}

fn arg_schema(arg: &ArgSpec) -> Value {
    let mut merged = match arg.value_type {
        "boolean" => Map::from_iter([("type".to_string(), Value::String("boolean".to_string()))]),
        "integer" => Map::from_iter([
            ("type".to_string(), Value::String("integer".to_string())),
            ("minimum".to_string(), Value::from(0)),
        ]),
        "array" => Map::from_iter([
            ("type".to_string(), Value::String("array".to_string())),
            ("items".to_string(), json!({ "type": "string" })),
        ]),
        _ => Map::from_iter([("type".to_string(), Value::String("string".to_string()))]),
    };
    merged.insert(
        "description".to_string(),
        Value::String(arg.description.to_string()),
    );
    merged.insert(
        "x-cli-syntax".to_string(),
        Value::String(arg.syntax.to_string()),
    );
    merged.insert("x-cli-positional".to_string(), Value::Bool(arg.positional));
    if !arg.values.is_empty() {
        merged.insert(
            "enum".to_string(),
            Value::Array(
                arg.values
                    .iter()
                    .map(|value| Value::String((*value).to_string()))
                    .collect(),
            ),
        );
    }
    Value::Object(merged)
}

fn schema_version_string() -> String {
    CLI_SCHEMA_VERSION.to_string()
}

fn default_true() -> bool {
    true
}

fn default_continue_on_error() -> bool {
    true
}

fn parse_schema(body: &str) -> Value {
    serde_json::from_str(body).expect("valid embedded schema")
}

fn run_result_schema() -> Value {
    parse_schema(RUN_RESULT_SCHEMA)
}

fn cli_completed_schema() -> Value {
    parse_schema(CLI_COMPLETED_SCHEMA)
}

fn cli_error_schema() -> Value {
    parse_schema(CLI_ERROR_SCHEMA)
}

fn evidence_trace_schema() -> Value {
    parse_schema(EVIDENCE_TRACE_SCHEMA)
}

fn coverage_report_schema() -> Value {
    parse_schema(COVERAGE_REPORT_SCHEMA)
}

fn contract_snapshot_schema() -> Value {
    parse_schema(CONTRACT_SNAPSHOT_SCHEMA)
}

fn contract_lock_schema() -> Value {
    parse_schema(CONTRACT_LOCK_SCHEMA)
}

fn contract_drift_schema() -> Value {
    parse_schema(CONTRACT_DRIFT_SCHEMA)
}

fn selfcheck_schema() -> Value {
    parse_schema(SELF_CHECK_SCHEMA)
}

fn inspect_request_schema() -> Value {
    parse_schema(INSPECT_REQUEST_SCHEMA)
}

fn inspect_response_schema() -> Value {
    parse_schema(INSPECT_RESPONSE_SCHEMA)
}

fn check_request_schema() -> Value {
    parse_schema(CHECK_REQUEST_SCHEMA)
}

fn explain_response_schema() -> Value {
    parse_schema(EXPLAIN_RESPONSE_SCHEMA)
}

fn minimize_response_schema() -> Value {
    parse_schema(MINIMIZE_RESPONSE_SCHEMA)
}

fn testgen_request_schema() -> Value {
    parse_schema(TESTGEN_REQUEST_SCHEMA)
}

fn testgen_response_schema() -> Value {
    parse_schema(TESTGEN_RESPONSE_SCHEMA)
}

fn orchestrate_request_schema() -> Value {
    parse_schema(ORCHESTRATE_REQUEST_SCHEMA)
}

fn orchestrate_response_schema() -> Value {
    parse_schema(ORCHESTRATE_RESPONSE_SCHEMA)
}

fn capabilities_request_schema() -> Value {
    parse_schema(CAPABILITIES_REQUEST_SCHEMA)
}

fn capabilities_response_schema() -> Value {
    parse_schema(CAPABILITIES_RESPONSE_SCHEMA)
}

fn lint_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.lint_response",
        "type": "object",
        "required": ["schema_version", "request_id", "status", "model_id", "capabilities", "findings"],
        "properties": {
            "schema_version": { "type": "string" },
            "request_id": { "type": "string" },
            "status": { "type": "string" },
            "model_id": { "type": "string" },
            "capabilities": { "type": "object" },
            "findings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["category", "severity", "code", "message"],
                    "properties": {
                        "category": { "type": "string" },
                        "severity": { "type": "string" },
                        "code": { "type": "string" },
                        "message": { "type": "string" },
                        "suggestion": { "type": ["string", "null"] },
                        "snippet": { "type": ["string", "null"] }
                    }
                }
            }
        }
    })
}

fn benchmark_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.benchmark_response",
        "type": "object",
        "required": ["summary", "baseline"],
        "properties": {
            "artifact_path": { "type": "string" },
            "summary": {
                "type": "object",
                "required": ["schema_version", "request_id", "model_id", "backend", "repeat", "pass_count", "fail_count", "unknown_count", "error_count", "iterations"],
                "properties": {
                    "schema_version": { "type": "string" },
                    "request_id": { "type": "string" },
                    "model_id": { "type": "string" },
                    "backend": { "type": "string" },
                    "property_id": { "type": ["string", "null"] },
                    "repeat": { "type": "integer", "minimum": 0 },
                    "total_elapsed_ms": { "type": "integer", "minimum": 0 },
                    "min_elapsed_ms": { "type": "integer", "minimum": 0 },
                    "max_elapsed_ms": { "type": "integer", "minimum": 0 },
                    "average_elapsed_ms": { "type": "integer", "minimum": 0 },
                    "pass_count": { "type": "integer", "minimum": 0 },
                    "fail_count": { "type": "integer", "minimum": 0 },
                    "unknown_count": { "type": "integer", "minimum": 0 },
                    "error_count": { "type": "integer", "minimum": 0 },
                    "iterations": { "type": "array", "items": { "type": "object" } }
                }
            },
            "baseline": { "type": ["object", "null"] }
        }
    })
}

fn migration_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.migration_response",
        "type": "object",
        "required": ["schema_version", "request_id", "status", "model_id", "snippets", "check"],
        "properties": {
            "written": { "type": "string" },
            "schema_version": { "type": "string" },
            "request_id": { "type": "string" },
            "status": { "type": "string" },
            "model_id": { "type": "string" },
            "snippets": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["code", "snippet"],
                    "properties": {
                        "code": { "type": "string" },
                        "action": { "type": ["string", "null"] },
                        "snippet": { "type": "string" }
                    }
                }
            },
            "check": { "type": ["object", "null"] }
        }
    })
}

fn replay_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.replay_response",
        "type": "object",
        "required": ["schema_version", "status", "property_id", "replayed_actions", "terminal_state", "focus_action_enabled", "path_tags"],
        "properties": {
            "schema_version": { "type": "string" },
            "status": { "type": "string" },
            "property_id": { "type": "string" },
            "replayed_actions": { "type": "array", "items": { "type": "string" } },
            "terminal_state": { "type": "object" },
            "focus_action_id": { "type": ["string", "null"] },
            "focus_action_enabled": { "type": ["boolean", "null"] },
            "property_holds": { "type": ["boolean", "null"] },
            "path_tags": { "type": "array", "items": { "type": "string" } }
        }
    })
}

fn clean_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.clean_response",
        "type": "object",
        "required": ["status", "root", "removed"],
        "properties": {
            "status": { "type": "string", "enum": ["ok"] },
            "root": { "type": "string" },
            "removed": { "type": "array", "items": { "type": "string" } }
        }
    })
}

fn artifact_inventory_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.artifact_inventory",
        "type": "object",
        "required": ["schema_version", "artifact_index_path", "run_history_path", "artifacts", "runs"],
        "properties": {
            "schema_version": { "type": "string" },
            "artifact_index_path": { "type": "string" },
            "run_history_path": { "type": "string" },
            "artifacts": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["artifact_kind", "path", "run_id"],
                    "properties": {
                        "artifact_kind": { "type": "string" },
                        "path": { "type": "string" },
                        "run_id": { "type": "string" },
                        "model_id": { "type": ["string", "null"] },
                        "property_id": { "type": ["string", "null"] },
                        "evidence_id": { "type": ["string", "null"] },
                        "vector_id": { "type": ["string", "null"] },
                        "suite_id": { "type": ["string", "null"] }
                    }
                }
            },
            "runs": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["run_id", "artifact_paths", "artifact_kinds", "model_ids", "property_ids"],
                    "properties": {
                        "run_id": { "type": "string" },
                        "artifact_paths": { "type": "array", "items": { "type": "string" } },
                        "artifact_kinds": { "type": "array", "items": { "type": "string" } },
                        "model_ids": { "type": "array", "items": { "type": "string" } },
                        "property_ids": { "type": "array", "items": { "type": "string" } }
                    }
                }
            }
        }
    })
}

fn init_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.init_response",
        "type": "object",
        "required": ["status", "created", "registry", "scaffolded_registry", "generated_tests_dir", "root", "cargo_init_ran", "created_files", "created_directories", "skipped_existing", "model_files", "mcp_configs", "ai_bootstrap_guide", "rdd_guide"],
        "properties": {
            "status": { "type": "string", "enum": ["ok"] },
            "root": { "type": "string" },
            "cargo_init_ran": { "type": "boolean" },
            "created": { "type": "string" },
            "registry": { "type": "string" },
            "scaffolded_registry": { "type": "string" },
            "generated_tests_dir": { "type": "string" },
            "artifacts_dir": { "type": "string" },
            "benchmarks_baseline_dir": { "type": "string" },
            "created_files": {
                "type": "array",
                "items": { "type": "string" }
            },
            "created_directories": {
                "type": "array",
                "items": { "type": "string" }
            },
            "skipped_existing": {
                "type": "array",
                "items": { "type": "string" }
            },
            "model_files": {
                "type": "array",
                "items": { "type": "string" }
            },
            "mcp_configs": {
                "type": "array",
                "items": { "type": "string" }
            },
            "ai_bootstrap_guide": { "type": "string" },
            "rdd_guide": { "type": "string" }
        }
    })
}

fn list_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.list_response",
        "type": "object",
        "required": ["models"],
        "properties": {
            "models": { "type": "array", "items": { "type": "string" } }
        }
    })
}

fn batch_runs_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.batch_runs_response",
        "type": "object",
        "required": ["runs"],
        "properties": {
            "runs": {
                "type": "array",
                "items": run_result_schema()
            }
        }
    })
}

fn valid_contract_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.contract_response",
        "oneOf": [
            contract_snapshot_schema(),
            contract_lock_schema(),
            contract_drift_schema()
        ]
    })
}

fn registry_contract_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.contract_response",
        "oneOf": [
            { "type": "object", "required": ["snapshots"], "properties": { "snapshots": { "type": "array", "items": contract_snapshot_schema() } } },
            contract_lock_schema(),
            { "type": "object", "required": ["reports"], "properties": { "reports": { "type": "array", "items": contract_drift_schema() } } }
        ]
    })
}

fn commands_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.commands_response",
        "type": "object",
        "required": ["schema_version", "surface", "commands"],
        "properties": {
            "schema_version": { "type": "string" },
            "surface": { "type": "string" },
            "commands": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["name", "aliases", "description", "usage", "supports_json", "supports_progress", "positional", "options", "schemas"],
                    "properties": {
                        "name": { "type": "string" },
                        "aliases": { "type": "array", "items": { "type": "string" } },
                        "description": { "type": "string" },
                        "usage": { "type": "string" },
                        "supports_json": { "type": "boolean" },
                        "supports_progress": { "type": "boolean" },
                        "positional": { "type": "array", "items": { "type": "object" } },
                        "options": { "type": "array", "items": { "type": "object" } },
                        "schemas": { "type": "object" }
                    }
                }
            }
        }
    })
}

fn schema_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.schema_response",
        "type": "object",
        "required": ["schema_version", "surface", "command", "parameter_schema_id", "parameter_schema", "error_schema_id", "error_schema"],
        "properties": {
            "schema_version": { "type": "string" },
            "surface": { "type": "string" },
            "command": { "type": "string" },
            "aliases": { "type": "array", "items": { "type": "string" } },
            "description": { "type": "string" },
            "usage": { "type": "string" },
            "parameter_schema_id": { "type": "string" },
            "parameter_schema": { "type": "object" },
            "request_schema_id": { "type": ["string", "null"] },
            "request_schema": { "type": ["object", "null"] },
            "response_schema_id": { "type": ["string", "null"] },
            "response_schema": { "type": ["object", "null"] },
            "error_schema_id": { "type": "string" },
            "error_schema": { "type": "object" },
            "progress_schema_id": { "type": ["string", "null"] },
            "progress_schema": { "type": ["object", "null"] }
        }
    })
}

fn cli_progress_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.progress",
        "type": "object",
        "required": ["schema_version", "kind", "command", "event"],
        "properties": {
            "schema_version": { "type": "string" },
            "kind": { "type": "string", "enum": ["progress"] },
            "command": { "type": "string" },
            "event": { "type": "string" },
            "total": { "type": ["integer", "null"] },
            "index": { "type": "integer" },
            "target": { "type": "string" },
            "status": { "type": "string" },
            "exit_code": { "type": "integer" }
        }
    })
}

fn batch_request_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.batch_request",
        "type": "object",
        "required": ["schema_version", "continue_on_error", "operations"],
        "properties": {
            "schema_version": { "type": "string" },
            "continue_on_error": { "type": "boolean" },
            "operations": {
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "required": ["command", "args", "json"],
                    "properties": {
                        "command": { "type": "string" },
                        "args": { "type": "array", "items": { "type": "string" } },
                        "json": { "type": "boolean" }
                    }
                }
            }
        }
    })
}

fn batch_response_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "schema.cli.batch_response",
        "type": "object",
        "required": ["schema_version", "status", "exit_code", "results"],
        "properties": {
            "schema_version": { "type": "string" },
            "status": { "type": "string", "enum": ["SUCCESS", "FAIL", "UNKNOWN", "ERROR"] },
            "exit_code": { "type": "integer", "enum": [0, 1, 2, 3] },
            "results": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["index", "command", "args", "exit_code", "stdout", "stderr"],
                    "properties": {
                        "index": { "type": "integer", "minimum": 0 },
                        "command": { "type": "string" },
                        "args": { "type": "array", "items": { "type": "string" } },
                        "exit_code": { "type": "integer" },
                        "stdout": { "type": ["object", "array", "string", "number", "boolean", "null"] },
                        "stderr": { "type": ["object", "array", "string", "number", "boolean", "null"] }
                    }
                }
            }
        }
    })
}
