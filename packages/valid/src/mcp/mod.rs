use std::{
    collections::BTreeSet,
    fs,
    io::{self, BufRead, Write},
    path::Path as FsPath,
    process::Command,
};

use serde::Deserialize;
use serde_json::{json, Map, Value};

mod docs_catalog;
mod prompts_catalog;

use crate::{
    api::{
        check_source, compile_source, explain_source, inspect_source, lint_source,
        render_explain_json, render_inspect_json, render_lint_json, testgen_source, CheckRequest,
        InspectRequest, TestgenRequest,
    },
    bundled_models::list_bundled_models,
    contract::{compare_snapshot, parse_lock_file, snapshot_model},
    coverage::{collect_coverage, render_coverage_json},
    engine::CheckOutcome,
    evidence::{render_diagnostics_json, render_outcome_json},
    frontend::compile_model,
    ir::Path,
    kernel::{eval::eval_expr, replay::replay_actions, transition::apply_action},
    project::{rerun_recommendations, ProjectConfig},
    reporter::{
        build_failure_graph_slice, render_model_dot_failure, render_model_dot_with_view,
        render_model_mermaid_failure, render_model_mermaid_with_view, render_model_svg_failure,
        render_model_svg_with_view, render_model_text_failure, GraphView,
    },
    testgen::render_replay_json,
};

const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &[
    "2025-11-25",
    "2025-11-05",
    "2025-06-18",
    "2025-03-26",
    "2024-11-05",
];
const JSON_SCHEMA_DRAFT_2020_12: &str = "https://json-schema.org/draft/2020-12/schema";
const PAGE_SIZE: usize = 64;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub server_name: String,
    pub default_model_file: Option<String>,
    pub default_registry_binary: Option<String>,
    pub project_config: Option<ProjectConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum LogLevel {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
    Critical,
    Alert,
    Emergency,
}

impl LogLevel {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "notice" => Some(Self::Notice),
            "warning" => Some(Self::Warning),
            "error" => Some(Self::Error),
            "critical" => Some(Self::Critical),
            "alert" => Some(Self::Alert),
            "emergency" => Some(Self::Emergency),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Notice => "notice",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
            Self::Alert => "alert",
            Self::Emergency => "emergency",
        }
    }
}

#[derive(Debug)]
struct SessionState {
    initialize_seen: bool,
    client_initialized: bool,
    log_level: LogLevel,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            initialize_seen: false,
            client_initialized: false,
            log_level: LogLevel::Info,
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        let project_config = std::env::current_dir()
            .ok()
            .and_then(|cwd| crate::project::load_project_config(&cwd).ok().flatten());
        Self {
            server_name: "valid".to_string(),
            default_model_file: std::env::var("VALID_MCP_MODEL_FILE").ok(),
            default_registry_binary: std::env::var("VALID_MCP_REGISTRY_BINARY").ok(),
            project_config,
        }
    }
}

pub fn serve_stdio(config: ServerConfig) -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    let mut session = SessionState::default();

    for line in stdin.lock().lines() {
        let line = line.map_err(|err| format!("failed to read stdin: {err}"))?;
        if line.trim().is_empty() {
            continue;
        }

        let incoming = match serde_json::from_str::<Value>(&line) {
            Ok(value) => value,
            Err(error) => {
                write_message(
                    &mut writer,
                    &error_response(Value::Null, -32700, &format!("parse error: {error}")),
                )?;
                continue;
            }
        };

        let maybe_response = if let Some(batch) = incoming.as_array() {
            let responses = batch
                .iter()
                .filter_map(|message| handle_message(message, &config, &mut session, &mut writer))
                .collect::<Vec<_>>();
            if responses.is_empty() {
                None
            } else {
                Some(Value::Array(responses))
            }
        } else {
            handle_message(&incoming, &config, &mut session, &mut writer)
        };

        if let Some(response) = maybe_response {
            write_message(&mut writer, &response)?;
        }
    }

    Ok(())
}

fn write_message(writer: &mut impl Write, payload: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, payload)
        .map_err(|err| format!("failed to encode response: {err}"))?;
    writer
        .write_all(b"\n")
        .map_err(|err| format!("failed to write response: {err}"))?;
    writer
        .flush()
        .map_err(|err| format!("failed to flush response: {err}"))
}

fn handle_message(
    message: &Value,
    config: &ServerConfig,
    session: &mut SessionState,
    writer: &mut impl Write,
) -> Option<Value> {
    let object = match message.as_object() {
        Some(object) => object,
        None => {
            return Some(error_response(
                Value::Null,
                -32600,
                "request must be an object",
            ))
        }
    };

    let jsonrpc = object
        .get("jsonrpc")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if jsonrpc != "2.0" {
        return Some(error_response(
            object.get("id").cloned().unwrap_or(Value::Null),
            -32600,
            "jsonrpc must be \"2.0\"",
        ));
    }

    let method = match object.get("method").and_then(Value::as_str) {
        Some(method) => method,
        None => {
            return Some(error_response(
                object.get("id").cloned().unwrap_or(Value::Null),
                -32600,
                "method must be present",
            ))
        }
    };

    let id = object.get("id").cloned();
    let params = object.get("params").cloned().unwrap_or(Value::Null);

    if id.is_none() {
        handle_notification(method, &params, session);
        return None;
    }

    let id = id.unwrap_or(Value::Null);
    if !session.initialize_seen && !matches!(method, "initialize" | "ping") {
        return Some(error_response(
            id,
            -32002,
            "server must be initialized before calling this method",
        ));
    }

    Some(match method {
        "initialize" => {
            session.initialize_seen = true;
            maybe_log(
                writer,
                session,
                LogLevel::Info,
                "initialize",
                json!({
                    "requestedProtocolVersion": params.get("protocolVersion").and_then(Value::as_str),
                    "serverName": config.server_name,
                }),
            );
            response(id, initialize_result(config, &params))
        }
        "ping" => response(id, json!({})),
        "logging/setLevel" => match logging_set_level(session, &params) {
            Ok(result) => response(id, result),
            Err(error) => error_response(id, -32602, &error),
        },
        "tools/list" => match list_tools(&params) {
            Ok(result) => {
                maybe_log(writer, session, LogLevel::Debug, "tools/list", json!({}));
                response(id, result)
            }
            Err(error) => error_response(id, -32602, &error),
        },
        "tools/call" => {
            let tool_name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let result = handle_tool_call(config, &params);
            maybe_log(
                writer,
                session,
                if result.is_error {
                    LogLevel::Warning
                } else {
                    LogLevel::Info
                },
                "tools/call",
                json!({
                    "tool": tool_name,
                    "isError": result.is_error
                }),
            );
            response(id, result.into_value())
        }
        "resources/list" => match list_resources(config, &params) {
            Ok(result) => {
                maybe_log(
                    writer,
                    session,
                    LogLevel::Debug,
                    "resources/list",
                    json!({}),
                );
                response(id, result)
            }
            Err(error) => error_response(id, -32602, &error),
        },
        "resources/read" => match read_resource(config, &params) {
            Ok(result) => {
                maybe_log(
                    writer,
                    session,
                    LogLevel::Info,
                    "resources/read",
                    json!({
                        "uri": params.get("uri").and_then(Value::as_str)
                    }),
                );
                response(id, result)
            }
            Err(error) => error_response(id, -32002, &error),
        },
        "resources/templates/list" => match list_resource_templates(&params) {
            Ok(result) => response(id, result),
            Err(error) => error_response(id, -32602, &error),
        },
        "prompts/list" => match list_prompts(&params) {
            Ok(result) => {
                maybe_log(writer, session, LogLevel::Debug, "prompts/list", json!({}));
                response(id, result)
            }
            Err(error) => error_response(id, -32602, &error),
        },
        "prompts/get" => match get_prompt(config, &params) {
            Ok(result) => {
                maybe_log(
                    writer,
                    session,
                    LogLevel::Info,
                    "prompts/get",
                    json!({
                        "name": params.get("name").and_then(Value::as_str)
                    }),
                );
                response(id, result)
            }
            Err(error) => error_response(id, -32002, &error),
        },
        _ => error_response(id, -32601, &format!("method `{method}` is not supported")),
    })
}

fn handle_notification(method: &str, params: &Value, session: &mut SessionState) {
    if matches!(
        method,
        "notifications/initialized" | "notifications/cancelled"
    ) {
        if method == "notifications/initialized" {
            session.client_initialized = true;
        }
        return;
    }
    if method == "notifications/message" && params.get("level").is_some() {
        return;
    }
}

