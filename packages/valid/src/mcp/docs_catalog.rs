use serde_json::{json, Value};

#[derive(Clone, Copy)]
pub(crate) struct DocEntry {
    pub id: &'static str,
    pub title: &'static str,
    pub kind: &'static str,
    pub audience: &'static str,
    pub recommended_for: &'static [&'static str],
    pub canonical_entry: bool,
    pub summary: &'static str,
    pub key_points: &'static [&'static str],
    pub canonical_rules: &'static [&'static str],
    pub supported_features: &'static [&'static str],
    pub unsupported_features: &'static [&'static str],
    pub related_docs: &'static [&'static str],
    pub source_path: &'static str,
    pub body_markdown: &'static str,
}

#[derive(Clone, Copy)]
pub(crate) struct ExampleEntry {
    pub id: &'static str,
    pub title: &'static str,
    pub difficulty: &'static str,
    pub concepts: &'static [&'static str],
    pub mode: &'static str,
    pub backend_expectation: &'static str,
    pub source_path: &'static str,
    pub recommended_order: u64,
    pub recommended_docs: &'static [&'static str],
    pub focus_models: &'static [&'static str],
    pub summary: &'static str,
    pub commands: &'static [&'static str],
    pub source_text: &'static str,
}

pub(crate) const DOCS: &[DocEntry] = &[
    DocEntry {
        id: "docs-index",
        title: "Documentation Index",
        kind: "index",
        audience: "humans-and-agents",
        recommended_for: &["navigation", "docs-overview"],
        canonical_entry: false,
        summary: "Top-level documentation index for install, AI authoring, DSL, architecture, and RDD documents.",
        key_points: &[
            "Provides the top-level map of the non-RDD and RDD documentation tree",
            "Points LLM and MCP users to the AI authoring guide first",
        ],
        canonical_rules: &[
            "Use the AI authoring guide as the first AI-facing entrypoint",
        ],
        supported_features: &[
            "Top-level docs navigation",
            "Short descriptions for each major doc set",
        ],
        unsupported_features: &[
            "Normative language reference",
        ],
        related_docs: &["ai-authoring-guide", "dsl-guide", "install-guide", "architecture-note"],
        source_path: "docs/README.md",
        body_markdown: include_str!("../../../../docs/README.md"),
    },
    DocEntry {
        id: "frontend-adr",
        title: "ADR-0001: valid_model! Frontend Decision",
        kind: "adr",
        audience: "humans-and-agents",
        recommended_for: &["frontend-design", "macro-strategy"],
        canonical_entry: false,
        summary: "Decision record for keeping valid_model! on the macro_rules track unless rust-analyzer compatibility work fails.",
        key_points: &[
            "Explains why the current frontend stays on macro_rules",
            "Documents the fallback to a function-like proc-macro if acceptance fails",
        ],
        canonical_rules: &[
            "Treat the ADR as design rationale, not end-user syntax documentation",
        ],
        supported_features: &[
            "Frontend decision history",
            "Design tradeoff context",
        ],
        unsupported_features: &[
            "Current syntax reference",
        ],
        related_docs: &["dsl-guide", "language-evolution", "language-spec"],
        source_path: "docs/adr/0001-valid-model-frontend.md",
        body_markdown: include_str!("../../../../docs/adr/0001-valid-model-frontend.md"),
    },
    DocEntry {
        id: "ai-authoring-guide",
        title: "AI Authoring Guide",
        kind: "guide",
        audience: "llm-agents",
        recommended_for: &["first-read", "mcp-clients", "ai-assisted-authoring"],
        canonical_entry: true,
        summary: "Shortest canonical entrypoint for LLM agents writing or reviewing valid models.",
        key_points: &[
            "Use registry mode and cargo valid for new Rust-first work",
            "Prefer declarative transitions over step",
            "Check readiness before claiming solver support",
        ],
        canonical_rules: &[
            "Always write model Name<State, Action>;",
            "Treat step as explicit-first or migration-oriented",
            "Use valid_docs_index then valid_docs_get before model-specific tools",
        ],
        supported_features: &[
            "Minimal registry skeleton",
            "Finite state/action modeling rules",
            "Command and MCP workflow guidance",
        ],
        unsupported_features: &[
            "General containers like Vec and HashMap",
            "Implicit frame-condition inference",
            "Non-invariant property kinds",
        ],
        related_docs: &[
            "ai-modeling-checklist",
            "ai-common-pitfalls",
            "ai-examples-curriculum",
            "language-spec",
        ],
        source_path: "docs/ai/authoring-guide.md",
        body_markdown: include_str!("../../../../docs/ai/authoring-guide.md"),
    },
    DocEntry {
        id: "ai-common-pitfalls",
        title: "Common Pitfalls",
        kind: "pitfalls",
        audience: "llm-agents",
        recommended_for: &["error-avoidance", "review"],
        canonical_entry: false,
        summary: "Common mistakes LLMs make when generating or reviewing valid models.",
        key_points: &[
            "Do not use shorthand model headers",
            "Do not assume implicit field retention",
            "Do not overclaim solver support for string-heavy models",
        ],
        canonical_rules: &[
            "Registry mode is primary for new work",
            "Use transitions for canonical long-lived models",
        ],
        supported_features: &[
            "Examples of wrong and correct patterns",
            "Mode-selection reminders",
        ],
        unsupported_features: &[
            "Full supported-syntax inventory",
        ],
        related_docs: &["ai-authoring-guide", "language-spec"],
        source_path: "docs/ai/common-pitfalls.md",
        body_markdown: include_str!("../../../../docs/ai/common-pitfalls.md"),
    },
    DocEntry {
        id: "ai-examples-curriculum",
        title: "Examples Curriculum",
        kind: "examples-curriculum",
        audience: "llm-agents",
        recommended_for: &["learning-order", "few-shot-selection"],
        canonical_entry: false,
        summary: "Learning order for examples that teach valid incrementally.",
        key_points: &[
            "Start with the counter registry example",
            "Move to relations/maps and grouped transitions before string-heavy models",
        ],
        canonical_rules: &[
            "Inspect first, then readiness, then verify",
        ],
        supported_features: &[
            "Ordered curriculum",
            "Example selection by concept",
        ],
        unsupported_features: &[
            "Raw grammar reference",
        ],
        related_docs: &["ai-authoring-guide", "dsl-guide", "language-spec"],
        source_path: "docs/ai/examples-curriculum.md",
        body_markdown: include_str!("../../../../docs/ai/examples-curriculum.md"),
    },
    DocEntry {
        id: "ai-modeling-checklist",
        title: "Modeling Checklist",
        kind: "checklist",
        audience: "llm-agents",
        recommended_for: &["generation-review", "preflight"],
        canonical_entry: false,
        summary: "Preflight checklist for generated or reviewed valid models.",
        key_points: &[
            "Validate state, action, init, transition, and property shape",
            "Check capability expectations before verify claims",
        ],
        canonical_rules: &[
            "Every bounded integer field needs a range",
            "Every action variant should carry reads and writes metadata",
        ],
        supported_features: &[
            "Generation review checklist",
            "Capability review checklist",
        ],
        unsupported_features: &[
            "Product strategy",
            "Full grammar reference",
        ],
        related_docs: &["ai-authoring-guide", "language-spec"],
        source_path: "docs/ai/modeling-checklist.md",
        body_markdown: include_str!("../../../../docs/ai/modeling-checklist.md"),
    },
    DocEntry {
        id: "architecture-note",
        title: "Architecture",
        kind: "architecture",
        audience: "humans-and-agents",
        recommended_for: &["system-design", "package-roles"],
        canonical_entry: false,
        summary: "Repository architecture note covering package roles, layering, DTO boundaries, and solver-neutral design.",
        key_points: &[
            "Describes the clean-architecture split across packages and boundaries",
            "Explains solver-neutral layering and package responsibilities",
        ],
        canonical_rules: &[
            "Use this doc for repository design questions, not DSL syntax questions",
        ],
        supported_features: &[
            "Architecture overview",
            "Layering and DTO boundary explanation",
        ],
        unsupported_features: &[
            "End-user modeling syntax",
        ],
        related_docs: &["docs-index", "dsl-guide", "frontend-adr"],
        source_path: "docs/architecture.md",
        body_markdown: include_str!("../../../../docs/architecture.md"),
    },
    DocEntry {
        id: "dsl-guide",
        title: "Rust DSL Guide",
        kind: "guide",
        audience: "humans-and-agents",
        recommended_for: &["user-guide", "authoring"],
        canonical_entry: false,
        summary: "User-facing guide for writing, operating, and choosing between transitions and step.",
        key_points: &[
            "Explains the user-facing modeling language",
            "Covers canonical modeling path and current surface pieces",
        ],
        canonical_rules: &[
            "Prefer transitions for long-lived models",
            "Use readiness when diagnostics are ambiguous",
        ],
        supported_features: &[
            "Conceptual explanation of state, actions, model definition, and readiness",
        ],
        unsupported_features: &[
            "Normative grammar-level completeness",
        ],
        related_docs: &["ai-authoring-guide", "language-spec"],
        source_path: "docs/dsl/README.md",
        body_markdown: include_str!("../../../../docs/dsl/README.md"),
    },
    DocEntry {
        id: "fizzbuzz-validation-report",
        title: "FizzBuzz Model Validation Report",
        kind: "report",
        audience: "humans-and-agents",
        recommended_for: &["worked-example", "validation-review"],
        canonical_entry: false,
        summary: "Worked validation report for the FizzBuzz model, including strengths and known limitations in exploration depth.",
        key_points: &[
            "Shows how a concrete model behaves across inspect, verify, readiness, graph, coverage, and testgen",
            "Highlights a real limitation around shallow exploration depth",
        ],
        canonical_rules: &[
            "Treat this report as an example assessment, not as a normative spec",
        ],
        supported_features: &[
            "Worked example of command-by-command validation",
            "Concrete notes about strengths and current weaknesses",
        ],
        unsupported_features: &[
            "General language definition",
        ],
        related_docs: &["dsl-guide", "language-spec"],
        source_path: "docs/dsl/fizzbuzz-validation-report.md",
        body_markdown: include_str!("../../../../docs/dsl/fizzbuzz-validation-report.md"),
    },
    DocEntry {
        id: "language-evolution",
        title: "DSL Language Evolution",
        kind: "evolution",
        audience: "humans-and-agents",
        recommended_for: &["future-direction", "design-notes"],
        canonical_entry: false,
        summary: "Non-normative design notes about candidate directions for the valid DSL.",
        key_points: &[
            "Separates implemented behavior from future candidate features",
            "Covers relational actions, richer properties, text abstractions, and packaging",
        ],
        canonical_rules: &[
            "Do not treat this document as implemented behavior",
        ],
        supported_features: &[
            "Future design notes",
            "Candidate feature rationale",
        ],
        unsupported_features: &[
            "Normative current syntax guarantees",
        ],
        related_docs: &["language-spec", "frontend-adr", "dsl-guide"],
        source_path: "docs/dsl/language-evolution.md",
        body_markdown: include_str!("../../../../docs/dsl/language-evolution.md"),
    },
    DocEntry {
        id: "language-spec",
        title: "DSL Language Spec",
        kind: "spec",
        audience: "humans-and-agents",
        recommended_for: &["normative-reference", "supported-surface"],
        canonical_entry: false,
        summary: "Normative description of the currently implemented valid DSL surface and capability boundaries.",
        key_points: &[
            "Separates supported syntax, capability constraints, and non-goals",
            "Defines the implemented surface rather than future ideas",
        ],
        canonical_rules: &[
            "Shorthand model headers are unsupported",
            "Property kind surface is invariant-only",
        ],
        supported_features: &[
            "Supported types, metadata, expressions, and capability fields",
        ],
        unsupported_features: &[
            "General-purpose containers",
            "Infinite string theory",
            "Higher-order logic",
        ],
        related_docs: &["ai-authoring-guide", "dsl-guide"],
        source_path: "docs/dsl/language-spec.md",
        body_markdown: include_str!("../../../../docs/dsl/language-spec.md"),
    },
    DocEntry {
        id: "install-guide",
        title: "Install Guide",
        kind: "install",
        audience: "humans-and-agents",
        recommended_for: &["setup", "distribution"],
        canonical_entry: false,
        summary: "Installation and packaging guidance for binary users, Rust model authors, and Docker-based execution.",
        key_points: &[
            "Explains the practical install modes and feature flags",
            "Clarifies the Rust toolchain requirement for cargo valid workflows",
        ],
        canonical_rules: &[
            "Use cargo install for Rust model authoring and prebuilt binaries for binary-only usage",
        ],
        supported_features: &[
            "Install modes",
            "Backend selection notes",
            "Project setup commands",
        ],
        unsupported_features: &[
            "DSL syntax reference",
        ],
        related_docs: &["docs-index", "ai-authoring-guide", "dsl-guide"],
        source_path: "docs/install.md",
        body_markdown: include_str!("../../../../docs/install.md"),
    },
];

