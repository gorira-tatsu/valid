use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;
use valid::api::{
    check_source, distinguish_source, explain_source, inspect_source, lint_source,
    orchestrate_source, render_inspect_json, testgen_source, CheckRequest, DistinguishRequest,
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

fn cargo_valid_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-valid"))
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

fn rewrite_valid_dependency_to_local_path(project_dir: &Path) {
    let cargo_toml_path = project_dir.join("Cargo.toml");
    let cargo_toml = fs::read_to_string(&cargo_toml_path).expect("Cargo.toml must exist");
    let local = format!(
        "valid = {{ path = {:?}, features = [\"verification-runtime\"] }}",
        Path::new(env!("CARGO_MANIFEST_DIR"))
    );
    let rewritten = cargo_toml.replace(
        "valid = { git = \"https://github.com/gorira-tatsu/valid\", branch = \"main\" }",
        &local,
    );
    fs::write(cargo_toml_path, rewritten).expect("Cargo.toml should be rewritten");
}

fn local_valid_dep_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).display().to_string()
}

#[test]
fn safe_counter_passes_via_api() {
    let source = read_fixture("tests/fixtures/models/safe_counter.valid");
    let outcome = check_source(&CheckRequest {
        request_id: "req-test-safe".to_string(),
        source_name: "safe_counter.valid".to_string(),
        source,
        property_id: None,
        scenario_id: None,
        seed: None,
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
    let source = read_fixture("tests/fixtures/models/failing_counter.valid");
    let outcome = check_source(&CheckRequest {
        request_id: "req-test-fail".to_string(),
        source_name: "failing_counter.valid".to_string(),
        source: source.clone(),
        property_id: None,
        scenario_id: None,
        seed: None,
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
        scenario_id: None,
        seed: Some(61),
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
    })
    .expect("explain should succeed");
    assert!(!explain.candidate_causes.is_empty());
    assert!(!explain.repair_hints.is_empty());
    assert!(!explain.repair_targets.is_empty());
    assert!(!explain.changed_fields.is_empty());

    let testgen = testgen_source(&TestgenRequest {
        request_id: "req-test-testgen".to_string(),
        source_name: "failing_counter.valid".to_string(),
        source,
        property_id: None,
        strategy: "counterexample".to_string(),
        focus_action_id: None,
        seed: None,
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
fn cli_testgen_supports_deadlock_strategy() {
    let temp_dir = unique_temp_dir("valid-deadlock-testgen");
    fs::create_dir_all(&temp_dir).expect("temp dir should exist");
    let model_path = temp_dir.join("deadlock.valid");
    fs::write(
        &model_path,
        "model A\nstate:\n  x: u8[0..1]\ninit:\n  x = 0\naction Advance:\n  pre: x == 0\n  post:\n    x = 1\nproperty P_LIVE: deadlock_freedom\n",
    )
    .expect("model should be written");

    let output = Command::new(binary_path())
        .arg("testgen")
        .arg(&model_path)
        .arg("--strategy=deadlock")
        .arg("--json")
        .output()
        .expect("deadlock testgen should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"strategy\":\"deadlock\""));
    assert!(stdout.contains("\"source_kind\":\"deadlock\""));
    assert!(stdout.contains("\"generated_files\":["));
}

#[test]
fn cli_testgen_supports_enablement_strategy() {
    let temp_dir = unique_temp_dir("valid-enablement-testgen");
    fs::create_dir_all(&temp_dir).expect("temp dir should exist");
    let model_path = temp_dir.join("enablement.valid");
    fs::write(
        &model_path,
        "model A\nstate:\n  x: u8[0..1]\ninit:\n  x = 0\naction Enable:\n  pre: x == 0\n  post:\n    x = 1\naction Target:\n  pre: x == 1\n  post:\n    x = 1\nproperty P_SAFE:\n  invariant: x <= 1\n",
    )
    .expect("model should be written");

    let output = Command::new(binary_path())
        .arg("testgen")
        .arg(&model_path)
        .arg("--strategy=enablement")
        .arg("--focus-action=Target")
        .arg("--json")
        .output()
        .expect("enablement testgen should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"strategy\":\"enablement\""));
    assert!(stdout.contains("\"focus_action_id\":\"Target\""));
    assert!(stdout.contains("\"expected_guard_enabled\":true"));
}

#[test]
fn multi_property_orchestrate_returns_aggregate_coverage() {
    let source = read_fixture("tests/fixtures/models/multi_property.valid");
    let response = orchestrate_source(&OrchestrateRequest {
        request_id: "req-test-orchestrate".to_string(),
        source_name: "multi_property.valid".to_string(),
        source,
        seed: None,
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
    })
    .expect("orchestrate should succeed");
    assert_eq!(response.runs.len(), 2);
    assert!(response.aggregate_coverage.is_some());
}

#[test]
fn distinguish_source_finds_divergence_between_models() {
    let left = "\
model ResetCounter
state:
  x: u8[0..2]
init:
  x = 0
action Inc:
  pre: x <= 1
  post:
    x = x + 1
action Reset:
  pre: x <= 2
  post:
    x = 0
";
    let right = "\
model StayCounter
state:
  x: u8[0..2]
init:
  x = 0
action Inc:
  pre: x <= 1
  post:
    x = x + 1
action Reset:
  pre: x <= 2
  post:
    x = x
";
    let response = distinguish_source(&DistinguishRequest {
        request_id: "req-distinguish".to_string(),
        source_name: "left.valid".to_string(),
        source: left.to_string(),
        compare_source_name: Some("right.valid".to_string()),
        compare_source: Some(right.to_string()),
        property_id: None,
        compare_property_id: None,
        max_depth: Some(4),
    })
    .expect("distinguish should succeed");
    assert_eq!(response.comparison_kind, "models");
    assert_eq!(response.trace.divergence_kind, "state_transition");
    assert_eq!(
        response
            .trace
            .checkpoints
            .last()
            .and_then(|item| item.action_id.as_deref()),
        Some("Reset")
    );
}

#[test]
fn inspect_includes_metadata_details() {
    let source = read_fixture("tests/fixtures/models/safe_counter.valid");
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
    assert!(json.contains("\"temporal\":{\"property_ids\":[]"));
}

#[test]
fn cli_capabilities_reports_temporal_backend_details() {
    let output = Command::new(binary_path())
        .arg("capabilities")
        .arg("--backend")
        .arg("mock-bmc")
        .arg("--json")
        .output()
        .expect("capabilities should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"temporal\":{\"status\":\"bounded\""));
    assert!(
        stdout.contains("\"supported_operators\":[\"always\",\"eventually\",\"next\",\"until\"]")
    );
}

#[test]
fn cli_capabilities_reports_sat_backend_availability() {
    let output = Command::new(binary_path())
        .arg("capabilities")
        .arg("--backend")
        .arg("sat-varisat")
        .arg("--json")
        .output()
        .expect("capabilities should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"backend\":\"sat-varisat\""));
    #[cfg(feature = "varisat-backend")]
    {
        assert!(stdout.contains("\"compiled_in\":true"));
        assert!(stdout.contains("\"available\":true"));
    }
    #[cfg(not(feature = "varisat-backend"))]
    {
        assert!(stdout.contains("\"compiled_in\":false"));
        assert!(stdout.contains("\"available\":false"));
        assert!(stdout.contains(
            "\"availability_reason\":\"this binary was built without the varisat-backend feature\""
        ));
        assert!(stdout.contains("\"remediation\":\"reinstall or rebuild valid with `--features varisat-backend`, or use `cargo valid --backend=sat-varisat` so the feature is added automatically\""));
    }
}

#[test]
fn cli_distinguish_reports_divergence_as_json() {
    let temp_root = unique_temp_dir("valid-cli-distinguish");
    fs::create_dir_all(&temp_root).expect("temp root");
    let left = temp_root.join("left.valid");
    let right = temp_root.join("right.valid");
    fs::write(
        &left,
        "\
model ResetCounter
state:
  x: u8[0..2]
init:
  x = 0
action Inc:
  pre: x <= 1
  post:
    x = x + 1
action Reset:
  pre: x <= 2
  post:
    x = 0
",
    )
    .expect("left model");
    fs::write(
        &right,
        "\
model StayCounter
state:
  x: u8[0..2]
init:
  x = 0
action Inc:
  pre: x <= 1
  post:
    x = x + 1
action Reset:
  pre: x <= 2
  post:
    x = x
",
    )
    .expect("right model");
    let output = Command::new(binary_path())
        .arg("distinguish")
        .arg(&left)
        .arg(format!("--compare={}", right.display()))
        .arg("--json")
        .output()
        .expect("distinguish should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"comparison_kind\":\"models\""));
    assert!(stdout.contains("\"divergence_kind\":\"state_transition\""));
}

