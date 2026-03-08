use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn cargo_valid_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cargo-valid"))
}

fn cargo_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn cargo_guard() -> std::sync::MutexGuard<'static, ()> {
    cargo_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")
}

fn example_registry_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("valid_models.rs")
}

fn fizzbuzz_registry_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("fizzbuzz.rs")
}

fn saas_registry_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("saas_multi_tenant_registry.rs")
}

fn tenant_relation_registry_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("tenant_relation_registry.rs")
}

fn password_policy_registry_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("password_policy.rs")
}

fn cleanup_generated_files(stdout: &str) {
    for path in stdout
        .split('"')
        .filter(|entry| entry.starts_with("generated-tests/") && entry.ends_with(".rs"))
    {
        let _ = fs::remove_file(path);
    }
}

fn extract_generated_files(stdout: &str) -> Vec<String> {
    stdout
        .split('"')
        .filter(|entry| entry.starts_with("generated-tests/") && entry.ends_with(".rs"))
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
            "[package]\nname = \"valid-autodiscover-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nvalid = {{ path = {:?}, features = [\"verification-runtime\"] }}\n",
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

fn write_multi_property_fixture(project_dir: &Path, model_name: &str) {
    fs::create_dir_all(project_dir.join("examples")).expect("temp examples dir");
    fs::write(
        project_dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"valid-suite-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nvalid = {{ path = {:?}, features = [\"verification-runtime\"] }}\n",
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
        retries: u8 [range = "0..=2"],
    }}
}}

valid_actions! {{
    enum Action {{
        Enable => "ENABLE" [reads = ["ready"], writes = ["ready"]],
        Retry => "RETRY" [reads = ["retries"], writes = ["retries"]],
    }}
}}

valid_model! {{
    model AutoDiscoverModel<State, Action>;
    init [State {{ ready: false, retries: 0 }}];
    transitions {{
        transition Enable [tags = ["allow_path"]] when |state| state.ready == false => [State {{ ready: true, ..state }}];
        transition Retry [role = setup] [tags = ["setup_path"]] when |state| state.retries < 2 => [State {{ retries: state.retries + 1, ..state }}];
    }}
    properties {{
        invariant P_READY_BOOLEAN |state| state.ready == false || state.ready == true;
        invariant P_RETRIES_BOUNDED |state| state.retries <= 2;
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
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("models")
        .arg("--json")
        .output()
        .expect("cargo-valid list should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"counter\""));
    assert!(stdout.contains("\"failing-counter\""));
}

#[test]
fn cargo_subcommand_style_prefix_is_accepted() {
    let _guard = cargo_guard();
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
    assert!(stdout.contains("\"counter\""));
}

#[test]
fn cargo_valid_registry_flag_alias_works() {
    let _guard = cargo_guard();
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
    let _guard = cargo_guard();
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
    assert!(stdout.contains("\"temporal\":{\"property_ids\":[]"));
    assert!(stdout.contains("\"state_field_details\""));
    assert!(stdout.contains("\"action_details\""));
    assert!(stdout.contains("\"transition_details\""));
}

#[test]
fn cargo_valid_inspects_fizzbuzz_as_solver_ready() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(fizzbuzz_registry_file())
        .arg("inspect")
        .arg("fizzbuzz")
        .arg("--json")
        .output()
        .expect("cargo-valid inspect fizzbuzz should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"machine_ir_ready\":true"));
    assert!(stdout.contains("\"solver_ready\":true"));
    assert!(stdout.contains("\"P_FIZZBUZZ_DIVISIBLE_BY_BOTH\""));
    assert!(stdout.contains("\"guard\":\"state.i < 15 && (state.i + 1) % 15 == 0\""));
}

#[test]
fn cargo_valid_inspects_grouped_saas_registry() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(saas_registry_file())
        .arg("inspect")
        .arg("tenant-isolation-safe")
        .arg("--json")
        .output()
        .expect("cargo-valid inspect saas registry should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"machine_ir_ready\":true"));
    assert!(stdout.contains("\"solver_ready\":true"));
    assert!(stdout.contains("\"tenant_isolation_path\""));
    assert!(stdout.contains("\"P_NO_CROSS_TENANT_ACCESS\""));
}

