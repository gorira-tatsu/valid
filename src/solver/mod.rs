//! Solver capability descriptions and adapter traits.

use crate::{
    engine::{check_explicit, BackendKind, CheckOutcome, RunPlan},
    evidence::EvidenceTrace,
    ir::ModelIr,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityMatrix {
    pub backend_name: String,
    pub supports_explicit: bool,
    pub supports_bmc: bool,
    pub supports_certificate: bool,
    pub supports_trace: bool,
    pub supports_witness: bool,
    pub selfcheck_compatible: bool,
}

pub trait SolverAdapter {
    fn backend_kind(&self) -> BackendKind;
    fn capabilities(&self) -> CapabilityMatrix;
    fn build_plan(&self, model: &ModelIr, run_plan: &RunPlan) -> Result<SolverRunPlan, String>;
    fn run(&self, model: &ModelIr, plan: &SolverRunPlan) -> Result<RawSolverResult, String>;
    fn normalize(
        &self,
        model: &ModelIr,
        run_plan: &RunPlan,
        raw: RawSolverResult,
    ) -> Result<NormalizedRunResult, String>;
}

pub struct ExplicitAdapter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolverRunPlan {
    pub run_id: String,
    pub backend: BackendKind,
    pub target_property_ids: Vec<String>,
    pub horizon: Option<u32>,
    pub encoded_model_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawSolverResult {
    Explicit(CheckOutcome),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedRunResult {
    pub outcome: CheckOutcome,
    pub trace: Option<EvidenceTrace>,
}

impl SolverAdapter for ExplicitAdapter {
    fn backend_kind(&self) -> BackendKind {
        BackendKind::Explicit
    }

    fn capabilities(&self) -> CapabilityMatrix {
        CapabilityMatrix {
            backend_name: "explicit".to_string(),
            supports_explicit: true,
            supports_bmc: false,
            supports_certificate: false,
            supports_trace: true,
            supports_witness: false,
            selfcheck_compatible: true,
        }
    }

    fn build_plan(&self, _model: &ModelIr, run_plan: &RunPlan) -> Result<SolverRunPlan, String> {
        let target_property_ids = match &run_plan.property_selection {
            crate::engine::PropertySelection::ExactlyOne(id) => vec![id.clone()],
        };
        Ok(SolverRunPlan {
            run_id: run_plan.manifest.run_id.clone(),
            backend: BackendKind::Explicit,
            target_property_ids,
            horizon: run_plan.search_bounds.max_depth.map(|value| value as u32),
            encoded_model_hash: format!("encoded:{}", run_plan.manifest.source_hash),
        })
    }

    fn run(&self, model: &ModelIr, plan: &SolverRunPlan) -> Result<RawSolverResult, String> {
        let mut run_plan = RunPlan::default();
        run_plan.manifest.run_id = plan.run_id.clone();
        if let Some(property_id) = plan.target_property_ids.first() {
            run_plan.property_selection =
                crate::engine::PropertySelection::ExactlyOne(property_id.clone());
        }
        run_plan.search_bounds.max_depth = plan.horizon.map(|value| value as usize);
        Ok(RawSolverResult::Explicit(check_explicit(model, &run_plan)))
    }

    fn normalize(
        &self,
        _model: &ModelIr,
        _run_plan: &RunPlan,
        raw: RawSolverResult,
    ) -> Result<NormalizedRunResult, String> {
        match raw {
            RawSolverResult::Explicit(outcome) => {
                let trace = match &outcome {
                    CheckOutcome::Completed(result) => result.trace.clone(),
                    CheckOutcome::Errored(_) => None,
                };
                Ok(NormalizedRunResult { outcome, trace })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        engine::{PropertySelection, RunPlan},
        frontend::compile_model,
    };

    use super::{ExplicitAdapter, SolverAdapter};

    #[test]
    fn explicit_adapter_normalizes_completed_outcome() {
        let model = compile_model(
            "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_SAFE:\n  invariant: x <= 1\n",
        )
        .unwrap();
        let mut run_plan = RunPlan::default();
        run_plan.property_selection = PropertySelection::ExactlyOne("P_SAFE".to_string());
        let adapter = ExplicitAdapter;
        let plan = adapter.build_plan(&model, &run_plan).unwrap();
        let raw = adapter.run(&model, &plan).unwrap();
        let normalized = adapter.normalize(&model, &run_plan, raw).unwrap();
        assert!(normalized.trace.is_some());
    }
}
