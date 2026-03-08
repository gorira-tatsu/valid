use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use serde::{Deserialize, Serialize};

use crate::support::{
    artifact::{artifact_index_path, run_history_path},
    io::write_text_file,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub artifact_kind: String,
    pub path: String,
    pub run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suite_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactIndex {
    pub schema_version: String,
    pub artifacts: Vec<ArtifactRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunHistoryEntry {
    pub run_id: String,
    pub artifact_paths: Vec<String>,
    pub artifact_kinds: Vec<String>,
    #[serde(default)]
    pub model_ids: Vec<String>,
    #[serde(default)]
    pub property_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunHistory {
    pub schema_version: String,
    pub runs: Vec<RunHistoryEntry>,
}

impl Default for ArtifactIndex {
    fn default() -> Self {
        Self {
            schema_version: "1.0.0".to_string(),
            artifacts: Vec::new(),
        }
    }
}

impl Default for RunHistory {
    fn default() -> Self {
        Self {
            schema_version: "1.0.0".to_string(),
            runs: Vec::new(),
        }
    }
}

pub fn record_artifact(record: ArtifactRecord) -> Result<(), String> {
    let _guard = artifact_index_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if record.run_id.trim().is_empty() {
        return Err("artifact record requires a non-empty run_id".to_string());
    }
    if record.path.trim().is_empty() {
        return Err("artifact record requires a non-empty path".to_string());
    }

    let index_path = artifact_index_path();
    let history_path = run_history_path();
    let mut index = load_artifact_index_from_path(Path::new(&index_path))?;
    index.artifacts.retain(|entry| entry.path != record.path);
    index.artifacts.push(record.clone());
    index.artifacts.sort_by(|left, right| {
        left.run_id
            .cmp(&right.run_id)
            .then(left.path.cmp(&right.path))
    });
    write_text_file(
        &index_path,
        &serde_json::to_string_pretty(&index).map_err(|err| err.to_string())?,
    )?;

    let mut history = load_run_history_from_path(Path::new(&history_path))?;
    let mut entry = history
        .runs
        .iter()
        .find(|run| run.run_id == record.run_id)
        .cloned()
        .unwrap_or(RunHistoryEntry {
            run_id: record.run_id.clone(),
            artifact_paths: Vec::new(),
            artifact_kinds: Vec::new(),
            model_ids: Vec::new(),
            property_ids: Vec::new(),
        });
    if !entry.artifact_paths.iter().any(|path| path == &record.path) {
        entry.artifact_paths.push(record.path.clone());
    }
    if !entry
        .artifact_kinds
        .iter()
        .any(|kind| kind == &record.artifact_kind)
    {
        entry.artifact_kinds.push(record.artifact_kind.clone());
    }
    if let Some(model_id) = &record.model_id {
        if !entry.model_ids.iter().any(|value| value == model_id) {
            entry.model_ids.push(model_id.clone());
        }
    }
    if let Some(property_id) = &record.property_id {
        if !entry.property_ids.iter().any(|value| value == property_id) {
            entry.property_ids.push(property_id.clone());
        }
    }
    entry.artifact_paths.sort();
    entry.artifact_kinds.sort();
    entry.model_ids.sort();
    entry.property_ids.sort();
    history.runs.retain(|run| run.run_id != record.run_id);
    history.runs.push(entry);
    history
        .runs
        .sort_by(|left, right| left.run_id.cmp(&right.run_id));
    write_text_file(
        &history_path,
        &serde_json::to_string_pretty(&history).map_err(|err| err.to_string())?,
    )?;
    Ok(())
}

fn artifact_index_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn load_artifact_index() -> Result<ArtifactIndex, String> {
    load_artifact_index_from_path(Path::new(&artifact_index_path()))
}

pub fn load_run_history() -> Result<RunHistory, String> {
    load_run_history_from_path(Path::new(&run_history_path()))
}

fn load_artifact_index_from_path(path: &Path) -> Result<ArtifactIndex, String> {
    load_json(path)
}

fn load_run_history_from_path(path: &Path) -> Result<RunHistory, String> {
    load_json(path)
}

fn load_json<T>(path: &Path) -> Result<T, String>
where
    T: for<'de> Deserialize<'de> + Default,
{
    if !path.exists() {
        return Ok(T::default());
    }
    let body = fs::read_to_string(path)
        .map_err(|err| format!("failed to read `{}`: {err}", path.display()))?;
    if body.trim().is_empty() {
        return Ok(T::default());
    }
    serde_json::from_str(&body)
        .map_err(|err| format!("failed to parse `{}`: {err}", path.display()))
}

pub fn render_artifact_inventory_json(
    index: &ArtifactIndex,
    history: &RunHistory,
) -> Result<String, String> {
    serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": "1.0.0",
        "artifact_index_path": artifact_index_path(),
        "run_history_path": run_history_path(),
        "artifacts": index.artifacts,
        "runs": history.runs,
    }))
    .map_err(|err| err.to_string())
}

pub fn render_artifact_inventory_text(index: &ArtifactIndex, history: &RunHistory) -> String {
    let mut out = String::new();
    out.push_str(&format!("artifact_index: {}\n", artifact_index_path()));
    out.push_str(&format!("run_history: {}\n", run_history_path()));
    out.push_str(&format!("artifact_count: {}\n", index.artifacts.len()));
    out.push_str(&format!("run_count: {}\n", history.runs.len()));
    for run in &history.runs {
        out.push_str(&format!("\nrun_id: {}\n", run.run_id));
        if !run.model_ids.is_empty() {
            out.push_str(&format!("  models: {}\n", run.model_ids.join(", ")));
        }
        if !run.property_ids.is_empty() {
            out.push_str(&format!("  properties: {}\n", run.property_ids.join(", ")));
        }
        out.push_str(&format!("  kinds: {}\n", run.artifact_kinds.join(", ")));
        for path in &run.artifact_paths {
            out.push_str(&format!("  artifact: {path}\n"));
        }
    }
    out
}

pub fn synthetic_run_id(prefix: &str, key: &str) -> String {
    let mut parts = BTreeSet::new();
    for token in key
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
    {
        if !token.is_empty() {
            parts.insert(token.to_ascii_lowercase());
        }
    }
    let suffix = if parts.is_empty() {
        "artifact".to_string()
    } else {
        parts.into_iter().collect::<Vec<_>>().join("-")
    };
    format!("{prefix}-{suffix}")
}

pub fn repo_relative_path(path: &str) -> String {
    let path_buf = PathBuf::from(path);
    path_buf.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::{record_artifact, ArtifactRecord};

    #[test]
    fn synthetic_run_id_is_stable() {
        assert_eq!(
            super::synthetic_run_id("doc", "CounterModel.md"),
            "doc-countermodel-md"
        );
    }

    #[test]
    fn record_artifact_rejects_empty_fields() {
        let error = record_artifact(ArtifactRecord {
            artifact_kind: "doc".to_string(),
            path: String::new(),
            run_id: String::new(),
            model_id: None,
            property_id: None,
            evidence_id: None,
            vector_id: None,
            suite_id: None,
        })
        .expect_err("empty record should fail");
        assert!(error.contains("run_id") || error.contains("path"));
    }
}
