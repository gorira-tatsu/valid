use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub(crate) struct DocEntry {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub audience: String,
    pub recommended_for: Vec<String>,
    pub canonical_entry: bool,
    pub summary: String,
    pub key_points: Vec<String>,
    pub canonical_rules: Vec<String>,
    pub supported_features: Vec<String>,
    pub unsupported_features: Vec<String>,
    pub related_docs: Vec<String>,
    pub source_path: String,
    pub body_markdown: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct ExampleEntry {
    pub id: String,
    pub title: String,
    pub difficulty: String,
    pub concepts: Vec<String>,
    pub mode: String,
    pub backend_expectation: String,
    pub source_path: String,
    pub recommended_order: u64,
    pub recommended_docs: Vec<String>,
    pub focus_models: Vec<String>,
    pub summary: String,
    pub commands: Vec<String>,
    pub source_text: &'static str,
}

#[derive(Debug, Deserialize)]
struct DocManifestEntry {
    id: String,
    title: String,
    kind: String,
    audience: String,
    recommended_for: Vec<String>,
    canonical_entry: bool,
    summary: String,
    key_points: Vec<String>,
    canonical_rules: Vec<String>,
    supported_features: Vec<String>,
    unsupported_features: Vec<String>,
    related_docs: Vec<String>,
    source_path: String,
}

#[derive(Debug, Deserialize)]
struct ExampleManifestEntry {
    id: String,
    title: String,
    difficulty: String,
    concepts: Vec<String>,
    mode: String,
    backend_expectation: String,
    source_path: String,
    recommended_order: u64,
    recommended_docs: Vec<String>,
    focus_models: Vec<String>,
    summary: String,
    commands: Vec<String>,
}

const DOC_MANIFEST_JSON: &str = include_str!("../../../../docs/mcp/catalog.docs.json");
const EXAMPLE_MANIFEST_JSON: &str = include_str!("../../../../docs/mcp/catalog.examples.json");

static DOCS: OnceLock<Vec<DocEntry>> = OnceLock::new();
static EXAMPLES: OnceLock<Vec<ExampleEntry>> = OnceLock::new();

pub(crate) fn docs() -> &'static [DocEntry] {
    DOCS.get_or_init(load_docs).as_slice()
}

pub(crate) fn examples() -> &'static [ExampleEntry] {
    EXAMPLES.get_or_init(load_examples).as_slice()
}

pub(crate) fn docs_index() -> Vec<Value> {
    docs().iter().map(doc_summary).collect()
}

pub(crate) fn docs_canonical_entry() -> &'static str {
    docs()
        .iter()
        .find(|doc| doc.canonical_entry)
        .map(|doc| doc.id.as_str())
        .unwrap_or("ai-authoring-guide")
}

pub(crate) fn doc_entry(id: &str) -> Option<&'static DocEntry> {
    docs().iter().find(|doc| doc.id == id)
}

pub(crate) fn examples_index() -> Vec<Value> {
    examples().iter().map(example_summary).collect()
}

pub(crate) fn example_entry(id: &str) -> Option<&'static ExampleEntry> {
    examples().iter().find(|example| example.id == id)
}

fn load_docs() -> Vec<DocEntry> {
    let manifest: Vec<DocManifestEntry> =
        serde_json::from_str(DOC_MANIFEST_JSON).expect("docs manifest should parse");
    manifest
        .into_iter()
        .map(|entry| DocEntry {
            body_markdown: doc_body_markdown(&entry.source_path),
            id: entry.id,
            title: entry.title,
            kind: entry.kind,
            audience: entry.audience,
            recommended_for: entry.recommended_for,
            canonical_entry: entry.canonical_entry,
            summary: entry.summary,
            key_points: entry.key_points,
            canonical_rules: entry.canonical_rules,
            supported_features: entry.supported_features,
            unsupported_features: entry.unsupported_features,
            related_docs: entry.related_docs,
            source_path: entry.source_path,
        })
        .collect()
}

