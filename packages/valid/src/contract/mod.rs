//! Contract snapshot, lock, and drift management.

use crate::{
    ir::{FieldType, ModelIr, PropertyKind},
    support::{hash::stable_hash_hex, io::write_text_file},
};

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
    let actions = model
        .actions
        .iter()
        .map(|action| action.action_id.clone())
        .collect::<Vec<_>>();
    let properties = model
        .properties
        .iter()
        .map(|property| {
            format!(
                "{}:{}",
                property.property_id,
                property_kind_label(&property.kind)
            )
        })
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
        if index > 0 {
            out.push(',');
        }
        out.push('{');
        out.push_str(&format!("\"model_id\":\"{}\"", entry.model_id));
        out.push_str(&format!(",\"contract_hash\":\"{}\"", entry.contract_hash));
        out.push_str(&format!(
            ",\"state_fields\":[{}]",
            entry
                .state_fields
                .iter()
                .map(|s| format!("\"{}\"", s))
                .collect::<Vec<_>>()
                .join(",")
        ));
        out.push_str(&format!(
            ",\"actions\":[{}]",
            entry
                .actions
                .iter()
                .map(|s| format!("\"{}\"", s))
                .collect::<Vec<_>>()
                .join(",")
        ));
        out.push_str(&format!(
            ",\"properties\":[{}]",
            entry
                .properties
                .iter()
                .map(|s| format!("\"{}\"", s))
                .collect::<Vec<_>>()
                .join(",")
        ));
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
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\"", change));
    }
    out.push_str("]}");
    out
}

pub fn compare_snapshot(
    expected: &ContractSnapshot,
    actual: &ContractSnapshot,
) -> ContractDriftReport {
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
        status: if expected.contract_hash == actual.contract_hash {
            "unchanged"
        } else {
            "changed"
        }
        .to_string(),
        contract_id: actual.model_id.clone(),
        old_hash: expected.contract_hash.clone(),
        new_hash: actual.contract_hash.clone(),
        changes,
    }
}

pub fn write_lock_file(path: &str, lock: &ContractLockFile) -> Result<(), String> {
    write_text_file(path, &render_lock_json(lock))
}

pub fn parse_lock_file(body: &str) -> Result<ContractLockFile, String> {
    let schema_version = extract_json_string(body, "schema_version")
        .ok_or_else(|| "missing schema_version".to_string())?;
    let generated_at = extract_json_string(body, "generated_at")
        .ok_or_else(|| "missing generated_at".to_string())?;
    let entries_body =
        extract_array_body(body, "entries").ok_or_else(|| "missing entries".to_string())?;
    let mut entries = Vec::new();
    for object in split_top_level_objects(&entries_body) {
        let model_id = extract_json_string(&object, "model_id")
            .ok_or_else(|| "missing model_id".to_string())?;
        let contract_hash = extract_json_string(&object, "contract_hash")
            .ok_or_else(|| "missing contract_hash".to_string())?;
        let state_fields = extract_string_array(&object, "state_fields")
            .ok_or_else(|| "missing state_fields".to_string())?;
        let actions = extract_string_array(&object, "actions")
            .ok_or_else(|| "missing actions".to_string())?;
        let properties = extract_string_array(&object, "properties")
            .ok_or_else(|| "missing properties".to_string())?;
        entries.push(ContractSnapshot {
            model_id,
            state_fields,
            actions,
            properties,
            contract_hash,
        });
    }
    Ok(ContractLockFile {
        schema_version,
        generated_at,
        entries,
    })
}

fn field_type_label(field_type: &FieldType) -> String {
    match field_type {
        FieldType::Bool => "bool".to_string(),
        FieldType::String { min_len, max_len } => match (min_len, max_len) {
            (Some(min), Some(max)) => format!("string[{min}..={max}]"),
            _ => "string".to_string(),
        },
        FieldType::BoundedU8 { min, max } => format!("u8[{min}..{max}]"),
        FieldType::BoundedU16 { min, max } => format!("u16[{min}..{max}]"),
        FieldType::BoundedU32 { min, max } => format!("u32[{min}..{max}]"),
        FieldType::Enum { variants } => format!("enum[{}]", variants.join("|")),
        FieldType::EnumSet { variants } => format!("enum_set[{}]", variants.join("|")),
        FieldType::EnumRelation {
            left_variants,
            right_variants,
        } => format!(
            "enum_relation[{} -> {}]",
            left_variants.join("|"),
            right_variants.join("|")
        ),
        FieldType::EnumMap {
            key_variants,
            value_variants,
        } => format!(
            "enum_map[{} => {}]",
            key_variants.join("|"),
            value_variants.join("|")
        ),
    }
}

fn property_kind_label(kind: &PropertyKind) -> &'static str {
    kind.as_str()
        PropertyKind::DeadlockFreedom => "deadlock_freedom",
}

fn extract_json_string(body: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":\"");
    let start = body.find(&marker)? + marker.len();
    let rest = &body[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn extract_array_body(body: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":[");
    let start = body.find(&marker)? + marker.len();
    let rest = &body[start..];
    let mut depth = 1i32;
    for (index, ch) in rest.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(rest[..index].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_string_array(body: &str, key: &str) -> Option<Vec<String>> {
    let inner = extract_array_body(body, key)?;
    if inner.trim().is_empty() {
        return Some(vec![]);
    }
    Some(
        inner
            .split(',')
            .map(|item| item.trim().trim_matches('"').to_string())
            .collect(),
    )
}

fn split_top_level_objects(body: &str) -> Vec<String> {
    let mut objects = Vec::new();
    let mut depth = 0i32;
    let mut start = None;
    for (index, ch) in body.char_indices() {
        match ch {
            '{' => {
                if depth == 0 {
                    start = Some(index);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(from) = start {
                        objects.push(body[from..=index].to_string());
                    }
                    start = None;
                }
            }
            _ => {}
        }
    }
    objects
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::frontend::compile_model;

    use super::{
        build_lock_file, compare_snapshot, parse_lock_file, render_drift_json, render_lock_json,
        snapshot_model, write_lock_file,
    };

    #[test]
    fn snapshot_hash_changes_when_contract_changes() {
        let model_a = compile_model("model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\nproperty P_SAFE:\n  invariant: x <= 7\n").unwrap();
        let model_b = compile_model("model A\nstate:\n  y: u8[0..7]\ninit:\n  y = 0\nproperty P_SAFE:\n  invariant: y <= 7\n").unwrap();
        let snap_a = snapshot_model(&model_a);
        let snap_b = snapshot_model(&model_b);
        assert!(compare_snapshot(&snap_a, &snap_b)
            .changes
            .contains(&"state_fields".to_string()));
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

    #[test]
    fn parses_rendered_lock_json() {
        let model = compile_model("model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\nproperty P_SAFE:\n  invariant: x <= 7\n").unwrap();
        let lock = build_lock_file(vec![snapshot_model(&model)]);
        let parsed = parse_lock_file(&render_lock_json(&lock)).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.entries[0].model_id, "A");
    }
}