#[test]
fn cargo_valid_inspects_relation_and_map_registry() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(tenant_relation_registry_file())
        .arg("inspect")
        .arg("tenant-relation-safe")
        .arg("--json")
        .output()
        .expect("cargo-valid inspect tenant relation registry should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"machine_ir_ready\":true"));
    assert!(stdout.contains("\"solver_ready\":true"));
    assert!(stdout.contains("\"rust_type\":\"FiniteRelation<Member, Tenant>\""));
    assert!(stdout.contains("\"rust_type\":\"FiniteMap<Tenant, Plan>\""));
    assert!(stdout.contains("\"left:Alice|Bob\""));
    assert!(stdout.contains("\"right:Alpha|Beta\""));
    assert!(stdout.contains("\"keys:Alpha|Beta\""));
    assert!(stdout.contains("\"values:Free|Enterprise\""));
    assert!(stdout.contains("membership_path"));
    assert!(stdout.contains("tenant_isolation_path"));
}

#[test]
fn cargo_valid_verifies_relation_map_regression() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(tenant_relation_registry_file())
        .arg("verify")
        .arg("tenant-relation-regression")
        .arg("--property=P_NO_CROSS_TENANT_ACCESS")
        .arg("--json")
        .output()
        .expect("cargo-valid verify tenant relation regression should run");
    assert!(output.status.code() == Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\":\"FAIL\""));
    assert!(stdout.contains("\"property_id\":\"P_NO_CROSS_TENANT_ACCESS\""));
}

#[test]
fn cargo_valid_inspects_password_policy_as_explicit_ready() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(password_policy_registry_file())
        .arg("inspect")
        .arg("password-policy-safe")
        .arg("--json")
        .output()
        .expect("cargo-valid inspect password policy should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"machine_ir_ready\":true"));
    assert!(stdout.contains("\"solver_ready\":false"));
    assert!(stdout.contains("\"string_fields_require_explicit_backend\""));
    assert!(stdout.contains("\"regex_match_requires_explicit_backend\""));
    assert!(stdout.contains("\"rust_type\":\"String\""));
    assert!(stdout.contains("\"P_PASSWORD_POLICY_MATCHES_FLAG\""));
}

#[test]
fn cargo_valid_readiness_reports_password_solver_limitations() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(password_policy_registry_file())
        .arg("readiness")
        .arg("password-policy-safe")
        .arg("--json")
        .output()
        .expect("cargo-valid readiness password policy should run");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"code\":\"string_fields_require_explicit_backend\""));
    assert!(stdout.contains("\"code\":\"regex_match_requires_explicit_backend\""));
}

#[test]
fn cargo_valid_verifies_password_policy_models() {
    let _guard = cargo_guard();
    let safe_output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(password_policy_registry_file())
        .arg("verify")
        .arg("password-policy-safe")
        .arg("--property=P_PASSWORD_POLICY_MATCHES_FLAG")
        .arg("--json")
        .output()
        .expect("cargo-valid verify safe password model should run");
    assert_eq!(safe_output.status.code(), Some(0));
    let safe_stdout = String::from_utf8_lossy(&safe_output.stdout);
    assert!(safe_stdout.contains("\"status\":\"PASS\""));
    assert!(safe_stdout.contains("\"property_id\":\"P_PASSWORD_POLICY_MATCHES_FLAG\""));

    let regression_output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(password_policy_registry_file())
        .arg("verify")
        .arg("password-policy-regression")
        .arg("--property=P_PASSWORD_POLICY_MATCHES_FLAG")
        .arg("--json")
        .output()
        .expect("cargo-valid verify regression password model should run");
    assert_eq!(regression_output.status.code(), Some(1));
    let regression_stdout = String::from_utf8_lossy(&regression_output.stdout);
    assert!(regression_stdout.contains("\"status\":\"FAIL\""));
    assert!(regression_stdout.contains("\"property_id\":\"P_PASSWORD_POLICY_MATCHES_FLAG\""));
    assert!(regression_stdout.contains("SET_WEAK_PASSWORD"));
}

#[test]
fn cargo_valid_graph_renders_mermaid_for_bundled_model() {
    let _guard = cargo_guard();
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
    let _guard = cargo_guard();
    let dot_output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(saas_registry_file())
        .arg("graph")
        .arg("tenant-isolation-safe")
        .arg("--format=dot")
        .output()
        .expect("cargo-valid graph dot should run");
    assert!(dot_output.status.success());
    let dot = String::from_utf8_lossy(&dot_output.stdout);
    assert!(dot.contains("digraph model"));
    assert!(dot.contains("ENABLE_SHARED_SEARCH"));

    let svg_output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(saas_registry_file())
        .arg("graph")
        .arg("tenant-isolation-safe")
        .arg("--format=svg")
        .output()
        .expect("cargo-valid graph svg should run");
    assert!(svg_output.status.success());
    let svg = String::from_utf8_lossy(&svg_output.stdout);
    assert!(svg.contains("<svg"));
    assert!(svg.contains("TenantIsolationSafeModel"));
}

