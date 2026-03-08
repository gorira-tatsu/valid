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
- introducing a second incompatible action-reporting surface

### Later: richer payload support

Anything beyond explicit finite parameter domains is a follow-on design phase.

That later phase must not be implied by v1 docs or examples. It needs separate
decisions for:

- surface syntax
- finiteness rules
- lowering shape
- explain/testgen ergonomics
- backend capability reporting

## V1 scope boundary

V1 is intentionally narrow.

Supported conceptual shapes:

- one conceptual action with one bounded finite enum parameter
- one conceptual action with multiple bounded finite enum parameters only if the
  combined cardinality stays reviewable and the lowering remains explicit
- parameter values that already participate in the finite state vocabulary of
  the model

Deferred from v1:

- bounded strings, even when they have explicit length limits
- tuple or struct payloads
- payload fields that need custom per-backend semantics
- parameter domains inferred from runtime collections or external data
- parameter values that require custom JSON decoding beyond additive reporting

If implementation pressure appears around any deferred case, it should be
treated as a follow-on design item rather than quietly folded into v1.

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

### Lowering invariants

Implementation work should preserve these invariants:

- the explored machine remains finite before execution starts
- every lowered concrete choice is attributable back to one conceptual action
- parameter expansion happens before solver/backend handoff
- existing transition metadata stays available after lowering
- explicit backends, solver backends, CLI JSON, MCP, and registry APIs can all
  keep consuming a flat action universe

## Surface and authoring guardrails

The exact Rust syntax is still open, but the semantic contract for v1 should be
stable before parser work starts.

Required authoring semantics:

- the author writes one conceptual action definition
- the parameter domain is explicit in the source and finite at author time
- `action_id`, `reads`, `writes`, `role`, and `tags` attach once at the
  conceptual action level
- transition authoring remains reviewable without forcing one user-authored
  branch per parameter value

Current docs and examples should treat any exploded action set as a temporary
surface workaround, not as a design template for new registries.

## Compatibility policy

Compatibility rules:

- existing enum-only action definitions remain supported
- existing action ids remain stable
- existing `reads` / `writes` / `role` / `tags` semantics remain intact
- existing JSON/CLI/MCP consumers should not be forced to understand richer
  payloads just to keep working

If new action-reporting fields are introduced, they should be additive.

### Compatibility checklist for implementation

V1 should be treated as incomplete unless all of these stay true:

1. `inspect`, `graph`, `check`, `explain`, `trace`, `coverage`, and
   `generate-tests` continue to work for existing enum-only registries without
   schema breakage.
2. Existing registry binaries and derive-based models do not need source
   changes unless they opt into the new feature.
3. Existing action identifiers used in CI snapshots, fixtures, or docs remain
   valid for enum-only actions.
4. New parameter detail in machine-readable outputs is additive and ignorable by
   older consumers.

## Metadata policy

Metadata should attach to the conceptual action definition first.

V1 expectations:

- `action_id` identifies the conceptual action
- `reads` and `writes` describe the same conceptual action contract
- `role` still classifies setup vs business intent
- `tags` still classify decision/path meaning

Lowered concrete choices may add derived detail, but they should not require
authors to duplicate metadata across one variant per parameter value.

### Derived identity policy

Implementation should keep two names available in outputs:

- conceptual action identity:
  stable for docs, inspect summaries, lint, and migration hints
- derived concrete choice identity:
  stable enough for witnesses, traces, and generated tests

The important constraint is that evidence can name the specific parameter
choice without making the conceptual action disappear.

## Tooling expectations

### Inspect

`inspect` should show:

- the conceptual action name
- its parameter domains
- the lowered finite cardinality when relevant
- derived concrete choices only when the output needs them for evidence

### Graph

`graph` should stay readable at the conceptual action level by default.

If a future command or flag exposes the fully expanded graph, that view should
be explicitly opt-in.

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

## Documentation and lint policy before implementation

The repository should steer users away from action explosion now, even before
bounded parameterized actions exist.

Documentation policy:

- docs may use tiny duplicated-action fixtures for teaching or regression
  purposes
- every such example should say that it is a bounded stopgap under today's
  enum-only surface
- docs should prefer examples where distinct variants are genuinely distinct
  business events, not stand-ins for input values

Lint/readiness policy:

- no new hard lint is required for issue #53
- maintainability guidance may flag likely action explosion patterns once v1
  exists
- until then, docs should describe likely future guidance in advisory terms,
  not as implemented behavior

## Example policy and migration shape

Preferred current guidance for bounded business choices:

1. keep the conceptual action visible in prose and comments
2. use tiny duplicated variants only when the example would otherwise become
   harder to teach
3. keep duplicated variants obviously finite and local to the example
4. avoid naming patterns that imply open-ended user input should become one
   variant per value

Illustrative current stopgap:

```rust
enum PasswordAction {
    SetStrongPassword,
    SetWeakPassword,
}
```

Document this as a teaching fixture for two bounded cases, not as a template
for arbitrary password entry.

Illustrative v1 target shape:

```rust
enum PasswordStrength {
    Strong,
    Weak,
}

// Surface syntax intentionally omitted here.
// The semantic target is one conceptual action:
// SetPassword(strength: PasswordStrength)
```

This distinction keeps examples useful today while making the migration target
explicit.

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

## Incremental delivery plan

The roadmap is ready to implement incrementally in this order:

1. finalize the v1 semantic contract:
   one conceptual action, explicit finite domains, additive reporting
2. choose surface syntax that can express that contract without weakening
   metadata attachment
3. lower bounded parameters into the existing flat action/transition universe
4. add additive reporting fields for conceptual action plus concrete parameter
   choice
5. update inspect/explain/coverage/testgen output and fixtures
6. add readiness/lint guidance and migration notes only after reporting is
   stable

Each step should be mergeable without forcing richer payload support into the
same milestone.

## Open questions for later implementation work

- exact surface syntax for bounded parameters in `valid_actions!` and derives
- whether v1 should allow more than one bounded parameter or defer that to a
  follow-on once cardinality/reporting experience is clear
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
