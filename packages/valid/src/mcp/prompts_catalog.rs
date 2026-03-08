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
        name: "refine_requirement",
        title: "Refine Requirement",
        description: "Guide an LLM through turning an ambiguous product requirement into a stable modeling brief before model authoring or review.",
        arguments: &[
            PromptArgument {
                name: "requirement",
                description: "Natural-language requirement, feature request, policy text, or incident summary that still needs refinement.",
                required: true,
            },
            PromptArgument {
                name: "current_brief",
                description: "Optional current modeling brief, assumptions list, or requirement summary to tighten instead of starting from scratch.",
                required: false,
            },
            PromptArgument {
                name: "risk_area",
                description: "Optional business, compliance, UX, or operational risk area to prioritize while asking follow-up questions.",
                required: false,
            },
        ],
    },
    PromptEntry {
        name: "clarify_requirement",
        title: "Clarify Requirement",
        description: "Guide an LLM through turning an ambiguous requirement into a concrete modeling brief before authoring begins.",
        arguments: &[
            PromptArgument {
                name: "requirement",
                description: "Natural-language requirement or feature description that still needs clarification.",
                required: true,
            },
            PromptArgument {
                name: "risk_area",
                description: "Optional business, compliance, or UX risk area to prioritize while asking follow-up questions.",
                required: false,
            },
        ],
    },
    PromptEntry {
        name: "refine_requirement_from_evidence",
        title: "Refine Requirement From Evidence",
        description: "Guide an LLM through asking targeted follow-up questions when counterexamples, dead actions, vacuity, coverage gaps, or mismatches show the requirement brief is incomplete.",
        arguments: &[
            PromptArgument {
                name: "current_brief",
                description: "Current requirement brief or modeling brief that should be treated as the baseline.",
                required: true,
            },
            PromptArgument {
                name: "evidence_kind",
                description: "Triggering signal such as counterexample, dead_action, vacuity, coverage_gap, or conformance_mismatch.",
                required: true,
            },
            PromptArgument {
                name: "evidence_summary",
                description: "Short summary of the failing trace, dead action report, vacuity clue, or mismatch.",
                required: true,
            },
            PromptArgument {
                name: "risk_area",
                description: "Optional business, compliance, UX, or operational risk area to keep centered during follow-up.",
                required: false,
            },
        ],
    },
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
    PromptEntry {
        name: "triage_conformance_failure",
        title: "Triage Conformance Failure",
        description: "Guide an LLM through classifying a model-versus-implementation mismatch and deciding the next repair surface.",
        arguments: &[
            PromptArgument {
                name: "target",
                description: "Model file path, registry model name, or inline source label.",
                required: true,
            },
            PromptArgument {
                name: "conformance_result",
                description: "Optional conformance JSON summary or copied mismatch text.",
                required: false,
            },
            PromptArgument {
                name: "sut_surface",
                description: "Optional implementation surface such as api, ui, handler, or runner.",
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
