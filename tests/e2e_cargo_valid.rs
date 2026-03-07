use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn cargo_valid_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-valid"))
}

fn cargo_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")
}

fn example_registry_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("valid_models.rs")
}

fn cleanup_generated_files(stdout: &str) {
    for path in stdout
        .split('"')
        .filter(|entry| entry.starts_with("tests/generated/") && entry.ends_with(".rs"))
    {
        let _ = fs::remove_file(path);
    }
}

fn extract_generated_files(stdout: &str) -> Vec<String> {
    stdout
        .split('"')
        .filter(|entry| entry.starts_with("tests/generated/") && entry.ends_with(".rs"))
        .map(|entry| entry.to_string())
        .collect()
}

fn unique_temp_project_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic enough")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
}

fn write_autodiscover_fixture(project_dir: &Path, model_name: &str) {
    fs::create_dir_all(project_dir.join("examples")).expect("temp examples dir");
    fs::write(
        project_dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"valid-autodiscover-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nvalid = {{ path = {:?} }}\n",
            env!("CARGO_MANIFEST_DIR")
        ),
    )
    .expect("temp Cargo.toml");
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
    model AutoDiscoverModel<State, Action>;
    init [State {{ ready: false }}];
    transitions {{
        transition Enable [tags = ["allow_path"]] when |state| state.ready == false => [State {{ ready: true }}];
    }}
    properties {{
        invariant P_READY_EVENTUAL |state| state.ready == false || state.ready == true;
    }}
}}

fn main() {{
    run_registry_cli(valid_models![
        "{model_name}" => AutoDiscoverModel,
    ]);
}}
"#
        ),
    )
    .expect("temp valid_models example");
}

#[test]
fn cargo_valid_lists_registered_models() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("models")
        .arg("--json")
        .output()
        .expect("cargo-valid list should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"prod-deploy-safe\""));
    assert!(stdout.contains("\"breakglass-access-regression\""));
    assert!(stdout.contains("\"refund-control\""));
}

#[test]
fn cargo_subcommand_style_prefix_is_accepted() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("valid")
        .arg("models")
        .arg("--json")
        .output()
        .expect("cargo-valid should accept cargo subcommand style prefix");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"prod-deploy-safe\""));
}

#[test]
fn cargo_valid_registry_flag_alias_works() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("models")
        .arg("--json")
        .output()
        .expect("cargo-valid models via --registry should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"counter\""));
}

#[test]
fn cargo_valid_inspects_registered_model() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("inspect")
        .arg("counter")
        .arg("--json")
        .output()
        .expect("cargo-valid inspect should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"model_id\":\"CounterModel\""));
    assert!(stdout.contains("\"P_LOCKED_RANGE\""));
    assert!(stdout.contains("\"machine_ir_ready\":false"));
    assert!(stdout.contains("\"capabilities\":{\"parse_ready\":true"));
    assert!(stdout.contains("\"solver_ready\":false"));
    assert!(stdout.contains("\"opaque_step_closure\""));
    assert!(stdout.contains("\"state_field_details\""));
    assert!(stdout.contains("\"action_details\""));
    assert!(stdout.contains("\"transition_details\""));
}

#[test]
fn cargo_valid_graph_renders_mermaid_for_bundled_model() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("iam_transition_registry.rs"),
        )
        .arg("graph")
        .arg("iam-access")
        .output()
        .expect("cargo-valid graph should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("flowchart LR"));
    assert!(stdout.contains("IamAccessModel"));
    assert!(stdout.contains("ATTACH_BOUNDARY"));
    assert!(stdout.contains("P_BILLING_READ_REQUIRES_SESSION"));
}

