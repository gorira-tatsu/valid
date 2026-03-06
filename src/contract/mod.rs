//! Contract snapshot, lock, and drift management.

use crate::{ir::{FieldType, ModelIr, PropertyKind}, support::{hash::stable_hash_hex, io::write_text_file}};

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
    pub generated_at: String,
    pub entries: Vec<ContractSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractDriftReport {
    pub schema_version: String,
    pub status: String,
    pub contract_id: String,
    pub old_hash: String,
    pub new_hash: String,
    pub changes: Vec<String>,
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

pub fn build_lock_file(snapshots: Vec<ContractSnapshot>) -> ContractLockFile {
    ContractLockFile {
        schema_version: "1.0.0".to_string(),
        generated_at: "1970-01-01T00:00:00Z".to_string(),
        entries: snapshots,
    }
}

pub fn render_lock_json(lock: &ContractLockFile) -> String {
    let mut out = String::from("{");
    out.push_str(&format!("\"schema_version\":\"{}\"", lock.schema_version));
    out.push_str(&format!(",\"generated_at\":\"{}\"", lock.generated_at));
    out.push_str(",\"entries\":[");
    for (index, entry) in lock.entries.iter().enumerate() {
        if index > 0 { out.push(','); }
        out.push('{');
        out.push_str(&format!("\"model_id\":\"{}\"", entry.model_id));
        out.push_str(&format!(",\"contract_hash\":\"{}\"", entry.contract_hash));
        out.push_str(&format!(",\"state_fields\":[{}]", entry.state_fields.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(",")));
        out.push_str(&format!(",\"actions\":[{}]", entry.actions.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(",")));
        out.push_str(&format!(",\"properties\":[{}]", entry.properties.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(",")));
        out.push('}');
    }
    out.push_str("]}");
    out
}

pub fn render_drift_json(report: &ContractDriftReport) -> String {
    let mut out = String::from("{");
    out.push_str(&format!("\"schema_version\":\"{}\"", report.schema_version));
    out.push_str(&format!(",\"status\":\"{}\"", report.status));
    out.push_str(&format!(",\"contract_id\":\"{}\"", report.contract_id));
    out.push_str(&format!(",\"old_hash\":\"{}\"", report.old_hash));
    out.push_str(&format!(",\"new_hash\":\"{}\"", report.new_hash));
    out.push_str(",\"changes\":[");
    for (index, change) in report.changes.iter().enumerate() {
        if index > 0 { out.push(','); }
        out.push_str(&format!("\"{}\"", change));
    }
    out.push_str("]}");
    out
}

pub fn compare_snapshot(expected: &ContractSnapshot, actual: &ContractSnapshot) -> ContractDriftReport {
    let mut changes = Vec::new();
    if expected.state_fields != actual.state_fields {
        changes.push("state_fields".to_string());
    }
    if expected.actions != actual.actions {
        changes.push("actions".to_string());
    }
    if expected.properties != actual.properties {
        changes.push("properties".to_string());
    }
    ContractDriftReport {
        schema_version: "1.0.0".to_string(),
        status: if expected.contract_hash == actual.contract_hash { "unchanged" } else { "changed" }.to_string(),
        contract_id: actual.model_id.clone(),
        old_hash: expected.contract_hash.clone(),
        new_hash: actual.contract_hash.clone(),
        changes,
    }
}

pub fn write_lock_file(path: &str, lock: &ContractLockFile) -> Result<(), String> {
    write_text_file(path, &render_lock_json(lock))
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
    use std::fs;

    use crate::frontend::compile_model;

    use super::{build_lock_file, compare_snapshot, render_drift_json, render_lock_json, snapshot_model, write_lock_file};

    #[test]
    fn snapshot_hash_changes_when_contract_changes() {
        let model_a = compile_model("model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\nproperty P_SAFE:\n  invariant: x <= 7\n").unwrap();
        let model_b = compile_model("model A\nstate:\n  y: u8[0..7]\ninit:\n  y = 0\nproperty P_SAFE:\n  invariant: y <= 7\n").unwrap();
        let snap_a = snapshot_model(&model_a);
        let snap_b = snapshot_model(&model_b);
        assert!(compare_snapshot(&snap_a, &snap_b).changes.contains(&"state_fields".to_string()));
    }

    #[test]
    fn renders_and_writes_lock_json() {
        let model = compile_model("model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\nproperty P_SAFE:\n  invariant: x <= 7\n").unwrap();
        let lock = build_lock_file(vec![snapshot_model(&model)]);
        let json = render_lock_json(&lock);
        assert!(json.contains("\"entries\""));
        let path = "/tmp/valid-contract-lock.json";
        write_lock_file(path, &lock).unwrap();
        assert!(fs::read_to_string(path).unwrap().contains("contract_hash"));
    }

    #[test]
    fn renders_drift_json() {
        let model_a = compile_model("model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\nproperty P_SAFE:\n  invariant: x <= 7\n").unwrap();
        let model_b = compile_model("model A\nstate:\n  y: u8[0..7]\ninit:\n  y = 0\nproperty P_SAFE:\n  invariant: y <= 7\n").unwrap();
        let report = compare_snapshot(&snapshot_model(&model_a), &snapshot_model(&model_b));
        assert!(render_drift_json(&report).contains("\"status\":\"changed\""));
    }
}
