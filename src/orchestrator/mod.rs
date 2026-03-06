//! Thin orchestration layer that expands higher-level intents into concrete runs.

use crate::{
    engine::{check_explicit, CheckErrorEnvelope, CheckOutcome, PropertySelection, RunPlan},
    ir::ModelIr,
    solver::{run_with_adapter, AdapterConfig},
    support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestratedRun {
    pub property_id: String,
    pub outcome: CheckOutcome,
}

pub fn run_all_properties(model: &ModelIr, base_plan: &RunPlan) -> Vec<OrchestratedRun> {
    run_all_properties_with_backend(model, base_plan, &AdapterConfig::Explicit)
}

pub fn run_all_properties_with_backend(
    model: &ModelIr,
    base_plan: &RunPlan,
    backend: &AdapterConfig,
) -> Vec<OrchestratedRun> {
    model
        .properties
        .iter()
        .map(|property| {
            let mut plan = base_plan.clone();
            plan.property_selection = PropertySelection::ExactlyOne(property.property_id.clone());
            let suffix = property
                .property_id
                .replace(|ch: char| !ch.is_ascii_alphanumeric(), "_");
            plan.manifest.run_id = format!("{}-{suffix}", base_plan.manifest.run_id);
            OrchestratedRun {
                property_id: property.property_id.clone(),
                outcome: match backend {
                    AdapterConfig::Explicit => check_explicit(model, &plan),
                    _ => match run_with_adapter(model, &plan, backend) {
                        Ok(normalized) => normalized.outcome,
                        Err(message) => CheckOutcome::Errored(CheckErrorEnvelope {
                            manifest: plan.manifest.clone(),
                            status: crate::engine::ErrorStatus::Error,
                            assurance_level: crate::engine::AssuranceLevel::Incomplete,
                            diagnostics: vec![Diagnostic::new(
                                ErrorCode::SearchError,
                                DiagnosticSegment::EngineSearch,
                                message,
                            )],
                        }),
                    },
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{
        engine::{RunPlan, RunStatus},
        frontend::compile_model,
    };

    use super::{run_all_properties, run_all_properties_with_backend};

    #[test]
    fn expands_one_run_per_property() {
        let model = compile_model(
            "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P1:\n  invariant: x <= 1\nproperty P2:\n  invariant: x <= 7\n",
        )
        .unwrap();
        let runs = run_all_properties(&model, &RunPlan::default());
        assert_eq!(runs.len(), 2);
        assert!(matches!(
            runs[0].outcome,
            crate::engine::CheckOutcome::Completed(_)
        ));
        let statuses = runs
            .iter()
            .map(|run| match &run.outcome {
                crate::engine::CheckOutcome::Completed(result) => result.status,
                crate::engine::CheckOutcome::Errored(_) => RunStatus::Unknown,
            })
            .collect::<Vec<_>>();
        assert!(statuses.contains(&RunStatus::Fail));
        assert!(statuses.contains(&RunStatus::Pass));
    }

    #[test]
    fn can_orchestrate_with_mock_bmc_backend() {
        let model = compile_model(
            "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P1:\n  invariant: x <= 7\n",
        )
        .unwrap();
        let runs = run_all_properties_with_backend(
            &model,
            &RunPlan::default(),
            &crate::solver::AdapterConfig::MockBmc,
        );
        assert_eq!(runs.len(), 1);
        assert!(matches!(
            runs[0].outcome,
            crate::engine::CheckOutcome::Completed(_)
        ));
    }
}