#[test]
fn cargo_valid_graph_supports_dot_and_svg_formats() {
    let _guard = cargo_lock().lock().unwrap();
    let dot_output = Command::new(cargo_valid_path())
        .arg("graph")
        .arg("refund-control")
        .arg("--format=dot")
        .output()
        .expect("cargo-valid graph dot should run");
    assert!(dot_output.status.success());
    let dot = String::from_utf8_lossy(&dot_output.stdout);
    assert!(dot.contains("digraph model"));
    assert!(dot.contains("ISSUE_REFUND"));

    let svg_output = Command::new(cargo_valid_path())
        .arg("graph")
        .arg("refund-control")
        .arg("--format=svg")
        .output()
        .expect("cargo-valid graph svg should run");
    assert!(svg_output.status.success());
    let svg = String::from_utf8_lossy(&svg_output.stdout);
    assert!(svg.contains("<svg"));
    assert!(svg.contains("RefundControlModel"));
}

#[test]
fn cargo_valid_graph_marks_step_models_as_explicit_only() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("graph")
        .arg("counter")
        .output()
        .expect("cargo-valid graph should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("explicit-only / opaque-step"));
    assert!(stdout.contains("opaque_step_closure"));
    assert!(stdout.contains("transition internals hidden"));
    assert!(!stdout.contains("transition_INC_0"));
}

#[test]
fn cargo_valid_checks_registered_model() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("check")
        .arg("failing-counter")
        .arg("--json")
        .output()
        .expect("cargo-valid check should run");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"property_id\":\"P_FAIL\""));
    assert!(stdout.contains("\"ci\":{\"exit_code\":2"));
    assert!(stdout.contains("\"review_summary\""));
}

#[test]
fn cargo_valid_lints_registered_model_with_migration_hints() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("readiness")
        .arg("counter")
        .arg("--json")
        .output()
        .expect("cargo-valid lint should run");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"code\":\"opaque_step_closure\""));
    assert!(stdout.contains("\"code\":\"missing_declarative_transitions\""));
    assert!(stdout.contains("\"snippet\":\"transition INC"));
}

#[test]
fn cargo_valid_explain_includes_review_metadata() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("explain")
        .arg("breakglass-access-regression")
        .arg("--json")
        .output()
        .expect("cargo-valid explain should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"failing_action_id\""));
    assert!(stdout.contains("\"next_steps\""));
    assert!(stdout.contains("\"review_summary\""));
}

#[test]
fn cargo_valid_lists_example_models() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--example")
        .arg("valid_models")
        .arg("list")
        .arg("--json")
        .output()
        .expect("cargo-valid list for example registry should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"counter\""));
    assert!(stdout.contains("\"failing-counter\""));
}

#[test]
fn cargo_valid_checks_example_model() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--example")
        .arg("valid_models")
        .arg("check")
        .arg("failing-counter")
        .arg("--json")
        .output()
        .expect("cargo-valid check for example registry should run");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"property_id\":\"P_FAIL\""));
}

#[test]
fn cargo_valid_lists_example_models_from_file() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(example_registry_file())
        .arg("list")
        .arg("--json")
        .output()
        .expect("cargo-valid list for file-backed example registry should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"counter\""));
    assert!(stdout.contains("\"failing-counter\""));
}

#[test]
fn cargo_valid_checks_all_example_models_from_file() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(example_registry_file())
        .arg("all")
        .arg("--json")
        .output()
        .expect("cargo-valid all for file-backed example registry should run");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"runs\":["));
    assert!(stdout.contains("\"model_id\":\"counter\""));
    assert!(stdout.contains("\"property_id\":\"P_FAIL\""));
}

#[test]
fn cargo_valid_testgen_witness_generates_files() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("generate-tests")
        .arg("counter")
        .arg("--strategy=witness")
        .arg("--json")
        .output()
        .expect("cargo-valid testgen should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"generated_files\":["));
    assert!(stdout.contains("tests/generated/"));
    for path in extract_generated_files(&stdout) {
        let body = fs::read_to_string(&path).expect("generated file must exist");
        assert!(body.contains("assert_replay_output_json"));
    }
    cleanup_generated_files(&stdout);
}

