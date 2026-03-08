# Modeling Checklist

Use this checklist before returning a generated model or reviewing one.

Related documents:

- [AI Authoring Guide](./authoring-guide.md)
- [Model Authoring Best Practices](./model-authoring-best-practices.md)
- [Common Pitfalls](./common-pitfalls.md)
- [Examples Curriculum](./examples-curriculum.md)

## Before writing

- Choose registry mode unless the task explicitly targets `.valid` fixtures.
- Prefer declarative `transitions`.
- Pick finite enums, bounded integers, and explicit metadata.
- Decide whether the model is expected to be solver-ready or only
  explicit-ready.

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
- Do not claim solver-ready unless readiness supports it

## Final review

- The model can be explained from one example path
- The finite domains are small enough to inspect mentally
- The CLI/MCP commands suggested to the user match the chosen mode
- If the requirement is still fuzzy, start again with the `clarify_requirement`
  MCP prompt before editing the model further
## Task-specific follow-up

- For review: [Review Workflow](./review-workflow.md)
- For migration: [Migration Guide](./migration-guide.md)
- For implementation handoff: [Conformance Workflow](./conformance-workflow.md)
- The model comment still matches the actual behavior after the latest edit
