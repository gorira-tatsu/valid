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
2. [Examples Curriculum](./examples-curriculum.md)
   Learn the examples in increasing modeling complexity.
3. [Modeling Checklist](./modeling-checklist.md)
   Use this as a preflight before returning a new or edited model.
4. [Common Pitfalls](./common-pitfalls.md)
   Read this after the first draft so you can catch common mistakes quickly.
5. [Review Workflow](./review-workflow.md)
   Switch here when the task becomes "review and explain" instead of "write".
6. [Migration Guide](./migration-guide.md)
   Use this when moving from `step` to declarative `transitions`, or when
   reducing readiness/lint findings.
7. [Conformance Workflow](./conformance-workflow.md)
   Use this when you need to connect model evidence to a real implementation.

## Task order

### Author a new model

1. [AI Authoring Guide](./authoring-guide.md)
2. [Examples Curriculum](./examples-curriculum.md)
3. [Modeling Checklist](./modeling-checklist.md)
4. Current MCP prompts:
   - `author_model`
   - `explain_readiness_failure`

### Review an existing model

1. [Review Workflow](./review-workflow.md)
2. [Common Pitfalls](./common-pitfalls.md)
3. [Modeling Checklist](./modeling-checklist.md)
4. Current MCP prompts:
   - `review_model`
   - `explain_readiness_failure`

### Migrate a model

1. [Migration Guide](./migration-guide.md)
2. [AI Authoring Guide](./authoring-guide.md)
3. [Examples Curriculum](./examples-curriculum.md)
4. Current MCP prompt:
   - `migrate_step_to_transitions`

### Review conformance and runtime mismatch

1. [Conformance Workflow](./conformance-workflow.md)
2. [Review Workflow](./review-workflow.md)
3. [Modeling Checklist](./modeling-checklist.md)
4. Current MCP prompt:
   - `review_model`

## Coherent AI workflow

For an AI-assisted loop, use the same sequence every time:

1. `valid_docs_index`
2. `valid_docs_get` for this curriculum page and the task-specific page
3. `valid_example_get` for one nearby example
4. `valid_inspect`
5. `valid_lint` or readiness-oriented review
6. `valid_check`, `valid_explain`, `valid_coverage`, or `valid_testgen`

This keeps docs, resources, examples, and prompts aligned instead of making
the client invent a flow ad hoc.

## See also

- [AI Authoring Guide](./authoring-guide.md)
- [Review Workflow](./review-workflow.md)
- [Migration Guide](./migration-guide.md)
- [Conformance Workflow](./conformance-workflow.md)
