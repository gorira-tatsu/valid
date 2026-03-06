//! Deterministic artifact path helpers.

pub fn run_result_path(run_id: &str) -> String {
    format!("artifacts/{run_id}/check-result.json")
}

pub fn evidence_path(run_id: &str, evidence_id: &str) -> String {
    format!("artifacts/{run_id}/evidence/{evidence_id}.trace.json")
}

pub fn vector_path(run_id: &str, vector_id: &str) -> String {
    format!("artifacts/{run_id}/vectors/{vector_id}.json")
}

pub fn generated_test_path(vector_id: &str) -> String {
    format!("tests/generated/{vector_id}.rs")
}
