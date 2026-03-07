//! Deterministic artifact path helpers.

fn path_from_env_or_default(env_key: &str, default_prefix: &str, suffix: &str) -> String {
    match std::env::var(env_key) {
        Ok(prefix) if !prefix.trim().is_empty() => {
            format!("{}/{}", prefix.trim_end_matches('/'), suffix)
        }
        _ => format!("{default_prefix}/{suffix}"),
    }
}

pub fn run_result_path(run_id: &str) -> String {
    path_from_env_or_default(
        "VALID_ARTIFACTS_DIR",
        "artifacts",
        &format!("{run_id}/check-result.json"),
    )
}

pub fn evidence_path(run_id: &str, evidence_id: &str) -> String {
    path_from_env_or_default(
        "VALID_ARTIFACTS_DIR",
        "artifacts",
        &format!("{run_id}/evidence/{evidence_id}.trace.json"),
    )
}

pub fn vector_path(run_id: &str, vector_id: &str) -> String {
    path_from_env_or_default(
        "VALID_ARTIFACTS_DIR",
        "artifacts",
        &format!("{run_id}/vectors/{vector_id}.json"),
    )
}

pub fn generated_test_path(vector_id: &str) -> String {
    path_from_env_or_default(
        "VALID_GENERATED_TESTS_DIR",
        "generated-tests",
        &format!("{vector_id}.rs"),
    )
}

pub fn selfcheck_report_path(suite_id: &str, run_id: &str) -> String {
    path_from_env_or_default(
        "VALID_ARTIFACTS_DIR",
        "artifacts",
        &format!("selfcheck/{suite_id}/{run_id}/report.json"),
    )
}

pub fn benchmark_report_path(report_id: &str) -> String {
    path_from_env_or_default(
        "VALID_BENCHMARKS_DIR",
        "artifacts/benchmarks",
        &format!("{report_id}.json"),
    )
}

pub fn benchmark_baseline_path(report_id: &str) -> String {
    path_from_env_or_default(
        "VALID_BENCHMARK_BASELINES_DIR",
        "artifacts/benchmarks/baselines",
        &format!("{report_id}.json"),
    )
}
