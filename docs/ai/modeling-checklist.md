# Modeling Checklist

Use this checklist before returning a generated model or reviewing one.

Related documents:

- [AI Authoring Guide](./authoring-guide.md)
- [Requirement Refinement Workflow](./requirement-refinement-workflow.md)
- [Model Authoring Best Practices](./model-authoring-best-practices.md)
- [Common Pitfalls](./common-pitfalls.md)
- [Examples Curriculum](./examples-curriculum.md)

## Before writing

- Choose registry mode unless the task explicitly targets `.valid` fixtures.
- Prefer declarative `transitions`.
- Pick finite enums, bounded integers, and explicit metadata.
- Decide whether the model is expected to be solver-ready or only
  explicit-ready.
- Draft a short intent comment that explains the business rule and boundary.

## State

- Every bounded integer field has `range = "..."`
- Enum-like fields are marked with `enum`
- Set fields are marked with `set`
- Relation fields are marked with `relation`
- Map fields are marked with `map`
- String fields use explicit-first expectations

## Actions

- Every action variant has an `action_id`
- Every action variant has `reads`
- Every action variant has `writes`
- Action names and `action_id`s match intent
- Do not split one conceptual action into many variants unless the bounded
  choice is deliberate and example-sized

## Model header and init

- The header is `model Name<State, Action>;`
- `init [ ... ];` exists
- Declarative solver-ready models use a single initial state
- A short source-adjacent comment explains summary, scope, assumptions, and
  critical properties

## Transitions

- Prefer `transitions { ... }`
- Guards use supported expressions only
- Updates retain fields explicitly
- If unchanged fields should be kept, use `..state`
- Tags are present when they add review value

## Properties

- Property kinds are chosen intentionally:
  - `invariant`
  - `reachability`
  - `deadlock_freedom`
  - `cover`
  - action-scoped `transition`
- Property ids are stable and descriptive
- Properties talk about reachable-state semantics, not Rust type-level claims

## Capability check

- If the model uses `String`, `str_contains`, or `regex_match`, expect
  explicit-first constraints
- Run `cargo valid readiness <model>` or `valid_lint`
- Review maintainability findings, not just capability blockers
- For integration models, use `lint` / `readiness` findings to check that the
  model still has one explicit shared-state purpose instead of silently
  absorbing whole standalone workflows
- If the cross-domain requirement can be explained by a small shared-state
  contract, keep it as an integration model; if the issue is only interface or
  implementation conformance, prefer a contract-only check
- Do not claim solver-ready unless readiness supports it

## Final review

- The model can be explained from one example path
- The finite domains are small enough to inspect mentally
- Repeated guards or property expressions have been extracted into predicates
- The CLI/MCP commands suggested to the user match the chosen mode
<<<<<<< HEAD
- For shared-state cross-domain checks, the model comment names the
  participating subdomains, the restated shared fields, and the critical
  cross-domain properties
- For shared-state cross-domain checks, review one of these examples if the
  boundary still feels fuzzy:
  `examples/tenant_relation_registry.rs`,
  `examples/saas_multi_tenant_registry.rs`
- If the requirement is still fuzzy, go back to the requirement refinement
  workflow and use `refine_requirement` or `refine_requirement_from_evidence`
  before editing the model further. `clarify_requirement` remains available as
  a compatibility alias for older clients.
||||||| 5d4ca8a
- If the requirement is still fuzzy, start again with the `clarify_requirement`
  MCP prompt before editing the model further
=======
- For shared-state cross-domain checks, the model comment names the
  participating subdomains, the restated shared fields, and the critical
  cross-domain properties
- For shared-state cross-domain checks, review one of these examples if the
  boundary still feels fuzzy:
  `examples/tenant_relation_registry.rs`,
  `examples/saas_multi_tenant_registry.rs`
- If the requirement is still fuzzy, start again with the `clarify_requirement`
  MCP prompt before editing the model further

>>>>>>> origin/main
## Task-specific follow-up

- For requirement refinement: [Requirement Refinement Workflow](./requirement-refinement-workflow.md)
- For review: [Review Workflow](./review-workflow.md)
- For migration: [Migration Guide](./migration-guide.md)
- For implementation handoff: [Conformance Workflow](./conformance-workflow.md)
- The model comment still matches the actual behavior after the latest edit