fn initialize_result(config: &ServerConfig, params: &Value) -> Value {
    let requested_version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(SUPPORTED_PROTOCOL_VERSIONS[0]);
    let protocol_version = if SUPPORTED_PROTOCOL_VERSIONS.contains(&requested_version) {
        requested_version
    } else {
        SUPPORTED_PROTOCOL_VERSIONS[0]
    };
    json!({
        "protocolVersion": protocol_version,
        "capabilities": {
            "tools": { "listChanged": true },
            "resources": {
                "subscribe": false,
                "listChanged": true
            },
            "prompts": { "listChanged": true },
            "logging": {}
        },
        "serverInfo": {
            "name": config.server_name,
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": "Use resources/list or valid_docs_index to discover guidance. Start with ai-authoring-guide, then valid_example_get or resources/read for examples. Use model_file or source for .valid files, or registry_binary plus model_name for Rust registry mode."
    })
}

fn response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

fn tool_definitions() -> Vec<Value> {
    vec![
        tool(
            "valid_docs_index",
            "List AI-facing docs with stable ids, audience, and recommended entrypoints.",
            input_schema_empty(),
            output_schema_docs_index(),
            true,
        ),
        tool(
            "valid_docs_get",
            "Fetch a documentation entry by stable doc id with structured guidance and markdown body.",
            input_schema_with_doc_id(),
            output_schema_docs_get(),
            true,
        ),
        tool(
            "valid_examples_list",
            "List curated learning examples with concepts, mode, and recommended order.",
            input_schema_empty(),
            output_schema_examples_list(),
            true,
        ),
        tool(
            "valid_example_get",
            "Fetch a curated example by stable example id with commands, concepts, and source text.",
            input_schema_with_example_id(),
            output_schema_example_get(),
            true,
        ),
        tool(
            "valid_inspect",
            "Inspect a valid model and return state fields, actions, properties, and capabilities.",
            input_schema_with_backend(),
            output_schema_with_success_required(&["model_id", "actions", "properties", "capabilities"]),
            true,
        ),
        tool(
            "valid_check",
            "Run property verification and return PASS, FAIL, or UNKNOWN with evidence details.",
            input_schema_with_backend_and_property(),
            output_schema_with_success_required(&["status", "property_result"]),
            true,
        ),
        tool(
            "valid_explain",
            "Explain a counterexample and return likely causes, hints, and involved fields.",
            input_schema_with_backend_and_property(),
            output_schema_with_success_required(&["property_id"]),
            true,
        ),
        tool(
            "valid_coverage",
            "Compute transition and guard coverage from the current verification trace.",
            input_schema_with_backend_and_property(),
            output_schema_with_success_required(&["model_id", "summary"]),
            true,
        ),
        tool(
            "valid_testgen",
            "Generate regression or witness vectors for a model.",
            input_schema_with_testgen(),
            output_schema_with_success_required(&["status", "vector_ids", "generated_files"]),
            false,
        ),
        tool(
            "valid_replay",
            "Replay an action sequence and report the terminal state and property result.",
            input_schema_with_replay(),
            output_schema_with_success_required(&["status"]),
            true,
        ),
        tool(
            "valid_contract_snapshot",
            "Return the current contract hash for a model or registry.",
            input_schema_with_contract_snapshot(),
            output_schema_any_object(),
            true,
        ),
        tool(
            "valid_contract_check",
            "Compare the current contract against a lock file and report drift.",
            input_schema_with_contract_check(),
            output_schema_any_object(),
            true,
        ),
        tool(
            "valid_suite_run",
            "Run configured critical properties or a named property suite.",
            input_schema_suite_run(),
            output_schema_with_success_required(&["selection_mode", "runs"]),
            false,
        ),
        tool(
            "valid_list_models",
            "List bundled models or models exported by a registry binary.",
            input_schema_list_models(),
            output_schema_with_success_required(&["models"]),
            true,
        ),
        tool(
            "valid_graph",
            "Render a model graph as Mermaid, DOT, SVG, text, or JSON.",
            input_schema_with_graph(),
            output_schema_with_success_required(&["format"]),
            true,
        ),
        tool(
            "valid_lint",
            "Run static analysis and capability lint checks on a model.",
            input_schema_basic(),
            output_schema_with_success_required(&["status", "findings"]),
            true,
        ),
    ]
}

fn tool(
    name: &str,
    description: &str,
    input_schema: Value,
    output_schema: Value,
    read_only: bool,
) -> Value {
    json!({
        "name": name,
        "title": name,
        "description": description,
        "inputSchema": input_schema,
        "outputSchema": output_schema,
        "annotations": {
            "readOnlyHint": read_only,
            "destructiveHint": false,
            "idempotentHint": read_only,
            "openWorldHint": false
        }
    })
}

fn output_schema_any_object() -> Value {
    json!({
        "$schema": JSON_SCHEMA_DRAFT_2020_12,
        "oneOf": [
            {
                "type": "object",
                "additionalProperties": true
            },
            error_object_schema()
        ]
    })
}

fn output_schema_with_success_required(required: &[&str]) -> Value {
    json!({
        "$schema": JSON_SCHEMA_DRAFT_2020_12,
        "oneOf": [
            {
                "type": "object",
                "required": required,
                "additionalProperties": true
            },
            error_object_schema()
        ]
    })
}

fn output_schema_docs_index() -> Value {
    json!({
        "$schema": JSON_SCHEMA_DRAFT_2020_12,
        "oneOf": [
            {
                "type": "object",
                "required": ["canonical_entry", "docs"],
                "properties": {
                    "canonical_entry": { "type": "string" },
                    "docs": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["doc_id", "title", "source_path"],
                            "additionalProperties": true
                        }
                    }
                },
                "additionalProperties": true
            },
            error_object_schema()
        ]
    })
}

fn output_schema_docs_get() -> Value {
    json!({
        "$schema": JSON_SCHEMA_DRAFT_2020_12,
        "oneOf": [
            {
                "type": "object",
                "required": ["doc_id", "title", "source_path", "body_markdown"],
                "properties": {
                    "doc_id": { "type": "string" },
                    "title": { "type": "string" },
                    "source_path": { "type": "string" },
                    "body_markdown": { "type": "string" }
                },
                "additionalProperties": true
            },
            error_object_schema()
        ]
    })
}

fn output_schema_examples_list() -> Value {
    json!({
        "$schema": JSON_SCHEMA_DRAFT_2020_12,
        "oneOf": [
            {
                "type": "object",
                "required": ["examples"],
                "properties": {
                    "examples": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "required": ["example_id", "title", "source_path"],
                            "additionalProperties": true
                        }
                    }
                },
                "additionalProperties": true
            },
            error_object_schema()
        ]
    })
}

fn output_schema_example_get() -> Value {
    json!({
        "$schema": JSON_SCHEMA_DRAFT_2020_12,
        "oneOf": [
            {
                "type": "object",
                "required": ["example_id", "title", "source_path", "source_text"],
                "properties": {
                    "example_id": { "type": "string" },
                    "title": { "type": "string" },
                    "source_path": { "type": "string" },
                    "source_text": { "type": "string" }
                },
                "additionalProperties": true
            },
            error_object_schema()
        ]
    })
}

fn error_object_schema() -> Value {
    json!({
        "type": "object",
        "required": ["error"],
        "properties": {
            "error": { "type": "string" }
        },
        "additionalProperties": true
    })
}

fn input_schema_empty() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

fn input_schema_with_doc_id() -> Value {
    json!({
        "type": "object",
        "properties": {
            "doc_id": { "type": "string" }
        },
        "required": ["doc_id"],
        "additionalProperties": false
    })
}

fn input_schema_with_example_id() -> Value {
    json!({
        "type": "object",
        "properties": {
            "example_id": { "type": "string" }
        },
        "required": ["example_id"],
        "additionalProperties": false
    })
}

fn input_schema_basic() -> Value {
    json!({
        "type": "object",
        "properties": common_target_properties(),
        "additionalProperties": false,
        "anyOf": [
            { "required": ["model_file"] },
            { "required": ["source"] },
            { "required": ["registry_binary", "model_name"] }
        ]
    })
}

fn input_schema_with_backend() -> Value {
    let mut properties = common_target_properties();
    properties.insert(
        "backend".to_string(),
        json!({
            "type": "string",
            "enum": ["explicit", "mock-bmc", "sat-varisat", "smt-cvc5", "command"]
        }),
    );
    properties.insert("solver_executable".to_string(), json!({ "type": "string" }));
    properties.insert(
        "solver_args".to_string(),
        json!({
            "type": "array",
            "items": { "type": "string" }
        }),
    );
    json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false,
        "anyOf": [
            { "required": ["model_file"] },
            { "required": ["source"] },
            { "required": ["registry_binary", "model_name"] }
        ]
    })
}

fn input_schema_with_backend_and_property() -> Value {
    let mut properties = common_target_properties();
    properties.insert("property_id".to_string(), json!({ "type": "string" }));
    properties.insert("scenario_id".to_string(), json!({ "type": "string" }));
    properties.insert(
        "backend".to_string(),
        json!({
            "type": "string",
            "enum": ["explicit", "mock-bmc", "sat-varisat", "smt-cvc5", "command"]
        }),
    );
    properties.insert("solver_executable".to_string(), json!({ "type": "string" }));
    properties.insert(
        "solver_args".to_string(),
        json!({
            "type": "array",
            "items": { "type": "string" }
        }),
    );
    json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false,
        "anyOf": [
            { "required": ["model_file"] },
            { "required": ["source"] },
            { "required": ["registry_binary", "model_name"] }
        ]
    })
}

fn input_schema_with_testgen() -> Value {
    let mut properties = common_target_properties();
    properties.insert("property_id".to_string(), json!({ "type": "string" }));
    properties.insert("strategy".to_string(), json!({
        "type": "string",
        "enum": ["counterexample", "transition", "witness", "guard", "boundary", "path", "random"]
    }));
    properties.insert(
        "backend".to_string(),
        json!({
            "type": "string",
            "enum": ["explicit", "mock-bmc", "sat-varisat", "smt-cvc5", "command"]
        }),
    );
    properties.insert("solver_executable".to_string(), json!({ "type": "string" }));
    properties.insert(
        "solver_args".to_string(),
        json!({
            "type": "array",
            "items": { "type": "string" }
        }),
    );
    json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false,
        "anyOf": [
            { "required": ["model_file"] },
            { "required": ["source"] },
            { "required": ["registry_binary", "model_name"] }
        ]
    })
}

fn input_schema_with_replay() -> Value {
    let mut properties = common_target_properties();
    properties.insert("property_id".to_string(), json!({ "type": "string" }));
    properties.insert("focus_action_id".to_string(), json!({ "type": "string" }));
    properties.insert(
        "actions".to_string(),
        json!({
            "type": "array",
            "items": { "type": "string" }
        }),
    );
    json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false,
        "anyOf": [
            { "required": ["model_file"] },
            { "required": ["source"] },
            { "required": ["registry_binary", "model_name"] }
        ]
    })
}

fn input_schema_with_contract_snapshot() -> Value {
    json!({
        "type": "object",
        "properties": {
            "model_file": { "type": "string" },
            "source_name": { "type": "string" },
            "source": { "type": "string" },
            "registry_binary": { "type": "string" },
            "model_name": { "type": "string" }
        },
        "additionalProperties": false
    })
}

fn input_schema_with_contract_check() -> Value {
    let mut properties = common_target_properties();
    properties.insert("lock_file".to_string(), json!({ "type": "string" }));
    json!({
        "type": "object",
        "properties": properties,
        "required": ["lock_file"],
        "additionalProperties": false,
        "anyOf": [
            { "required": ["model_file", "lock_file"] },
            { "required": ["source", "lock_file"] },
            { "required": ["registry_binary", "lock_file"] }
        ]
    })
}

fn input_schema_list_models() -> Value {
    json!({
        "type": "object",
        "properties": {
            "registry_binary": { "type": "string" }
        },
        "additionalProperties": false
    })
}

fn input_schema_suite_run() -> Value {
    let mut properties = common_target_properties();
    properties.insert("critical".to_string(), json!({ "type": "boolean" }));
    properties.insert("suite_name".to_string(), json!({ "type": "string" }));
    properties.insert(
        "backend".to_string(),
        json!({
            "type": "string",
            "enum": ["explicit", "mock-bmc", "sat-varisat", "smt-cvc5", "command"]
        }),
    );
    properties.insert("solver_executable".to_string(), json!({ "type": "string" }));
    properties.insert(
        "solver_args".to_string(),
        json!({ "type": "array", "items": { "type": "string" } }),
    );
    json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false
    })
}