#[test]
fn cargo_valid_clean_removes_generated_and_artifacts() {
    let _guard = cargo_lock().lock().unwrap();
    let temp_root = unique_temp_project_dir("valid-clean");
    let generated = temp_root
        .join("tests")
        .join("generated")
        .join("clean-sentinel.rs");
    let artifact_dir = temp_root.join("artifacts").join("clean-sentinel");
    fs::create_dir_all(generated.parent().unwrap()).expect("generated dir");
    fs::create_dir_all(&artifact_dir).expect("artifact dir");
    fs::write(&generated, "// sentinel\n").expect("generated sentinel");
    fs::write(artifact_dir.join("report.json"), "{}\n").expect("artifact sentinel");

    let output = Command::new(cargo_valid_path())
        .current_dir(&temp_root)
        .arg("clean")
        .arg("all")
        .arg("--json")
        .output()
        .expect("cargo-valid clean should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("clean-sentinel.rs"));
    assert!(stdout.contains("artifacts/clean-sentinel"));
    assert!(!generated.exists());
    assert!(!artifact_dir.exists());
    let _ = fs::remove_dir_all(temp_root);
}

#[test]
fn cargo_valid_testgen_guard_generates_files_for_registry_file() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("iam_transition_registry.rs"),
        )
        .arg("testgen")
        .arg("iam-access")
        .arg("--strategy=guard")
        .arg("--json")
        .output()
        .expect("cargo-valid guard testgen should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"generated_files\":["));
    assert!(stdout.contains("tests/generated/"));
    for path in extract_generated_files(&stdout) {
        let body = fs::read_to_string(&path).expect("generated file must exist");
        assert!(body.contains("assert_replay_output_json"));
    }
    cleanup_generated_files(&stdout);
}

#[test]
fn cargo_valid_testgen_path_generates_tagged_files() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("iam_transition_registry.rs"),
        )
        .arg("testgen")
        .arg("iam-access")
        .arg("--strategy=path")
        .arg("--json")
        .output()
        .expect("cargo-valid path testgen should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"generated_files\":["));
    for path in extract_generated_files(&stdout) {
        let body = fs::read_to_string(&path).expect("generated file must exist");
        assert!(body.contains("path_tag:"));
    }
    cleanup_generated_files(&stdout);
}

#[test]
fn cargo_valid_check_can_target_specific_property() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("verify")
        .arg("failing-counter")
        .arg("--property=P_FAIL")
        .arg("--json")
        .output()
        .expect("cargo-valid property-specific check should run");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"property_id\":\"P_FAIL\""));
}

#[test]
fn cargo_valid_external_registry_can_use_command_backend() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples").join("iam_transition_registry.rs"))
        .arg("check")
        .arg("iam-access")
        .arg("--property=P_BILLING_READ_REQUIRES_SESSION")
        .arg("--backend=command")
        .arg("--solver-exec")
        .arg("sh")
        .arg("--solver-arg")
        .arg("-c")
        .arg("--solver-arg")
        .arg("printf 'STATUS=FAIL\nACTIONS=ATTACH_BOUNDARY,ASSUME_SESSION,EVAL_BILLING_READ\nASSURANCE_LEVEL=BOUNDED\nREASON_CODE=MOCK_COUNTEREXAMPLE\nSUMMARY=registry command backend\n'")
        .arg("--json")
        .output()
        .expect("cargo-valid command backend check should run");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"property_id\":\"P_BILLING_READ_REQUIRES_SESSION\""));
    assert!(stdout.contains("\"status\":\"FAIL\""));
    assert!(stdout.contains("ATTACH_BOUNDARY,ASSUME_SESSION,EVAL_BILLING_READ"));
}

#[test]
fn cargo_valid_inspects_bundled_declarative_model() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("iam_transition_registry.rs"),
        )
        .arg("inspect")
        .arg("iam-access")
        .arg("--json")
        .output()
        .expect("cargo-valid inspect for bundled declarative model should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"machine_ir_ready\":true"));
    assert!(stdout.contains("\"solver_ready\":true"));
    assert!(stdout.contains("\"reasons\":[]"));
    assert!(stdout.contains("\"transition_details\""));
    assert!(stdout.contains("\"path_tags\""));
    assert!(stdout.contains("\"allow_path\""));
    assert!(stdout.contains("\"boundary_path\""));
    assert!(stdout.contains("\"P_BILLING_READ_REQUIRES_SESSION\""));
}

