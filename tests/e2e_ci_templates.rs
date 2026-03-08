use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn cleanup_ci_template_outputs() {
    let _ = fs::remove_dir_all(repo_path("template-artifacts"));
    let _ = fs::remove_file(repo_path(
        "tests/fixtures/projects/ci_template_project/artifacts/docs/safe_counter.md",
    ));

    let generated_dir = repo_path("tests/fixtures/projects/ci_template_project/generated-tests");
    if let Ok(entries) = fs::read_dir(&generated_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
                let _ = fs::remove_file(path);
            }
        }
    }
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
    cleanup_ci_template_outputs();

    let fixture_manifest = repo_path("tests/fixtures/projects/ci_template_project/Cargo.toml");
    let fixture_model =
        repo_path("tests/fixtures/projects/ci_template_project/fixtures/models/safe_counter.valid");
    let fixture_runner = repo_path(
        "tests/fixtures/projects/ci_template_project/fixtures/runners/mock_counter_pass.sh",
    );
    let fixture_doc_output =
        repo_path("tests/fixtures/projects/ci_template_project/artifacts/docs/safe_counter.md");

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

    cleanup_ci_template_outputs();
}
