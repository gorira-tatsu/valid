use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

fn cargo_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn cargo_guard() -> std::sync::MutexGuard<'static, ()> {
    cargo_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn unique_temp_project_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be monotonic enough")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
}

#[test]
fn valid_model_reports_shorthand_diagnostic() {
    let _guard = cargo_guard();
    let project_dir = unique_temp_project_dir("valid-ui-diagnostics");
    fs::create_dir_all(project_dir.join("src")).expect("temp src dir");
    fs::write(
        project_dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"valid-ui-diagnostics\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nvalid = {{ path = {:?} }}\n",
            env!("CARGO_MANIFEST_DIR")
        ),
    )
    .expect("temp Cargo.toml");
    fs::write(
        project_dir.join("src").join("main.rs"),
        r#"use valid::{valid_actions, valid_model, valid_state};

valid_state! {
    struct State {
        ready: bool,
    }
}

valid_actions! {
    enum Action {
        Enable => "ENABLE" [reads = ["ready"], writes = ["ready"]],
    }
}

valid_model! {
    model BrokenModel;
    init [State { ready: false }];
    transitions {
        on Enable {
            [tags = ["allow_path"]]
            when |state| state.ready == false => [State { ready: true }];
        }
    }
    properties {
        invariant P_READY |state| state.ready == false || state.ready == true;
    }
}

fn main() {}
"#,
    )
    .expect("temp main.rs");

    let output = Command::new("cargo")
        .arg("check")
        .arg("--offline")
        .current_dir(&project_dir)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .expect("cargo check should run");
    assert!(
        !output.status.success(),
        "cargo check unexpectedly succeeded"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("valid_model! requires explicit state/action types"),
        "unexpected stderr: {stderr}"
    );

    let _ = fs::remove_dir_all(project_dir);
}