#[test]
fn cli_check_accepts_scenario_selection() {
    let temp_dir = unique_temp_dir("valid-scenario");
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    let model_path = temp_dir.join("scenario.valid");
    fs::write(
        &model_path,
        "model PostFlow\nstate:\n  visible: bool\n  deleted: bool\ninit:\n  visible = true\n  deleted = false\nscenarios:\n  DeletedPost: deleted == true\naction Delete:\n  pre: visible == true\n  post:\n    visible = false\n    deleted = true\nproperty P_VISIBLE_ONLY_AFTER_DELETE:\n  invariant: visible == false\n",
    )
    .expect("fixture should be written");

    let output = Command::new(binary_path())
        .arg("check")
        .arg(&model_path)
        .arg("--property=P_VISIBLE_ONLY_AFTER_DELETE")
        .arg("--scenario=DeletedPost")
        .arg("--json")
        .output()
        .expect("scenario check should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"scenario_id\":\"DeletedPost\""));
    assert!(stdout.contains("\"vacuous\":false"));

    let _ = fs::remove_file(&model_path);
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn cli_inspect_and_check_support_bounded_action_choices() {
    let temp_dir = unique_temp_dir("valid-bounded-choice");
    let model_path = temp_dir.join("choice.valid");
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    fs::write(
        &model_path,
        "model Counter\nstate:\n  x: u8[0..2]\ninit:\n  x = 0\naction Add:\n  choose delta: 1, 2\n  pre: x + {{delta}} <= 2\n  post:\n    x = x + {{delta}}\nproperty P_REACH_TWO:\n  reachability: x == 2\n",
    )
    .expect("model file should be written");

    let inspect = Command::new(binary_path())
        .arg("inspect")
        .arg(&model_path)
        .arg("--json")
        .output()
        .expect("inspect should run");
    assert!(inspect.status.success());
    let inspect_stdout = String::from_utf8_lossy(&inspect.stdout);
    assert!(inspect_stdout.contains("Add[delta=1]"));
    assert!(inspect_stdout.contains("Add[delta=2]"));

    let check = Command::new(binary_path())
        .arg("check")
        .arg(&model_path)
        .arg("--property=P_REACH_TWO")
        .arg("--json")
        .output()
        .expect("check should run");
    let check_stdout = String::from_utf8_lossy(&check.stdout);
    assert_eq!(check.status.code(), Some(1));
    assert!(check_stdout.contains("\"property_kind\":\"reachability\""));
    assert!(check_stdout.contains("\"reason_code\":\"TARGET_REACHED\""));

    let _ = fs::remove_file(&model_path);
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn cli_artifacts_lists_selfcheck_run_history() {
    let temp_dir = unique_temp_dir("valid-artifacts");
    fs::create_dir_all(&temp_dir).expect("temp dir should be created");

    let selfcheck = Command::new(binary_path())
        .arg("selfcheck")
        .arg("--json")
        .current_dir(&temp_dir)
        .output()
        .expect("selfcheck should run");
    assert!(selfcheck.status.success());

    let output = Command::new(binary_path())
        .arg("artifacts")
        .arg("--json")
        .current_dir(&temp_dir)
        .output()
        .expect("artifacts should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"artifacts\""));
    assert!(stdout.contains("\"runs\""));
    assert!(stdout.contains("\"run_id\": \"selfcheck-local-0001\""));
    assert!(stdout.contains("\"artifact_kind\": \"selfcheck_report\""));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn cli_graph_renders_mermaid_for_valid_model() {
    let safe = repo_path("tests/fixtures/models/safe_counter.valid");
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
    let safe = repo_path("tests/fixtures/models/safe_counter.valid");
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
fn cli_graph_supports_failure_view() {
    let failing = repo_path("tests/fixtures/models/failing_counter.valid");
    let output = Command::new(binary_path())
        .arg("graph")
        .arg(&failing)
        .arg("--view=failure")
        .arg("--property=P_FAIL")
        .output()
        .expect("graph failure view should run");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Failure Slice"));
    assert!(stdout.contains("P_FAIL"));
    assert!(stdout.contains("property P_FAIL fails"));

    let json_output = Command::new(binary_path())
        .arg("graph")
        .arg(&failing)
        .arg("--view=failure")
        .arg("--property=P_FAIL")
        .arg("--format=json")
        .output()
        .expect("graph failure json should run");
    assert_eq!(json_output.status.code(), Some(0));
    let json_stdout = String::from_utf8_lossy(&json_output.stdout);
    assert!(json_stdout.contains("\"graph_view\":\"failure\""));
    assert!(json_stdout.contains("\"graph_slice\""));
    assert!(json_stdout.contains("\"property_id\":\"P_FAIL\""));
}

#[test]
fn cli_graph_supports_deadlock_and_scc_views() {
    let safe = repo_path("tests/fixtures/models/safe_counter.valid");
    let deadlock_output = Command::new(binary_path())
        .arg("graph")
        .arg(&safe)
        .arg("--format=text")
        .arg("--view=deadlock")
        .output()
        .expect("graph deadlock should run");
    assert_eq!(deadlock_output.status.code(), Some(0));
    let deadlock = String::from_utf8_lossy(&deadlock_output.stdout);
    assert!(deadlock.contains("graph_view: deadlock"));

    let scc_output = Command::new(binary_path())
        .arg("graph")
        .arg(&safe)
        .arg("--view=scc")
        .output()
        .expect("graph scc should run");
    assert_eq!(scc_output.status.code(), Some(0));
    let scc = String::from_utf8_lossy(&scc_output.stdout);
    assert!(scc.contains("SCC 0"));
}

#[test]
fn lint_reports_clean_valid_models() {
    let source = read_fixture("tests/fixtures/models/safe_counter.valid");
    let response = lint_source(&InspectRequest {
        request_id: "req-test-lint".to_string(),
        source_name: "safe_counter.valid".to_string(),
        source,
    })
    .expect("lint should succeed");
    assert_eq!(response.status, "warn");
    assert_eq!(response.findings.len(), 1);
    assert_eq!(response.findings[0].category, "maintainability");
    assert_eq!(response.findings[0].code, "missing_model_documentation");
}

#[test]
fn multi_property_testgen_can_target_specific_property() {
    let source = read_fixture("tests/fixtures/models/multi_property.valid");
    let response = testgen_source(&TestgenRequest {
        request_id: "req-testgen-property".to_string(),
        source_name: "multi_property.valid".to_string(),
        source,
        property_id: Some("P_STRICT".to_string()),
        strategy: "counterexample".to_string(),
        focus_action_id: None,
        seed: Some(67),
        backend: None,
        solver_executable: None,
        solver_args: Vec::new(),
    })
    .expect("property-specific testgen should succeed");
    assert_eq!(response.vector_ids.len(), 1);
    cleanup_generated_files(&response.generated_files);
}

#[test]
fn cli_conformance_compares_runner_output_to_spec() {
    let safe = repo_path("tests/fixtures/models/safe_counter.valid");
    let output = Command::new(binary_path())
        .arg("conformance")
        .arg(&safe)
        .arg("--property=P_SAFE")
        .arg("--actions=Inc")
        .arg("--runner=/bin/sh")
        .arg("--runner-arg")
        .arg("-c")
        .arg("--runner-arg")
        .arg("cat >/dev/null; printf '%s' '{\"schema_version\":\"1.0.0\",\"status\":\"ok\",\"observations\":[{\"x\":1}],\"property_holds\":true}'")
        .arg("--json")
        .output()
        .expect("conformance should run");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\":\"PASS\""));
    assert!(stdout.contains("\"mismatch_count\":0"));
    assert!(stdout.contains("\"mismatch_categories\":[]"));
}

