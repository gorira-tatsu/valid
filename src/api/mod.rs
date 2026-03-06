//! Machine-readable API layer for AI and CLI integration.

use crate::{
    engine::{check_explicit, CheckErrorEnvelope, CheckOutcome, PropertySelection, RunManifest, RunPlan},
    frontend,
    ir::ModelIr,
    support::{diagnostics::Diagnostic, hash::stable_hash_hex},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectResponse {
    pub schema_version: String,
    pub request_id: String,
    pub status: String,
    pub model_id: String,
    pub state_fields: Vec<String>,
    pub actions: Vec<String>,
    pub properties: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRequest {
    pub request_id: String,
    pub source_name: String,
    pub source: String,
    pub property_id: Option<String>,
}

pub fn inspect_source(request_id: &str, source: &str) -> Result<InspectResponse, Vec<Diagnostic>> {
    let model = frontend::compile_model(source)?;
    Ok(InspectResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        status: "ok".to_string(),
        model_id: model.model_id.clone(),
        state_fields: model.state_fields.iter().map(|f| f.name.clone()).collect(),
        actions: model.actions.iter().map(|a| a.action_id.clone()).collect(),
        properties: model.properties.iter().map(|p| p.property_id.clone()).collect(),
    })
}

pub fn compile_source(source: &str) -> Result<ModelIr, Vec<Diagnostic>> {
    frontend::compile_model(source)
}

pub fn check_source(request: &CheckRequest) -> CheckOutcome {
    let source_hash = stable_hash_hex(&request.source);
    match frontend::compile_model(&request.source) {
        Ok(model) => {
            let property_id = request
                .property_id
                .clone()
                .or_else(|| model.properties.first().map(|property| property.property_id.clone()))
                .unwrap_or_else(|| "P_SAFE".to_string());
            let mut plan = RunPlan::default();
            plan.manifest = RunManifest {
                request_id: request.request_id.clone(),
                run_id: format!("run-{}", stable_hash_hex(&(request.request_id.clone() + &property_id)).replace("sha256:", "")),
                schema_version: "1.0.0".to_string(),
                source_hash,
                contract_hash: stable_hash_hex(&model.model_id),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                backend_name: crate::engine::BackendKind::Explicit,
                backend_version: env!("CARGO_PKG_VERSION").to_string(),
                seed: None,
            };
            plan.property_selection = PropertySelection::ExactlyOne(property_id);
            check_explicit(&model, &plan)
        }
        Err(diagnostics) => CheckOutcome::Errored(CheckErrorEnvelope {
            manifest: RunManifest {
                request_id: request.request_id.clone(),
                run_id: format!("run-{}", stable_hash_hex(&request.request_id).replace("sha256:", "")),
                schema_version: "1.0.0".to_string(),
                source_hash,
                contract_hash: "sha256:unknown".to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                backend_name: crate::engine::BackendKind::Explicit,
                backend_version: env!("CARGO_PKG_VERSION").to_string(),
                seed: None,
            },
            status: crate::engine::ErrorStatus::Error,
            assurance_level: crate::engine::AssuranceLevel::Incomplete,
            diagnostics,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{check_source, inspect_source, CheckRequest};
    use crate::engine::CheckOutcome;

    #[test]
    fn inspect_returns_model_outline() {
        let source = "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\nproperty P_SAFE:\n  invariant: x <= 7\n";
        let response = inspect_source("req-1", source).unwrap();
        assert_eq!(response.model_id, "A");
        assert_eq!(response.properties, vec!["P_SAFE"]);
    }

    #[test]
    fn check_wraps_frontend_errors_in_error_outcome() {
        let outcome = check_source(&CheckRequest {
            request_id: "req-1".to_string(),
            source_name: "broken.valid".to_string(),
            source: "model A\nstate:\n  x: u8[0..7]\ninit:\n  y = 0\n".to_string(),
            property_id: None,
        });
        assert!(matches!(outcome, CheckOutcome::Errored(_)));
    }
}
