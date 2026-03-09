# Documentation

## Public Guides

- [Task Guide](#task-guide)
  Recommended starting points by workflow.
- [Quickstart Guide](./quickstart.md)
  Shortest path from install to `valid onboarding`, `inspect`, and `handoff`.
- [Install Guide](./install.md)
  Installation modes, binary vs Cargo usage, Docker, and backend selection.
- [CI Workflow Templates](./ci/README.md)
  Reusable GitHub Actions patterns plus shell command equivalents for inspect,
  check, testgen, conformance, and doc drift checks.
- [AI Authoring Guide](./ai/authoring-guide.md)
  Short canonical entrypoint for LLM agents and AI-assisted authoring flows.
- [AI Docs Curriculum](./ai/curriculum.md)
  Learning-order and task-order map for authoring, review, migration, and
  conformance workflows.
- [Requirement Refinement Workflow](./ai/requirement-refinement-workflow.md)
  Clarification-first and evidence-driven loop for turning ambiguous product
  requirements into stable modeling briefs.
- [Model Authoring Best Practices](./ai/model-authoring-best-practices.md)
  Guidance for documenting model intent, scope, assumptions, scenarios, and
  critical properties close to the source.
- [Project Organization Guide](./project-organization.md)
  Recommended layout for model files, registries, shared types, integration
  models, generated artifacts, and the pre-compose integration-model pattern.
- [Testgen and Handoff Guide](./testgen-and-handoff.md)
  Language-agnostic test specs, handoff summaries, and conformance-oriented
  workflows.
- [Testgen Strategies Guide](./testgen-strategies.md)
  Strategy-by-strategy guidance for replay, witness, deadlock, enablement, and
  grouped vectors.
- [Graph and Review Guide](./graph-and-review.md)
  Review-oriented use of `graph --view`, `trace`, `explain`, and field-diff
  evidence.
- [Composition Guide](./composition.md)
  Current supported composition helpers, integration-model guidance, and
  composition limits.
- [Architecture](./architecture.md)
  Clean-architecture view of the repository, package roles, DTO boundary, and
  solver-neutral layering.
- [Artifact Inventory and Run History](./artifacts.md)
  Artifact index layout, run-history files, and CLI listing surfaces.
- [Rust DSL Guide](./dsl/README.md)
  User-facing documentation for writing and operating models with the `valid`
  Rust DSL.
- [DSL Language Spec](./dsl/language-spec.md)
  Current implemented surface and semantic subset for the Rust DSL.
- [DSL Language Evolution](./dsl/language-evolution.md)
  Design notes for proposed and in-flight language features.
- [Parameterized Action Roadmap](./dsl/parameterized-action-roadmap.md)
  Incremental plan for moving from enum-only actions to bounded parameterized
  actions without encouraging action explosion in docs or examples.
If you want the shortest first-run walkthrough, start with the quickstart guide.
If onboarding fails, move to the install guide for `valid doctor` and
`valid init --repair`.
If you want to wire an LLM or MCP client into `valid`, start with the AI
authoring guide, then move through the AI docs curriculum.
If you want to model and verify a system, start with the Rust DSL guide.

## Task Guide

| Task | Start Here | Then Read |
| --- | --- | --- |
| Bootstrap a new project | [Quickstart Guide](./quickstart.md) | [Install Guide](./install.md) |
| Learn the current public surface | [Rust DSL Guide](./dsl/README.md) | [DSL Language Spec](./dsl/language-spec.md) |
| Author or review with AI | [AI Authoring Guide](./ai/authoring-guide.md) | [AI Docs Curriculum](./ai/curriculum.md) |
| Refine an ambiguous requirement | [Requirement Refinement Workflow](./ai/requirement-refinement-workflow.md) | [Candidate Comparison Workflow](./ai/candidate-comparison-workflow.md) |
| Generate implementation-facing tests | [Testgen and Handoff Guide](./testgen-and-handoff.md) | [Testgen Strategies Guide](./testgen-strategies.md) |
| Review failures and deadlocks | [Graph and Review Guide](./graph-and-review.md) | [Artifact Inventory and Run History](./artifacts.md) |
| Organize a larger multi-model project | [Project Organization Guide](./project-organization.md) | [Composition Guide](./composition.md) |

## Maintainer and Internal References

- [ADR-0001: `valid_model!` Frontend Decision](./adr/0001-valid-model-frontend.md)
  Maintainer-facing decision record for the current frontend implementation.
- [RDD](./rdd/README.md)
  Requirements, planning, architecture, and delivery documents for the project
  itself.