#[test]
fn cargo_valid_graph_marks_step_models_as_explicit_only() {
    let _guard = cargo_guard();
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
    assert!(stdout.contains("-. reads .->"));
    assert!(stdout.contains("-->|writes|"));

    let logic_output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("graph")
        .arg("counter")
        .arg("--view=logic")
        .output()
        .expect("cargo-valid logic graph should run");
    assert!(logic_output.status.success());
    let logic_stdout = String::from_utf8_lossy(&logic_output.stdout);
    assert!(logic_stdout.contains("transition internals hidden"));
    assert!(!logic_stdout.contains("transition_INC_0"));
}

#[test]
fn cargo_valid_graph_supports_failure_view() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("graph")
        .arg("failing-counter")
        .arg("--view=failure")
        .arg("--property=P_FAIL")
        .output()
        .expect("cargo-valid failure graph should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Failure Slice"));
    assert!(stdout.contains("P_FAIL"));

    let json_output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("graph")
        .arg("failing-counter")
        .arg("--view=failure")
        .arg("--property=P_FAIL")
        .arg("--format=json")
        .output()
        .expect("cargo-valid failure graph json should run");
    assert!(json_output.status.success());
    let json_stdout = String::from_utf8_lossy(&json_output.stdout);
    assert!(json_stdout.contains("\"graph_view\":\"failure\""));
    assert!(json_stdout.contains("\"graph_slice\""));
    assert!(json_stdout.contains("\"property_id\":\"P_FAIL\""));
}

#[test]
fn cargo_valid_checks_registered_model() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("check")
        .arg("failing-counter")
        .arg("--json")
        .output()
        .expect("cargo-valid check should run");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"property_id\":\"P_FAIL\""));
    assert!(stdout.contains("\"ci\":{\"exit_code\":2"));
    assert!(stdout.contains("\"review_summary\""));
    assert!(stdout.contains("\"traceback\""));
}

#[test]
fn cargo_valid_verifies_fizzbuzz_declaratively() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(fizzbuzz_registry_file())
        .arg("verify")
        .arg("fizzbuzz")
        .arg("--property=P_FIZZBUZZ_DIVISIBLE_BY_BOTH")
        .arg("--json")
        .output()
        .expect("cargo-valid verify fizzbuzz should run");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\":\"PASS\""));
    assert!(stdout.contains("\"property_id\":\"P_FIZZBUZZ_DIVISIBLE_BY_BOTH\""));
    assert!(stdout.contains("\"explored_states\":16"));
}

#[test]
fn cargo_valid_verifies_grouped_saas_regression() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(saas_registry_file())
        .arg("verify")
        .arg("tenant-isolation-regression")
        .arg("--property=P_NO_CROSS_TENANT_ACCESS")
        .arg("--json")
        .output()
        .expect("cargo-valid verify saas regression should run");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\":\"FAIL\""));
    assert!(stdout.contains("\"property_id\":\"P_NO_CROSS_TENANT_ACCESS\""));
    assert!(stdout.contains("SERVE_CROSS_TENANT_QUERY"));
}

#[test]
fn cargo_valid_reports_fizzbuzz_coverage_and_generates_strictness_metadata() {
    let _guard = cargo_guard();
    let coverage = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(fizzbuzz_registry_file())
        .arg("coverage")
        .arg("fizzbuzz")
        .arg("--json")
        .output()
        .expect("cargo-valid coverage fizzbuzz should run");
    assert!(coverage.status.success());
    let coverage_stdout = String::from_utf8_lossy(&coverage.stdout);
    assert!(coverage_stdout.contains("\"transition_coverage_percent\":100"));
    assert!(coverage_stdout.contains("\"guard_full_coverage_percent\":100"));
    assert!(coverage_stdout.contains("\"visited_state_count\":16"));

    let testgen = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(fizzbuzz_registry_file())
        .arg("generate-tests")
        .arg("fizzbuzz")
        .arg("--strategy=path")
        .arg("--json")
        .output()
        .expect("cargo-valid path testgen for fizzbuzz should run");
    assert!(testgen.status.success());
    let stdout = String::from_utf8_lossy(&testgen.stdout);
    assert!(stdout.contains("\"strictness\":\"heuristic\""));
    assert!(stdout.contains("\"derivation\":\"path_tag_search\""));
    assert!(stdout.contains("\"source_kind\":\"path\""));
    for path in extract_generated_files(&stdout) {
        let body = fs::read_to_string(&path).expect("generated file must exist");
        assert!(body.contains("let strictness = \"heuristic\";"));
        assert!(body.contains("let derivation = \"path_tag_search\";"));
    }
    cleanup_generated_files(&stdout);
}