fn input_schema_with_graph() -> Value {
    let mut properties = common_target_properties();
    properties.insert(
        "format".to_string(),
        json!({
            "type": "string",
            "enum": ["mermaid", "dot", "svg", "text", "json"]
        }),
    );
    properties.insert(
        "view".to_string(),
        json!({
            "type": "string",
            "enum": ["overview", "logic", "failure"]
        }),
    );
    properties.insert("property_id".to_string(), json!({ "type": "string" }));
    json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false,
        "anyOf": [
            { "required": ["model_file"] },
            { "required": ["source"] },
            { "required": ["registry_binary", "model_name"] }
        ]
    })
}

fn common_target_properties() -> serde_json::Map<String, Value> {
    let mut properties = serde_json::Map::new();
    properties.insert("model_file".to_string(), json!({ "type": "string" }));
    properties.insert("source_name".to_string(), json!({ "type": "string" }));
    properties.insert("source".to_string(), json!({ "type": "string" }));
    properties.insert("registry_binary".to_string(), json!({ "type": "string" }));
    properties.insert("model_name".to_string(), json!({ "type": "string" }));
    properties
}

fn handle_tool_call(config: &ServerConfig, params: &Value) -> ToolResult {
    let call: ToolCallParams = match serde_json::from_value(params.clone()) {
        Ok(call) => call,
        Err(err) => return ToolResult::error_message(format!("invalid tool call: {err}")),
    };
    let arguments = call.arguments.unwrap_or_else(|| json!({}));

    let outcome = match call.name.as_str() {
        "valid_docs_index" => docs_index_tool(),
        "valid_docs_get" => {
            let args = parse_args::<DocGetArgs>(&arguments);
            match args {
                Ok(args) => docs_get_tool(&args),
                Err(error) => Ok(ToolResult::error_message(error)),
            }
        }
        "valid_examples_list" => examples_list_tool(),
        "valid_example_get" => {
            let args = parse_args::<ExampleGetArgs>(&arguments);
            match args {
                Ok(args) => example_get_tool(&args),
                Err(error) => Ok(ToolResult::error_message(error)),
            }
        }
        "valid_inspect" => {
            let args = parse_args::<BasicArgs>(&arguments);
            match args {
                Ok(args) => inspect_tool(config, &args),
                Err(error) => Ok(ToolResult::error_message(error)),
            }
        }
        "valid_check" => {
            let args = parse_args::<BackendArgs>(&arguments);
            match args {
                Ok(args) => check_tool(config, &args),
                Err(error) => Ok(ToolResult::error_message(error)),
            }
        }
        "valid_explain" => {
            let args = parse_args::<BackendArgs>(&arguments);
            match args {
                Ok(args) => explain_tool(config, &args),
                Err(error) => Ok(ToolResult::error_message(error)),
            }
        }
        "valid_coverage" => {
            let args = parse_args::<BackendArgs>(&arguments);
            match args {
                Ok(args) => coverage_tool(config, &args),
                Err(error) => Ok(ToolResult::error_message(error)),
            }
        }
        "valid_testgen" => {
            let args = parse_args::<TestgenArgs>(&arguments);
            match args {
                Ok(args) => testgen_tool(config, &args),
                Err(error) => Ok(ToolResult::error_message(error)),
            }
        }
        "valid_replay" => {
            let args = parse_args::<ReplayArgs>(&arguments);
            match args {
                Ok(args) => replay_tool(config, &args),
                Err(error) => Ok(ToolResult::error_message(error)),
            }
        }
        "valid_contract_snapshot" => {
            let args = parse_args::<ContractSnapshotArgs>(&arguments);
            match args {
                Ok(args) => contract_snapshot_tool(config, &args),
                Err(error) => Ok(ToolResult::error_message(error)),
            }
        }
        "valid_contract_check" => {
            let args = parse_args::<ContractCheckArgs>(&arguments);
            match args {
                Ok(args) => contract_check_tool(config, &args),
                Err(error) => Ok(ToolResult::error_message(error)),
            }
        }
        "valid_list_models" => {
            let args = parse_args::<ListModelsArgs>(&arguments);
            match args {
                Ok(args) => list_models_tool(config, &args),
                Err(error) => Ok(ToolResult::error_message(error)),
            }
        }
        "valid_suite_run" => {
            let args = parse_args::<SuiteRunArgs>(&arguments);
            match args {
                Ok(args) => suite_run_tool(config, &args),
                Err(error) => Ok(ToolResult::error_message(error)),
            }
        }
        "valid_graph" => {
            let args = parse_args::<GraphArgs>(&arguments);
            match args {
                Ok(args) => graph_tool(config, &args),
                Err(error) => Ok(ToolResult::error_message(error)),
            }
        }
        "valid_lint" => {
            let args = parse_args::<BasicArgs>(&arguments);
            match args {
                Ok(args) => lint_tool(config, &args),
                Err(error) => Ok(ToolResult::error_message(error)),
            }
        }
        other => Ok(ToolResult::error_message(format!("unknown tool `{other}`"))),
    };
    outcome.unwrap_or_else(ToolResult::error_message)
}

fn parse_args<T>(arguments: &Value) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(arguments.clone())
        .map_err(|err| format!("invalid tool arguments: {err}"))
}

