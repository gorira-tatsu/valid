# Parameterized Action Roadmap

This document is the planning reference for evolving `valid` actions from
today's finite enum surface toward richer parameterized actions.

It is intentionally design-oriented. The current normative surface remains
[DSL Language Spec](./language-spec.md).

## Goal

Define an incremental path from today's bounded action choices to future
parameterized actions without breaking existing CLI, MCP, or registry flows.

The roadmap must keep four things explicit before implementation work starts:

- what v1 bounded parameterization means
- what is deferred to later richer payload support
- how lowering and compatibility should behave at each stage
- what `inspect`, `coverage`, `explain`, and `generate-tests` should report

## Current baseline

Today, actions are finite enums.

That has two important properties:

- the action universe is explicit and finite before exploration starts
- action metadata such as `action_id`, `reads`, `writes`, `role`, and `tags`
  attaches cleanly to each variant

It also creates pressure toward action explosion when users want an action with
business inputs such as:

- `SetPlan(Pro, Enterprise, ...)`
- `ApproveWithReason(ManualReview, Fraud, Chargeback, ...)`
- `SetPassword("...", "...")`
- `ServeRequest(tenant, resource, decision)`

The roadmap separates bounded finite cases from later richer payload cases
instead of treating them as one feature.

## Definitions

### Action explosion

`valid` should avoid guidance that turns one business action into many variants
just to encode input values.

Bad long-term shape:

```rust
enum PasswordAction {
    SetStrongPassword,
    SetWeakPassword,
    SetVeryWeakPassword,
    SetPasswordMissingSymbol,
    SetPasswordMissingDigit,
}
```

This may still be acceptable for tiny pedagogical or regression-focused
examples, but it should not be the recommended modeling pattern for production
registries.

### Bounded parameterization

Bounded parameterization means:

- one conceptual action carries a parameter chosen from an explicit finite
  domain
- the domain is known up front and small enough to enumerate in IR
- metadata remains attached to the conceptual action, not duplicated across
  many user-authored variants

Examples:

- `SetPlan(plan: Plan)` where `Plan` is a finite enum
- `ApproveWithReason(reason: ReviewReason)` where `ReviewReason` is a finite
  enum
- `AssignTenant(tenant: TenantId)` only if `TenantId` is modeled as an
  explicit finite enum domain

### Richer payload actions

Richer payload actions mean action parameters that are not just small finite
enums or that need structured values.

Examples:

- tuples or multi-field payloads
- bounded strings or text fragments
- map/relation-like input payloads
- values that need domain-dependent projection into multiple state fields

This is explicitly out of v1 scope.

## Decision

The implementation plan is split into two stages.

### V1: bounded parameterized actions

V1 should support only action parameters whose values come from an explicit
finite domain.

Requirements:

- the surface should preserve the idea of one conceptual action with one or
  more bounded parameters
- every parameter domain must lower to a finite enumerated set before
  exploration
- existing enum-only registries must continue to work unchanged
- existing CLI and MCP entrypoints should keep reporting actions in a stable,
  reviewable way

Non-goals for v1:

- arbitrary Rust payload types
- open-ended strings
- general collections as action payloads
- backend-specific payload semantics

### Later: richer payload support

Anything beyond explicit finite parameter domains is a follow-on design phase.

That later phase must not be implied by v1 docs or examples. It needs separate
decisions for:

- surface syntax
- finiteness rules
- lowering shape
- explain/testgen ergonomics
- backend capability reporting

## Expected lowering model

V1 should lower parameterized actions to the same flat machine-transition shape
used today.

The conceptual rule is:

- user authoring keeps one conceptual action
- lowering expands it into one concrete finite action choice per parameter
  combination
- reports keep both views available: conceptual action and lowered concrete
  choice

That means implementation should prefer hidden or derived expansion over
user-authored variant duplication.

## Compatibility policy

Compatibility rules:

- existing enum-only action definitions remain supported
- existing action ids remain stable
- existing `reads` / `writes` / `role` / `tags` semantics remain intact
- existing JSON/CLI/MCP consumers should not be forced to understand richer
  payloads just to keep working

If new action-reporting fields are introduced, they should be additive.

## Metadata policy

Metadata should attach to the conceptual action definition first.

V1 expectations:

- `action_id` identifies the conceptual action
- `reads` and `writes` describe the same conceptual action contract
- `role` still classifies setup vs business intent
- `tags` still classify decision/path meaning

Lowered concrete choices may add derived detail, but they should not require
authors to duplicate metadata across one variant per parameter value.

## Tooling expectations

### Inspect

`inspect` should show:

- the conceptual action name
- its parameter domains
- the lowered finite cardinality when relevant
- derived concrete choices only when the output needs them for evidence

### Graph

`graph` should stay readable at the conceptual action level by default.

### Explain

`explain` should identify both:

- the conceptual action that mattered
- the specific finite parameter choice that produced the witness/counterexample

### Coverage

Coverage should not overstate progress just because a conceptual action lowers
to many concrete choices.

V1 expectation:

- report conceptual-action coverage and concrete-choice coverage separately when
  both are useful
- keep `role = setup` behavior consistent with today's setup/business split

### Generate-tests

`generate-tests` should prefer stable, reviewable vectors.

V1 expectation:

- generated vectors refer to the conceptual action plus explicit parameter
  values
- strategies such as `transition`, `guard`, and `path` should not require
  authors to duplicate actions manually

## Authoring guidance before implementation

Until v1 exists:

- keep using finite enums for actions because that is the implemented surface
- avoid teaching users to create one variant per business input value unless
  the domain is intentionally tiny and example-sized
- prefer state predicates, scenarios, and explicit bounded state domains over
  fixture ladders that simulate payloads through many nearly identical actions
- document when an example is using bounded duplicated actions only as a
  teaching or regression fixture

## Recommended example policy

Examples and docs should follow these rules now:

- acceptable: tiny examples that use two or three action variants to illustrate
  good vs bad outcomes
- acceptable: bounded decision examples where each variant is genuinely a
  distinct business event
- avoid: examples that imply every user input should become a distinct action
  variant
- avoid: recommending variant-per-string or variant-per-id modeling

## Open questions for later implementation work

- exact surface syntax for bounded parameters in `valid_actions!` and derives
- whether multiple parameters land in v1 or only one finite parameter per
  action
- how concrete expanded identities appear in JSON schemas
- whether lint should flag likely action explosion patterns
- whether `readiness` should surface a migration hint from exploded variants to
  bounded parameterization once v1 exists

## Summary

The roadmap decision is:

- keep today's finite enum actions as the current norm
- implement bounded parameterized actions first
- lower bounded parameters to finite concrete choices behind the scenes
- keep compatibility and reporting additive
- defer richer payload actions until their own design is explicit