#[test]
fn cargo_valid_lints_registered_model_with_migration_hints() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("readiness")
        .arg("counter")
        .arg("--json")
        .output()
        .expect("cargo-valid lint should run");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"code\":\"opaque_step_closure\""));
    assert!(stdout.contains("\"code\":\"missing_declarative_transitions\""));
    assert!(stdout.contains("\"snippet\":\"transition INC"));
}

#[test]
fn cargo_valid_migrate_surfaces_transition_snippets() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("migrate")
        .arg("counter")
        .arg("--json")
        .output()
        .expect("cargo-valid migrate should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\":\"ok\""));
    assert!(stdout.contains("\"snippets\":["));
    assert!(stdout.contains("transition INC"));
}

#[test]
fn cargo_valid_migrate_can_write_snippets_to_file() {
    let _guard = cargo_guard();
    let output_path = std::env::temp_dir().join(format!(
        "valid-migrate-{}.rs",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("migrate")
        .arg("counter")
        .arg(format!("--write={}", output_path.display()))
        .arg("--json")
        .output()
        .expect("cargo-valid migrate write should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"written\""));
    let body = fs::read_to_string(&output_path).expect("migration output file should exist");
    assert!(body.contains("transition INC"));
    let _ = fs::remove_file(output_path);
}

#[test]
fn cargo_valid_migrate_check_marks_step_models_for_manual_review() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("migrate")
        .arg("counter")
        .arg("--check")
        .arg("--json")
        .output()
        .expect("cargo-valid migrate check should run");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"check\":{"));
    assert!(stdout.contains("\"status\":\"candidate-complete\""));
    assert!(stdout.contains("\"verified_equivalence\":false"));
    assert!(stdout.contains("\"covered_actions\":[\"INC\",\"LOCK\",\"UNLOCK\"]"));
}

#[test]
fn cargo_valid_migrate_check_passes_for_declarative_models() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(saas_registry_file())
        .arg("migrate")
        .arg("tenant-isolation-safe")
        .arg("--check")
        .arg("--json")
        .output()
        .expect("cargo-valid migrate check should run");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\":\"no-op\""));
    assert!(stdout.contains("\"check\":{"));
    assert!(stdout.contains("\"status\":\"already-declarative\""));
    assert!(stdout.contains("\"verified_equivalence\":true"));
}

#[test]
fn cargo_valid_explain_includes_review_metadata() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(saas_registry_file())
        .arg("explain")
        .arg("tenant-isolation-regression")
        .arg("--json")
        .output()
        .expect("cargo-valid explain should run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"failing_action_id\""));
    assert!(stdout.contains("\"next_steps\""));
    assert!(stdout.contains("\"review_summary\""));
    assert!(stdout.contains("tenant_isolation_path"));
}

#[test]
fn cargo_valid_lists_example_models() {
    let _guard = cargo_guard();
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
    let _guard = cargo_guard();
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
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"property_id\":\"P_FAIL\""));
}

#[test]
fn cargo_valid_doc_check_reports_drift() {
    let _guard = cargo_guard();
    let output_path = std::env::temp_dir().join(format!(
        "cargo-valid-doc-check-{}.md",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let first = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("doc")
        .arg("counter")
        .arg(format!("--write={}", output_path.display()))
        .arg("--json")
        .output()
        .expect("cargo-valid doc should run");
    assert!(first.status.success());

    let unchanged = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("doc")
        .arg("counter")
        .arg(format!("--write={}", output_path.display()))
        .arg("--check")
        .arg("--json")
        .output()
        .expect("cargo-valid doc check should run");
    assert_eq!(unchanged.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&unchanged.stdout).contains("\"status\":\"unchanged\""));

    let mut body = fs::read_to_string(&output_path).expect("doc body");
    body.push_str("\nmanual drift\n");
    fs::write(&output_path, body).expect("drift write");

    let changed = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("doc")
        .arg("counter")
        .arg(format!("--write={}", output_path.display()))
        .arg("--check")
        .arg("--json")
        .output()
        .expect("cargo-valid drift check should run");
    assert_eq!(changed.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&changed.stdout);
    assert!(stdout.contains("\"status\":\"changed\""));
    assert!(stdout.contains("\"drift_sections\""));
    let _ = fs::remove_file(output_path);
}

#[test]
fn cargo_valid_lists_example_models_from_file() {
    let _guard = cargo_guard();
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
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(example_registry_file())
        .arg("all")
        .arg("--json")
        .output()
        .expect("cargo-valid all for file-backed example registry should run");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"runs\":["));
    assert!(stdout.contains("\"model_id\":\"counter\""));
    assert!(stdout.contains("\"property_id\":\"P_FAIL\""));
}