pub(crate) const EXAMPLES: &[ExampleEntry] = &[
    ExampleEntry {
        id: "registry-counter-basics",
        title: "Counter basics",
        difficulty: "intro",
        concepts: &["registry-shape", "bounded-int", "action-metadata", "step"],
        mode: "registry",
        backend_expectation: "explicit-ready",
        source_path: "examples/valid_models.rs",
        recommended_order: 1,
        recommended_docs: &["ai-authoring-guide", "ai-examples-curriculum", "dsl-guide"],
        focus_models: &["counter", "failing-counter"],
        summary: "Smallest registry-style example and the easiest place to learn inspect, verify, and counterexamples.",
        commands: &[
            "cargo valid --registry examples/valid_models.rs inspect counter",
            "cargo valid --registry examples/valid_models.rs verify failing-counter",
        ],
        source_text: include_str!("../../../../examples/valid_models.rs"),
    },
    ExampleEntry {
        id: "tenant-relations-map",
        title: "Tenant relations and map",
        difficulty: "intermediate",
        concepts: &["FiniteRelation", "FiniteMap", "tenant-isolation", "transitions"],
        mode: "registry",
        backend_expectation: "solver-ready",
        source_path: "examples/tenant_relation_registry.rs",
        recommended_order: 2,
        recommended_docs: &["ai-authoring-guide", "language-spec", "ai-examples-curriculum"],
        focus_models: &["tenant-relation-safe", "tenant-relation-regression"],
        summary: "Shows relation and map modeling in a small declarative cross-tenant policy example.",
        commands: &[
            "cargo valid --registry examples/tenant_relation_registry.rs inspect tenant-relation-safe",
            "cargo valid --registry examples/tenant_relation_registry.rs verify tenant-relation-regression --property=P_NO_CROSS_TENANT_ACCESS",
        ],
        source_text: include_str!("../../../../examples/tenant_relation_registry.rs"),
    },
    ExampleEntry {
        id: "saas-grouped-transitions",
        title: "SaaS grouped transitions",
        difficulty: "intermediate",
        concepts: &["grouped-transitions", "entitlements", "path-tags", "tenant-isolation"],
        mode: "registry",
        backend_expectation: "solver-ready",
        source_path: "examples/saas_multi_tenant_registry.rs",
        recommended_order: 3,
        recommended_docs: &["ai-authoring-guide", "dsl-guide", "ai-examples-curriculum"],
        focus_models: &["tenant-isolation-safe", "tenant-isolation-regression"],
        summary: "Demonstrates grouped declarative transitions for SaaS isolation and entitlement flows.",
        commands: &[
            "cargo valid --registry examples/saas_multi_tenant_registry.rs inspect tenant-isolation-safe",
            "cargo valid --registry examples/saas_multi_tenant_registry.rs verify tenant-isolation-regression --property=P_NO_CROSS_TENANT_ACCESS",
        ],
        source_text: include_str!("../../../../examples/saas_multi_tenant_registry.rs"),
    },
    ExampleEntry {
        id: "password-explicit-ready",
        title: "Password explicit-ready model",
        difficulty: "advanced",
        concepts: &["String", "regex_match", "explicit-ready", "readiness"],
        mode: "registry",
        backend_expectation: "explicit-ready",
        source_path: "examples/password_policy.rs",
        recommended_order: 4,
        recommended_docs: &["ai-authoring-guide", "language-spec", "ai-common-pitfalls"],
        focus_models: &["password-policy-safe", "password-policy-regression"],
        summary: "String and regex-heavy policy example that teaches capability boundaries and explicit-first expectations.",
        commands: &[
            "cargo valid --registry examples/password_policy.rs readiness password-policy-safe",
            "cargo valid --registry examples/password_policy.rs verify password-policy-regression --property=P_PASSWORD_POLICY_MATCHES_FLAG",
        ],
        source_text: include_str!("../../../../examples/password_policy.rs"),
    },
];