#[test]
fn cli_conformance_reports_structured_mismatch_categories() {
    let safe = repo_path("tests/fixtures/models/safe_counter.valid");
    let output = Command::new(binary_path())
        .arg("conformance")
        .arg(&safe)
        .arg("--property=P_SAFE")
        .arg("--actions=Inc")
        .arg("--runner=/bin/sh")
        .arg("--runner-arg")
        .arg("-c")
        .arg("--runner-arg")
        .arg("cat >/dev/null; printf '%s' '{\"schema_version\":\"1.0.0\",\"status\":\"ok\",\"observations\":[],\"property_holds\":false}'")
        .arg("--json")
        .output()
        .expect("conformance should run");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\":\"FAIL\""));
    assert!(stdout.contains("\"mismatch_categories\":[\"output\",\"property\"]"));
    assert!(stdout.contains("\"kind\":\"output\""));
    assert!(stdout.contains("\"kind\":\"property\""));
}

#[test]
fn cli_conformance_text_output_names_mismatch_categories() {
    let safe = repo_path("tests/fixtures/models/safe_counter.valid");
    let output = Command::new(binary_path())
        .arg("conformance")
        .arg(&safe)
        .arg("--property=P_SAFE")
        .arg("--actions=Inc")
        .arg("--runner=/bin/sh")
        .arg("--runner-arg")
        .arg("-c")
        .arg("--runner-arg")
        .arg("cat >/dev/null; printf '%s' '{\"schema_version\":\"1.0.0\",\"status\":\"ok\",\"observations\":[],\"property_holds\":false}'")
        .output()
        .expect("conformance should run");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("mismatch_categories: output,property"));
    assert!(stdout.contains("mismatch output fix_surface=implementation_output step=0"));
    assert!(stdout.contains("mismatch property fix_surface=implementation_or_model"));
}

