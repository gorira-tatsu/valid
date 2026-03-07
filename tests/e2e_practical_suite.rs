use std::fs;
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

fn practical_registry_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("practical_use_cases_registry.rs")
}

fn extract_generated_files(stdout: &str) -> Vec<String> {
    stdout
        .split('"')
        .filter(|entry| entry.starts_with("tests/generated/") && entry.ends_with(".rs"))
        .map(|entry| entry.to_string())
        .collect()
}

fn cleanup_generated_files(stdout: &str) {
    for path in extract_generated_files(stdout) {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn practical_suite_lists_models() {
    let _guard = lock_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(practical_registry_file())
        .arg("list")
        .arg("--json")
        .output()
        .expect("cargo-valid list should run for practical suite");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"prod-deploy-safe\""));
    assert!(stdout.contains("\"breakglass-access-regression\""));
    assert!(stdout.contains("\"refund-control\""));
    assert!(stdout.contains("\"data-export-control\""));
}

#[test]
fn practical_suite_pass_model_is_solver_ready_and_passes() {
    let _guard = lock_guard();
    let inspect = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(practical_registry_file())
        .arg("inspect")
        .arg("prod-deploy-safe")
        .arg("--json")
        .output()
        .expect("inspect should run for deployment model");
    assert!(inspect.status.success());
    let inspect_stdout = String::from_utf8_lossy(&inspect.stdout);
    assert!(inspect_stdout.contains("\"machine_ir_ready\":true"));
    assert!(inspect_stdout.contains("\"solver_ready\":true"));
    assert!(inspect_stdout.contains("\"approval_path\""));
    assert!(inspect_stdout.contains("\"incident_path\""));

    let lint = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(practical_registry_file())
        .arg("lint")
        .arg("prod-deploy-safe")
        .arg("--json")
        .output()
        .expect("lint should run for deployment model");
    assert_eq!(lint.status.code(), Some(0));
    let lint_stdout = String::from_utf8_lossy(&lint.stdout);
    assert!(lint_stdout.contains("\"findings\":[]"));

    let check = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(practical_registry_file())
        .arg("check")
        .arg("prod-deploy-safe")
        .arg("--property=P_DEPLOY_REQUIRES_APPROVALS")
        .arg("--json")
        .output()
        .expect("check should run for deployment model");
    assert_eq!(check.status.code(), Some(0));
    let check_stdout = String::from_utf8_lossy(&check.stdout);
    assert!(check_stdout.contains("\"status\":\"PASS\""));
    assert!(check_stdout.contains("\"property_id\":\"P_DEPLOY_REQUIRES_APPROVALS\""));
}

#[test]
fn practical_suite_regression_model_fails_with_explainable_path() {
    let _guard = lock_guard();
    let check = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(practical_registry_file())
        .arg("check")
        .arg("breakglass-access-regression")
        .arg("--property=P_ACCESS_REQUIRES_INCIDENT")
        .arg("--json")
        .output()
        .expect("check should run for regression model");
    assert_eq!(check.status.code(), Some(2));
    let check_stdout = String::from_utf8_lossy(&check.stdout);
    assert!(check_stdout.contains("\"status\":\"FAIL\""));
    assert!(check_stdout.contains("\"property_id\":\"P_ACCESS_REQUIRES_INCIDENT\""));
    assert!(check_stdout.contains("FORCE_GRANT"));

    let explain = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(practical_registry_file())
        .arg("explain")
        .arg("breakglass-access-regression")
        .arg("--property=P_ACCESS_REQUIRES_INCIDENT")
        .arg("--json")
        .output()
        .expect("explain should run for regression model");
    assert!(explain.status.success());
    let explain_stdout = String::from_utf8_lossy(&explain.stdout);
    assert!(explain_stdout.contains("\"decision_path_tags\""));
    assert!(explain_stdout.contains("exception_path"));
    assert!(explain_stdout.contains("deny_path"));
}

#[test]
fn practical_suite_coverage_and_path_testgen_surface_business_paths() {
    let _guard = lock_guard();
    let coverage = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(practical_registry_file())
        .arg("coverage")
        .arg("refund-control")
        .arg("--json")
        .output()
        .expect("coverage should run for refund model");
    assert!(coverage.status.success());
    let coverage_stdout = String::from_utf8_lossy(&coverage.stdout);
    assert!(coverage_stdout.contains("\"path_tags\""));
    assert!(coverage_stdout.contains("finance_path"));
    assert!(coverage_stdout.contains("risk_path"));

    let testgen = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(practical_registry_file())
        .arg("testgen")
        .arg("refund-control")
        .arg("--strategy=path")
        .arg("--json")
        .output()
        .expect("path testgen should run for refund model");
    assert!(testgen.status.success());
    let testgen_stdout = String::from_utf8_lossy(&testgen.stdout);
    assert!(testgen_stdout.contains("\"generated_files\":["));
    let mut saw_business_tag = false;
    for path in extract_generated_files(&testgen_stdout) {
        let body = fs::read_to_string(&path).expect("generated file must exist");
        assert!(body.contains("path_tag:"));
        if body.contains("finance_path") || body.contains("risk_path") {
            saw_business_tag = true;
        }
    }
    assert!(saw_business_tag);
    cleanup_generated_files(&testgen_stdout);
}