pub(crate) fn docs_index() -> Vec<Value> {
    DOCS.iter().copied().map(doc_summary).collect()
}

pub(crate) fn docs_canonical_entry() -> &'static str {
    DOCS.iter()
        .find(|doc| doc.canonical_entry)
        .map(|doc| doc.id)
        .unwrap_or("ai-authoring-guide")
}

pub(crate) fn doc_entry(id: &str) -> Option<DocEntry> {
    DOCS.iter().copied().find(|doc| doc.id == id)
}

pub(crate) fn examples_index() -> Vec<Value> {
    EXAMPLES.iter().copied().map(example_summary).collect()
}

pub(crate) fn example_entry(id: &str) -> Option<ExampleEntry> {
    EXAMPLES.iter().copied().find(|example| example.id == id)
}

fn doc_summary(doc: DocEntry) -> Value {
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

fn example_summary(example: ExampleEntry) -> Value {
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

    use super::{doc_entry, docs_canonical_entry, example_entry, DOCS, EXAMPLES};

    fn manifest_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn doc_ids_are_unique_and_have_one_canonical_entry() {
        let ids = DOCS.iter().map(|doc| doc.id).collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), DOCS.len(), "duplicate doc ids");

        let canonical = DOCS.iter().filter(|doc| doc.canonical_entry).count();
        assert_eq!(canonical, 1, "expected exactly one canonical entry");
        assert!(doc_entry(docs_canonical_entry()).is_some());
    }

    #[test]
    fn example_ids_are_unique() {
        let ids = EXAMPLES
            .iter()
            .map(|example| example.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), EXAMPLES.len(), "duplicate example ids");
        for example in EXAMPLES {
            assert!(
                example_entry(example.id).is_some(),
                "missing example entry {}",
                example.id
            );
        }
    }

    #[test]
    fn catalog_covers_all_non_rdd_markdown_docs() {
        let docs_root = manifest_dir().join("docs");
        let actual = walk_markdown_files(&docs_root)
            .into_iter()
            .filter(|path| !path.starts_with("docs/rdd/"))
            .collect::<BTreeSet<_>>();
        let catalog = DOCS
            .iter()
            .map(|doc| doc.source_path.to_string())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            catalog, actual,
            "docs catalog is missing or over-reporting markdown docs"
        );
    }

    fn walk_markdown_files(root: &PathBuf) -> Vec<String> {
        let mut files = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let entries = fs::read_dir(&dir).expect("docs directory should be readable");
            for entry in entries {
                let entry = entry.expect("docs entry should be readable");
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                    continue;
                }
                let relative = path
                    .strip_prefix(manifest_dir())
                    .expect("path should be inside manifest dir")
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push(relative);
            }
        }
        files.sort();
        files
    }
}
