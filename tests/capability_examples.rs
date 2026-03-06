use std::{
    path::{Path, PathBuf},
    process::Command,
};

use valid::api::{
    check_source, explain_source, inspect_source, orchestrate_source, testgen_source, CheckRequest,
    InspectRequest, OrchestrateRequest, TestgenRequest,
};
use valid::engine::CheckOutcome;

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read_fixture(relative: &str) -> String {
    std::fs::read_to_string(repo_path(relative)).expect("fixture must exist")
}

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_valid"))
}

#[test]
fn safe_counter_passes_via_api() {
    let source = read_fixture("examples/models/safe_counter.valid");
    let outcome = check_source(&CheckRequest {
        request_id: "req-test-safe".to_string(),
        source_name: "safe_counter.valid".to_string(),
        source,
        property_id: None,
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
    });
    match outcome {
        CheckOutcome::Completed(result) => {
            assert_eq!(format!("{:?}", result.status), "Pass");
            assert_eq!(format!("{:?}", result.assurance_level), "Complete");
        }
        CheckOutcome::Errored(error) => panic!("unexpected error: {:?}", error.diagnostics),
    }
}

#[test]
fn failing_counter_explains_and_generates_vectors() {
    let source = read_fixture("examples/models/failing_counter.valid");
    let outcome = check_source(&CheckRequest {
        request_id: "req-test-fail".to_string(),
        source_name: "failing_counter.valid".to_string(),
        source: source.clone(),
        property_id: None,
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
    });
    match outcome {
        CheckOutcome::Completed(result) => {
            assert_eq!(format!("{:?}", result.status), "Fail");
            assert!(result.trace.is_some());
        }
        CheckOutcome::Errored(error) => panic!("unexpected error: {:?}", error.diagnostics),
    }

    let explain = explain_source(&CheckRequest {
        request_id: "req-test-explain".to_string(),
        source_name: "failing_counter.valid".to_string(),
        source: source.clone(),
        property_id: None,
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
    })
    .expect("explain should succeed");
    assert!(!explain.candidate_causes.is_empty());
    assert!(!explain.repair_hints.is_empty());

    let testgen = testgen_source(&TestgenRequest {
        request_id: "req-test-testgen".to_string(),
        source_name: "failing_counter.valid".to_string(),
        source,
        strategy: "counterexample".to_string(),
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
    })
    .expect("testgen should succeed");
    assert!(!testgen.vector_ids.is_empty());
}

#[test]
fn multi_property_orchestrate_returns_aggregate_coverage() {
    let source = read_fixture("examples/models/multi_property.valid");
    let response = orchestrate_source(&OrchestrateRequest {
        request_id: "req-test-orchestrate".to_string(),
        source_name: "multi_property.valid".to_string(),
        source,
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
    })
    .expect("orchestrate should succeed");
    assert_eq!(response.runs.len(), 2);
    assert!(response.aggregate_coverage.is_some());
}

#[test]
fn parse_and_type_errors_are_visible_via_api() {
    let parse_diagnostics = inspect_source(&InspectRequest {
        request_id: "req-test-parse".to_string(),
        source_name: "parse_error.valid".to_string(),
        source: read_fixture("examples/models/parse_error.valid"),
    })
    .expect_err("parse fixture must fail");
    assert_eq!(parse_diagnostics[0].error_code.as_str(), "PARSE_ERROR");

    let type_outcome = check_source(&CheckRequest {
        request_id: "req-test-type".to_string(),
        source_name: "type_error.valid".to_string(),
        source: read_fixture("examples/models/type_error.valid"),
        property_id: None,
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
    });
    match type_outcome {
        CheckOutcome::Errored(error) => {
            assert_eq!(error.diagnostics[0].error_code.as_str(), "TYPECHECK_ERROR");
        }
        CheckOutcome::Completed(result) => panic!("unexpected success: {:?}", result.status),
    }
}

#[test]
fn cli_check_and_orchestrate_work_against_repo_examples() {
    let safe = repo_path("examples/models/safe_counter.valid");
    let fail = repo_path("examples/models/failing_counter.valid");
    let multi = repo_path("examples/models/multi_property.valid");

    let safe_output = Command::new(binary_path())
        .arg("check")
        .arg(&safe)
        .arg("--json")
        .output()
        .expect("safe check should run");
    assert_eq!(safe_output.status.code(), Some(0));

    let fail_output = Command::new(binary_path())
        .arg("check")
        .arg(&fail)
        .arg("--json")
        .output()
        .expect("fail check should run");
    assert_eq!(fail_output.status.code(), Some(2));

    let orchestrate = Command::new(binary_path())
        .arg("orchestrate")
        .arg(&multi)
        .arg("--json")
        .arg("--backend=mock-bmc")
        .output()
        .expect("orchestrate should run");
    assert!(orchestrate.status.success());
    let stdout = String::from_utf8_lossy(&orchestrate.stdout);
    assert!(stdout.contains("\"aggregate_coverage\""));
}

#[test]
fn cli_command_backend_demo_script_normalizes_failures() {
    let fail = repo_path("examples/models/failing_counter.valid");
    let solver = repo_path("examples/solvers/mock_command_solver.sh");

    let output = Command::new(binary_path())
        .arg("check")
        .arg(&fail)
        .arg("--json")
        .arg("--backend=command")
        .arg("--solver-exec")
        .arg("sh")
        .arg("--solver-arg")
        .arg(solver)
        .output()
        .expect("command backend should run");
    assert_eq!(output.status.code(), Some(2));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MOCK_SOLVER_COUNTEREXAMPLE"));
    assert!(stdout.contains("\"status\":\"FAIL\""));
}

#[test]
fn rust_native_examples_run_successfully() {
    for example in [
        "iam_like_authz",
        "iam_policy_diff",
        "train_fare",
        "saas_entitlements",
    ] {
        let output = Command::new("cargo")
            .arg("run")
            .arg("--example")
            .arg(example)
            .output()
            .expect("example should run");
        assert!(
            output.status.success(),
            "example {} failed: {}",
            example,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn native_demo_cli_reports_realistic_systems() {
    for demo in [
        "iam-authz",
        "iam-policy-diff",
        "train-fare",
        "saas-entitlements",
    ] {
        let output = Command::new(binary_path())
            .arg("native-demo")
            .arg(demo)
            .arg("--json")
            .output()
            .expect("native-demo should run");
        assert!(
            output.status.success(),
            "native-demo {} failed: {}",
            demo,
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("\"schema_version\""));
        assert!(stdout.contains("\"demo_id\""));
    }
}