#[test]
fn cargo_valid_lints_declarative_model_cleanly() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("iam_transition_registry.rs"),
        )
        .arg("lint")
        .arg("iam-access")
        .arg("--json")
        .output()
        .expect("cargo-valid lint for declarative model should run");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\":\"ok\""));
    assert!(stdout.contains("\"findings\":[]"));
}

#[test]
fn cargo_valid_enterprise_registry_supports_or_and_eq_lowering() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("iam_enterprise_registry.rs"),
        )
        .arg("inspect")
        .arg("iam-enterprise")
        .arg("--json")
        .output()
        .expect("cargo-valid inspect for enterprise registry should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"machine_ir_ready\":true"));
    assert!(stdout.contains("\"allow_path\""));
    assert!(stdout.contains("\"session_path\""));
}

#[test]
fn cargo_valid_auto_discovers_external_registry_from_project_root() {
    let _guard = cargo_lock().lock().unwrap();
    let project_dir = unique_temp_project_dir("valid-autodiscover");
    write_autodiscover_fixture(&project_dir, "auto-discover");

    let output = Command::new(cargo_valid_path())
        .current_dir(&project_dir)
        .arg("inspect")
        .arg("auto-discover")
        .arg("--json")
        .output()
        .expect("cargo-valid autodiscovery should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"model_id\":\"AutoDiscoverModel\""));
    assert!(stdout.contains("\"allow_path\""));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cargo_valid_init_writes_valid_toml() {
    let _guard = cargo_lock().lock().unwrap();
    let project_dir = unique_temp_project_dir("valid-init");
    fs::create_dir_all(&project_dir).expect("temp project dir");
    fs::write(
        project_dir.join("Cargo.toml"),
        "[package]\nname = \"valid-init-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("temp Cargo.toml");

    let output = Command::new(cargo_valid_path())
        .current_dir(&project_dir)
        .arg("init")
        .arg("--json")
        .output()
        .expect("cargo-valid init should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let body = fs::read_to_string(project_dir.join("valid.toml")).expect("valid.toml must exist");
    assert!(body.contains("registry = \"examples/valid_models.rs\""));
    assert!(body.contains("default_backend = \"explicit\""));
    assert!(body.contains("generated_tests_dir = \"tests/generated\""));
    assert!(project_dir
        .join("examples")
        .join("valid_models.rs")
        .exists());
    assert!(project_dir
        .join("tests")
        .join("generated")
        .join(".gitkeep")
        .exists());

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cargo_valid_uses_valid_toml_registry_without_flags() {
    let _guard = cargo_lock().lock().unwrap();
    let project_dir = unique_temp_project_dir("valid-project-config");
    write_autodiscover_fixture(&project_dir, "project-config-model");
    fs::write(
        project_dir.join("valid.toml"),
        "registry = \"examples/valid_models.rs\"\ndefault_backend = \"explicit\"\nsuite_models = [\"project-config-model\"]\n",
    )
    .expect("valid.toml");

    let output = Command::new(cargo_valid_path())
        .current_dir(&project_dir)
        .arg("inspect")
        .arg("project-config-model")
        .arg("--json")
        .output()
        .expect("cargo-valid inspect via valid.toml should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"model_id\":\"AutoDiscoverModel\""));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cargo_valid_suite_uses_valid_toml_suite_models() {
    let _guard = cargo_lock().lock().unwrap();
    let project_dir = unique_temp_project_dir("valid-suite-config");
    write_autodiscover_fixture(&project_dir, "suite-only-model");
    fs::write(
        project_dir.join("valid.toml"),
        "registry = \"examples/valid_models.rs\"\ndefault_backend = \"explicit\"\nsuite_models = [\"suite-only-model\"]\n",
    )
    .expect("valid.toml");

    let output = Command::new(cargo_valid_path())
        .current_dir(&project_dir)
        .arg("suite")
        .arg("--json")
        .output()
        .expect("cargo-valid suite via valid.toml should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"model_id\":\"suite-only-model\""));
    assert!(!stdout.contains("\"model_id\":\"counter\""));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cargo_valid_bundled_declarative_model_can_use_command_backend() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("iam_transition_registry.rs"),
        )
        .arg("check")
        .arg("iam-access")
        .arg("--property=P_BILLING_READ_REQUIRES_SESSION")
        .arg("--backend=command")
        .arg("--solver-exec")
        .arg("sh")
        .arg("--solver-arg")
        .arg("-c")
        .arg("--solver-arg")
        .arg("printf 'STATUS=FAIL\nACTIONS=ATTACH_BOUNDARY,ASSUME_SESSION,EVAL_BILLING_READ\nASSURANCE_LEVEL=BOUNDED\nREASON_CODE=MOCK_COUNTEREXAMPLE\nSUMMARY=bundled declarative command backend\n'")
        .arg("--json")
        .output()
        .expect("cargo-valid bundled declarative command backend check should run");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"property_id\":\"P_BILLING_READ_REQUIRES_SESSION\""));
    assert!(stdout.contains("\"status\":\"FAIL\""));
    assert!(stdout.contains("ATTACH_BOUNDARY,ASSUME_SESSION,EVAL_BILLING_READ"));
}