#[test]
fn cargo_valid_testgen_witness_generates_files() {
    let _guard = cargo_guard();
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
    assert!(stdout.contains("generated-tests/"));
    for path in extract_generated_files(&stdout) {
        let body = fs::read_to_string(&path).expect("generated file must exist");
        assert!(body.contains("assert_replay_output_json"));
    }
    cleanup_generated_files(&stdout);
}

#[test]
fn cargo_valid_clean_removes_generated_and_artifacts() {
    let _guard = cargo_guard();
    let temp_root = unique_temp_project_dir("valid-clean");
    let generated = temp_root.join("generated-tests").join("clean-sentinel.rs");
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
    let _guard = cargo_guard();
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
    assert!(stdout.contains("generated-tests/"));
    for path in extract_generated_files(&stdout) {
        let body = fs::read_to_string(&path).expect("generated file must exist");
        assert!(body.contains("assert_replay_output_json"));
    }
    cleanup_generated_files(&stdout);
}

#[test]
fn cargo_valid_testgen_path_generates_tagged_files() {
    let _guard = cargo_guard();
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
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("verify")
        .arg("failing-counter")
        .arg("--property=P_FAIL")
        .arg("--json")
        .output()
        .expect("cargo-valid property-specific check should run");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"property_id\":\"P_FAIL\""));
}

#[test]
fn cargo_valid_external_registry_can_use_command_backend() {
    let _guard = cargo_guard();
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
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"property_id\":\"P_BILLING_READ_REQUIRES_SESSION\""));
    assert!(stdout.contains("\"status\":\"FAIL\""));
    assert!(stdout.contains("ATTACH_BOUNDARY,ASSUME_SESSION,EVAL_BILLING_READ"));
}

