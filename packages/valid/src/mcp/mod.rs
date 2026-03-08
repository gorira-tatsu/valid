use std::{
    collections::BTreeSet,
    fs,
    io::{self, BufRead, Write},
    process::Command,
};

use serde::Deserialize;
use serde_json::{json, Value};

mod docs_catalog;

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
    reporter::{
        render_model_dot_with_view, render_model_mermaid_with_view, render_model_svg_with_view,
        GraphView,
    },
    testgen::render_replay_json,
};

const SUPPORTED_PROTOCOL_VERSIONS: &[&str] =
    &["2025-11-05", "2025-06-18", "2025-03-26", "2024-11-05"];

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub server_name: String,
    pub default_model_file: Option<String>,
    pub default_registry_binary: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            server_name: "valid".to_string(),
            default_model_file: std::env::var("VALID_MCP_MODEL_FILE").ok(),
            default_registry_binary: std::env::var("VALID_MCP_REGISTRY_BINARY").ok(),
        }
    }
}

pub fn serve_stdio(config: ServerConfig) -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut writer = stdout.lock();

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
                .filter_map(|message| handle_message(message, &config))
                .collect::<Vec<_>>();
            if responses.is_empty() {
                None
            } else {
                Some(Value::Array(responses))
            }
        } else {
            handle_message(&incoming, &config)
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

fn handle_message(message: &Value, config: &ServerConfig) -> Option<Value> {
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
        handle_notification(method);
        return None;
    }

    let id = id.unwrap_or(Value::Null);
    Some(match method {
        "initialize" => response(id, initialize_result(config, &params)),
        "ping" => response(id, json!({})),
        "tools/list" => response(id, json!({ "tools": tool_definitions() })),
        "tools/call" => match handle_tool_call(config, &params) {
            Ok(result) => response(id, result.into_value()),
            Err(error) => error_response(id, -32602, &error),
        },
        _ => error_response(id, -32601, &format!("method `{method}` is not supported")),
    })
}

fn handle_notification(method: &str) {
    if matches!(
        method,
        "notifications/initialized" | "notifications/cancelled"
    ) {
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
            "tools": {}
        },
        "serverInfo": {
            "name": config.server_name,
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": "Use valid_docs_index then valid_docs_get for guidance. Use model_file or source for .valid files, or registry_binary plus model_name for Rust registry mode."
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
            true,
        ),
        tool(
            "valid_docs_get",
            "Fetch a documentation entry by stable doc id with structured guidance and markdown body.",
            input_schema_with_doc_id(),
            true,
        ),
        tool(
            "valid_examples_list",
            "List curated learning examples with concepts, mode, and recommended order.",
            input_schema_empty(),
            true,
        ),
        tool(
            "valid_example_get",
            "Fetch a curated example by stable example id with commands, concepts, and source text.",
            input_schema_with_example_id(),
            true,
        ),
        tool(
            "valid_inspect",
            "Inspect a valid model and return state fields, actions, properties, and capabilities.",
            input_schema_with_backend(),
            true,
        ),
        tool(
            "valid_check",
            "Run property verification and return PASS, FAIL, or UNKNOWN with evidence details.",
            input_schema_with_backend_and_property(),
            true,
        ),
        tool(
            "valid_explain",
            "Explain a counterexample and return likely causes, hints, and involved fields.",
            input_schema_with_backend_and_property(),
            true,
        ),
        tool(
            "valid_coverage",
            "Compute transition and guard coverage from the current verification trace.",
            input_schema_with_backend_and_property(),
            true,
        ),
        tool(
            "valid_testgen",
            "Generate regression or witness vectors for a model.",
            input_schema_with_testgen(),
            false,
        ),
        tool(
            "valid_replay",
            "Replay an action sequence and report the terminal state and property result.",
            input_schema_with_replay(),
            true,
        ),
        tool(
            "valid_contract_snapshot",
            "Return the current contract hash for a model or registry.",
            input_schema_with_contract_snapshot(),
            true,
        ),
        tool(
            "valid_contract_check",
            "Compare the current contract against a lock file and report drift.",
            input_schema_with_contract_check(),
            true,
        ),
        tool(
            "valid_list_models",
            "List bundled models or models exported by a registry binary.",
            input_schema_list_models(),
            true,
        ),
        tool(
            "valid_graph",
            "Render a model graph as Mermaid, DOT, SVG, text, or JSON.",
            input_schema_with_graph(),
            true,
        ),
        tool(
            "valid_lint",
            "Run static analysis and capability lint checks on a model.",
            input_schema_basic(),
            true,
        ),
    ]
}

