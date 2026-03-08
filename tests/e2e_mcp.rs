use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> PathBuf {
    manifest_dir()
        .join("tests")
        .join("fixtures")
        .join("models")
        .join(name)
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be monotonic enough for tests")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("valid-{prefix}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).expect("temp dir should be created");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpClient {
    fn spawn(args: &[&str], cwd: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_valid-mcp"))
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("valid-mcp should start");
        let stdin = child.stdin.take().expect("stdin should be piped");
        let stdout = child.stdout.take().expect("stdout should be piped");
        Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        }
    }

    fn initialize(&mut self) -> Value {
        let result = self.request(
            "initialize",
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": { "name": "valid-test", "version": "0.1.0" }
            }),
        );
        self.notify("notifications/initialized", json!({}));
        result
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }));
        loop {
            let response = self.read_message();
            if response.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = response.get("error") {
                    panic!("mcp request failed: {error}");
                }
                return response
                    .get("result")
                    .cloned()
                    .expect("successful response should contain result");
            }
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        }));
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({
                "name": name,
                "arguments": arguments
            }),
        )
    }

    fn set_log_level(&mut self, level: &str) -> Value {
        self.request("logging/setLevel", json!({ "level": level }))
    }

    fn send(&mut self, message: Value) {
        let encoded = serde_json::to_string(&message).expect("message should serialize");
        self.stdin
            .write_all(encoded.as_bytes())
            .expect("message should be written");
        self.stdin
            .write_all(b"\n")
            .expect("newline should be written");
        self.stdin.flush().expect("stdin should flush");
    }

    fn read_message(&mut self) -> Value {
        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .expect("response should be readable");
        assert!(!line.is_empty(), "server closed stdout unexpectedly");
        serde_json::from_str(line.trim()).expect("response should be valid json")
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn structured_content(tool_result: Value) -> Value {
    assert_eq!(
        tool_result.get("isError").and_then(Value::as_bool),
        Some(false),
        "tool returned error: {tool_result}"
    );
    tool_result
        .get("structuredContent")
        .cloned()
        .expect("tool result should include structuredContent")
}

#[test]
fn valid_mcp_lists_tools_and_executes_dsl_mode() {
    let temp = TempDir::new("mcp-dsl");
    let mut client = McpClient::spawn(&[], temp.path());

    let initialize = client.initialize();
    assert_eq!(
        initialize
            .get("protocolVersion")
            .and_then(Value::as_str)
            .expect("protocol version should be present"),
        "2025-11-25"
    );
    assert_eq!(
        initialize["capabilities"]["tools"]["listChanged"].as_bool(),
        Some(true)
    );
    assert_eq!(
        initialize["capabilities"]["resources"]["subscribe"].as_bool(),
        Some(false)
    );
    assert!(initialize["capabilities"]["logging"].is_object());

    let tools = client.request("tools/list", json!({}));
    let tool_names = tools
        .get("tools")
        .and_then(Value::as_array)
        .expect("tools/list should return tools")
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    for expected in [
        "valid_docs_index",
        "valid_docs_get",
        "valid_examples_list",
        "valid_example_get",
        "valid_inspect",
        "valid_check",
        "valid_explain",
        "valid_coverage",
        "valid_testgen",
        "valid_replay",
        "valid_contract_snapshot",
        "valid_contract_check",
        "valid_list_models",
        "valid_graph",
        "valid_lint",
    ] {
        assert!(tool_names.contains(&expected), "missing tool {expected}");
    }
    assert!(tools["tools"]
        .as_array()
        .expect("tools should be present")
        .iter()
        .all(|tool| tool.get("outputSchema").is_some()));

    let resources = client.request("resources/list", json!({}));
    assert!(resources["resources"]
        .as_array()
        .expect("resources should be present")
        .iter()
        .any(|item| item["uri"] == "valid://docs/ai-authoring-guide"));

    let authoring_resource = client.request(
        "resources/read",
        json!({ "uri": "valid://docs/ai-authoring-guide" }),
    );
    assert!(authoring_resource["contents"][0]["text"]
        .as_str()
        .expect("resource text should be present")
        .contains("AI Authoring Guide"));

    let prompts = client.request("prompts/list", json!({}));
    assert!(prompts["prompts"]
        .as_array()
        .expect("prompts should be present")
        .iter()
        .any(|item| item["name"] == "author_model"));

    let prompt = client.request(
        "prompts/get",
        json!({
            "name": "author_model",
            "arguments": {
                "domain": "approval flow with review and export policy"
            }
        }),
    );
    assert!(prompt["messages"]
        .as_array()
        .expect("prompt messages should be present")
        .iter()
        .any(|item| item["content"]["type"] == "resource"));

    let logging = client.set_log_level("debug");
    assert_eq!(logging, json!({}));

    let docs_index = structured_content(client.call_tool("valid_docs_index", json!({})));
    assert_eq!(docs_index["canonical_entry"], "ai-authoring-guide");
    assert!(docs_index["docs"]
        .as_array()
        .expect("docs should be present")
        .iter()
        .any(|item| item["doc_id"] == "language-spec"));

    let authoring_guide = structured_content(
        client.call_tool("valid_docs_get", json!({ "doc_id": "ai-authoring-guide" })),
    );
    assert_eq!(authoring_guide["doc_id"], "ai-authoring-guide");
    assert_eq!(authoring_guide["kind"], "guide");
    assert!(authoring_guide["body_markdown"]
        .as_str()
        .expect("body markdown should be text")
        .contains("declarative `transitions { ... }`"));

    let examples = structured_content(client.call_tool("valid_examples_list", json!({})));
    assert!(examples["examples"]
        .as_array()
        .expect("examples should be present")
        .iter()
        .any(|item| item["example_id"] == "registry-counter-basics"));

    let counter_example = structured_content(client.call_tool(
        "valid_example_get",
        json!({ "example_id": "registry-counter-basics" }),
    ));
    assert_eq!(counter_example["example_id"], "registry-counter-basics");
    assert_eq!(counter_example["recommended_order"], 1);
    assert!(counter_example["source_text"]
        .as_str()
        .expect("source text should be present")
        .contains("valid_step_model!"));

    let model_file = fixture("failing_counter.valid");
    let model_file_str = model_file.to_string_lossy().to_string();

    let inspect = structured_content(
        client.call_tool("valid_inspect", json!({ "model_file": model_file_str })),
    );
    assert_eq!(inspect["model_id"], "FailingCounter");
    assert_eq!(inspect["actions"].as_array().map(Vec::len), Some(2));

    let check = structured_content(
        client.call_tool("valid_check", json!({ "model_file": model_file_str })),
    );
    assert_eq!(check["status"], "FAIL");
    assert_eq!(check["property_result"]["property_id"], "P_FAIL");
    assert!(
        check["trace"]["steps"]
            .as_array()
            .expect("trace steps should be present")
            .len()
            >= 2
    );

    let explain = structured_content(
        client.call_tool("valid_explain", json!({ "model_file": model_file_str })),
    );
    assert_eq!(explain["property_id"], "P_FAIL");
    assert!(
        explain["candidate_causes"]
            .as_array()
            .expect("candidate causes should exist")
            .len()
            >= 1
    );

    let coverage = structured_content(
        client.call_tool("valid_coverage", json!({ "model_file": model_file_str })),
    );
    assert_eq!(coverage["model_id"], "FailingCounter");
    assert!(
        coverage["summary"]["step_count"]
            .as_u64()
            .expect("step count should be numeric")
            >= 1
    );

    let graph = structured_content(client.call_tool(
        "valid_graph",
        json!({ "model_file": model_file_str, "format": "mermaid", "view": "logic" }),
    ));
    assert_eq!(graph["format"], "mermaid");
    assert!(graph["graph"]
        .as_str()
        .expect("graph should be text")
        .contains("flowchart"));

    let failure_graph = structured_content(client.call_tool(
        "valid_graph",
        json!({
            "model_file": model_file_str,
            "format": "json",
            "view": "failure",
            "property_id": "P_FAIL"
        }),
    ));
    assert_eq!(failure_graph["graph_view"], "failure");
    assert_eq!(failure_graph["graph_slice"]["property_id"], "P_FAIL");
    assert!(failure_graph["graph_slice"]["summary"]
        .as_str()
        .expect("summary should exist")
        .contains("P_FAIL"));

    let lint =
        structured_content(client.call_tool("valid_lint", json!({ "model_file": model_file_str })));
    assert_eq!(lint["status"], "ok");
    assert!(lint["findings"].is_array());

    let replay = structured_content(client.call_tool(
        "valid_replay",
        json!({
            "model_file": model_file_str,
            "property_id": "P_FAIL",
            "actions": ["Inc", "Inc"]
        }),
    ));
    assert_eq!(replay["status"], "ok");
    assert_eq!(replay["property_holds"], false);

    let snapshot = structured_content(client.call_tool(
        "valid_contract_snapshot",
        json!({ "model_file": model_file_str }),
    ));
    let lock_file = temp.path().join("valid.lock.json");
    fs::write(
        &lock_file,
        serde_json::to_vec(&json!({
            "schema_version": "1.0.0",
            "generated_at": "1970-01-01T00:00:00Z",
            "entries": [{
                "model_id": snapshot["model_id"],
                "contract_hash": snapshot["contract_hash"],
                "state_fields": snapshot["state_fields"],
                "actions": snapshot["actions"],
                "properties": snapshot["properties"]
            }]
        }))
        .expect("lock json should serialize"),
    )
    .expect("lock file should be written");

    let contract_check = structured_content(client.call_tool(
        "valid_contract_check",
        json!({
            "model_file": model_file_str,
            "lock_file": lock_file.to_string_lossy().to_string()
        }),
    ));
    assert_eq!(contract_check["status"], "unchanged");

    let testgen = structured_content(client.call_tool(
        "valid_testgen",
        json!({
            "model_file": model_file_str,
            "strategy": "counterexample"
        }),
    ));
    assert!(
        testgen["vector_ids"]
            .as_array()
            .expect("vector ids should be present")
            .len()
            >= 1
    );
    let generated_file = testgen["generated_files"]
        .as_array()
        .and_then(|items| items.first())
        .and_then(Value::as_str)
        .expect("generated file should be present");
    assert!(temp.path().join(generated_file).exists() || PathBuf::from(generated_file).exists());

    let bundled = structured_content(client.call_tool("valid_list_models", json!({})));
    assert!(bundled["models"]
        .as_array()
        .expect("bundled models should be present")
        .iter()
        .any(|item| item == "counter"));
}

#[test]
fn valid_mcp_accepts_older_protocol_versions() {
    let temp = TempDir::new("mcp-older-protocol");
    let mut client = McpClient::spawn(&[], temp.path());

    let result = client.request(
        "initialize",
        json!({
            "protocolVersion": "2025-11-05",
            "capabilities": {},
            "clientInfo": { "name": "valid-test", "version": "0.1.0" }
        }),
    );
    client.notify("notifications/initialized", json!({}));

    assert_eq!(
        result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .expect("protocol version should be present"),
        "2025-11-05"
    );
    assert!(result["capabilities"]["tools"].is_object());
}

#[cfg(unix)]
fn make_mock_registry(temp: &TempDir) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = temp.path().join("mock-registry.sh");
    let body = r#"#!/bin/sh
cmd="$1"
sub="$2"

if [ "$cmd" = "list" ]; then
  printf '%s\n' '{"models":["mock-safe","mock-broken"]}'
  exit 0
fi

if [ "$cmd" = "inspect" ] && [ "$2" = "mock-safe" ]; then
  printf '%s\n' '{"schema_version":"1.0.0","request_id":"registry-inspect","status":"ok","model_id":"mock-safe","machine_ir_ready":true,"machine_ir_error":null,"capabilities":{"parse_ready":true,"parse":{"reason":"","migration_hint":null,"unsupported_features":[]},"explicit_ready":true,"explicit":{"reason":"","migration_hint":null,"unsupported_features":[]},"ir_ready":true,"ir":{"reason":"","migration_hint":null,"unsupported_features":[]},"solver_ready":true,"solver":{"reason":"","migration_hint":null,"unsupported_features":[]},"coverage_ready":true,"coverage":{"reason":"","migration_hint":null,"unsupported_features":[]},"explain_ready":true,"explain":{"reason":"","migration_hint":null,"unsupported_features":[]},"testgen_ready":true,"testgen":{"reason":"","migration_hint":null,"unsupported_features":[]},"reasons":[]},"state_fields":["ready"],"actions":["ENABLE"],"properties":["P_SAFE"],"state_field_details":[],"action_details":[],"transition_details":[],"property_details":[]}'
  exit 0
fi

if [ "$cmd" = "inspect" ] && [ "$2" = "mock-broken" ]; then
  printf '%s\n' '{"schema_version":"1.0.0","request_id":"registry-inspect","status":"ok","model_id":"mock-broken","machine_ir_ready":true,"machine_ir_error":null,"capabilities":{"parse_ready":true,"parse":{"reason":"","migration_hint":null,"unsupported_features":[]},"explicit_ready":true,"explicit":{"reason":"","migration_hint":null,"unsupported_features":[]},"ir_ready":true,"ir":{"reason":"","migration_hint":null,"unsupported_features":[]},"solver_ready":true,"solver":{"reason":"","migration_hint":null,"unsupported_features":[]},"coverage_ready":true,"coverage":{"reason":"","migration_hint":null,"unsupported_features":[]},"explain_ready":true,"explain":{"reason":"","migration_hint":null,"unsupported_features":[]},"testgen_ready":true,"testgen":{"reason":"","migration_hint":null,"unsupported_features":[]},"reasons":[]},"state_fields":["ready"],"actions":["ENABLE"],"properties":["P_FAIL","P_GUARD"],"state_field_details":[],"action_details":[],"transition_details":[],"property_details":[]}'
  exit 0
fi

if [ "$cmd" = "check" ]; then
  printf '%s\n' '{"kind":"completed","model_id":"mock-broken","manifest":{"request_id":"registry-check","run_id":"run-registry","schema_version":"1.0.0","source_hash":"sha256:source","contract_hash":"sha256:contract","engine_version":"0.1.0","backend_name":"explicit","backend_version":"0.1.0","seed":null},"status":"FAIL","assurance_level":"COMPLETE","explored_states":3,"explored_transitions":2,"property_result":{"property_id":"P_FAIL","property_kind":"invariant","status":"FAIL","assurance_level":"COMPLETE","reason_code":"MOCK_COUNTEREXAMPLE","unknown_reason":null,"terminal_state_id":"s2","evidence_id":"ev-1","summary":"mock registry fail"},"trace":null,"ci":{"exit_code":2,"status":"FAIL","backend":"explicit"},"review_summary":{"headline":"FAIL P_FAIL for mock-broken","trace_steps":0,"failing_action_id":null,"action_sequence":[],"next_steps":[]}}'
  exit 2
fi

if [ "$cmd" = "contract" ] && [ "$sub" = "snapshot" ]; then
  printf '%s\n' '{"snapshots":[{"model_id":"mock-safe","contract_hash":"sha256:safe"},{"model_id":"mock-broken","contract_hash":"sha256:broken"}]}'
  exit 0
fi

if [ "$cmd" = "contract" ] && [ "$sub" = "check" ]; then
  printf '%s\n' '{"reports":[{"schema_version":"1.0.0","status":"changed","contract_id":"mock-broken","old_hash":"sha256:old","new_hash":"sha256:new","changes":["actions"]}]}'
  exit 2
fi

printf '%s\n' '{"diagnostics":[{"error_code":"SEARCH","segment":"engine-search","message":"unsupported mock command","primary_span":null,"help":[],"best_practices":[]}]}'
exit 3
"#;
    fs::write(&path, body).expect("mock registry script should be written");
    let mut permissions = fs::metadata(&path)
        .expect("mock registry metadata should be available")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).expect("mock registry should be executable");
    path
}

