use serde_json::Value;

#[derive(Clone, Copy)]
pub(crate) struct PromptArgument {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct PromptEntry {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub arguments: &'static [PromptArgument],
}

pub(crate) const PROMPTS: &[PromptEntry] = &[
    PromptEntry {
        name: "author_model",
        title: "Author Model",
        description: "Guide an LLM through authoring a new valid model from a domain description.",
        arguments: &[
            PromptArgument {
                name: "domain",
                description: "Short natural-language description of the business rule or workflow to model.",
                required: true,
            },
            PromptArgument {
                name: "preferred_mode",
                description: "Preferred modeling mode such as registry or dsl.",
                required: false,
            },
            PromptArgument {
                name: "constraints",
                description: "Optional implementation constraints, backend expectations, or property goals.",
                required: false,
            },
        ],
    },
    PromptEntry {
        name: "review_model",
        title: "Review Model",
        description: "Guide an LLM through reviewing an existing valid model for correctness, readiness, and migration risks.",
        arguments: &[
            PromptArgument {
                name: "target",
                description: "Model file path, registry model name, or inline source label.",
                required: true,
            },
            PromptArgument {
                name: "review_focus",
                description: "Optional focus such as readiness, invariants, transitions, or AI ergonomics.",
                required: false,
            },
        ],
    },
    PromptEntry {
        name: "migrate_step_to_transitions",
        title: "Migrate Step To Transitions",
        description: "Guide an LLM through migrating a step-oriented model toward declarative transitions.",
        arguments: &[
            PromptArgument {
                name: "target",
                description: "Model file path, registry model name, or inline source label.",
                required: true,
            },
            PromptArgument {
                name: "constraints",
                description: "Optional migration constraints such as preserving action ids or tags.",
                required: false,
            },
        ],
    },
    PromptEntry {
        name: "explain_readiness_failure",
        title: "Explain Readiness Failure",
        description: "Guide an LLM through interpreting valid_lint or readiness failures and deciding next steps.",
        arguments: &[
            PromptArgument {
                name: "target",
                description: "Model file path, registry model name, or inline source label.",
                required: true,
            },
            PromptArgument {
                name: "lint_result",
                description: "Optional lint/readiness JSON summary or copied finding text.",
                required: false,
            },
        ],
    },
];

pub(crate) fn prompt_entry(name: &str) -> Option<PromptEntry> {
    PROMPTS.iter().copied().find(|entry| entry.name == name)
}

pub(crate) fn prompt_definition(entry: PromptEntry) -> Value {
    serde_json::json!({
        "name": entry.name,
        "title": entry.title,
        "description": entry.description,
        "arguments": entry.arguments.iter().map(|argument| serde_json::json!({
            "name": argument.name,
            "title": argument.name,
            "description": argument.description,
            "required": argument.required
        })).collect::<Vec<_>>()
    })
}