fn load_examples() -> Vec<ExampleEntry> {
    let manifest: Vec<ExampleManifestEntry> =
        serde_json::from_str(EXAMPLE_MANIFEST_JSON).expect("examples manifest should parse");
    manifest
        .into_iter()
        .map(|entry| ExampleEntry {
            source_text: example_source_text(&entry.source_path),
            id: entry.id,
            title: entry.title,
            difficulty: entry.difficulty,
            concepts: entry.concepts,
            mode: entry.mode,
            backend_expectation: entry.backend_expectation,
            source_path: entry.source_path,
            recommended_order: entry.recommended_order,
            recommended_docs: entry.recommended_docs,
            focus_models: entry.focus_models,
            summary: entry.summary,
            commands: entry.commands,
        })
        .collect()
}

fn doc_body_markdown(source_path: &str) -> &'static str {
    match source_path {
        "docs/README.md" => include_str!("../../../../docs/README.md"),
        "docs/ai/authoring-guide.md" => include_str!("../../../../docs/ai/authoring-guide.md"),
        "docs/ai/requirement-refinement-workflow.md" => {
            include_str!("../../../../docs/ai/requirement-refinement-workflow.md")
        }
        "docs/ai/candidate-comparison-workflow.md" => {
            include_str!("../../../../docs/ai/candidate-comparison-workflow.md")
        }
        "docs/ai/conformance-workflow.md" => {
            include_str!("../../../../docs/ai/conformance-workflow.md")
        }
        "docs/ai/common-pitfalls.md" => include_str!("../../../../docs/ai/common-pitfalls.md"),
        "docs/ai/curriculum.md" => include_str!("../../../../docs/ai/curriculum.md"),
        "docs/ai/examples-curriculum.md" => {
            include_str!("../../../../docs/ai/examples-curriculum.md")
        }
        "docs/ai/migration-guide.md" => include_str!("../../../../docs/ai/migration-guide.md"),
        "docs/ai/modeling-checklist.md" => {
            include_str!("../../../../docs/ai/modeling-checklist.md")
        }
        "docs/ai/model-authoring-best-practices.md" => {
            include_str!("../../../../docs/ai/model-authoring-best-practices.md")
        }
        "docs/ai/review-workflow.md" => include_str!("../../../../docs/ai/review-workflow.md"),
        "docs/architecture.md" => include_str!("../../../../docs/architecture.md"),
        "docs/artifacts.md" => include_str!("../../../../docs/artifacts.md"),
        "docs/ci/README.md" => include_str!("../../../../docs/ci/README.md"),
        "docs/composition.md" => include_str!("../../../../docs/composition.md"),
        "docs/dsl/README.md" => include_str!("../../../../docs/dsl/README.md"),
        "docs/graph-and-review.md" => include_str!("../../../../docs/graph-and-review.md"),
        "docs/dsl/language-evolution.md" => {
            include_str!("../../../../docs/dsl/language-evolution.md")
        }
        "docs/dsl/parameterized-action-roadmap.md" => {
            include_str!("../../../../docs/dsl/parameterized-action-roadmap.md")
        }
        "docs/dsl/language-spec.md" => include_str!("../../../../docs/dsl/language-spec.md"),
        "docs/install.md" => include_str!("../../../../docs/install.md"),
        "docs/quickstart.md" => include_str!("../../../../docs/quickstart.md"),
        "docs/project-organization.md" => {
            include_str!("../../../../docs/project-organization.md")
        }
        "docs/testgen-and-handoff.md" => {
            include_str!("../../../../docs/testgen-and-handoff.md")
        }
        "docs/testgen-strategies.md" => include_str!("../../../../docs/testgen-strategies.md"),
        other => panic!("unmapped docs catalog source_path `{other}`"),
    }
}

fn example_source_text(source_path: &str) -> &'static str {
    match source_path {
        "examples/valid_models.rs" => include_str!("../../../../examples/valid_models.rs"),
        "examples/tenant_relation_registry.rs" => {
            include_str!("../../../../examples/tenant_relation_registry.rs")
        }
        "examples/saas_multi_tenant_registry.rs" => {
            include_str!("../../../../examples/saas_multi_tenant_registry.rs")
        }
        "examples/password_policy.rs" => include_str!("../../../../examples/password_policy.rs"),
        "examples/compose_helper_registry.rs" => {
            include_str!("../../../../examples/compose_helper_registry.rs")
        }
        "examples/deadlock_enablement_registry.rs" => {
            include_str!("../../../../examples/deadlock_enablement_registry.rs")
        }
        "examples/handoff_testgen_registry.rs" => {
            include_str!("../../../../examples/handoff_testgen_registry.rs")
        }
        other => panic!("unmapped examples catalog source_path `{other}`"),
    }
}

