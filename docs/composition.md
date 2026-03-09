# Composition Guide

Use this guide when one review question spans more than one model or bounded
context.

The current supported public path is still helper-oriented. `valid` has
deterministic composition helpers today, but it does not yet expose a broad
first-class compose DSL. Document the current helper path as the supported
surface, and treat larger composition syntax as future work.

## What is supported today

Today you should think in three layers:

- standalone model
  Local rules and local invariants.
- integration model
  A thin shared-state review surface across bounded contexts.
- composition helper
  Deterministic helper-based composition when you want to reuse multiple models
  and synchronize selected shared fields.

This makes the current story additive rather than magical.

## When to use each option

Use a standalone model when:

- the question is local to one workflow
- the state and action vocabulary are already coherent
- the review does not need cross-domain alignment

Use an integration model when:

- the review depends on fields from more than one subdomain
- the shared assumptions are small enough to state directly
- you want a review-friendly surface, not maximal reuse

Use the composition helper when:

- you already have multiple useful models
- the review needs deterministic shared-field synchronization
- you want to compose existing work without manually restating every rule

## Current helper expectations

The helper path is best suited to:

- reusing existing models
- syncing explicit shared fields
- preserving deterministic combined review surfaces

It is not the right place to invent a large implicit merge of arbitrary model
semantics.

## Minimal workflow

1. Keep each standalone model reviewable on its own.
2. Decide whether the cross-domain question is better served by:
   - one small integration model, or
   - a helper-based composition of existing models
3. Verify the combined surface with the same `inspect`, `check`, `explain`,
   `graph`, and `testgen` tools you already use elsewhere.

## Constraints to keep in mind

- composition only works well when the shared state slice is explicit
- it is still better to keep registries thin and models small
- if the combined model becomes the only source of truth, the standalone models
  are probably too weak or too fragmented

## Example direction

Keep the concrete wiring thin:

- one model for local approval rules
- one model for fulfillment or entitlement rules
- one integration/composed surface for the shared decision boundary

That keeps the review surface explicit and test generation still understandable.

Canonical example:

```sh
cargo run --example compose_helper_registry
```

That example keeps the current supported story honest: compile two small models,
compose them with explicit sync fields, then inspect/check the composed helper
surface directly.

## Next read

- [Project Organization Guide](./project-organization.md)
- [Rust DSL Guide](./dsl/README.md)
- [Graph and Review Guide](./graph-and-review.md)
