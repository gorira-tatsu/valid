use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use valid::api::{
    check_source, explain_source, inspect_source, lint_source, orchestrate_source,
    render_inspect_json, testgen_source, CheckRequest, InspectRequest, OrchestrateRequest,
    TestgenRequest,
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

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic enough")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
}

fn cleanup_generated_files(paths: &[String]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
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
        property_id: None,
        strategy: "counterexample".to_string(),
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
    })
    .expect("testgen should succeed");
    assert!(!testgen.vector_ids.is_empty());
    for path in &testgen.generated_files {
        let body = fs::read_to_string(path).expect("generated file must exist");
        assert!(body.contains("assert_replay_output_json"));
    }
    cleanup_generated_files(&testgen.generated_files);
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
fn inspect_includes_metadata_details() {
    let source = read_fixture("examples/models/safe_counter.valid");
    let response = inspect_source(&InspectRequest {
        request_id: "req-test-inspect".to_string(),
        source_name: "safe_counter.valid".to_string(),
        source,
    })
    .expect("inspect should succeed");
    let json = render_inspect_json(&response);
    assert!(response.machine_ir_ready);
    assert!(response.capabilities.solver_ready);
    assert!(response.capabilities.reasons.is_empty());
    assert!(json.contains("\"state_field_details\""));
    assert!(json.contains("\"capabilities\""));
    assert!(json.contains("\"action_details\""));
    assert!(json.contains("\"path_tags\""));
    assert!(json.contains("\"property_details\""));
}

#[test]
fn cli_graph_renders_mermaid_for_valid_model() {
    let safe = repo_path("examples/models/safe_counter.valid");
    let output = Command::new(binary_path())
        .arg("graph")
        .arg(&safe)
        .output()
        .expect("graph should run");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("flowchart LR"));
    assert!(stdout.contains("SafeCounter"));
    assert!(stdout.contains("Inc"));
    assert!(stdout.contains("P_SAFE"));
}

#[test]
fn cli_graph_supports_dot_and_svg_formats() {
    let safe = repo_path("examples/models/safe_counter.valid");
    let dot_output = Command::new(binary_path())
        .arg("graph")
        .arg(&safe)
        .arg("--format=dot")
        .output()
        .expect("graph dot should run");
    assert_eq!(dot_output.status.code(), Some(0));
    let dot = String::from_utf8_lossy(&dot_output.stdout);
    assert!(dot.contains("digraph model"));
    assert!(dot.contains("SafeCounter"));

    let svg_output = Command::new(binary_path())
        .arg("graph")
        .arg(&safe)
        .arg("--format=svg")
        .output()
        .expect("graph svg should run");
    assert_eq!(svg_output.status.code(), Some(0));
    let svg = String::from_utf8_lossy(&svg_output.stdout);
    assert!(svg.contains("<svg"));
    assert!(svg.contains("SafeCounter"));
}

#[test]
fn lint_reports_clean_valid_models() {
    let source = read_fixture("examples/models/safe_counter.valid");
    let response = lint_source(&InspectRequest {
        request_id: "req-test-lint".to_string(),
        source_name: "safe_counter.valid".to_string(),
        source,
    })
    .expect("lint should succeed");
    assert_eq!(response.status, "ok");
    assert!(response.findings.is_empty());
}

#[test]
fn multi_property_testgen_can_target_specific_property() {
    let source = read_fixture("examples/models/multi_property.valid");
    let response = testgen_source(&TestgenRequest {
        request_id: "req-testgen-property".to_string(),
        source_name: "multi_property.valid".to_string(),
        source,
        property_id: Some("P_STRICT".to_string()),
        strategy: "counterexample".to_string(),
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
    })
    .expect("property-specific testgen should succeed");
    assert_eq!(response.vector_ids.len(), 1);
    cleanup_generated_files(&response.generated_files);
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
        .arg("verify")
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
        .output()
        .expect("orchestrate should run");
    assert!(orchestrate.status.success());
    let stdout = String::from_utf8_lossy(&orchestrate.stdout);
    assert!(stdout.contains("\"aggregate_coverage\""));
}