fn doc_summary(doc: &DocEntry) -> Value {
    json!({
        "doc_id": doc.id,
        "title": doc.title,
        "kind": doc.kind,
        "audience": doc.audience,
        "recommended_for": doc.recommended_for,
        "canonical_entry": doc.canonical_entry,
        "summary": doc.summary,
        "related_docs": doc.related_docs,
        "source_path": doc.source_path
    })
}

fn example_summary(example: &ExampleEntry) -> Value {
    json!({
        "example_id": example.id,
        "title": example.title,
        "difficulty": example.difficulty,
        "concepts": example.concepts,
        "mode": example.mode,
        "backend_expectation": example.backend_expectation,
        "recommended_order": example.recommended_order,
        "source_path": example.source_path,
        "focus_models": example.focus_models,
        "summary": example.summary
    })
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, fs, path::PathBuf};

    use super::{doc_entry, docs, docs_canonical_entry, example_entry, examples};

    fn manifest_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn doc_ids_are_unique_and_have_one_canonical_entry() {
        let ids = docs()
            .iter()
            .map(|doc| doc.id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), docs().len(), "duplicate doc ids");

        let canonical = docs().iter().filter(|doc| doc.canonical_entry).count();
        assert_eq!(canonical, 1, "expected exactly one canonical entry");
        assert!(doc_entry(docs_canonical_entry()).is_some());
    }

    #[test]
    fn example_ids_are_unique() {
        let ids = examples()
            .iter()
            .map(|example| example.id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), examples().len(), "duplicate example ids");
        for example in examples() {
            assert!(
                example_entry(&example.id).is_some(),
                "missing example entry {}",
                example.id
            );
        }
    }

    #[test]
    fn catalog_covers_all_manifest_listed_public_docs() {
        for doc in docs() {
            let path = manifest_dir().join(&doc.source_path);
            assert!(
                path.is_file(),
                "missing public doc source {}",
                doc.source_path
            );
        }
    }

    #[test]
    fn related_doc_references_resolve() {
        let ids = docs()
            .iter()
            .map(|doc| doc.id.as_str())
            .collect::<BTreeSet<_>>();
        for doc in docs() {
            for related in &doc.related_docs {
                assert!(
                    ids.contains(related.as_str()),
                    "unknown related doc `{related}` in {}",
                    doc.id
                );
            }
        }
    }

    #[test]
    fn internal_docs_are_not_exposed() {
        let catalog = docs()
            .iter()
            .map(|doc| doc.source_path.as_str())
            .collect::<BTreeSet<_>>();
        assert!(!catalog.contains("docs/adr/0001-valid-model-frontend.md"));
        assert!(
            catalog.iter().all(|path| !path.starts_with("docs/rdd/")),
            "RDD docs must stay out of the MCP docs catalog"
        );
        assert!(!catalog.contains("docs/dsl/fizzbuzz-validation-report.md"));
        assert!(doc_entry("frontend-adr").is_none());
        assert!(doc_entry("fizzbuzz-validation-report").is_none());
    }

    #[test]
    fn example_manifest_source_paths_exist() {
        for example in examples() {
            let path = manifest_dir().join(&example.source_path);
            assert!(
                path.is_file(),
                "missing example source {}",
                example.source_path
            );
        }
    }

    #[test]
    fn manifests_are_valid_json_files() {
        let docs_manifest = manifest_dir().join("docs/mcp/catalog.docs.json");
        let examples_manifest = manifest_dir().join("docs/mcp/catalog.examples.json");
        let docs_text =
            fs::read_to_string(docs_manifest).expect("docs manifest should be readable");
        let examples_text =
            fs::read_to_string(examples_manifest).expect("examples manifest should be readable");
        serde_json::from_str::<serde_json::Value>(&docs_text).expect("docs manifest should parse");
        serde_json::from_str::<serde_json::Value>(&examples_text)
            .expect("examples manifest should parse");
    }
}