#[test]
fn cargo_valid_inspects_bundled_declarative_model() {
    let _guard = cargo_guard();
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
    let _guard = cargo_guard();
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
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("benchmarks")
                .join("registries")
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
    let _guard = cargo_guard();
    let project_dir = unique_temp_project_dir("valid-autodiscover");
    write_autodiscover_fixture(&project_dir, "auto-discover");

    let output = Command::new(cargo_valid_path())
        .env("CARGO_NET_OFFLINE", "true")
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
    let _guard = cargo_guard();
    let project_dir = unique_temp_project_dir("valid-init");
    fs::create_dir_all(&project_dir).expect("temp project dir");
    fs::write(
        project_dir.join("Cargo.toml"),
        "[package]\nname = \"valid-init-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("temp Cargo.toml");

    let output = Command::new(cargo_valid_path())
        .env("CARGO_NET_OFFLINE", "true")
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
    assert!(body.contains("default_solver_args = []"));
    assert!(body.contains("benchmark_repeats = 3"));
    assert!(body.contains("benchmarks_dir = \"artifacts/benchmarks\""));
    assert!(body.contains("generated_tests_dir = \"generated-tests\""));
    assert!(project_dir
        .join("examples")
        .join("valid_models.rs")
        .exists());
    assert!(project_dir
        .join("generated-tests")
        .join(".gitkeep")
        .exists());
    assert!(project_dir.join("artifacts").join(".gitkeep").exists());
    assert!(project_dir
        .join("benchmarks")
        .join("baselines")
        .join(".gitkeep")
        .exists());
    let codex_config = fs::read_to_string(project_dir.join(".mcp").join("codex.toml"))
        .expect("codex bootstrap config");
    assert!(codex_config.contains("command = \"valid\""));
    assert!(codex_config.contains("\"--project\", \".\""));
    assert!(!codex_config.contains("/Users/"));
    let claude_code_config = fs::read_to_string(project_dir.join(".mcp").join("claude-code.json"))
        .expect("claude code bootstrap config");
    assert!(claude_code_config.contains("\"valid-registry\""));
    assert!(claude_code_config.contains("\"--project\""));
    let bootstrap_guide =
        fs::read_to_string(project_dir.join("docs").join("ai").join("bootstrap.md"))
            .expect("bootstrap guide");
    assert!(bootstrap_guide.contains(".mcp/codex.toml"));
    assert!(bootstrap_guide.contains("critical_properties"));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cargo_valid_project_first_mode_requires_registry_or_valid_toml() {
    let _guard = cargo_guard();
    let project_dir = unique_temp_project_dir("valid-project-first");
    fs::create_dir_all(&project_dir).expect("temp project dir");
    fs::write(
        project_dir.join("Cargo.toml"),
        "[package]\nname = \"valid-project-first-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("temp Cargo.toml");

    let output = Command::new(cargo_valid_path())
        .env("CARGO_NET_OFFLINE", "true")
        .current_dir(&project_dir)
        .arg("models")
        .output()
        .expect("cargo-valid models should run");
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("project-first mode expects valid.toml or --registry"));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cargo_valid_uses_valid_toml_registry_without_flags() {
    let _guard = cargo_guard();
    let project_dir = unique_temp_project_dir("valid-project-config");
    write_autodiscover_fixture(&project_dir, "project-config-model");
    fs::write(
        project_dir.join("valid.toml"),
        "registry = \"examples/valid_models.rs\"\ndefault_backend = \"explicit\"\nsuite_models = [\"project-config-model\"]\n",
    )
    .expect("valid.toml");

    let output = Command::new(cargo_valid_path())
        .env("CARGO_NET_OFFLINE", "true")
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
    let _guard = cargo_guard();
    let project_dir = unique_temp_project_dir("valid-suite-config");
    write_autodiscover_fixture(&project_dir, "suite-only-model");
    fs::write(
        project_dir.join("valid.toml"),
        "registry = \"examples/valid_models.rs\"\ndefault_backend = \"explicit\"\nsuite_models = [\"suite-only-model\"]\n",
    )
    .expect("valid.toml");

    let output = Command::new(cargo_valid_path())
        .env("CARGO_NET_OFFLINE", "true")
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
fn cargo_valid_suite_can_run_critical_properties_from_valid_toml() {
    let _guard = cargo_guard();
    let project_dir = unique_temp_project_dir("valid-critical-suite");
    write_multi_property_fixture(&project_dir, "critical-suite-model");
    fs::write(
        project_dir.join("valid.toml"),
        "registry = \"examples/valid_models.rs\"\ndefault_backend = \"explicit\"\n\n[critical_properties]\ncritical-suite-model = [\"P_READY_BOOLEAN\"]\n",
    )
    .expect("valid.toml");

    let output = Command::new(cargo_valid_path())
        .env("CARGO_NET_OFFLINE", "true")
        .current_dir(&project_dir)
        .arg("suite")
        .arg("--critical")
        .arg("--json")
        .output()
        .expect("cargo-valid critical suite should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"selection_mode\":\"critical\""));
    assert!(stdout.contains("\"property_id\":\"P_READY_BOOLEAN\""));
    assert!(!stdout.contains("\"property_id\":\"P_RETRIES_BOUNDED\""));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cargo_valid_suite_can_run_named_property_suite_from_valid_toml() {
    let _guard = cargo_guard();
    let project_dir = unique_temp_project_dir("valid-named-suite");
    write_multi_property_fixture(&project_dir, "named-suite-model");
    fs::write(
        project_dir.join("valid.toml"),
        "registry = \"examples/valid_models.rs\"\ndefault_backend = \"explicit\"\n\n[property_suites.smoke]\nentries = [{ model = \"named-suite-model\", properties = [\"P_RETRIES_BOUNDED\"] }]\n",
    )
    .expect("valid.toml");

    let output = Command::new(cargo_valid_path())
        .env("CARGO_NET_OFFLINE", "true")
        .current_dir(&project_dir)
        .arg("suite")
        .arg("--suite=smoke")
        .arg("--json")
        .output()
        .expect("cargo-valid named suite should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"selection_mode\":\"named_suite\""));
    assert!(stdout.contains("\"suite_name\":\"smoke\""));
    assert!(stdout.contains("\"property_id\":\"P_RETRIES_BOUNDED\""));
    assert!(!stdout.contains("\"property_id\":\"P_READY_BOOLEAN\""));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cargo_valid_list_exposes_verification_policy_from_valid_toml() {
    let _guard = cargo_guard();
    let project_dir = unique_temp_project_dir("valid-list-policy");
    write_multi_property_fixture(&project_dir, "policy-model");
    fs::write(
        project_dir.join("valid.toml"),
        "registry = \"examples/valid_models.rs\"\ndefault_backend = \"explicit\"\nsuite_models = [\"policy-model\"]\npreferred_backends = [\"explicit\", \"smt-cvc5\"]\ndefault_suite = \"smoke\"\nminimum_overall_coverage_percent = 85\nminimum_business_coverage_percent = 70\nminimum_setup_coverage_percent = 100\nminimum_requirement_coverage_percent = 65\n\n[critical_properties]\npolicy-model = [\"P_READY_BOOLEAN\"]\n\n[property_suites.smoke]\nentries = [{ model = \"policy-model\", properties = [\"P_RETRIES_BOUNDED\"] }]\n",
    )
    .expect("valid.toml");

    let output = Command::new(cargo_valid_path())
        .env("CARGO_NET_OFFLINE", "true")
        .current_dir(&project_dir)
        .arg("list")
        .arg("--json")
        .output()
        .expect("cargo-valid list should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"verification_policy\""));
    assert!(stdout.contains("\"default_suite\":\"smoke\""));
    assert!(stdout.contains("\"preferred_backends\":[\"explicit\",\"smt-cvc5\"]"));
    assert!(stdout.contains("\"minimum_overall_coverage_percent\":85"));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cargo_valid_suite_uses_default_suite_from_project_policy() {
    let _guard = cargo_guard();
    let project_dir = unique_temp_project_dir("valid-default-suite");
    write_multi_property_fixture(&project_dir, "policy-default-suite-model");
    fs::write(
        project_dir.join("valid.toml"),
        "registry = \"examples/valid_models.rs\"\ndefault_backend = \"explicit\"\ndefault_suite = \"smoke\"\n\n[property_suites.smoke]\nentries = [{ model = \"policy-default-suite-model\", properties = [\"P_RETRIES_BOUNDED\"] }]\n",
    )
    .expect("valid.toml");

    let output = Command::new(cargo_valid_path())
        .env("CARGO_NET_OFFLINE", "true")
        .current_dir(&project_dir)
        .arg("suite")
        .arg("--json")
        .output()
        .expect("cargo-valid suite should honor default suite");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"selection_mode\":\"named_suite\""));
    assert!(stdout.contains("\"suite_name\":\"smoke\""));
    assert!(stdout.contains("\"property_id\":\"P_RETRIES_BOUNDED\""));
    assert!(stdout.contains("\"verification_policy\""));

    let _ = fs::remove_dir_all(project_dir);
}

#[test]
fn cargo_valid_benchmark_uses_project_config_targets() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .arg("benchmark")
        .arg("--json")
        .output()
        .expect("cargo-valid benchmark should run");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"artifact_path\""));
    assert!(stdout.contains("\"runs\":["));
    assert!(stdout.contains("counter"));
    assert!(stdout.contains("\"average_elapsed_ms\""));
}

