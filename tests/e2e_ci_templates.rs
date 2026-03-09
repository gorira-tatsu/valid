use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be available")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
}

fn copy_dir_recursive(src: &Path, dest: &Path) {
    fs::create_dir_all(dest).expect("destination dir should exist");
    for entry in fs::read_dir(src).expect("source dir should be readable") {
        let entry = entry.expect("dir entry");
        let source_path = entry.path();
        let target_path = dest.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path);
        } else {
            fs::copy(&source_path, &target_path).expect("file should copy");
        }
    }
}

fn prepare_ci_template_fixture_copy() -> PathBuf {
    let temp_root = unique_temp_dir("valid-ci-template");
    let fixture_src = repo_path("tests/fixtures/projects/ci_template_project");
    let fixture_dest = temp_root.join("ci_template_project");
    copy_dir_recursive(&fixture_src, &fixture_dest);
    let cargo_toml_path = fixture_dest.join("Cargo.toml");
    let cargo_toml = fs::read_to_string(&cargo_toml_path).expect("fixture Cargo.toml");
    let rewritten = cargo_toml.replace(
        "valid = { path = \"../../../..\", features = [\"verification-runtime\"] }",
        &format!(
            "valid = {{ path = {:?}, features = [\"verification-runtime\"] }}",
            Path::new(env!("CARGO_MANIFEST_DIR"))
        ),
    );
    fs::write(cargo_toml_path, rewritten).expect("fixture Cargo.toml should be rewritten");
    temp_root
}

fn run_script(script: &str, args: &[&Path]) {
    let output = Command::new("bash")
        .arg(repo_path(script))
        .args(args)
        .output()
        .expect("template script should run");
    assert!(
        output.status.success(),
        "stdout:\n{}\n\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn ci_template_scripts_run_against_fixture_project() {
    let temp_root = prepare_ci_template_fixture_copy();
    let fixture_root = temp_root.join("ci_template_project");
    let fixture_manifest = fixture_root.join("Cargo.toml");
    let fixture_model = fixture_root.join("fixtures/models/safe_counter.valid");
    let fixture_runner = fixture_root.join("fixtures/runners/mock_counter_pass.sh");
    let fixture_doc_output = fixture_root.join("artifacts/docs/safe_counter.md");

    run_script(
        "scripts/ci/template_inspect_check.sh",
        &[
            fixture_manifest.as_path(),
            Path::new("approval-model"),
            Path::new("P_APPROVAL_IS_BOOLEAN"),
        ],
    );
    run_script(
        "scripts/ci/template_testgen.sh",
        &[
            fixture_manifest.as_path(),
            Path::new("approval-model"),
            Path::new("witness"),
            Path::new("P_APPROVAL_IS_BOOLEAN"),
        ],
    );
    run_script(
        "scripts/ci/template_conformance.sh",
        &[
            fixture_model.as_path(),
            Path::new("P_SAFE"),
            Path::new("Inc"),
            fixture_runner.as_path(),
        ],
    );
    run_script(
        "scripts/ci/template_doc_check.sh",
        &[fixture_model.as_path(), fixture_doc_output.as_path()],
    );

    assert!(repo_path("template-artifacts/inspect-check/inspect.json").exists());
    assert!(repo_path("template-artifacts/inspect-check/check.json").exists());
    assert!(repo_path("template-artifacts/testgen/testgen.json").exists());
    assert!(repo_path("template-artifacts/conformance/conformance.json").exists());
    assert!(repo_path("template-artifacts/doc-check/doc-write.json").exists());
    assert!(repo_path("template-artifacts/doc-check/doc-check.json").exists());
    assert!(repo_path("template-artifacts/doc-check/safe_counter.md").exists());

    let generated_tests_dir = repo_path("template-artifacts/testgen/generated-tests");
    let generated = fs::read_dir(&generated_tests_dir)
        .expect("generated tests dir should exist")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rs"))
        .collect::<Vec<_>>();
    assert!(!generated.is_empty(), "expected generated test vectors");

    let inspect_json =
        fs::read_to_string(repo_path("template-artifacts/inspect-check/inspect.json"))
            .expect("inspect artifact should exist");
    assert!(inspect_json.contains("\"model_id\":\"ApprovalModel\""));

    let conformance_json =
        fs::read_to_string(repo_path("template-artifacts/conformance/conformance.json"))
            .expect("conformance artifact should exist");
    assert!(conformance_json.contains("\"status\":\"PASS\""));

    let doc_markdown =
        fs::read_to_string(repo_path("template-artifacts/doc-check/safe_counter.md"))
            .expect("doc artifact should exist");
    assert!(doc_markdown.contains("# SafeCounter"));

    let _ = fs::remove_dir_all(repo_path("template-artifacts"));
    let _ = fs::remove_dir_all(temp_root);
}
