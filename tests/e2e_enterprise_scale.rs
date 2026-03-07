use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};

fn cargo_valid_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-valid"))
}

fn cargo_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_guard() -> std::sync::MutexGuard<'static, ()> {
    cargo_lock().lock().unwrap_or_else(|err| err.into_inner())
}

fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")
}

fn scale_registry_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("enterprise_scale_registry.rs")
}

#[test]
fn enterprise_scale_registry_lists_models() {
    let _guard = lock_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(scale_registry_file())
        .arg("models")
        .arg("--json")
        .output()
        .expect("cargo-valid models should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"access-review-scale\""));
    assert!(stdout.contains("\"quota-guardrail-regression\""));
}

#[test]
fn enterprise_scale_model_is_solver_ready_and_reports_u16_fields() {
    let _guard = lock_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(scale_registry_file())
        .arg("inspect")
        .arg("access-review-scale")
        .arg("--json")
        .output()
        .expect("cargo-valid inspect should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"machine_ir_ready\":true"));
    assert!(stdout.contains("\"solver_ready\":true"));
    assert!(stdout.contains("\"rust_type\":\"u16\""));
    assert!(stdout.contains("\"range\":\"0..=12\""));
    assert!(stdout.contains("exception_path"));
}

#[test]
fn enterprise_scale_regression_fails_with_review_summary() {
    let _guard = lock_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(scale_registry_file())
        .arg("verify")
        .arg("quota-guardrail-regression")
        .arg("--property=P_EXPORT_REQUIRES_BUDGET_DISCIPLINE")
        .arg("--json")
        .output()
        .expect("cargo-valid verify should run");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\":\"FAIL\""));
    assert!(stdout.contains("\"ci\":{\"exit_code\":2"));
    assert!(stdout.contains("\"review_summary\""));
    assert!(stdout.contains("ENABLE_EXPORT"));
}

#[test]
fn enterprise_scale_readiness_surfaces_clean_declarative_models() {
    let _guard = lock_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(scale_registry_file())
        .arg("readiness")
        .arg("access-review-scale")
        .arg("--json")
        .output()
        .expect("cargo-valid readiness should run");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"findings\":[]"));
}