#[cfg(unix)]
#[test]
fn valid_mcp_supports_registry_mode_with_default_binary() {
    let temp = TempDir::new("mcp-registry");
    let registry = make_mock_registry(&temp);
    let registry_str = registry.to_string_lossy().to_string();
    let mut client = McpClient::spawn(&["--registry-binary", &registry_str], temp.path());
    client.initialize();

    let resources = client.request("resources/list", json!({}));
    assert!(resources["resources"]
        .as_array()
        .expect("resources should be present")
        .iter()
        .any(|item| item["uri"] == "valid://targets/default-registry-binary"));

    let listed = structured_content(client.call_tool("valid_list_models", json!({})));
    assert_eq!(listed["models"][0], "mock-safe");

    let check =
        structured_content(client.call_tool("valid_check", json!({ "model_name": "mock-broken" })));
    assert_eq!(check["status"], "FAIL");
    assert_eq!(
        check["property_result"]["reason_code"],
        "MOCK_COUNTEREXAMPLE"
    );

    let snapshot = structured_content(client.call_tool(
        "valid_contract_snapshot",
        json!({ "model_name": "mock-broken" }),
    ));
    assert_eq!(snapshot["contract_hash"], "sha256:broken");

    let lock_file = temp.path().join("mock.lock.json");
    fs::write(&lock_file, "{}").expect("placeholder lock file should be written");
    let drift = structured_content(client.call_tool(
        "valid_contract_check",
        json!({
            "model_name": "mock-broken",
            "lock_file": lock_file.to_string_lossy().to_string()
        }),
    ));
    assert_eq!(drift["status"], "changed");
    assert_eq!(drift["contract_id"], "mock-broken");
}