#[test]
fn cargo_valid_benchmark_can_record_and_compare_baselines() {
    let _guard = cargo_guard();
    let baseline_dir = std::env::temp_dir().join(format!(
        "valid-bench-baselines-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&baseline_dir).expect("baseline dir");

    let record = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(example_registry_file())
        .arg("benchmark")
        .arg("failing-counter")
        .arg("--repeat=1")
        .arg("--baseline=record")
        .arg("--json")
        .env("VALID_BENCHMARK_BASELINES_DIR", &baseline_dir)
        .output()
        .expect("cargo-valid benchmark record should run");
    assert_eq!(record.status.code(), Some(0));
    let record_stdout = String::from_utf8_lossy(&record.stdout);
    assert!(record_stdout.contains("\"status\":\"recorded\""));

    let compare = Command::new(cargo_valid_path())
        .arg("--manifest-path")
        .arg(manifest_path())
        .arg("--file")
        .arg(example_registry_file())
        .arg("benchmark")
        .arg("failing-counter")
        .arg("--repeat=1")
        .arg("--baseline=compare")
        .arg("--threshold-percent=1000")
        .arg("--json")
        .env("VALID_BENCHMARK_BASELINES_DIR", &baseline_dir)
        .output()
        .expect("cargo-valid benchmark compare should run");
    assert_eq!(compare.status.code(), Some(0));
    let compare_stdout = String::from_utf8_lossy(&compare.stdout);
    assert!(compare_stdout.contains("\"baseline\""));
    assert!(compare_stdout.contains("\"status\":\"ok\""));

    let _ = fs::remove_dir_all(baseline_dir);
}

#[test]
fn cargo_valid_bundled_declarative_model_can_use_command_backend() {
    let _guard = cargo_guard();
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
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"property_id\":\"P_BILLING_READ_REQUIRES_SESSION\""));
    assert!(stdout.contains("\"status\":\"FAIL\""));
    assert!(stdout.contains("ATTACH_BOUNDARY,ASSUME_SESSION,EVAL_BILLING_READ"));
}

