//! Selfcheck smoke suite for the current kernel and engine contracts.

use crate::{
    engine::{check_explicit, CheckOutcome, PropertySelection, RunPlan},
    frontend::compile_model,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfcheckCase {
    pub case_id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfcheckReport {
    pub schema_version: String,
    pub suite_id: String,
    pub run_id: String,
    pub status: String,
    pub cases: Vec<SelfcheckCase>,
}

pub fn run_smoke_selfcheck() -> SelfcheckReport {
    let mut cases = Vec::new();

    let source = "model Selfcheck\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_SAFE:\n  invariant: x <= 1\n";
    let model = compile_model(source).expect("selfcheck model should compile");
    let mut plan = RunPlan::default();
    plan.property_selection = PropertySelection::ExactlyOne("P_SAFE".to_string());
    let outcome = check_explicit(&model, &plan);
    cases.push(SelfcheckCase {
        case_id: "kernel-explicit-counterexample".to_string(),
        status: match outcome {
            CheckOutcome::Completed(result) if result.status == crate::engine::RunStatus::Fail => "ok".to_string(),
            _ => "failed".to_string(),
        },
    });

    let status = if cases.iter().all(|case| case.status == "ok") {
        "ok"
    } else {
        "failed"
    };

    SelfcheckReport {
        schema_version: "1.0.0".to_string(),
        suite_id: "selfcheck-smoke".to_string(),
        run_id: "selfcheck-local-0001".to_string(),
        status: status.to_string(),
        cases,
    }
}

#[cfg(test)]
mod tests {
    use super::run_smoke_selfcheck;

    #[test]
    fn smoke_selfcheck_passes() {
        let report = run_smoke_selfcheck();
        assert_eq!(report.status, "ok");
    }
}