#[cfg(unix)]
#[test]
fn valid_mcp_allows_explicit_dsl_calls_when_default_registry_is_configured() {
    let temp = TempDir::new("mcp-dsl-overrides-registry");
    let registry = make_mock_registry(&temp);
    let registry_str = registry.to_string_lossy().to_string();
    let mut client = McpClient::spawn(&["--registry-binary", &registry_str], temp.path());
    client.initialize();

    let inspect = structured_content(client.call_tool(
        "valid_inspect",
        json!({
            "source": "model Counter\nstate:\n  count: u8[0..2]\ninit:\n  count = 0\naction Inc:\n  pre: count <= 1\n  post:\n    count = count + 1\nproperty P_OK:\n  invariant: count <= 2\n"
        }),
    ));
    assert_eq!(inspect["model_id"], "Counter");

    let bundled =
        structured_content(client.call_tool("valid_list_models", json!({ "registry_binary": "" })));
    assert!(bundled["models"]
        .as_array()
        .expect("bundled models should be present")
        .iter()
        .any(|item| item == "counter"));
}

#[cfg(unix)]
#[test]
fn valid_mcp_exposes_project_property_metadata_and_suite_runs() {
    let temp = TempDir::new("mcp-suite-runs");
    let registry = make_mock_registry(&temp);
    fs::write(
        temp.path().join("valid.toml"),
        "registry = \"examples/valid_models.rs\"\n\n[critical_properties]\nmock-broken = [\"P_FAIL\"]\n\n[property_suites.smoke]\nentries = [{ model = \"mock-broken\", properties = [\"P_GUARD\"] }]\n",
    )
    .expect("valid.toml");
    let registry_str = registry.to_string_lossy().to_string();
    let mut client = McpClient::spawn(&["--registry-binary", &registry_str], temp.path());
    client.initialize();

    let listed = structured_content(client.call_tool("valid_list_models", json!({})));
    assert_eq!(listed["critical_properties"]["mock-broken"][0], "P_FAIL");
    assert_eq!(
        listed["property_suites"]["smoke"][0]["properties"][0],
        "P_GUARD"
    );

    let suite =
        structured_content(client.call_tool("valid_suite_run", json!({ "suite_name": "smoke" })));
    assert_eq!(suite["selection_mode"], "named_suite");
    assert_eq!(suite["suite_name"], "smoke");
    assert_eq!(suite["runs"][0]["property_id"], "P_GUARD");

    let lock_file = temp.path().join("mock.lock.json");
    fs::write(&lock_file, "{}").expect("placeholder lock file should be written");
    let drift = structured_content(client.call_tool(
        "valid_contract_check",
        json!({
            "model_name": "mock-broken",
            "lock_file": lock_file.to_string_lossy().to_string()
        }),
    ));
    assert_eq!(drift["affected_critical_properties"][0], "P_FAIL");
    assert_eq!(drift["affected_property_suites"][0], "smoke");
}

#[test]
fn valid_mcp_exposes_default_model_file_as_resource() {
    let temp = TempDir::new("mcp-default-model-file");
    let model_file = fixture("failing_counter.valid");
    let model_file_str = model_file.to_string_lossy().to_string();
    let mut client = McpClient::spawn(&["--model-file", &model_file_str], temp.path());
    client.initialize();

    let resources = client.request("resources/list", json!({}));
    assert!(resources["resources"]
        .as_array()
        .expect("resources should be present")
        .iter()
        .any(|item| item["uri"] == "valid://targets/default-model-file"));

    let contents = client.request(
        "resources/read",
        json!({ "uri": "valid://targets/default-model-file" }),
    );
    assert!(contents["contents"][0]["text"]
        .as_str()
        .expect("resource text should be present")
        .contains("model FailingCounter"));
}
