use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_valid"))
}

fn cvc5_path() -> Option<String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg("command -v cvc5")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        None
    } else {
        Some(path)
    }
}

#[test]
fn real_cvc5_backend_finds_counterexample_when_available() {
    let Some(cvc5) = cvc5_path() else {
        return;
    };

    let fail = repo_path("tests/fixtures/models/failing_counter.valid");
    let output = Command::new(binary_path())
        .arg("check")
        .arg(&fail)
        .arg("--json")
        .arg("--backend=smt-cvc5")
        .arg("--solver-exec")
        .arg(cvc5)
        .arg("--solver-arg")
        .arg("--lang")
        .arg("--solver-arg")
        .arg("smt2")
        .output()
        .expect("cvc5 backend should run");

    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\"status\":\"FAIL\""));
    assert!(stdout.contains("\"backend_name\":\"smt-cvc5\""));
}