#[test]
fn parse_and_type_errors_are_visible_via_api() {
    let parse_diagnostics = inspect_source(&InspectRequest {
        request_id: "req-test-parse".to_string(),
        source_name: "parse_error.valid".to_string(),
        source: read_fixture("tests/fixtures/models/parse_error.valid"),
    })
    .expect_err("parse fixture must fail");
    assert_eq!(parse_diagnostics[0].error_code.as_str(), "PARSE_ERROR");

    let type_outcome = check_source(&CheckRequest {
        request_id: "req-test-type".to_string(),
        source_name: "type_error.valid".to_string(),
        source: read_fixture("tests/fixtures/models/type_error.valid"),
        property_id: None,
        scenario_id: None,
        seed: None,
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
    let safe = repo_path("tests/fixtures/models/safe_counter.valid");
    let fail = repo_path("tests/fixtures/models/failing_counter.valid");
    let multi = repo_path("tests/fixtures/models/multi_property.valid");

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
    assert_eq!(fail_output.status.code(), Some(1));

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
    let safe = repo_path("tests/fixtures/models/safe_counter.valid");
    let temp_root = unique_temp_dir("valid-cli-clean");
    let generated = temp_root.join("generated-tests/valid-clean-sentinel.rs");
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
    assert_eq!(lint.status.code(), Some(1));
    let lint_stdout = String::from_utf8_lossy(&lint.stdout);
    assert!(lint_stdout.contains("\"severity\":\"warn\""));

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
    let fail = repo_path("tests/fixtures/models/failing_counter.valid");
    let solver = repo_path("tests/fixtures/solvers/mock_command_solver.sh");

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
    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("MOCK_SOLVER_COUNTEREXAMPLE"));
    assert!(stdout.contains("\"status\":\"FAIL\""));
}

#[test]
fn cli_cvc5_backend_demo_script_normalizes_failures() {
    let fail = repo_path("tests/fixtures/models/failing_counter.valid");
    let solver = repo_path("tests/fixtures/solvers/mock_cvc5_solver.sh");

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
    assert_eq!(output.status.code(), Some(1));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("CVC5_COUNTEREXAMPLE"));
    assert!(stdout.contains("\"status\":\"FAIL\""));
    assert!(stdout.contains("\"backend_name\":\"smt-cvc5\""));
}