#[test]
fn cargo_valid_bundled_declarative_model_can_use_mock_cvc5_backend() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("iam_transition_registry.rs"),
        )
        .arg("check")
        .arg("iam-access")
        .arg("--property=P_BILLING_READ_REQUIRES_SESSION")
        .arg("--backend=smt-cvc5")
        .arg("--solver-exec")
        .arg("sh")
        .arg("--solver-arg")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("solvers")
                .join("mock_cvc5_solver.sh"),
        )
        .arg("--json")
        .output()
        .expect("cargo-valid bundled mock cvc5 check should run");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"backend_name\":\"smt-cvc5\""));
    assert!(stdout.contains("\"property_id\":\"P_BILLING_READ_REQUIRES_SESSION\""));
    assert!(stdout.contains("\"status\":\"FAIL\""));
}

#[test]
fn cargo_valid_external_registry_can_use_mock_cvc5_backend() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("iam_transition_registry.rs"),
        )
        .arg("check")
        .arg("iam-access")
        .arg("--property=P_BILLING_READ_REQUIRES_SESSION")
        .arg("--backend=smt-cvc5")
        .arg("--solver-exec")
        .arg("sh")
        .arg("--solver-arg")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("solvers")
                .join("mock_cvc5_solver.sh"),
        )
        .arg("--json")
        .output()
        .expect("cargo-valid external mock cvc5 check should run");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"backend_name\":\"smt-cvc5\""));
    assert!(stdout.contains("\"property_id\":\"P_BILLING_READ_REQUIRES_SESSION\""));
    assert!(stdout.contains("\"status\":\"FAIL\""));
}

#[test]
fn cargo_valid_bundled_declarative_testgen_can_use_command_backend() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("iam_transition_registry.rs"),
        )
        .arg("testgen")
        .arg("iam-access")
        .arg("--property=P_BILLING_READ_REQUIRES_SESSION")
        .arg("--strategy=counterexample")
        .arg("--backend=command")
        .arg("--solver-exec")
        .arg("sh")
        .arg("--solver-arg")
        .arg("-c")
        .arg("--solver-arg")
        .arg("printf 'STATUS=FAIL\nACTIONS=ATTACH_BOUNDARY,ASSUME_SESSION,EVAL_BILLING_READ\nASSURANCE_LEVEL=BOUNDED\nREASON_CODE=MOCK_COUNTEREXAMPLE\nSUMMARY=bundled declarative testgen command backend\n'")
        .arg("--json")
        .output()
        .expect("cargo-valid bundled declarative command backend testgen should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"generated_files\":["));
    for path in extract_generated_files(&stdout) {
        let body = fs::read_to_string(&path).expect("generated file must exist");
        assert!(body.contains("assert_replay_output_json"));
    }
    cleanup_generated_files(&stdout);
}