fn tool(name: &str, description: &str, input_schema: Value, read_only: bool) -> Value {
    json!({
        "name": name,
        "title": name,
        "description": description,
        "inputSchema": input_schema,
        "annotations": {
            "readOnlyHint": read_only,
            "destructiveHint": false,
            "idempotentHint": read_only,
            "openWorldHint": false
        }
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
            "enum": ["overview", "logic"]
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

fn common_target_properties() -> serde_json::Map<String, Value> {
    let mut properties = serde_json::Map::new();
    properties.insert("model_file".to_string(), json!({ "type": "string" }));
    properties.insert("source_name".to_string(), json!({ "type": "string" }));
    properties.insert("source".to_string(), json!({ "type": "string" }));
    properties.insert("registry_binary".to_string(), json!({ "type": "string" }));
    properties.insert("model_name".to_string(), json!({ "type": "string" }));
    properties
}

fn handle_tool_call(config: &ServerConfig, params: &Value) -> Result<ToolResult, String> {
    let call: ToolCallParams = serde_json::from_value(params.clone())
        .map_err(|err| format!("invalid tool call: {err}"))?;
    let arguments = call.arguments.unwrap_or_else(|| json!({}));

    match call.name.as_str() {
        "valid_docs_index" => docs_index_tool(),
        "valid_docs_get" => {
            let args = parse_args::<DocGetArgs>(&arguments)?;
            docs_get_tool(&args)
        }
        "valid_examples_list" => examples_list_tool(),
        "valid_example_get" => {
            let args = parse_args::<ExampleGetArgs>(&arguments)?;
            example_get_tool(&args)
        }
        "valid_inspect" => {
            let args = parse_args::<BasicArgs>(&arguments)?;
            inspect_tool(config, &args)
        }
        "valid_check" => {
            let args = parse_args::<BackendArgs>(&arguments)?;
            check_tool(config, &args)
        }
        "valid_explain" => {
            let args = parse_args::<BackendArgs>(&arguments)?;
            explain_tool(config, &args)
        }
        "valid_coverage" => {
            let args = parse_args::<BackendArgs>(&arguments)?;
            coverage_tool(config, &args)
        }
        "valid_testgen" => {
            let args = parse_args::<TestgenArgs>(&arguments)?;
            testgen_tool(config, &args)
        }
        "valid_replay" => {
            let args = parse_args::<ReplayArgs>(&arguments)?;
            replay_tool(config, &args)
        }
        "valid_contract_snapshot" => {
            let args = parse_args::<ContractSnapshotArgs>(&arguments)?;
            contract_snapshot_tool(config, &args)
        }
        "valid_contract_check" => {
            let args = parse_args::<ContractCheckArgs>(&arguments)?;
            contract_check_tool(config, &args)
        }
        "valid_list_models" => {
            let args = parse_args::<ListModelsArgs>(&arguments)?;
            list_models_tool(config, &args)
        }
        "valid_graph" => {
            let args = parse_args::<GraphArgs>(&arguments)?;
            graph_tool(config, &args)
        }
        "valid_lint" => {
            let args = parse_args::<BasicArgs>(&arguments)?;
            lint_tool(config, &args)
        }
        other => Err(format!("unknown tool `{other}`")),
    }
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
struct GraphArgs {
    #[serde(flatten)]
    target: TargetArgs,
    format: Option<String>,
    view: Option<String>,
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
                let drift = compare_snapshot(expected, &snapshot);
                json!({
                    "schema_version": "1.0.0",
                    "status": drift.status,
                    "contract_id": snapshot.model_id,
                    "old_hash": expected.contract_hash,
                    "new_hash": snapshot.contract_hash,
                    "changes": drift.changes,
                    "lock_file": args.lock_file
                })
            } else {
                json!({
                    "schema_version": "1.0.0",
                    "status": "missing",
                    "contract_id": snapshot.model_id,
                    "old_hash": Value::Null,
                    "new_hash": snapshot.contract_hash,
                    "changes": ["missing_from_lock_file"],
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
        return Ok(registry_tool_result(
            run_registry_json(&registry_binary, &["list", "--json"]),
            &[0],
        ));
    }

    Ok(ToolResult::success(json!({
        "models": list_bundled_models()
    })))
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
                Ok(response) => match format {
                    "json" => Ok(ToolResult::success(parse_embedded_json(
                        "inspect response",
                        &render_inspect_json(&response),
                    )?)),
                    "text" => Ok(ToolResult::success_with_text(
                        json!({
                            "format": "text",
                            "view": view_name(view),
                            "graph": crate::api::render_inspect_text(&response)
                        }),
                        crate::api::render_inspect_text(&response),
                    )),
                    "dot" => {
                        let graph = render_model_dot_with_view(&response, view);
                        Ok(ToolResult::success_with_text(
                            json!({ "format": "dot", "view": view_name(view), "graph": graph }),
                            render_model_dot_with_view(&response, view),
                        ))
                    }
                    "svg" => {
                        let graph = render_model_svg_with_view(&response, view);
                        Ok(ToolResult::success_with_text(
                            json!({ "format": "svg", "view": view_name(view), "graph": graph }),
                            render_model_svg_with_view(&response, view),
                        ))
                    }
                    _ => {
                        let graph = render_model_mermaid_with_view(&response, view);
                        Ok(ToolResult::success_with_text(
                            json!({ "format": "mermaid", "view": view_name(view), "graph": graph }),
                            render_model_mermaid_with_view(&response, view),
                        ))
                    }
                },
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
                return Ok(registry_tool_result(
                    run_registry_json(
                        &registry_binary,
                        &[
                            "graph",
                            &model_name,
                            "--format=json",
                            &format!("--view={}", view_name(view)),
                        ],
                    ),
                    &[0],
                ));
            }
            let mut command = Command::new(&registry_binary);
            command
                .arg("graph")
                .arg(&model_name)
                .arg(format!("--format={format}"))
                .arg(format!("--view={}", view_name(view)));
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
    }
}
