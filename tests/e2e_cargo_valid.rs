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
    cleanup_generated_files(&stdout);
}
