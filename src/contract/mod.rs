//! Contract snapshot, lock, and drift management.

use crate::{ir::{FieldType, ModelIr, PropertyKind}, support::hash::stable_hash_hex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractSnapshot {
    pub model_id: String,
    pub state_fields: Vec<String>,
    pub actions: Vec<String>,
    pub properties: Vec<String>,
    pub contract_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractLockFile {
    pub schema_version: String,
    pub snapshots: Vec<ContractSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractDriftReport {
    pub model_id: String,
    pub expected_hash: String,
    pub actual_hash: String,
    pub changed: bool,
}

pub fn snapshot_model(model: &ModelIr) -> ContractSnapshot {
    let state_fields = model
        .state_fields
        .iter()
        .map(|field| format!("{}:{}", field.name, field_type_label(&field.ty)))
        .collect::<Vec<_>>();
    let actions = model.actions.iter().map(|action| action.action_id.clone()).collect::<Vec<_>>();
    let properties = model
        .properties
        .iter()
        .map(|property| format!("{}:{}", property.property_id, property_kind_label(&property.kind)))
        .collect::<Vec<_>>();
    let mut canonical = String::new();
    canonical.push_str(&model.model_id);
    canonical.push('|');
    canonical.push_str(&state_fields.join(","));
    canonical.push('|');
    canonical.push_str(&actions.join(","));
    canonical.push('|');
    canonical.push_str(&properties.join(","));
    let contract_hash = stable_hash_hex(&canonical);
    ContractSnapshot {
        model_id: model.model_id.clone(),
        state_fields,
        actions,
        properties,
        contract_hash,
    }
}

pub fn compare_snapshot(expected: &ContractSnapshot, actual: &ContractSnapshot) -> ContractDriftReport {
    ContractDriftReport {
        model_id: actual.model_id.clone(),
        expected_hash: expected.contract_hash.clone(),
        actual_hash: actual.contract_hash.clone(),
        changed: expected.contract_hash != actual.contract_hash,
    }
}

fn field_type_label(field_type: &FieldType) -> String {
    match field_type {
        FieldType::Bool => "bool".to_string(),
        FieldType::BoundedU8 { min, max } => format!("u8[{min}..{max}]"),
    }
}

fn property_kind_label(kind: &PropertyKind) -> &'static str {
    match kind {
        PropertyKind::Invariant => "invariant",
    }
}

#[cfg(test)]
mod tests {
    use crate::frontend::compile_model;

    use super::{compare_snapshot, snapshot_model};

    #[test]
    fn snapshot_hash_changes_when_contract_changes() {
        let model_a = compile_model("model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\nproperty P_SAFE:\n  invariant: x <= 7\n").unwrap();
        let model_b = compile_model("model A\nstate:\n  y: u8[0..7]\ninit:\n  y = 0\nproperty P_SAFE:\n  invariant: y <= 7\n").unwrap();
        let snap_a = snapshot_model(&model_a);
        let snap_b = snapshot_model(&model_b);
        assert!(compare_snapshot(&snap_a, &snap_b).changed);
    }
}