#[derive(Debug, Deserialize)]
struct ToolCallParams {
    name: String,
    #[serde(default)]
    arguments: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PaginatedParams {
    cursor: Option<String>,
    #[serde(default, rename = "_meta")]
    _meta: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ReadResourceParams {
    uri: String,
    #[serde(default, rename = "_meta")]
    _meta: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct GetPromptParams {
    name: String,
    #[serde(default)]
    arguments: Map<String, Value>,
    #[serde(default, rename = "_meta")]
    _meta: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SetLevelParams {
    level: String,
    #[serde(default, rename = "_meta")]
    _meta: Option<Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct DocGetArgs {
    doc_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ExampleGetArgs {
    example_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct TargetArgs {
    model_file: Option<String>,
    source_name: Option<String>,
    source: Option<String>,
    registry_binary: Option<String>,
    model_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct BasicArgs {
    #[serde(flatten)]
    target: TargetArgs,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct BackendArgs {
    #[serde(flatten)]
    target: TargetArgs,
    property_id: Option<String>,
    scenario_id: Option<String>,
    backend: Option<String>,
    solver_executable: Option<String>,
    solver_args: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct TestgenArgs {
    #[serde(flatten)]
    target: TargetArgs,
    property_id: Option<String>,
    strategy: Option<String>,
    backend: Option<String>,
    solver_executable: Option<String>,
    solver_args: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ReplayArgs {
    #[serde(flatten)]
    target: TargetArgs,
    property_id: Option<String>,
    focus_action_id: Option<String>,
    actions: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ContractSnapshotArgs {
    #[serde(flatten)]
    target: TargetArgs,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ContractCheckArgs {
    #[serde(flatten)]
    target: TargetArgs,
    lock_file: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ListModelsArgs {
    registry_binary: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct SuiteRunArgs {
    #[serde(flatten)]
    target: TargetArgs,
    critical: bool,
    suite_name: Option<String>,
    backend: Option<String>,
    solver_executable: Option<String>,
    solver_args: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct GraphArgs {
    #[serde(flatten)]
    target: TargetArgs,
    format: Option<String>,
    view: Option<String>,
    property_id: Option<String>,
}

enum ResolvedTarget {
    Dsl {
        source_name: String,
        source: String,
    },
    Registry {
        registry_binary: String,
        model_name: String,
    },
}

enum ContractTarget {
    Dsl {
        source: String,
    },
    Registry {
        registry_binary: String,
        model_name: Option<String>,
    },
}

impl TargetArgs {
    fn resolve(&self, config: &ServerConfig) -> Result<ResolvedTarget, String> {
        let explicit_registry_binary = normalized_option(&self.registry_binary);
        let explicit_model_name = normalized_option(&self.model_name);
        let explicit_model_file = normalized_option(&self.model_file);
        let explicit_source = normalized_option(&self.source);
        let source_name = normalized_option(&self.source_name);
        let default_registry_binary =
            explicit_or_default(&self.registry_binary, &config.default_registry_binary);
        let default_model_file = explicit_or_default(&self.model_file, &config.default_model_file);
        let uses_registry = explicit_registry_binary.is_some() || explicit_model_name.is_some();
        let uses_dsl = explicit_model_file.is_some() || explicit_source.is_some();

        if uses_registry && uses_dsl {
            return Err(
                "provide either model_file/source or registry_binary+model_name, not both"
                    .to_string(),
            );
        }

        if uses_registry {
            let registry_binary = explicit_registry_binary
                .or(default_registry_binary)
                .ok_or_else(|| "registry_binary is required".to_string())?;
            let model_name = explicit_model_name
                .ok_or_else(|| "model_name is required when using registry mode".to_string())?;
            return Ok(ResolvedTarget::Registry {
                registry_binary,
                model_name,
            });
        }

        let source = if let Some(source) = explicit_source {
            source
        } else if let Some(path) = explicit_model_file.clone().or(default_model_file.clone()) {
            fs::read_to_string(&path).map_err(|err| format!("failed to read `{path}`: {err}"))?
        } else {
            return Err(
                "provide model_file or source for DSL mode, or registry_binary+model_name for registry mode"
                    .to_string(),
            );
        };

        if source.trim().is_empty() {
            return Err("source must not be empty".to_string());
        }

        Ok(ResolvedTarget::Dsl {
            source_name: source_name
                .or(explicit_model_file)
                .or(default_model_file)
                .unwrap_or_else(|| "inline.valid".to_string()),
            source,
        })
    }

    fn registry_binary(&self, config: &ServerConfig) -> Result<String, String> {
        explicit_or_default(&self.registry_binary, &config.default_registry_binary)
            .ok_or_else(|| "registry_binary is required".to_string())
    }

    fn resolve_contract_target(&self, config: &ServerConfig) -> Result<ContractTarget, String> {
        let explicit_model_file = normalized_option(&self.model_file);
        let explicit_source = normalized_option(&self.source);
        let explicit_dsl = explicit_model_file.is_some() || explicit_source.is_some();
        let explicit_registry = normalized_option(&self.registry_binary).is_some();
        if explicit_dsl && explicit_registry {
            return Err(
                "provide either model_file/source or registry_binary for contract operations"
                    .to_string(),
            );
        }
        if explicit_dsl {
            let source = if let Some(source) = explicit_source {
                source
            } else {
                let path = explicit_model_file
                    .or_else(|| explicit_or_default(&self.model_file, &config.default_model_file))
                    .ok_or_else(|| "model_file is required".to_string())?;
                fs::read_to_string(&path)
                    .map_err(|err| format!("failed to read `{path}`: {err}"))?
            };
            if source.trim().is_empty() {
                return Err("source must not be empty".to_string());
            }
            return Ok(ContractTarget::Dsl { source });
        }
        if explicit_registry
            || explicit_or_default(&self.registry_binary, &config.default_registry_binary).is_some()
        {
            return Ok(ContractTarget::Registry {
                registry_binary: self.registry_binary(config)?,
                model_name: normalized_option(&self.model_name),
            });
        }
        if let Some(path) = explicit_or_default(&self.model_file, &config.default_model_file) {
            let source = fs::read_to_string(&path)
                .map_err(|err| format!("failed to read `{path}`: {err}"))?;
            if source.trim().is_empty() {
                return Err("source must not be empty".to_string());
            }
            return Ok(ContractTarget::Dsl { source });
        }
        Err(
            "provide model_file/source for DSL mode, or registry_binary for registry mode"
                .to_string(),
        )
    }
}

#[derive(Debug)]
struct ToolResult {
    structured_content: Value,
    text: String,
    is_error: bool,
}

impl ToolResult {
    fn success(structured_content: Value) -> Self {
        let text = default_text(&structured_content);
        Self {
            structured_content,
            text,
            is_error: false,
        }
    }

    fn success_with_text(structured_content: Value, text: String) -> Self {
        Self {
            structured_content,
            text,
            is_error: false,
        }
    }

    fn error(structured_content: Value) -> Self {
        let text = default_text(&structured_content);
        Self {
            structured_content,
            text,
            is_error: true,
        }
    }

    fn error_message(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            structured_content: json!({ "error": message.clone() }),
            text: message,
            is_error: true,
        }
    }

    fn into_value(self) -> Value {
        json!({
            "content": [{
                "type": "text",
                "text": self.text
            }],
            "structuredContent": self.structured_content,
            "isError": self.is_error
        })
    }
}

fn default_text(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

fn registry_tool_result(result: Result<(i32, Value), String>, success_codes: &[i32]) -> ToolResult {
    match result {
        Ok((code, value)) if success_codes.contains(&code) => ToolResult::success(value),
        Ok((_, value)) => ToolResult::error(value),
        Err(message) => ToolResult::error_message(message),
    }
}

fn logging_set_level(session: &mut SessionState, params: &Value) -> Result<Value, String> {
    let request = parse_args::<SetLevelParams>(params)?;
    let level = LogLevel::parse(&request.level)
        .ok_or_else(|| format!("unsupported logging level `{}`", request.level))?;
    session.log_level = level;
    Ok(json!({}))
}

fn maybe_log(
    writer: &mut impl Write,
    session: &SessionState,
    level: LogLevel,
    method: &str,
    data: Value,
) {
    if !session.client_initialized || level < session.log_level {
        return;
    }
    let payload = json!({
        "jsonrpc": "2.0",
        "method": "notifications/message",
        "params": {
            "level": level.as_str(),
            "logger": "valid-mcp",
            "data": {
                "method": method,
                "details": data
            }
        }
    });
    let _ = write_message(writer, &payload);
}

fn list_tools(params: &Value) -> Result<Value, String> {
    let request = parse_args::<PaginatedParams>(params)?;
    let tools = tool_definitions();
    let (items, next_cursor) = paginate(&tools, request.cursor.as_deref())?;
    Ok(json!({
        "tools": items,
        "nextCursor": next_cursor
    }))
}

fn list_resources(config: &ServerConfig, params: &Value) -> Result<Value, String> {
    let request = parse_args::<PaginatedParams>(params)?;
    let resources = build_resources(config);
    let (items, next_cursor) = paginate(&resources, request.cursor.as_deref())?;
    Ok(json!({
        "resources": items,
        "nextCursor": next_cursor
    }))
}

fn list_resource_templates(params: &Value) -> Result<Value, String> {
    let request = parse_args::<PaginatedParams>(params)?;
    let empty: Vec<Value> = Vec::new();
    let (items, next_cursor) = paginate(&empty, request.cursor.as_deref())?;
    Ok(json!({
        "resourceTemplates": items,
        "nextCursor": next_cursor
    }))
}

fn read_resource(config: &ServerConfig, params: &Value) -> Result<Value, String> {
    let request = parse_args::<ReadResourceParams>(params)?;
    let (mime_type, text) = resource_contents(config, &request.uri)?;
    Ok(json!({
        "contents": [{
            "uri": request.uri,
            "mimeType": mime_type,
            "text": text
        }]
    }))
}

fn list_prompts(params: &Value) -> Result<Value, String> {
    let request = parse_args::<PaginatedParams>(params)?;
    let prompts = prompts_catalog::PROMPTS
        .iter()
        .copied()
        .map(prompts_catalog::prompt_definition)
        .collect::<Vec<_>>();
    let (items, next_cursor) = paginate(&prompts, request.cursor.as_deref())?;
    Ok(json!({
        "prompts": items,
        "nextCursor": next_cursor
    }))
}

fn get_prompt(config: &ServerConfig, params: &Value) -> Result<Value, String> {
    let request = parse_args::<GetPromptParams>(params)?;
    let entry = prompts_catalog::prompt_entry(&request.name)
        .ok_or_else(|| format!("unknown prompt `{}`", request.name))?;
    let messages = prompt_messages(config, entry, &request.arguments)?;
    Ok(json!({
        "description": entry.description,
        "messages": messages
    }))
}

fn paginate(items: &[Value], cursor: Option<&str>) -> Result<(Vec<Value>, Option<String>), String> {
    let start = match cursor {
        Some(cursor) if !cursor.trim().is_empty() => parse_cursor(cursor)?,
        _ => 0,
    };
    if start > items.len() {
        return Err("cursor is out of bounds".to_string());
    }
    let end = usize::min(start + PAGE_SIZE, items.len());
    let next_cursor = (end < items.len()).then(|| end.to_string());
    Ok((items[start..end].to_vec(), next_cursor))
}

fn parse_cursor(cursor: &str) -> Result<usize, String> {
    cursor
        .parse::<usize>()
        .map_err(|_| format!("invalid cursor `{cursor}`"))
}

fn build_resources(config: &ServerConfig) -> Vec<Value> {
    let mut resources = Vec::new();
    for doc in docs_catalog::DOCS {
        resources.push(json!({
            "uri": format!("valid://docs/{}", doc.id),
            "name": doc.title,
            "description": doc.summary,
            "mimeType": "text/markdown"
        }));
    }
    for example in docs_catalog::EXAMPLES {
        resources.push(json!({
            "uri": format!("valid://examples/{}", example.id),
            "name": example.title,
            "description": example.summary,
            "mimeType": "text/x-rust"
        }));
    }
    if let Some(model_file) = normalized_option(&config.default_model_file) {
        if FsPath::new(&model_file).is_file() {
            resources.push(json!({
                "uri": "valid://targets/default-model-file",
                "name": "Default model file",
                "description": format!("Configured default model file: {model_file}"),
                "mimeType": mime_type_for_path(&model_file)
            }));
        }
    }
    if let Some(registry_binary) = normalized_option(&config.default_registry_binary) {
        if FsPath::new(&registry_binary).exists() {
            resources.push(json!({
                "uri": "valid://targets/default-registry-binary",
                "name": "Default registry binary",
                "description": format!("Configured default registry binary: {registry_binary}"),
                "mimeType": "application/json"
            }));
        }
    }
    resources
}

fn resource_contents(config: &ServerConfig, uri: &str) -> Result<(&'static str, String), String> {
    if let Some(doc_id) = uri.strip_prefix("valid://docs/") {
        let doc = docs_catalog::doc_entry(doc_id)
            .ok_or_else(|| format!("unknown doc resource `{uri}`"))?;
        return Ok(("text/markdown", doc.body_markdown.to_string()));
    }
    if let Some(example_id) = uri.strip_prefix("valid://examples/") {
        let example = docs_catalog::example_entry(example_id)
            .ok_or_else(|| format!("unknown example resource `{uri}`"))?;
        return Ok(("text/x-rust", example.source_text.to_string()));
    }
    if uri == "valid://targets/default-model-file" {
        let model_file = normalized_option(&config.default_model_file)
            .ok_or_else(|| "default model file is not configured".to_string())?;
        let text = fs::read_to_string(&model_file)
            .map_err(|err| format!("failed to read `{model_file}`: {err}"))?;
        let mime = mime_type_for_path(&model_file);
        return Ok((mime, text));
    }
    if uri == "valid://targets/default-registry-binary" {
        let registry_binary = normalized_option(&config.default_registry_binary)
            .ok_or_else(|| "default registry binary is not configured".to_string())?;
        let metadata = fs::metadata(&registry_binary)
            .map_err(|err| format!("failed to stat `{registry_binary}`: {err}"))?;
        let body = json!({
            "path": registry_binary,
            "is_file": metadata.is_file(),
            "len": metadata.len(),
            "executable_hint": true
        });
        return Ok(("application/json", default_text(&body)));
    }
    Err(format!("unknown resource `{uri}`"))
}

fn mime_type_for_path(path: &str) -> &'static str {
    if path.ends_with(".md") {
        "text/markdown"
    } else if path.ends_with(".rs") {
        "text/x-rust"
    } else if path.ends_with(".valid") {
        "text/plain"
    } else {
        "text/plain"
    }
}

fn prompt_messages(
    config: &ServerConfig,
    entry: prompts_catalog::PromptEntry,
    arguments: &Map<String, Value>,
) -> Result<Vec<Value>, String> {
    let guide = docs_catalog::doc_entry("ai-authoring-guide")
        .ok_or_else(|| "ai-authoring-guide is missing from docs catalog".to_string())?;
    let args = arguments
        .iter()
        .map(|(key, value)| format!("- {key}: {}", render_prompt_value(value)))
        .collect::<Vec<_>>()
        .join("\n");
    let target_hint = target_prompt_hint(config);
    let body = match entry.name {
        "clarify_requirement" => format!(
            "Clarify the requirement before writing or editing a valid model.\n\nProvided arguments:\n{}\n\nWorkflow:\n1. Read the AI authoring guide and modeling checklist.\n2. Ask only the minimum follow-up questions needed to pin down state, actions, success/failure paths, and out-of-scope behavior.\n3. Separate requirement ambiguity from modeling ambiguity.\n4. End with a compact modeling brief that names likely scenarios, predicates, properties, and verification mode.\n\n{}",
            blank_if_empty(&args),
            target_hint
        ),
        "author_model" => format!(
            "Author a new valid model for the following domain.\n\nProvided arguments:\n{}\n\nWorkflow:\n1. Read the AI authoring guide.\n2. Read one curated example close to the domain.\n3. Prefer declarative transitions unless explicit-first constraints force step.\n4. Use inspect and lint before verify.\n\n{}",
            blank_if_empty(&args),
            target_hint
        ),
        "review_model" => format!(
            "Review the target valid model for correctness, capability/readiness, and migration risks.\n\nProvided arguments:\n{}\n\nWorkflow:\n1. Read the AI authoring guide and common pitfalls.\n2. Inspect the model.\n3. Run lint/readiness and explain the highest-impact findings.\n4. Separate bugs from capability limitations.\n\n{}",
            blank_if_empty(&args),
            target_hint
        ),
        "migrate_step_to_transitions" => format!(
            "Migrate the target model from step-oriented behavior to declarative transitions where feasible.\n\nProvided arguments:\n{}\n\nWorkflow:\n1. Read the AI authoring guide and examples curriculum.\n2. Inspect the existing model and identify state/action boundaries.\n3. Preserve action ids and properties unless the request says otherwise.\n4. Re-run lint/readiness after migration.\n\n{}",
            blank_if_empty(&args),
            target_hint
        ),
        "explain_readiness_failure" => format!(
            "Explain the readiness or lint failure for the target model and propose the minimum next actions.\n\nProvided arguments:\n{}\n\nWorkflow:\n1. Read the AI authoring guide and language spec.\n2. Inspect or lint the model if the finding payload is incomplete.\n3. Classify each issue as syntax, capability, unsupported expression, or migration guidance.\n4. Recommend the next doc, tool, or rewrite.\n\n{}",
            blank_if_empty(&args),
            target_hint
        ),
        "triage_conformance_failure" => format!(
            "Triage a conformance mismatch between the accepted model and an implementation surface.\n\nProvided arguments:\n{}\n\nWorkflow:\n1. Read the AI authoring guide and modeling checklist.\n2. If the failure payload is partial, gather the conformance result and any linked explain/check output.\n3. Classify each mismatch as likely requirement drift, model bug, implementation bug, or observability gap.\n4. Recommend the next tool, rerun target, or repair surface with the minimum follow-up steps.\n\n{}",
            blank_if_empty(&args),
            target_hint
        ),
        _ => return Err(format!("unsupported prompt `{}`", entry.name)),
    };
    Ok(vec![
        json!({
            "role": "user",
            "content": {
                "type": "text",
                "text": body
            }
        }),
        json!({
            "role": "user",
            "content": {
                "type": "resource",
                "resource": {
                    "uri": "valid://docs/ai-authoring-guide",
                    "mimeType": "text/markdown",
                    "text": guide.body_markdown
                }
            }
        }),
    ])
}

fn render_prompt_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        _ => default_text(value),
    }
}

fn blank_if_empty(text: &str) -> String {
    if text.trim().is_empty() {
        "- none supplied".to_string()
    } else {
        text.to_string()
    }
}

fn target_prompt_hint(config: &ServerConfig) -> String {
    let mut lines = Vec::new();
    if let Some(model_file) = normalized_option(&config.default_model_file) {
        lines.push(format!("Default model_file available: `{model_file}`."));
    }
    if let Some(registry_binary) = normalized_option(&config.default_registry_binary) {
        lines.push(format!(
            "Default registry_binary available: `{registry_binary}`."
        ));
    }
    if lines.is_empty() {
        "No startup default model target is configured.".to_string()
    } else {
        lines.join("\n")
    }
}

fn docs_index_tool() -> Result<ToolResult, String> {
    Ok(ToolResult::success(json!({
        "canonical_entry": docs_catalog::docs_canonical_entry(),
        "docs": docs_catalog::docs_index()
    })))
}

fn docs_get_tool(args: &DocGetArgs) -> Result<ToolResult, String> {
    let Some(doc) = docs_catalog::doc_entry(&args.doc_id) else {
        return Ok(ToolResult::error_message(format!(
            "unknown doc_id `{}`",
            args.doc_id
        )));
    };

    let structured = json!({
        "doc_id": doc.id,
        "title": doc.title,
        "kind": doc.kind,
        "audience": doc.audience,
        "recommended_for": doc.recommended_for,
        "canonical_entry": doc.canonical_entry,
        "summary": doc.summary,
        "key_points": doc.key_points,
        "canonical_rules": doc.canonical_rules,
        "supported_features": doc.supported_features,
        "unsupported_features": doc.unsupported_features,
        "related_docs": doc.related_docs,
        "source_path": doc.source_path,
        "body_markdown": doc.body_markdown
    });
    let text = format!(
        "# {}\n\n{}\n\nSource: `{}`\n\n{}",
        doc.title, doc.summary, doc.source_path, doc.body_markdown
    );
    Ok(ToolResult::success_with_text(structured, text))
}

fn examples_list_tool() -> Result<ToolResult, String> {
    Ok(ToolResult::success(json!({
        "examples": docs_catalog::examples_index()
    })))
}

fn example_get_tool(args: &ExampleGetArgs) -> Result<ToolResult, String> {
    let Some(example) = docs_catalog::example_entry(&args.example_id) else {
        return Ok(ToolResult::error_message(format!(
            "unknown example_id `{}`",
            args.example_id
        )));
    };

    let structured = json!({
        "example_id": example.id,
        "title": example.title,
        "difficulty": example.difficulty,
        "concepts": example.concepts,
        "mode": example.mode,
        "backend_expectation": example.backend_expectation,
        "source_path": example.source_path,
        "recommended_order": example.recommended_order,
        "recommended_docs": example.recommended_docs,
        "focus_models": example.focus_models,
        "summary": example.summary,
        "commands": example.commands,
        "source_text": example.source_text
    });
    let text = format!(
        "# {}\n\n{}\n\nSource: `{}`\n\nRecommended commands:\n{}\n\n```rust\n{}\n```",
        example.title,
        example.summary,
        example.source_path,
        example
            .commands
            .iter()
            .map(|command| format!("- `{command}`"))
            .collect::<Vec<_>>()
            .join("\n"),
        example.source_text
    );
    Ok(ToolResult::success_with_text(structured, text))
}

fn inspect_tool(config: &ServerConfig, args: &BasicArgs) -> Result<ToolResult, String> {
    match args.target.resolve(config)? {
        ResolvedTarget::Dsl {
            source_name,
            source,
        } => {
            let request = InspectRequest {
                request_id: "mcp-inspect".to_string(),
                source_name,
                source,
            };
            match inspect_source(&request) {
                Ok(response) => Ok(ToolResult::success(parse_embedded_json(
                    "inspect response",
                    &render_inspect_json(&response),
                )?)),
                Err(diagnostics) => Ok(ToolResult::error(parse_embedded_json(
                    "diagnostics",
                    &render_diagnostics_json(&diagnostics),
                )?)),
            }
        }
        ResolvedTarget::Registry {
            registry_binary,
            model_name,
        } => Ok(registry_tool_result(
            run_registry_json(&registry_binary, &["inspect", &model_name, "--json"]),
            &[0],
        )),
    }
}

fn check_tool(config: &ServerConfig, args: &BackendArgs) -> Result<ToolResult, String> {
    match args.target.resolve(config)? {
        ResolvedTarget::Dsl {
            source_name,
            source,
        } => {
            let request = CheckRequest {
                request_id: "mcp-check".to_string(),
                source_name: source_name.clone(),
                source,
                property_id: args.property_id.clone(),
                scenario_id: args.scenario_id.clone(),
                backend: args.backend.clone(),
                solver_executable: args.solver_executable.clone(),
                solver_args: args.solver_args.clone(),
                seed: None,
            };
            let outcome = check_source(&request);
            let rendered = render_outcome_json(&source_name, &outcome);
            let value = parse_embedded_json("check outcome", &rendered)?;
            Ok(match outcome {
                CheckOutcome::Completed(_) => ToolResult::success(value),
                CheckOutcome::Errored(_) => ToolResult::error(value),
            })
        }
        ResolvedTarget::Registry {
            registry_binary,
            model_name,
        } => Ok(registry_tool_result(
            run_registry_json(
                &registry_binary,
                &registry_command_args("check", &model_name, args),
            ),
            &[0, 2, 4],
        )),
    }
}

fn explain_tool(config: &ServerConfig, args: &BackendArgs) -> Result<ToolResult, String> {
    match args.target.resolve(config)? {
        ResolvedTarget::Dsl {
            source_name,
            source,
        } => {
            let request = CheckRequest {
                request_id: "mcp-explain".to_string(),
                source_name,
                source,
                property_id: args.property_id.clone(),
                scenario_id: args.scenario_id.clone(),
                backend: args.backend.clone(),
                solver_executable: args.solver_executable.clone(),
                solver_args: args.solver_args.clone(),
                seed: None,
            };
            match explain_source(&request) {
                Ok(response) => Ok(ToolResult::success(parse_embedded_json(
                    "explain response",
                    &render_explain_json(&response),
                )?)),
                Err(error) => Ok(ToolResult::error(parse_embedded_json(
                    "diagnostics",
                    &render_diagnostics_json(&error.diagnostics),
                )?)),
            }
        }
        ResolvedTarget::Registry {
            registry_binary,
            model_name,
        } => Ok(registry_tool_result(
            run_registry_json(
                &registry_binary,
                &registry_command_args("explain", &model_name, args),
            ),
            &[0],
        )),
    }
}

fn coverage_tool(config: &ServerConfig, args: &BackendArgs) -> Result<ToolResult, String> {
    match args.target.resolve(config)? {
        ResolvedTarget::Dsl {
            source_name,
            source,
        } => {
            let model = match compile_source(&source) {
                Ok(model) => model,
                Err(diagnostics) => {
                    return Ok(ToolResult::error(parse_embedded_json(
                        "diagnostics",
                        &render_diagnostics_json(&diagnostics),
                    )?));
                }
            };
            let request = CheckRequest {
                request_id: "mcp-coverage".to_string(),
                source_name,
                source,
                property_id: args.property_id.clone(),
                scenario_id: args.scenario_id.clone(),
                backend: args.backend.clone(),
                solver_executable: args.solver_executable.clone(),
                solver_args: args.solver_args.clone(),
                seed: None,
            };
            match check_source(&request) {
                CheckOutcome::Completed(result) => {
                    let traces = result.trace.into_iter().collect::<Vec<_>>();
                    let report = collect_coverage(&model, &traces);
                    Ok(ToolResult::success(parse_embedded_json(
                        "coverage report",
                        &render_coverage_json(&report),
                    )?))
                }
                CheckOutcome::Errored(error) => Ok(ToolResult::error(parse_embedded_json(
                    "diagnostics",
                    &render_diagnostics_json(&error.diagnostics),
                )?)),
            }
        }
        ResolvedTarget::Registry {
            registry_binary,
            model_name,
        } => Ok(registry_tool_result(
            run_registry_json(
                &registry_binary,
                &registry_command_args("coverage", &model_name, args),
            ),
            &[0],
        )),
    }
}

fn testgen_tool(config: &ServerConfig, args: &TestgenArgs) -> Result<ToolResult, String> {
    match args.target.resolve(config)? {
        ResolvedTarget::Dsl {
            source_name,
            source,
        } => {
            let request = TestgenRequest {
                request_id: "mcp-testgen".to_string(),
                source_name,
                source,
                property_id: args.property_id.clone(),
                strategy: args
                    .strategy
                    .clone()
                    .unwrap_or_else(|| "counterexample".to_string()),
                backend: args.backend.clone(),
                solver_executable: args.solver_executable.clone(),
                solver_args: args.solver_args.clone(),
                seed: None,
            };
            match testgen_source(&request) {
                Ok(response) => Ok(ToolResult::success(json!({
                    "schema_version": response.schema_version,
                    "request_id": response.request_id,
                    "status": response.status,
                    "vector_ids": response.vector_ids,
                    "vectors": response.vectors.into_iter().map(|vector| json!({
                        "vector_id": vector.vector_id,
                        "strictness": vector.strictness,
                        "derivation": vector.derivation,
                        "source_kind": vector.source_kind,
                        "strategy": vector.strategy
                    })).collect::<Vec<_>>(),
                    "generated_files": response.generated_files
                }))),
                Err(error) => Ok(ToolResult::error(parse_embedded_json(
                    "diagnostics",
                    &render_diagnostics_json(&error.diagnostics),
                )?)),
            }
        }
        ResolvedTarget::Registry {
            registry_binary,
            model_name,
        } => {
            let mut command = registry_testgen_command_args(&model_name, args);
            if let Some(strategy) = &args.strategy {
                command.push(format!("--strategy={strategy}"));
            }
            Ok(registry_tool_result(
                run_registry_json(&registry_binary, &command),
                &[0],
            ))
        }
    }
}

fn replay_tool(config: &ServerConfig, args: &ReplayArgs) -> Result<ToolResult, String> {
    match args.target.resolve(config)? {
        ResolvedTarget::Dsl {
            source_name: _,
            source,
        } => {
            let model = match compile_model(&source) {
                Ok(model) => model,
                Err(diagnostics) => {
                    return Ok(ToolResult::error(parse_embedded_json(
                        "diagnostics",
                        &render_diagnostics_json(&diagnostics),
                    )?));
                }
            };
            let property_id = args
                .property_id
                .clone()
                .or_else(|| {
                    model
                        .properties
                        .first()
                        .map(|property| property.property_id.clone())
                })
                .unwrap_or_else(|| "P_SAFE".to_string());
            let terminal = match replay_actions(&model, &args.actions) {
                Ok(terminal) => terminal,
                Err(diagnostic) => {
                    return Ok(ToolResult::error(parse_embedded_json(
                        "diagnostics",
                        &render_diagnostics_json(&[diagnostic]),
                    )?));
                }
            };
            let focus_action_enabled = args.focus_action_id.as_deref().map(|action_id| {
                apply_action(&model, &terminal, action_id)
                    .ok()
                    .flatten()
                    .is_some()
            });
            let Some(property) = model
                .properties
                .iter()
                .find(|candidate| candidate.property_id == property_id)
            else {
                return Ok(ToolResult::error_message(format!(
                    "unknown property `{property_id}`"
                )));
            };
            let property_holds = matches!(
                eval_expr(&model, &terminal, &property.expr),
                Ok(crate::ir::Value::Bool(true))
            );
            let mut path_tags = BTreeSet::new();
            for action_id in &args.actions {
                for action in model
                    .actions
                    .iter()
                    .filter(|action| action.action_id == *action_id)
                {
                    for tag in &action.path_tags {
                        path_tags.insert(tag.clone());
                    }
                }
            }
            Ok(ToolResult::success(parse_embedded_json(
                "replay response",
                &render_replay_json(
                    &property_id,
                    &args.actions,
                    &terminal.as_named_map(&model),
                    args.focus_action_id.as_deref(),
                    focus_action_enabled,
                    Some(property_holds),
                    &Path::from_legacy_tags(path_tags.into_iter().collect()),
                ),
            )?))
        }
        ResolvedTarget::Registry {
            registry_binary,
            model_name,
        } => {
            let mut command = vec!["replay".to_string(), model_name, "--json".to_string()];
            if let Some(property_id) = &args.property_id {
                command.push(format!("--property={property_id}"));
            }
            if let Some(focus_action_id) = &args.focus_action_id {
                command.push(format!("--focus-action={focus_action_id}"));
            }
            if !args.actions.is_empty() {
                command.push(format!("--actions={}", args.actions.join(",")));
            }
            Ok(registry_tool_result(
                run_registry_json(&registry_binary, &command),
                &[0],
            ))
        }
    }
}

fn contract_snapshot_tool(
    config: &ServerConfig,
    args: &ContractSnapshotArgs,
) -> Result<ToolResult, String> {
    match args.target.resolve_contract_target(config)? {
        ContractTarget::Registry {
            registry_binary,
            model_name,
        } => {
            let result = run_registry_json(&registry_binary, &["contract", "snapshot", "--json"]);
            let tool = match result {
                Ok((code, value)) if code == 0 => {
                    if let Some(model_name) = model_name.as_deref() {
                        match select_named_entry(value, "snapshots", model_name, &registry_binary) {
                            Ok(filtered) => ToolResult::success(filtered),
                            Err(message) => ToolResult::error_message(message),
                        }
                    } else {
                        ToolResult::success(value)
                    }
                }
                Ok((_, value)) => ToolResult::error(value),
                Err(message) => ToolResult::error_message(message),
            };
            Ok(tool)
        }
        ContractTarget::Dsl { source } => {
            let model = match compile_source(&source) {
                Ok(model) => model,
                Err(diagnostics) => {
                    return Ok(ToolResult::error(parse_embedded_json(
                        "diagnostics",
                        &render_diagnostics_json(&diagnostics),
                    )?));
                }
            };
            let snapshot = snapshot_model(&model);
            Ok(ToolResult::success(json!({
                "schema_version": "1.0.0",
                "model_id": snapshot.model_id,
                "contract_hash": snapshot.contract_hash,
                "state_fields": snapshot.state_fields,
                "actions": snapshot.actions,
                "properties": snapshot.properties
            })))
        }
    }
}

fn contract_check_tool(
    config: &ServerConfig,
    args: &ContractCheckArgs,
) -> Result<ToolResult, String> {
    match args.target.resolve_contract_target(config)? {
        ContractTarget::Registry {
            registry_binary,
            model_name,
        } => {
            let result = run_registry_json(
                &registry_binary,
                &["contract", "check", &args.lock_file, "--json"],
            );
            let tool = match result {
                Ok((code, value)) if matches!(code, 0 | 2) => {
                    let value = augment_contract_check_value(config, value);
                    if let Some(model_name) = model_name.as_deref() {
                        match select_named_entry(value, "reports", model_name, &registry_binary) {
                            Ok(filtered) => ToolResult::success(filtered),
                            Err(message) => ToolResult::error_message(message),
                        }
                    } else {
                        ToolResult::success(value)
                    }
                }
                Ok((_, value)) => ToolResult::error(value),
                Err(message) => ToolResult::error_message(message),
            };
            Ok(tool)
        }
        ContractTarget::Dsl { source } => {
            let model = match compile_source(&source) {
                Ok(model) => model,
                Err(diagnostics) => {
                    return Ok(ToolResult::error(parse_embedded_json(
                        "diagnostics",
                        &render_diagnostics_json(&diagnostics),
                    )?));
                }
            };
            let snapshot = snapshot_model(&model);
            let lock_body = match fs::read_to_string(&args.lock_file) {
                Ok(lock_body) => lock_body,
                Err(err) => {
                    return Ok(ToolResult::error_message(format!(
                        "failed to read `{}`: {err}",
                        args.lock_file
                    )));
                }
            };
            let lock = match parse_lock_file(&lock_body) {
                Ok(lock) => lock,
                Err(err) => {
                    return Ok(ToolResult::error_message(format!(
                        "failed to parse `{}`: {err}",
                        args.lock_file
                    )));
                }
            };
            let report = if let Some(expected) = lock
                .entries
                .iter()
                .find(|entry| entry.model_id == snapshot.model_id)
            {
                let mut drift = compare_snapshot(expected, &snapshot);
                let recommendations = config
                    .project_config
                    .as_ref()
                    .map(|project_config| rerun_recommendations(project_config, &snapshot.model_id))
                    .unwrap_or_default();
                drift.affected_critical_properties = recommendations.affected_critical_properties;
                drift.affected_property_suites = recommendations.affected_property_suites;
                json!({
                    "schema_version": "1.0.0",
                    "status": drift.status,
                    "contract_id": snapshot.model_id,
                    "old_hash": expected.contract_hash,
                    "new_hash": snapshot.contract_hash,
                    "changes": drift.changes,
                    "affected_critical_properties": drift.affected_critical_properties,
                    "affected_property_suites": drift.affected_property_suites,
                    "lock_file": args.lock_file
                })
            } else {
                let recommendations = config
                    .project_config
                    .as_ref()
                    .map(|project_config| rerun_recommendations(project_config, &snapshot.model_id))
                    .unwrap_or_default();
                json!({
                    "schema_version": "1.0.0",
                    "status": "missing",
                    "contract_id": snapshot.model_id,
                    "old_hash": Value::Null,
                    "new_hash": snapshot.contract_hash,
                    "changes": ["missing_from_lock_file"],
                    "affected_critical_properties": recommendations.affected_critical_properties,
                    "affected_property_suites": recommendations.affected_property_suites,
                    "lock_file": args.lock_file
                })
            };
            Ok(ToolResult::success(report))
        }
    }
}

fn list_models_tool(config: &ServerConfig, args: &ListModelsArgs) -> Result<ToolResult, String> {
    let registry_binary =
        explicit_or_default(&args.registry_binary, &config.default_registry_binary);
    if let Some(registry_binary) = registry_binary {
        let mut result = registry_tool_result(
            run_registry_json(&registry_binary, &["list", "--json"]),
            &[0],
        );
        if !result.is_error {
            result.structured_content =
                augment_list_models_value(config, result.structured_content.clone());
            result.text = default_text(&result.structured_content);
        }
        return Ok(result);
    }

    Ok(ToolResult::success(augment_list_models_value(
        config,
        json!({
            "models": list_bundled_models()
        }),
    )))
}

fn suite_run_tool(config: &ServerConfig, args: &SuiteRunArgs) -> Result<ToolResult, String> {
    let mode = if args.critical {
        "critical"
    } else if args.suite_name.is_some() {
        "named_suite"
    } else {
        "all"
    };
    if args.critical && args.suite_name.is_some() {
        return Ok(ToolResult::error_message(
            "use either `critical` or `suite_name`, not both".to_string(),
        ));
    }
    let project_config = match &config.project_config {
        Some(project_config) => project_config,
        None if mode == "all" => {
            return Ok(ToolResult::error_message(
                "valid_suite_run requires project config for suite selection".to_string(),
            ));
        }
        None => {
            return Ok(ToolResult::error_message(
                "valid_suite_run requires valid.toml project config".to_string(),
            ));
        }
    };
    let default_registry_binary = explicit_or_default(
        &args.target.registry_binary,
        &config.default_registry_binary,
    );
    if let Some(registry_binary) = default_registry_binary {
        let catalog = registry_model_property_catalog(&registry_binary)?;
        let runs = select_suite_runs(
            project_config,
            &catalog,
            args.critical,
            args.suite_name.as_deref(),
        )?;
        let mut outputs = Vec::new();
        for run in runs {
            let mut command = vec![
                "check".to_string(),
                run.model_id.clone(),
                "--json".to_string(),
            ];
            if let Some(property_id) = &run.property_id {
                command.push(format!("--property={property_id}"));
            }
            if let Some(backend) = &args.backend {
                command.push(format!("--backend={backend}"));
            }
            if let Some(solver_executable) = &args.solver_executable {
                command.push("--solver-exec".to_string());
                command.push(solver_executable.clone());
            }
            for solver_arg in &args.solver_args {
                command.push("--solver-arg".to_string());
                command.push(solver_arg.clone());
            }
            let (code, mut value) = run_registry_json(
                &registry_binary,
                &command.iter().map(String::as_str).collect::<Vec<_>>(),
            )?;
            if let Some(object) = value.as_object_mut() {
                object.insert("model_id".to_string(), Value::String(run.model_id));
                object.insert(
                    "property_id".to_string(),
                    run.property_id.map(Value::String).unwrap_or(Value::Null),
                );
                object.insert("exit_code".to_string(), Value::from(code));
            }
            outputs.push(value);
        }
        Ok(ToolResult::success(json!({
            "selection_mode": mode,
            "suite_name": args.suite_name,
            "runs": outputs
        })))
    } else {
        match args.target.resolve(config)? {
            ResolvedTarget::Registry { .. } => unreachable!("registry handled above"),
            ResolvedTarget::Dsl {
                source_name,
                source,
            } => {
                let model = match compile_source(&source) {
                    Ok(model) => model,
                    Err(diagnostics) => {
                        return Ok(ToolResult::error(parse_embedded_json(
                            "diagnostics",
                            &render_diagnostics_json(&diagnostics),
                        )?));
                    }
                };
                let catalog = std::collections::BTreeMap::from([(
                    model.model_id.clone(),
                    model
                        .properties
                        .iter()
                        .map(|property| property.property_id.clone())
                        .collect::<Vec<_>>(),
                )]);
                let runs = select_suite_runs(
                    project_config,
                    &catalog,
                    args.critical,
                    args.suite_name.as_deref(),
                )?;
                let mut outputs = Vec::new();
                for run in runs {
                    if run.model_id != model.model_id {
                        return Ok(ToolResult::error_message(format!(
                            "suite references model `{}` but DSL target is `{}`",
                            run.model_id, model.model_id
                        )));
                    }
                    let request = CheckRequest {
                        request_id: "mcp-suite-run".to_string(),
                        source_name: source_name.clone(),
                        source: source.clone(),
                        property_id: run.property_id.clone(),
                        scenario_id: None,
                        seed: None,
                        backend: args.backend.clone(),
                        solver_executable: args.solver_executable.clone(),
                        solver_args: args.solver_args.clone(),
                    };
                    let outcome = check_source(&request);
                    let mut value = parse_embedded_json(
                        "check outcome",
                        &render_outcome_json(&model.model_id, &outcome),
                    )?;
                    if let Some(object) = value.as_object_mut() {
                        object.insert("model_id".to_string(), Value::String(run.model_id));
                        object.insert(
                            "property_id".to_string(),
                            run.property_id.map(Value::String).unwrap_or(Value::Null),
                        );
                    }
                    outputs.push(value);
                }
                Ok(ToolResult::success(json!({
                    "selection_mode": mode,
                    "suite_name": args.suite_name,
                    "runs": outputs
                })))
            }
        }
    }
}

fn augment_list_models_value(config: &ServerConfig, mut value: Value) -> Value {
    if let Some(project_config) = &config.project_config {
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "critical_properties".to_string(),
                json!(project_config.critical_properties),
            );
            object.insert(
                "property_suites".to_string(),
                json!(project_config.property_suites),
            );
        }
    }
    value
}

fn augment_contract_check_value(config: &ServerConfig, mut value: Value) -> Value {
    let Some(project_config) = &config.project_config else {
        return value;
    };
    if let Some(reports) = value.get_mut("reports").and_then(Value::as_array_mut) {
        for report in reports {
            if let Some(object) = report.as_object_mut() {
                if let Some(contract_id) = object.get("contract_id").and_then(Value::as_str) {
                    let recommendations = rerun_recommendations(project_config, contract_id);
                    object.insert(
                        "affected_critical_properties".to_string(),
                        json!(recommendations.affected_critical_properties),
                    );
                    object.insert(
                        "affected_property_suites".to_string(),
                        json!(recommendations.affected_property_suites),
                    );
                }
            }
        }
    } else if let Some(object) = value.as_object_mut() {
        if let Some(contract_id) = object.get("contract_id").and_then(Value::as_str) {
            let recommendations = rerun_recommendations(project_config, contract_id);
            object.insert(
                "affected_critical_properties".to_string(),
                json!(recommendations.affected_critical_properties),
            );
            object.insert(
                "affected_property_suites".to_string(),
                json!(recommendations.affected_property_suites),
            );
        }
    }
    value
}

#[derive(Debug, Clone)]
struct McpSuiteRun {
    model_id: String,
    property_id: Option<String>,
}

fn select_suite_runs(
    project_config: &ProjectConfig,
    model_catalog: &std::collections::BTreeMap<String, Vec<String>>,
    critical: bool,
    suite_name: Option<&str>,
) -> Result<Vec<McpSuiteRun>, String> {
    if critical {
        if project_config.critical_properties.is_empty() {
            return Err("valid.toml does not declare critical_properties".to_string());
        }
        return expand_mcp_property_targets(
            model_catalog,
            project_config
                .critical_properties
                .iter()
                .flat_map(|(model, properties)| {
                    properties
                        .iter()
                        .cloned()
                        .map(|property| (model.clone(), property))
                })
                .collect(),
        );
    }
    if let Some(suite_name) = suite_name {
        let entries = project_config
            .property_suites
            .get(suite_name)
            .ok_or_else(|| format!("unknown property suite `{suite_name}`"))?;
        return expand_mcp_property_targets(
            model_catalog,
            entries
                .iter()
                .flat_map(|entry| {
                    entry
                        .properties
                        .iter()
                        .cloned()
                        .map(|property| (entry.model.clone(), property))
                })
                .collect(),
        );
    }
    Err("valid_suite_run requires `critical=true` or `suite_name`".to_string())
}

fn expand_mcp_property_targets(
    model_catalog: &std::collections::BTreeMap<String, Vec<String>>,
    requested: Vec<(String, String)>,
) -> Result<Vec<McpSuiteRun>, String> {
    let mut seen = BTreeSet::new();
    let mut runs = Vec::new();
    for (model_id, property_id) in requested {
        let properties = model_catalog
            .get(&model_id)
            .ok_or_else(|| format!("unknown model `{model_id}` in valid.toml"))?;
        if property_id.trim().is_empty() {
            return Err(format!(
                "empty property id configured for model `{model_id}`"
            ));
        }
        if !properties.contains(&property_id) {
            return Err(format!(
                "unknown property `{property_id}` configured for model `{model_id}`"
            ));
        }
        if seen.insert((model_id.clone(), property_id.clone())) {
            runs.push(McpSuiteRun {
                model_id,
                property_id: Some(property_id),
            });
        }
    }
    Ok(runs)
}

fn registry_model_property_catalog(
    registry_binary: &str,
) -> Result<std::collections::BTreeMap<String, Vec<String>>, String> {
    let (_, value) = run_registry_json(registry_binary, &["list", "--json"])?;
    let models = value["models"]
        .as_array()
        .ok_or_else(|| "registry list output missing models".to_string())?;
    let mut catalog = std::collections::BTreeMap::new();
    for model_name in models.iter().filter_map(Value::as_str) {
        let (_, inspect) = run_registry_json(registry_binary, &["inspect", model_name, "--json"])?;
        let properties = inspect["properties"]
            .as_array()
            .ok_or_else(|| {
                format!("registry inspect output missing properties for `{model_name}`")
            })?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>();
        catalog.insert(model_name.to_string(), properties);
    }
    Ok(catalog)
}

fn normalized_option(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn explicit_or_default(explicit: &Option<String>, default: &Option<String>) -> Option<String> {
    if explicit.is_some() {
        normalized_option(explicit)
    } else {
        normalized_option(default)
    }
}

fn graph_tool(config: &ServerConfig, args: &GraphArgs) -> Result<ToolResult, String> {
    let format = args.format.as_deref().unwrap_or("mermaid");
    let view = GraphView::parse(args.view.as_deref());
    match args.target.resolve(config)? {
        ResolvedTarget::Dsl {
            source_name,
            source,
        } => {
            let request = InspectRequest {
                request_id: "mcp-graph".to_string(),
                source_name,
                source,
            };
            match inspect_source(&request) {
                Ok(response) => graph_tool_result_for_dsl(&response, &request, args, format, view),
                Err(diagnostics) => Ok(ToolResult::error(parse_embedded_json(
                    "diagnostics",
                    &render_diagnostics_json(&diagnostics),
                )?)),
            }
        }
        ResolvedTarget::Registry {
            registry_binary,
            model_name,
        } => {
            if format == "json" {
                let mut command_args = vec![
                    "graph".to_string(),
                    model_name.clone(),
                    "--format=json".to_string(),
                    format!("--view={}", view_name(view)),
                ];
                if let Some(property_id) = &args.property_id {
                    command_args.push(format!("--property={property_id}"));
                }
                return Ok(registry_tool_result(
                    run_registry_json(&registry_binary, &command_args),
                    &[0],
                ));
            }
            let mut command = Command::new(&registry_binary);
            command
                .arg("graph")
                .arg(&model_name)
                .arg(format!("--format={format}"))
                .arg(format!("--view={}", view_name(view)));
            if let Some(property_id) = &args.property_id {
                command.arg(format!("--property={property_id}"));
            }
            let output = match command.output() {
                Ok(output) => output,
                Err(err) => {
                    return Ok(ToolResult::error_message(format!(
                        "failed to execute `{registry_binary}`: {err}"
                    )));
                }
            };
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if output.status.success() {
                Ok(ToolResult::success_with_text(
                    json!({
                        "format": format,
                        "view": view_name(view),
                        "graph": stdout
                    }),
                    stdout,
                ))
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                Ok(ToolResult::error_message(if stderr.is_empty() {
                    stdout
                } else {
                    stderr
                }))
            }
        }
    }
}

fn graph_tool_result_for_dsl(
    response: &crate::api::InspectResponse,
    request: &InspectRequest,
    args: &GraphArgs,
    format: &str,
    view: GraphView,
) -> Result<ToolResult, String> {
    if view != GraphView::Failure {
        return Ok(match format {
            "json" => ToolResult::success(parse_embedded_json(
                "inspect response",
                &render_inspect_json(response),
            )?),
            "text" => ToolResult::success_with_text(
                json!({
                    "format": "text",
                    "view": view_name(view),
                    "graph": crate::api::render_inspect_text(response)
                }),
                crate::api::render_inspect_text(response),
            ),
            "dot" => {
                let graph = render_model_dot_with_view(response, view);
                ToolResult::success_with_text(
                    json!({ "format": "dot", "view": view_name(view), "graph": graph }),
                    render_model_dot_with_view(response, view),
                )
            }
            "svg" => {
                let graph = render_model_svg_with_view(response, view);
                ToolResult::success_with_text(
                    json!({ "format": "svg", "view": view_name(view), "graph": graph }),
                    render_model_svg_with_view(response, view),
                )
            }
            _ => {
                let graph = render_model_mermaid_with_view(response, view);
                ToolResult::success_with_text(
                    json!({ "format": "mermaid", "view": view_name(view), "graph": graph }),
                    render_model_mermaid_with_view(response, view),
                )
            }
        });
    }

    let property_id = args
        .property_id
        .as_ref()
        .ok_or_else(|| "failure graph view requires property_id".to_string())?;
    let outcome = check_source(&CheckRequest {
        request_id: "mcp-graph-failure".to_string(),
        source_name: request.source_name.clone(),
        source: request.source.clone(),
        property_id: Some(property_id.clone()),
        scenario_id: None,
        seed: None,
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
    });
    let result = match outcome {
        CheckOutcome::Completed(result) => result,
        CheckOutcome::Errored(error) => {
            return Ok(ToolResult::error(parse_embedded_json(
                "diagnostics",
                &render_diagnostics_json(&error.diagnostics),
            )?))
        }
    };
    let trace = result
        .trace
        .as_ref()
        .ok_or_else(|| format!("property `{property_id}` did not produce evidence trace"))?;
    let slice = build_failure_graph_slice(response, trace, property_id)?;

    Ok(match format {
        "json" => {
            let mut body = parse_embedded_json("inspect response", &render_inspect_json(response))?;
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
            ToolResult::success(body)
        }
        "text" => ToolResult::success_with_text(
            json!({"format":"text","view":"failure","graph": render_model_text_failure(response, &slice)}),
            render_model_text_failure(response, &slice),
        ),
        "dot" => {
            let graph = render_model_dot_failure(response, &slice);
            ToolResult::success_with_text(
                json!({"format":"dot","view":"failure","graph": graph}),
                render_model_dot_failure(response, &slice),
            )
        }
        "svg" => {
            let graph = render_model_svg_failure(response, &slice);
            ToolResult::success_with_text(
                json!({"format":"svg","view":"failure","graph": graph}),
                render_model_svg_failure(response, &slice),
            )
        }
        _ => {
            let graph = render_model_mermaid_failure(response, &slice);
            ToolResult::success_with_text(
                json!({"format":"mermaid","view":"failure","graph": graph}),
                render_model_mermaid_failure(response, &slice),
            )
        }
    })
}

fn lint_tool(config: &ServerConfig, args: &BasicArgs) -> Result<ToolResult, String> {
    match args.target.resolve(config)? {
        ResolvedTarget::Dsl {
            source_name,
            source,
        } => {
            let request = InspectRequest {
                request_id: "mcp-lint".to_string(),
                source_name,
                source,
            };
            match lint_source(&request) {
                Ok(response) => Ok(ToolResult::success(parse_embedded_json(
                    "lint response",
                    &render_lint_json(&response),
                )?)),
                Err(diagnostics) => Ok(ToolResult::error(parse_embedded_json(
                    "diagnostics",
                    &render_diagnostics_json(&diagnostics),
                )?)),
            }
        }
        ResolvedTarget::Registry {
            registry_binary,
            model_name,
        } => Ok(registry_tool_result(
            run_registry_json(&registry_binary, &["lint", &model_name, "--json"]),
            &[0, 2],
        )),
    }
}

fn registry_command_args(command: &str, model_name: &str, args: &BackendArgs) -> Vec<String> {
    let mut command_args = vec![
        command.to_string(),
        model_name.to_string(),
        "--json".to_string(),
    ];
    if let Some(property_id) = &args.property_id {
        command_args.push(format!("--property={property_id}"));
    }
    if let Some(backend) = &args.backend {
        command_args.push(format!("--backend={backend}"));
    }
    if let Some(solver_executable) = &args.solver_executable {
        command_args.push("--solver-exec".to_string());
        command_args.push(solver_executable.clone());
    }
    for solver_arg in &args.solver_args {
        command_args.push("--solver-arg".to_string());
        command_args.push(solver_arg.clone());
    }
    command_args
}

fn registry_testgen_command_args(model_name: &str, args: &TestgenArgs) -> Vec<String> {
    let mut command_args = vec![
        "testgen".to_string(),
        model_name.to_string(),
        "--json".to_string(),
    ];
    if let Some(property_id) = &args.property_id {
        command_args.push(format!("--property={property_id}"));
    }
    if let Some(backend) = &args.backend {
        command_args.push(format!("--backend={backend}"));
    }
    if let Some(solver_executable) = &args.solver_executable {
        command_args.push("--solver-exec".to_string());
        command_args.push(solver_executable.clone());
    }
    for solver_arg in &args.solver_args {
        command_args.push("--solver-arg".to_string());
        command_args.push(solver_arg.clone());
    }
    command_args
}

fn run_registry_json(
    registry_binary: &str,
    args: &[impl AsRef<str>],
) -> Result<(i32, Value), String> {
    let mut command = Command::new(registry_binary);
    if let Ok(manifest_path) = std::env::var("VALID_MCP_MANIFEST_PATH") {
        command.env("VALID_REGISTRY_MANIFEST_PATH", manifest_path);
    }
    for arg in args {
        command.arg(arg.as_ref());
    }
    let output = command
        .output()
        .map_err(|err| format!("failed to execute `{registry_binary}`: {err}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("`{registry_binary}` returned no JSON output")
        } else {
            stderr
        });
    }
    let value = serde_json::from_str(&stdout)
        .map_err(|err| format!("failed to parse JSON from `{registry_binary}`: {err}: {stdout}"))?;
    Ok((output.status.code().unwrap_or(1), value))
}

fn parse_embedded_json(label: &str, body: &str) -> Result<Value, String> {
    serde_json::from_str(body).map_err(|err| format!("failed to parse {label}: {err}"))
}

fn select_named_entry(
    value: Value,
    field: &str,
    model_name: &str,
    registry_binary: &str,
) -> Result<Value, String> {
    let Some(entries) = value.get(field).and_then(Value::as_array) else {
        return Err(format!("registry response did not contain `{field}`"));
    };

    if let Some(entry) = entries
        .iter()
        .find(|entry| entry.get("model_id").and_then(Value::as_str) == Some(model_name))
    {
        return Ok(entry.clone());
    }

    if let Some(entry) = entries
        .iter()
        .find(|entry| entry.get("contract_id").and_then(Value::as_str) == Some(model_name))
    {
        return Ok(entry.clone());
    }

    let model_id = registry_model_id(registry_binary, model_name)?;
    entries
        .iter()
        .find(|entry| {
            entry.get("model_id").and_then(Value::as_str) == Some(model_id.as_str())
                || entry.get("contract_id").and_then(Value::as_str) == Some(model_id.as_str())
        })
        .cloned()
        .ok_or_else(|| format!("no entry found for model `{model_name}`"))
}

fn registry_model_id(registry_binary: &str, model_name: &str) -> Result<String, String> {
    let (code, value) = run_registry_json(registry_binary, &["inspect", model_name, "--json"])?;
    if code != 0 {
        return Err(format!("failed to inspect model `{model_name}`"));
    }
    value
        .get("model_id")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("inspect output did not contain model_id for `{model_name}`"))
}

fn view_name(view: GraphView) -> &'static str {
    match view {
        GraphView::Overview => "overview",
        GraphView::Logic => "logic",
        GraphView::Failure => "failure",
    }
}