#[test]
fn cargo_valid_bundled_declarative_model_can_use_mock_cvc5_backend() {
    let _guard = cargo_guard();
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
                .join("tests")
                .join("fixtures")
                .join("solvers")
                .join("mock_cvc5_solver.sh"),
        )
        .arg("--json")
        .output()
        .expect("cargo-valid bundled mock cvc5 check should run");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"backend_name\":\"smt-cvc5\""));
    assert!(stdout.contains("\"property_id\":\"P_BILLING_READ_REQUIRES_SESSION\""));
    assert!(stdout.contains("\"status\":\"FAIL\""));
}

#[test]
fn cargo_valid_external_registry_can_use_mock_cvc5_backend() {
    let _guard = cargo_guard();
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
                .join("tests")
                .join("fixtures")
                .join("solvers")
                .join("mock_cvc5_solver.sh"),
        )
        .arg("--json")
        .output()
        .expect("cargo-valid external mock cvc5 check should run");
    assert_eq!(output.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"backend_name\":\"smt-cvc5\""));
    assert!(stdout.contains("\"property_id\":\"P_BILLING_READ_REQUIRES_SESSION\""));
    assert!(stdout.contains("\"status\":\"FAIL\""));
}

#[test]
fn cargo_valid_bundled_declarative_testgen_can_use_command_backend() {
    let _guard = cargo_guard();
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

#[test]
fn cargo_valid_commands_and_schema_are_machine_readable() {
    let _guard = cargo_guard();
    let commands = Command::new(cargo_valid_path())
        .arg("commands")
        .arg("--json")
        .output()
        .expect("commands should run");
    assert!(commands.status.success());
    let commands_stdout = String::from_utf8_lossy(&commands.stdout);
    assert!(commands_stdout.contains("\"surface\":\"cargo-valid\""));
    assert!(commands_stdout.contains("\"name\":\"batch\""));
    assert!(commands_stdout.contains("\"response\":\"schema.cli.batch_response\""));

    let schema = Command::new(cargo_valid_path())
        .arg("schema")
        .arg("verify")
        .output()
        .expect("schema should run");
    assert!(schema.status.success());
    let schema_stdout = String::from_utf8_lossy(&schema.stdout);
    assert!(schema_stdout.contains("\"command\":\"check\""));
    assert!(schema_stdout
        .contains("\"parameter_schema_id\":\"schema.cli.cargo-valid.check.parameters\""));
    assert!(schema_stdout.contains("\"response_schema_id\":\"schema.run_result\""));
}

#[test]
fn cargo_valid_json_errors_go_to_stderr() {
    let _guard = cargo_guard();
    let output = Command::new(cargo_valid_path())
        .arg("--registry")
        .arg(example_registry_file())
        .arg("inspect")
        .arg("missing-model")
        .arg("--json")
        .output()
        .expect("json error case should run");
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stdout).trim().is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("\"kind\":\"cli_error\""));
    assert!(stderr.contains("\"unknown model `missing-model`\""));
}

#[test]
fn cargo_valid_batch_runs_multiple_operations() {
    let _guard = cargo_guard();
    let request = "{\"schema_version\":\"1.0.0\",\"continue_on_error\":true,\"operations\":[{\"command\":\"inspect\",\"args\":[\"counter\",\"--registry\",\"examples/valid_models.rs\"],\"json\":true},{\"command\":\"verify\",\"args\":[\"failing-counter\",\"--registry\",\"examples/valid_models.rs\"],\"json\":true}]}";
    let mut child = Command::new(cargo_valid_path())
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
    assert!(stdout.contains("\"command\":\"inspect\""));
    assert!(stdout.contains("\"command\":\"verify\""));
    assert!(stdout.contains("\"exit_code\":0"));
    assert!(stdout.contains("\"exit_code\":1"));
}
