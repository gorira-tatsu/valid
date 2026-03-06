//! Solver capability descriptions and adapter traits.

use crate::{engine::RunPlan, evidence::EvidenceTrace, ir::ModelIr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityMatrix {
    pub backend_name: String,
    pub supports_explicit: bool,
    pub supports_bmc: bool,
    pub supports_certificate: bool,
    pub supports_trace: bool,
}

pub trait SolverAdapter {
    fn name(&self) -> &'static str;
    fn capability_matrix(&self) -> CapabilityMatrix;
    fn normalize_trace(&self, _model: &ModelIr, _run_plan: &RunPlan) -> Result<Option<EvidenceTrace>, String> {
        Ok(None)
    }
}

pub struct ExplicitAdapter;

impl SolverAdapter for ExplicitAdapter {
    fn name(&self) -> &'static str {
        "explicit"
    }

    fn capability_matrix(&self) -> CapabilityMatrix {
        CapabilityMatrix {
            backend_name: "explicit".to_string(),
            supports_explicit: true,
            supports_bmc: false,
            supports_certificate: false,
            supports_trace: true,
        }
    }
}
