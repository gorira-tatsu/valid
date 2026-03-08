# AI Docs Curriculum

This page organizes the AI-facing `valid` docs in two ways:

- learning order: what to read when you are new to the tool
- task order: what to read when you are trying to do a specific job

Use this page when the AI or human reviewer needs more than the short
authoring guide.

## Learning order

1. [AI Authoring Guide](./authoring-guide.md)
   Start here for the canonical path, supported building blocks, and the
   shortest command/tool workflow.
2. [Requirement Refinement Workflow](./requirement-refinement-workflow.md)
   Use this before modeling when the product behavior is still ambiguous or
   when evidence shows requirement drift.
3. [Examples Curriculum](./examples-curriculum.md)
   Learn the examples in increasing modeling complexity.
4. [Modeling Checklist](./modeling-checklist.md)
   Use this as a preflight before returning a new or edited model.
5. [Common Pitfalls](./common-pitfalls.md)
   Read this after the first draft so you can catch common mistakes quickly.
6. [Review Workflow](./review-workflow.md)
   Switch here when the task becomes "review and explain" instead of "write".
7. [Candidate Comparison Workflow](./candidate-comparison-workflow.md)
   Use this when two candidate models both look plausible and you need a
   shortest distinguishing trace before choosing one.
8. [Migration Guide](./migration-guide.md)
   Use this when moving from `step` to declarative `transitions`, or when
   reducing readiness/lint findings.
9. [Conformance Workflow](./conformance-workflow.md)
   Use this when you need to connect model evidence to a real implementation.

### Refine an ambiguous requirement

1. [Requirement Refinement Workflow](./requirement-refinement-workflow.md)
2. [AI Authoring Guide](./authoring-guide.md)
3. [Modeling Checklist](./modeling-checklist.md)
4. Current MCP prompts:
   - `refine_requirement`
   - `refine_requirement_from_evidence`
   - `clarify_requirement` for compatibility-oriented clients

## Task order

### Author a new model

1. [AI Authoring Guide](./authoring-guide.md)
2. [Requirement Refinement Workflow](./requirement-refinement-workflow.md)
3. [Examples Curriculum](./examples-curriculum.md)
4. [Modeling Checklist](./modeling-checklist.md)
5. Current MCP prompts:
   - `refine_requirement`
   - `author_model`
   - `explain_readiness_failure`

### Review an existing model

1. [Review Workflow](./review-workflow.md)
2. [Requirement Refinement Workflow](./requirement-refinement-workflow.md)
3. [Candidate Comparison Workflow](./candidate-comparison-workflow.md)
4. [Common Pitfalls](./common-pitfalls.md)
5. [Modeling Checklist](./modeling-checklist.md)
5. Current MCP prompts:
   - `refine_requirement_from_evidence`
   - `review_model`
   - `explain_readiness_failure`

### Compare competing model candidates

1. [Candidate Comparison Workflow](./candidate-comparison-workflow.md)
2. [Review Workflow](./review-workflow.md)
3. [Requirement Refinement Workflow](./requirement-refinement-workflow.md)
4. Current MCP prompts:
   - `compare_candidate_models`
   - `refine_requirement_from_evidence`

### Migrate a model

1. [Migration Guide](./migration-guide.md)
2. [AI Authoring Guide](./authoring-guide.md)
3. [Requirement Refinement Workflow](./requirement-refinement-workflow.md)
4. [Examples Curriculum](./examples-curriculum.md)
5. Current MCP prompt:
   - `migrate_step_to_transitions`

### Review conformance and runtime mismatch

1. [Conformance Workflow](./conformance-workflow.md)
2. [Requirement Refinement Workflow](./requirement-refinement-workflow.md)
3. [Review Workflow](./review-workflow.md)
4. [Modeling Checklist](./modeling-checklist.md)
5. Current MCP prompts:
   - `refine_requirement_from_evidence`
   - `triage_conformance_failure`

## Coherent AI workflow

For an AI-assisted loop, use the same sequence every time:

1. `valid_docs_index`
2. `valid_docs_get` for this curriculum page, the task-specific page, and the
   requirement refinement workflow when the brief is not stable
3. `refine_requirement` for the first pass, then
   `refine_requirement_from_evidence` whenever traces expose ambiguity
4. `author_model` or `review_model`
5. `valid_example_get` for one nearby example
6. `valid_inspect`
7. `valid_lint` or readiness-oriented review
8. `valid_check`, `valid_explain`, `valid_coverage`, or `valid_testgen`

This keeps docs, resources, examples, and prompts aligned instead of making
the client invent a flow ad hoc.

## See also

- [AI Authoring Guide](./authoring-guide.md)
- [Requirement Refinement Workflow](./requirement-refinement-workflow.md)
- [Candidate Comparison Workflow](./candidate-comparison-workflow.md)
- [Review Workflow](./review-workflow.md)
- [Migration Guide](./migration-guide.md)
- [Conformance Workflow](./conformance-workflow.md)