#[test]
fn cli_readiness_and_clean_work() {
    let safe = repo_path("examples/models/safe_counter.valid");
    let temp_root = unique_temp_dir("valid-cli-clean");
    let generated = temp_root.join("tests/generated/valid-clean-sentinel.rs");
    let artifact_dir = temp_root.join("artifacts/valid-clean-sentinel");
    fs::create_dir_all(generated.parent().unwrap()).expect("generated dir");
    fs::create_dir_all(&artifact_dir).expect("artifact dir");
    fs::write(&generated, "// sentinel\n").expect("generated sentinel");
    fs::write(artifact_dir.join("report.json"), "{}\n").expect("artifact sentinel");

    let lint = Command::new(binary_path())
        .arg("readiness")
        .arg(&safe)
        .arg("--json")
        .output()
        .expect("readiness should run");
    assert_eq!(lint.status.code(), Some(0));

    let clean = Command::new(binary_path())
        .current_dir(&temp_root)
        .arg("clean")
        .arg("all")
        .arg("--json")
        .output()
        .expect("clean should run");
    assert!(clean.status.success());
    let stdout = String::from_utf8_lossy(&clean.stdout);
    assert!(stdout.contains("valid-clean-sentinel.rs"));
    assert!(stdout.contains("artifacts/valid-clean-sentinel"));
    assert!(!generated.exists());
    assert!(!artifact_dir.exists());
    let _ = fs::remove_dir_all(temp_root);
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
fn cli_cvc5_backend_demo_script_normalizes_failures() {
    let fail = repo_path("examples/models/failing_counter.valid");
    let solver = repo_path("examples/solvers/mock_cvc5_solver.sh");

    let output = Command::new(binary_path())
        .arg("check")
        .arg(&fail)
        .arg("--json")
        .arg("--backend=smt-cvc5")
        .arg("--solver-exec")
        .arg("sh")
        .arg("--solver-arg")
        .arg(solver)
        .output()
        .expect("cvc5 backend should run");
    assert_eq!(output.status.code(), Some(2));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CVC5_COUNTEREXAMPLE"));
    assert!(stdout.contains("\"status\":\"FAIL\""));
    assert!(stdout.contains("\"backend_name\":\"smt-cvc5\""));
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
fn bundled_rust_models_run_via_main_cli_path() {
    let inspect = Command::new(binary_path())
        .arg("inspect")
        .arg("rust:counter")
        .arg("--json")
        .output()
        .expect("inspect should run");
    assert!(inspect.status.success());
    assert!(String::from_utf8_lossy(&inspect.stdout).contains("\"model_id\":\"CounterModel\""));

    let check = Command::new(binary_path())
        .arg("check")
        .arg("rust:failing-counter")
        .arg("--json")
        .output()
        .expect("check should run");
    assert_eq!(check.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&check.stdout).contains("\"property_id\":\"P_FAIL\""));

    let coverage = Command::new(binary_path())
        .arg("coverage")
        .arg("rust:counter")
        .arg("--json")
        .output()
        .expect("coverage should run");
    assert!(coverage.status.success());
    let coverage_stdout = String::from_utf8_lossy(&coverage.stdout);
    assert!(coverage_stdout.contains("\"model_id\":\"CounterModel\""));
    assert!(coverage_stdout.contains("\"path_tags\""));
}

#[test]
fn main_cli_lints_bundled_step_model() {
    let lint = Command::new(binary_path())
        .arg("lint")
        .arg("rust:counter")
        .arg("--json")
        .output()
        .expect("lint should run");
    assert_eq!(lint.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&lint.stdout);
    assert!(stdout.contains("\"opaque_step_closure\""));
}