#[test]
fn small_registry_examples_run_successfully() {
    for example in [
        "valid_models",
        "fizzbuzz",
        "iam_transition_registry",
        "saas_multi_tenant_registry",
    ] {
        let output = Command::new("cargo")
            .arg("run")
            .arg("--features")
            .arg("verification-runtime")
            .arg("--example")
            .arg(example)
            .arg("--")
            .arg("models")
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
    assert_eq!(check.status.code(), Some(1));
    let check_stdout = String::from_utf8_lossy(&check.stdout);
    assert!(check_stdout.contains("\"kind\":\"completed\""));
    assert!(check_stdout.contains("\"property_id\":\"P_FAIL\""));
    assert!(check_stdout.contains("\"traceback\""));
    assert!(check_stdout.contains("\"changed_fields\""));
    assert!(check_stdout.contains("\"breakpoint_kind\""));

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
fn main_cli_doc_check_reports_drift_structurally() {
    let model = repo_path("tests/fixtures/models/safe_counter.valid");
    let output_path = std::env::temp_dir().join(format!(
        "valid-doc-check-{}.md",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let first = Command::new(binary_path())
        .arg("doc")
        .arg(&model)
        .arg(format!("--write={}", output_path.display()))
        .arg("--json")
        .output()
        .expect("doc generation should run");
    assert!(first.status.success());
    let check = Command::new(binary_path())
        .arg("doc")
        .arg(&model)
        .arg(format!("--write={}", output_path.display()))
        .arg("--check")
        .arg("--json")
        .output()
        .expect("doc check should run");
    assert_eq!(check.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("\"status\":\"unchanged\""));

    let mut body = std::fs::read_to_string(&output_path).expect("doc body");
    body.push_str("\nmanual drift\n");
    std::fs::write(&output_path, body).expect("doc drift written");

    let drift = Command::new(binary_path())
        .arg("doc")
        .arg(&model)
        .arg(format!("--write={}", output_path.display()))
        .arg("--check")
        .arg("--json")
        .output()
        .expect("doc drift check should run");
    assert_eq!(drift.status.code(), Some(2));
    let drift_stdout = String::from_utf8_lossy(&drift.stdout);
    assert!(drift_stdout.contains("\"status\":\"changed\""));
    assert!(drift_stdout.contains("\"drift_sections\""));
    let _ = std::fs::remove_file(output_path);
}

#[test]
fn main_cli_handoff_check_reports_drift_structurally() {
    let model = repo_path("tests/fixtures/models/safe_counter.valid");
    let output_path = std::env::temp_dir().join(format!(
        "valid-handoff-check-{}.md",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let first = Command::new(binary_path())
        .arg("handoff")
        .arg(&model)
        .arg("--property=P_SAFE")
        .arg(format!("--write={}", output_path.display()))
        .arg("--json")
        .output()
        .expect("handoff generation should run");
    assert!(first.status.success());
    let first_stdout = String::from_utf8_lossy(&first.stdout);
    assert!(first_stdout.contains("\"model_id\":\"SafeCounter\""));
    assert!(first_stdout.contains("\"contract_hash\""));
    assert!(first_stdout.contains("\"testgen_summary\""));
    assert!(first_stdout.contains("\"markdown\""));
    assert!(first_stdout.contains("Recommended Test Vectors"));

    let unchanged = Command::new(binary_path())
        .arg("handoff")
        .arg(&model)
        .arg("--property=P_SAFE")
        .arg(format!("--write={}", output_path.display()))
        .arg("--check")
        .arg("--json")
        .output()
        .expect("handoff check should run");
    assert_eq!(unchanged.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&unchanged.stdout).contains("\"status\":\"unchanged\""));

    let mut body = std::fs::read_to_string(&output_path).expect("handoff body");
    body.push_str("\nmanual drift\n");
    std::fs::write(&output_path, body).expect("handoff drift written");

    let changed = Command::new(binary_path())
        .arg("handoff")
        .arg(&model)
        .arg("--property=P_SAFE")
        .arg(format!("--write={}", output_path.display()))
        .arg("--check")
        .arg("--json")
        .output()
        .expect("handoff drift check should run");
    assert_eq!(changed.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&changed.stdout);
    assert!(stdout.contains("\"status\":\"changed\""));
    assert!(stdout.contains("\"drift_sections\""));
    let _ = std::fs::remove_file(output_path);
}

#[test]
fn main_cli_lints_bundled_step_model() {
    let lint = Command::new(binary_path())
        .arg("lint")
        .arg("rust:counter")
        .arg("--json")
        .output()
        .expect("lint should run");
    assert_eq!(lint.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&lint.stdout);
    assert!(stdout.contains("\"opaque_step_closure\""));
}

#[test]
fn cli_commands_and_schema_are_machine_readable() {
    let commands = Command::new(binary_path())
        .arg("commands")
        .arg("--json")
        .output()
        .expect("commands should run");
    assert!(commands.status.success());
    let commands_stdout = String::from_utf8_lossy(&commands.stdout);
    assert!(commands_stdout.contains("\"surface\":\"valid\""));
    assert!(commands_stdout.contains("\"name\":\"check\""));
    assert!(commands_stdout.contains("\"name\":\"completion\""));
    assert!(commands_stdout.contains("\"name\":\"mcp\""));
    assert!(commands_stdout.contains("\"response\":\"schema.cli.completed\""));

    let schema = Command::new(binary_path())
        .arg("schema")
        .arg("check")
        .output()
        .expect("schema should run");
    assert!(schema.status.success());
    let schema_stdout = String::from_utf8_lossy(&schema.stdout);
    assert!(schema_stdout.contains("\"command\":\"check\""));
    assert!(schema_stdout.contains("\"parameter_schema_id\":\"schema.cli.valid.check.parameters\""));
    assert!(schema_stdout.contains("\"response_schema_id\":\"schema.cli.completed\""));
    assert!(schema_stdout.contains("\"error_schema_id\":\"schema.cli.error\""));
}

#[test]
fn cli_can_generate_shell_completions() {
    for (shell, marker) in [
        ("fish", "complete -c valid"),
        ("bash", "_valid()"),
        ("zsh", "#compdef valid"),
    ] {
        let output = Command::new(binary_path())
            .arg("completion")
            .arg(shell)
            .output()
            .expect("completion should run");
        assert!(output.status.success(), "{shell} completion should succeed");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(marker),
            "{shell} completion should contain {marker}"
        );
    }

    let fish = Command::new(binary_path())
        .arg("completion")
        .arg("fish")
        .output()
        .expect("fish completion should run");
    let fish_stdout = String::from_utf8_lossy(&fish.stdout);
    assert!(fish_stdout.contains("valid completion candidates models 2>/dev/null"));
    assert!(fish_stdout.contains("valid completion candidates properties $model 2>/dev/null"));
    assert!(fish_stdout.contains("valid completion candidates actions $model 2>/dev/null"));
    assert!(fish_stdout.contains("valid completion candidates views 2>/dev/null"));

    let bash = Command::new(binary_path())
        .arg("completion")
        .arg("bash")
        .output()
        .expect("bash completion should run");
    let bash_stdout = String::from_utf8_lossy(&bash.stdout);
    assert!(bash_stdout.contains("__valid_completion_model()"));
    assert!(bash_stdout.contains("valid completion candidates properties \"$model\" 2>/dev/null"));
    assert!(bash_stdout.contains("valid completion candidates actions \"$model\" 2>/dev/null"));
    assert!(bash_stdout.contains("valid completion candidates views 2>/dev/null"));

    let zsh = Command::new(binary_path())
        .arg("completion")
        .arg("zsh")
        .output()
        .expect("zsh completion should run");
    let zsh_stdout = String::from_utf8_lossy(&zsh.stdout);
    assert!(zsh_stdout.contains("__valid_completion_model()"));
    assert!(zsh_stdout.contains("valid completion candidates properties \"$model\" 2>/dev/null"));
    assert!(zsh_stdout.contains("valid completion candidates actions \"$model\" 2>/dev/null"));
    assert!(zsh_stdout.contains("valid completion candidates views 2>/dev/null"));
}

#[test]
fn cli_can_install_shell_completions() {
    let temp_home = unique_temp_dir("valid-completion-install");
    fs::create_dir_all(&temp_home).expect("temp home should exist");
    let output = Command::new(binary_path())
        .arg("completion")
        .arg("install")
        .arg("zsh")
        .arg("--shell-config")
        .arg("--json")
        .env("HOME", &temp_home)
        .output()
        .expect("completion install should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\":\"installed\""));
    assert!(stdout.contains("\"shell\":\"zsh\""));
    let completion_path = temp_home.join(".zsh/completions/_valid");
    let rc_path = temp_home.join(".zshrc");
    assert!(completion_path.exists());
    assert!(rc_path.exists());
    let completion = fs::read_to_string(completion_path).expect("completion file must exist");
    assert!(completion.contains("#compdef valid"));
    let rc = fs::read_to_string(rc_path).expect("zshrc must exist");
    assert!(rc.contains("fpath=(~/.zsh/completions $fpath)"));
    assert!(rc.contains("autoload -Uz compinit && compinit"));
}

#[test]
fn cli_init_bootstraps_new_project() {
    let project_dir = unique_temp_dir("valid-cli-init");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let output = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .current_dir(&project_dir)
        .arg("init")
        .arg("--json")
        .output()
        .expect("valid init should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\":\"ok\""));
    assert!(stdout.contains("\"cargo_init_ran\":true"));
    assert!(stdout.contains("\"registry\":\"valid/registry.rs\""));
    assert!(project_dir.join("Cargo.toml").exists());
    assert!(project_dir.join("src").join("main.rs").exists());
    assert!(project_dir.join("valid").join("registry.rs").exists());
    assert!(project_dir
        .join("valid")
        .join("models")
        .join("mod.rs")
        .exists());
    assert!(project_dir
        .join("valid")
        .join("models")
        .join("approval.rs")
        .exists());
    assert!(project_dir
        .join("docs")
        .join("rdd")
        .join("README.md")
        .exists());

    let main_rs =
        fs::read_to_string(project_dir.join("src").join("main.rs")).expect("main.rs must exist");
    let cargo_toml = fs::read_to_string(project_dir.join("Cargo.toml")).expect("Cargo.toml");
    assert!(main_rs.contains("#[path = \"../valid/registry.rs\"]"));
    assert!(main_rs.contains("valid_registry::run()"));
    assert!(cargo_toml.contains(
        "valid = { git = \"https://github.com/gorira-tatsu/valid\", branch = \"main\" }"
    ));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_init_preserves_existing_main() {
    let project_dir = unique_temp_dir("valid-cli-init-existing");
    fs::create_dir_all(project_dir.join("src")).expect("src dir should exist");
    fs::write(
        project_dir.join("Cargo.toml"),
        "[package]\nname = \"valid-init-existing\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("Cargo.toml");
    fs::write(
        project_dir.join("src").join("main.rs"),
        "fn main() { println!(\"custom\"); }\n",
    )
    .expect("main.rs");

    let output = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .current_dir(&project_dir)
        .arg("init")
        .arg("--json")
        .output()
        .expect("valid init should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"cargo_init_ran\":false"));
    assert!(stdout.contains("src/main.rs"));

    let main_rs =
        fs::read_to_string(project_dir.join("src").join("main.rs")).expect("main.rs must exist");
    let cargo_toml = fs::read_to_string(project_dir.join("Cargo.toml")).expect("Cargo.toml");
    assert_eq!(main_rs, "fn main() { println!(\"custom\"); }\n");
    assert!(cargo_toml.contains(
        "valid = { git = \"https://github.com/gorira-tatsu/valid\", branch = \"main\" }"
    ));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_init_check_reports_ok_for_fresh_scaffold() {
    let project_dir = unique_temp_dir("valid-cli-init-check");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let init = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .current_dir(&project_dir)
        .arg("init")
        .arg("--json")
        .output()
        .expect("valid init should run");
    assert!(init.status.success());

    let check = Command::new(binary_path())
        .current_dir(&project_dir)
        .arg("init")
        .arg("--check")
        .arg("--json")
        .output()
        .expect("valid init --check should run");
    assert!(check.status.success());
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("\"status\":\"ok\""));
    assert!(stdout.contains("\"cargo_project_detected\":true"));
    assert!(stdout.contains("\"valid_toml_detected\":true"));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_init_check_reports_missing_scaffold_files() {
    let project_dir = unique_temp_dir("valid-cli-init-check-missing");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let init = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .current_dir(&project_dir)
        .arg("init")
        .arg("--json")
        .output()
        .expect("valid init should run");
    assert!(init.status.success());
    fs::remove_file(project_dir.join("valid").join("registry.rs"))
        .expect("registry should be removed");

    let check = Command::new(binary_path())
        .current_dir(&project_dir)
        .arg("init")
        .arg("--check")
        .arg("--json")
        .output()
        .expect("valid init --check should run");
    assert!(check.status.success());
    let stdout = String::from_utf8_lossy(&check.stdout);
    assert!(stdout.contains("\"status\":\"warn\""));
    assert!(stdout.contains("valid/registry.rs"));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_init_text_output_recommends_next_commands() {
    let project_dir = unique_temp_dir("valid-cli-init-next-steps");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let init = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .current_dir(&project_dir)
        .arg("init")
        .output()
        .expect("valid init should run");
    assert!(init.status.success());
    let stdout = String::from_utf8_lossy(&init.stdout);
    assert!(stdout.contains("Next Steps"));
    assert!(stdout.contains("valid init --check"));
    assert!(stdout.contains("cargo valid models"));
    assert!(stdout.contains("cargo valid inspect approval-model"));
    assert!(stdout.contains("cargo valid handoff approval-model"));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_init_generated_project_reaches_cargo_valid_models() {
    let project_dir = unique_temp_dir("valid-cli-init-smoke");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let init = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .current_dir(&project_dir)
        .arg("init")
        .arg("--json")
        .output()
        .expect("valid init should run");
    assert!(init.status.success());

    rewrite_valid_dependency_to_local_path(&project_dir);
    let output = Command::new(cargo_valid_binary_path())
        .current_dir(&project_dir)
        .env("CARGO_NET_OFFLINE", "true")
        .arg("models")
        .output()
        .expect("cargo-valid models should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("approval-model"));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_doctor_reports_ok_for_fresh_scaffold() {
    let project_dir = unique_temp_dir("valid-cli-doctor-ok");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let init = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .env("VALID_LOCAL_DEP_PATH", local_valid_dep_path())
        .current_dir(&project_dir)
        .arg("init")
        .arg("--json")
        .output()
        .expect("valid init should run");
    assert!(init.status.success());

    let output = Command::new(binary_path())
        .current_dir(&project_dir)
        .arg("doctor")
        .arg("--json")
        .output()
        .expect("valid doctor should run");
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("doctor json");
    let check_ids = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|check| check["check_id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(check_ids.contains(&"shell_path"));
    assert!(check_ids.contains(&"shell_completion"));
    assert!(check_ids.contains(&"mcp_project_readiness"));
    assert!(check_ids.contains(&"publish_readiness"));
    let scaffold = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["check_id"] == "project_scaffold")
        .expect("project scaffold check");
    assert_eq!(scaffold["status"], "ok");
    let publish = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["check_id"] == "publish_readiness")
        .expect("publish readiness check");
    assert_eq!(publish["status"], "ok");

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_doctor_reports_repair_hint_for_missing_scaffold_files() {
    let project_dir = unique_temp_dir("valid-cli-doctor-broken");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let init = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .env("VALID_LOCAL_DEP_PATH", local_valid_dep_path())
        .current_dir(&project_dir)
        .arg("init")
        .arg("--json")
        .output()
        .expect("valid init should run");
    assert!(init.status.success());
    fs::remove_file(project_dir.join(".mcp").join("codex.toml"))
        .expect("codex config should be removed");

    let output = Command::new(binary_path())
        .current_dir(&project_dir)
        .arg("doctor")
        .arg("--json")
        .output()
        .expect("valid doctor should run");
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("doctor json");
    assert_eq!(report["status"], "warn");
    let scaffold = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["check_id"] == "project_scaffold")
        .expect("project scaffold check");
    assert_eq!(scaffold["status"], "warn");
    assert!(scaffold["repair_hint"]
        .as_str()
        .unwrap()
        .contains("valid init --repair"));
    let mcp = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["check_id"] == "mcp_project_readiness")
        .expect("mcp project readiness check");
    assert_eq!(mcp["status"], "ok");

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_doctor_text_output_groups_non_ok_hints() {
    let project_dir = unique_temp_dir("valid-cli-doctor-hints");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let init = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .env("VALID_LOCAL_DEP_PATH", local_valid_dep_path())
        .current_dir(&project_dir)
        .arg("init")
        .arg("--json")
        .output()
        .expect("valid init should run");
    assert!(init.status.success());
    fs::remove_file(project_dir.join(".mcp").join("codex.toml"))
        .expect("codex config should be removed");

    let output = Command::new(binary_path())
        .current_dir(&project_dir)
        .arg("doctor")
        .output()
        .expect("valid doctor should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Hints"));
    assert!(stdout.contains("project_scaffold:"));
    assert!(stdout.contains("valid init --repair"));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_init_repair_restores_missing_scaffold_files_without_rewriting_registry() {
    let project_dir = unique_temp_dir("valid-cli-init-repair");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let init = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .env("VALID_LOCAL_DEP_PATH", local_valid_dep_path())
        .current_dir(&project_dir)
        .arg("init")
        .arg("--json")
        .output()
        .expect("valid init should run");
    assert!(init.status.success());

    fs::remove_file(project_dir.join(".mcp").join("codex.toml"))
        .expect("codex config should be removed");
    fs::remove_file(project_dir.join("docs").join("ai").join("bootstrap.md"))
        .expect("bootstrap readme should be removed");
    fs::write(
        project_dir.join("valid.toml"),
        fs::read_to_string(project_dir.join("valid.toml"))
            .expect("valid.toml should exist")
            .replace(
                "registry = \"valid/registry.rs\"",
                "registry = \"examples/alt_registry.rs\"",
            ),
    )
    .expect("valid.toml should be rewritten");

    let output = Command::new(binary_path())
        .current_dir(&project_dir)
        .arg("init")
        .arg("--repair")
        .arg("--json")
        .output()
        .expect("valid init --repair should run");
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("repair json");
    assert_eq!(report["status"], "warn");
    assert!(project_dir.join(".mcp").join("codex.toml").exists());
    assert!(project_dir
        .join("docs")
        .join("ai")
        .join("bootstrap.md")
        .exists());
    assert!(report["remaining_warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item.as_str().unwrap().contains("valid.toml registry")));

    let valid_toml = fs::read_to_string(project_dir.join("valid.toml")).expect("valid.toml");
    assert!(valid_toml.contains("registry = \"examples/alt_registry.rs\""));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_onboarding_bootstraps_empty_project_non_interactively() {
    let project_dir = unique_temp_dir("valid-cli-onboarding-fresh");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let output = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .env("VALID_LOCAL_DEP_PATH", local_valid_dep_path())
        .current_dir(&project_dir)
        .arg("onboarding")
        .arg("--non-interactive")
        .arg("--json")
        .output()
        .expect("valid onboarding should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("onboarding json");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["interactive"], false);
    assert_eq!(report["cargo_project_detected"], false);
    assert_eq!(report["valid_project_detected"], false);
    assert_eq!(report["stages"][1]["stage_id"], "bootstrap_project");
    assert_eq!(report["stages"][1]["status"], "success");
    assert_eq!(report["stages"][3]["stage_id"], "warm_project_build");
    assert_eq!(report["stages"][3]["status"], "success");
    assert_eq!(report["stages"][7]["stage_id"], "handoff_starter_model");
    assert_eq!(report["stages"][7]["status"], "success");
    assert!(report["next_path_summaries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["path_id"] == "connect_mcp"));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_onboarding_text_output_recaps_first_value() {
    let project_dir = unique_temp_dir("valid-cli-onboarding-text");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let output = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .env("VALID_LOCAL_DEP_PATH", local_valid_dep_path())
        .current_dir(&project_dir)
        .arg("onboarding")
        .arg("--non-interactive")
        .output()
        .expect("valid onboarding should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("You Now Have"));
    assert!(stdout.contains("approval-model"));
    assert!(stdout.contains("Recap Commands"));
    assert!(stdout.contains("cargo build --quiet"));
    assert!(stdout.contains("cargo valid models"));
    assert!(stdout.contains("cargo valid inspect approval-model"));
    assert!(stdout.contains("cargo valid handoff approval-model"));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_onboarding_interactive_shows_command_output_before_next_prompt() {
    let project_dir = unique_temp_dir("valid-cli-onboarding-interactive");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let mut child = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .env("VALID_LOCAL_DEP_PATH", local_valid_dep_path())
        .current_dir(&project_dir)
        .arg("onboarding")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("valid onboarding should spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin should be available")
        .write_all(b"\n\n\n\n\n\n\n\n")
        .expect("interactive prompts should accept newlines");

    let output = child
        .wait_with_output()
        .expect("valid onboarding should complete");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("stdout:"));
    assert!(stdout.contains("exit_code: 0"));
    assert!(stdout.contains("Press Enter for the next step"));
    assert!(stdout.contains("command: cargo build --quiet"));
    assert!(stdout.contains("command: cargo valid inspect approval-model"));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_onboarding_skips_init_for_existing_scaffold() {
    let project_dir = unique_temp_dir("valid-cli-onboarding-existing");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let init = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .env("VALID_LOCAL_DEP_PATH", local_valid_dep_path())
        .current_dir(&project_dir)
        .arg("init")
        .arg("--json")
        .output()
        .expect("valid init should run");
    assert!(init.status.success());

    let output = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .env("VALID_LOCAL_DEP_PATH", local_valid_dep_path())
        .current_dir(&project_dir)
        .arg("onboarding")
        .arg("--non-interactive")
        .arg("--json")
        .output()
        .expect("valid onboarding should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("onboarding json");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["cargo_project_detected"], true);
    assert_eq!(report["valid_project_detected"], true);
    assert_eq!(report["stages"][1]["stage_id"], "bootstrap_project");
    assert_eq!(report["stages"][1]["status"], "skipped");
    assert_eq!(report["stages"][3]["stage_id"], "warm_project_build");
    assert_eq!(report["stages"][3]["status"], "success");

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_onboarding_reports_partial_failure_for_broken_scaffold() {
    let project_dir = unique_temp_dir("valid-cli-onboarding-broken");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let init = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .env("VALID_LOCAL_DEP_PATH", local_valid_dep_path())
        .current_dir(&project_dir)
        .arg("init")
        .arg("--json")
        .output()
        .expect("valid init should run");
    assert!(init.status.success());
    fs::remove_file(project_dir.join("valid").join("registry.rs"))
        .expect("registry should be removed");

    let output = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .env("VALID_LOCAL_DEP_PATH", local_valid_dep_path())
        .current_dir(&project_dir)
        .arg("onboarding")
        .arg("--non-interactive")
        .arg("--json")
        .output()
        .expect("valid onboarding should run");
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).expect("onboarding json");
    assert_eq!(report["status"], "partial");
    assert_eq!(report["stages"][2]["stage_id"], "check_scaffold");
    assert_eq!(report["stages"][2]["status"], "error");
    assert!(report["stages"][2]["repair_hint"]
        .as_str()
        .expect("repair hint")
        .contains("valid doctor"));
    assert!(report["stages"][2]["repair_hint"]
        .as_str()
        .expect("repair hint")
        .contains("valid init --repair"));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_plain_flag_keeps_doctor_output_copy_pasteable() {
    let project_dir = unique_temp_dir("valid-cli-doctor-plain");
    fs::create_dir_all(&project_dir).expect("project dir should exist");

    let init = Command::new(binary_path())
        .env("CARGO_NET_OFFLINE", "true")
        .env("VALID_LOCAL_DEP_PATH", local_valid_dep_path())
        .current_dir(&project_dir)
        .arg("init")
        .arg("--json")
        .output()
        .expect("valid init should run");
    assert!(init.status.success());

    let output = Command::new(binary_path())
        .current_dir(&project_dir)
        .arg("--plain")
        .arg("doctor")
        .output()
        .expect("valid doctor --plain should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("[OK]"));
    assert!(stdout.contains("root:"));
    assert!(!stdout.contains("\u{1b}["));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cli_scenario_and_cover_examples_are_reviewable() {
    let scenario_output = Command::new(binary_path())
        .arg("check")
        .arg(repo_path("examples/scenario_focus.valid"))
        .arg("--scenario=DeletedPost")
        .arg("--json")
        .output()
        .expect("scenario example should run");
    assert!(scenario_output.status.success());
    let scenario_stdout = String::from_utf8_lossy(&scenario_output.stdout);
    assert!(scenario_stdout.contains("\"scenario_id\":\"DeletedPost\""));
    assert!(scenario_stdout.contains("\"property_id\":\"P_NOT_FOUND_WHEN_DELETED\""));

    let cover_output = Command::new(binary_path())
        .arg("check")
        .arg(repo_path("examples/cover_review.valid"))
        .arg("--property=C_RECOVERED_PATH")
        .arg("--json")
        .output()
        .expect("cover example should run");
    assert!(cover_output.status.success());
    let cover_stdout = String::from_utf8_lossy(&cover_output.stdout);
    assert!(cover_stdout.contains("\"property_id\":\"C_RECOVERED_PATH\""));
}

#[test]
fn cli_json_errors_go_to_stderr() {
    let missing = repo_path("tests/fixtures/models/does-not-exist.valid");
    let output = Command::new(binary_path())
        .arg("check")
        .arg(&missing)
        .arg("--json")
        .output()
        .expect("json error case should run");
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stdout).trim().is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("\"kind\":\"cli_error\""));
    assert!(stderr.contains("\"command\":\"check\""));
    assert!(stderr.contains("failed to read"));
}

#[test]
fn cli_batch_runs_multiple_operations() {
    let safe = repo_path("tests/fixtures/models/safe_counter.valid");
    let fail = repo_path("tests/fixtures/models/failing_counter.valid");
    let request = format!(
        "{{\"schema_version\":\"1.0.0\",\"continue_on_error\":true,\"operations\":[{{\"command\":\"check\",\"args\":[\"{}\"],\"json\":true}},{{\"command\":\"check\",\"args\":[\"{}\"],\"json\":true}}]}}",
        safe.display(),
        fail.display()
    );
    let mut child = Command::new(binary_path())
        .arg("batch")
        .arg("--json")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("batch should spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(request.as_bytes())
        .expect("write batch request");
    let output = child.wait_with_output().expect("batch should complete");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\":\"FAIL\""));
    assert!(stdout.contains("\"command\":\"check\""));
    assert!(stdout.contains("\"exit_code\":0"));
    assert!(stdout.contains("\"exit_code\":1"));
}
