use std::path::PathBuf;
use std::fs;
use std::sync::{Mutex, OnceLock};
use std::process::Command;

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

#[test]
fn cargo_valid_lists_registered_models() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("list")
        .arg("--json")
        .output()
        .expect("cargo-valid list should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"counter\""));
    assert!(stdout.contains("\"failing-counter\""));
    assert!(stdout.contains("\"iam-access\""));
}

#[test]
fn cargo_valid_inspects_registered_model() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
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
fn cargo_valid_checks_registered_model() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("check")
        .arg("failing-counter")
        .arg("--json")
        .output()
        .expect("cargo-valid check should run");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"property_id\":\"P_FAIL\""));
}

#[test]
fn cargo_valid_lints_registered_model_with_migration_hints() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("lint")
        .arg("counter")
        .arg("--json")
        .output()
        .expect("cargo-valid lint should run");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"code\":\"opaque_step_closure\""));
    assert!(stdout.contains("\"code\":\"missing_declarative_transitions\""));
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
        .arg("testgen")
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
fn cargo_valid_testgen_guard_generates_files_for_registry_file() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples").join("iam_transition_registry.rs"))
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
fn cargo_valid_check_can_target_specific_property() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
        .arg("check")
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
fn cargo_valid_bundled_declarative_model_can_use_command_backend() {
    let _guard = cargo_lock().lock().unwrap();
    let output = Command::new(cargo_valid_path())
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
        .arg("check")
        .arg("iam-access")
        .arg("--property=P_BILLING_READ_REQUIRES_SESSION")
        .arg("--backend=smt-cvc5")
        .arg("--solver-exec")
        .arg("sh")
        .arg("--solver-arg")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples").join("solvers").join("mock_cvc5_solver.sh"))
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
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples").join("iam_transition_registry.rs"))
        .arg("check")
        .arg("iam-access")
        .arg("--property=P_BILLING_READ_REQUIRES_SESSION")
        .arg("--backend=smt-cvc5")
        .arg("--solver-exec")
        .arg("sh")
        .arg("--solver-arg")
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples").join("solvers").join("mock_cvc5_solver.sh"))
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
