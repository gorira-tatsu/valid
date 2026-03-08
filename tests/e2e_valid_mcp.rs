use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};

fn valid_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_valid"))
}

fn build_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn build_guard() -> std::sync::MutexGuard<'static, ()> {
    build_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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
            .expect("clock should be monotonic enough")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("valid-{prefix}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).expect("temp dir should exist");
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
        let mut child = Command::new(valid_path())
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("valid mcp should start");
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
                    .expect("response should contain result");
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
    assert_eq!(tool_result["isError"].as_bool(), Some(false));
    tool_result["structuredContent"].clone()
}

fn write_registry_fixture(project_dir: &Path, model_name: &str) {
    fs::create_dir_all(project_dir.join("examples")).expect("examples dir");
    fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock"),
        project_dir.join("Cargo.lock"),
    )
    .expect("cargo lock");
    fs::write(
        project_dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"valid-mcp-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nvalid = {{ path = {:?}, features = [\"verification-runtime\"] }}\n",
            env!("CARGO_MANIFEST_DIR")
        ),
    )
    .expect("cargo manifest");
    fs::write(
        project_dir.join("examples").join("valid_models.rs"),
        format!(
            r#"use valid::{{registry::run_registry_cli, valid_actions, valid_model, valid_models, valid_state}};

valid_state! {{
    struct State {{
        ready: bool,
    }}
}}

valid_actions! {{
    enum Action {{
        Enable => "ENABLE" [reads = ["ready"], writes = ["ready"]],
    }}
}}

valid_model! {{
    model TempMcpModel<State, Action>;
    init [State {{ ready: false }}];
    transitions {{
        transition Enable [tags = ["allow_path"]] when |state| state.ready == false => [State {{ ready: true }}];
    }}
    properties {{
        invariant P_READY_IS_BOOLEAN |state| state.ready == false || state.ready == true;
    }}
}}

fn main() {{
    run_registry_cli(valid_models![
        "{model_name}" => TempMcpModel,
    ]);
}}
"#
        ),
    )
    .expect("registry example");
}

#[test]
fn valid_mcp_supports_dsl_mode() {
    let temp = TempDir::new("valid-mcp-dsl");
    let mut client = McpClient::spawn(
        &[
            "mcp",
            "--model-file",
            fixture("safe_counter.valid").to_string_lossy().as_ref(),
        ],
        temp.path(),
    );

    let initialize = client.initialize();
    assert_eq!(initialize["protocolVersion"].as_str(), Some("2025-11-25"));
    let tools = client.request("tools/list", json!({}));
    let tool_names = tools["tools"]
        .as_array()
        .expect("tools list")
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&"valid_inspect"));

    let inspect = structured_content(client.call_tool("valid_inspect", json!({})));
    assert_eq!(inspect["model_id"].as_str(), Some("SafeCounter"));
}

#[test]
fn valid_mcp_supports_project_first_registry_mode() {
    let _guard = build_guard();
    let temp = TempDir::new("valid-mcp-registry");
    write_registry_fixture(temp.path(), "temp-counter");
    let manifest_path = temp.path().join("Cargo.toml");

    let mut client = McpClient::spawn(
        &[
            "mcp",
            "--manifest-path",
            manifest_path.to_string_lossy().as_ref(),
        ],
        temp.path(),
    );

    let initialize = client.initialize();
    assert_eq!(initialize["protocolVersion"].as_str(), Some("2025-11-25"));
    let models = structured_content(client.call_tool("valid_list_models", json!({})));
    let listed = models["models"]
        .as_array()
        .expect("model list")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    assert!(listed.contains(&"temp-counter"));

    let inspect = structured_content(client.call_tool(
        "valid_inspect",
        json!({
            "model_name": "temp-counter"
        }),
    ));
    assert_eq!(inspect["model_id"].as_str(), Some("TempMcpModel"));
    assert_eq!(
        inspect["properties"][0].as_str(),
        Some("P_READY_IS_BOOLEAN")
    );
}
