# Rust DSL Guide

This document explains the user-facing modeling language exposed by `valid`.
It focuses on how to write models, how to decide between `step` and
`transitions`, and how to operate the resulting project from the CLI.

This is not the design spec. For requirements and architecture, see the RDD.

Related documents:

- [AI Authoring Guide](../ai/authoring-guide.md)
- [AI Docs Curriculum](../ai/curriculum.md)
- [Model Authoring Best Practices](../ai/model-authoring-best-practices.md)
- [Project Organization Guide](../project-organization.md)
- [Current Language Spec](./language-spec.md)
- [Language Evolution Notes](./language-evolution.md)
- [Parameterized Action Roadmap](./parameterized-action-roadmap.md)
- [Testgen and Handoff Guide](../testgen-and-handoff.md)
- [Testgen Strategies Guide](../testgen-strategies.md)
- [Graph and Review Guide](../graph-and-review.md)
- [Composition Guide](../composition.md)
- [ADR-0001: `valid_model!` Frontend Decision](../adr/0001-valid-model-frontend.md)

## What the DSL is

`valid` is a Rust-first finite-state verification tool. The primary modeling
path is:

1. Define state and actions in Rust.
2. Define the machine with `valid_model!`.
3. Verify and inspect it with `cargo valid`.

The DSL is intentionally embedded in Rust rather than being a standalone
language file format.

That means:

- Rust registry authoring requires a Rust toolchain
- the DSL is designed to feel natural in `rust-analyzer`
- macro/derive diagnostics and `cargo valid readiness` are part of the
  authoring experience, not just the runtime experience

## Canonical Modeling Path

There are two ways to describe behavior:

- Declarative `transitions { ... }`
  This is the canonical analysis path. It is the preferred form for solver
  lowering, graph generation, readiness checks, explain, coverage, and
  generated tests.
- Free-form `step |state, action| { ... }`
  This is still supported, but it is best treated as an explicit-only or
  migration-oriented form. Use it for quick prototypes or for models that are
  still being shaped.

If a model is meant to become part of a long-lived practical verification
suite, prefer declarative transitions.

## Source-Adjacent Documentation

Long-lived models should explain their intent directly above the model
definition.

At minimum, record:

- a one-line summary of the business behavior
- what is in scope
- what is out of scope
- key assumptions
- critical properties or suites
- scenario/setup intent when it is not obvious

See [Model Authoring Best Practices](../ai/model-authoring-best-practices.md)
for a concrete template.

## Building Blocks

The current surface DSL consists of these pieces:

- `#[derive(ValidState)]`
- `#[derive(ValidAction)]`
- `#[derive(ValidEnum)]`
- `valid_state!`
- `valid_actions!`
- `valid_model!`
- `valid_models!`

## When to use the newer DSL surfaces

- `predicates:`
  Use when the same domain condition appears in more than one guard, scenario,
  or property.
- `scenarios:`
  Use when you need a focused initial-state restriction instead of another
  setup-style transition.
- `cover`
  Use when "this should be reachable" is itself the review question.
- transition properties
  Use when the requirement is naturally about a specific action's `prev`/`next`
  postcondition rather than a pure state invariant.

Use [Graph and Review Guide](../graph-and-review.md) when the question becomes
"how do I inspect the failure?" rather than "how do I write the syntax?"

You can either define types through macros like `valid_state!`, or define plain
Rust types and attach semantics through derives.

## State

State is a plain Rust struct. The supported field classes today are:

- `bool`
- bounded unsigned integers: `u8`, `u16`, `u32`
- bounded arithmetic expressions including `+`, `-`, `%`, comparisons, `&&`,
  `||`, and `!`
- finite enums derived with `ValidEnum`
- `Option<FiniteEnum>`
- `FiniteEnumSet<FiniteEnum>`
- `String` with explicit-first helpers such as `len`, `str_contains`, and `regex_match`

Example:

```rust
use valid::{ValidEnum, ValidState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidEnum)]
enum ReviewStage {
    Draft,
    Approved,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, ValidState)]
struct ReviewState {
    #[valid(range = "0..=3")]
    score: u8,
    #[valid(enum)]
    waiver: Option<ReviewStage>,
    #[valid(set)]
    approvals: valid::FiniteEnumSet<ReviewStage>,
    approved: bool,
}
```

Notes:

- `#[valid(range = "...")]` constrains bounded integer exploration and solver
  lowering.
- `#[valid(enum)]` tells `valid` that the field should be treated as a finite
  symbolic domain rather than opaque Rust data.
- `#[valid(set)]` marks `FiniteEnumSet<T>` as a bounded finite-set field whose
  universe is the finite enum `T`.
- `valid_state!` supports the same enum metadata with `[enum]`, for example
  `waiver: Option<ReviewStage> [enum]`, and finite-set metadata with
  `approvals: FiniteEnumSet<ReviewStage> [set]`.

The current expression surface also includes helper sugar that lowers to the
same finite IR:

- `implies(a, b)`
- `iff(a, b)`
- `xor(a, b)`
- `contains(set, item)`
- `insert(set, item)`
- `remove(set, item)`
- `is_empty(set)`

For relationship-heavy models, the DSL also supports:

- `FiniteRelation<A, B>`
- `FiniteMap<K, V>`
- `rel_contains(rel, left, right)`
- `rel_insert(rel, left, right)`
- `rel_remove(rel, left, right)`
- `rel_intersects(left_rel, right_rel)`
- `map_contains_key(map, key)`
- `map_contains_entry(map, key, value)`
- `map_put(map, key, value)`
- `map_remove(map, key)`

These are intended for finite domains such as tenant membership, entitlement
bindings, resource ownership, plan assignment, and similar SaaS/IAM-style
relationships.

For password and token policies, the current DSL also supports explicit-first
text checks:

- `len(&state.password)`
- `str_contains(&state.password, "-")`
- `regex_match(&state.password, r"[A-Z]")`

These currently run on the explicit backend. `readiness` will mark such models
as `explicit-ready` rather than `solver-ready`.

## Actions

Actions are finite Rust enums. Each variant needs an `action_id`, and should
carry `reads` and `writes` metadata when possible.

```rust
use valid::ValidAction;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidAction)]
enum Action {
    #[valid(action_id = "APPROVE", reads = ["approved"], writes = ["approved"])]
    Approve,
    #[valid(action_id = "RESET", reads = ["approved"], writes = ["approved"])]
    Reset,
}
```

The metadata is used by:

- `inspect`
- `graph`
- `readiness`
- `lint`
- `explain`
- `coverage`
- `generate-tests`

If you omit `reads` or `writes`, the model can still run, but diagnostics and
generated artifacts become weaker.

For maintainability, keep model intent visible with named `predicates:`,
focused `scenarios:`, and descriptive action ids. Use `cargo valid lint` /
`cargo valid readiness` to catch repeated conditions, oversized model
structure, and setup-heavy transition sets before they drift.

Treat today's enum-only action surface as the current implementation boundary,
not as a recommendation to encode every business input as its own action
variant. If the real-world concept is "one action with a bounded choice", do
not automatically model it as `DoXForA`, `DoXForB`, `DoXForC`, and so on just
to fit the current surface. Reserve that duplication for intentionally tiny
examples or regression fixtures, and use the
[Parameterized Action Roadmap](./parameterized-action-roadmap.md) as the
planning reference for the long-term shape.

For tiny finite domains, you can now keep the conceptual action in one place:

```text
action Add:
  choose delta: 1, 2
  pre: x + {{delta}} <= 2
  post:
    x = x + {{delta}}
```

This lowers to one concrete action per choice combination for exploration and
evidence while keeping the authoring surface bounded and explicit.

Use bounded choices when the real concept is one action with a small finite
input space. Do not use them as a substitute for large arbitrary payloads or a
full parameterized-action system. The current implementation is intentionally
bounded and explicit.

When a model uses bounded choices, the reporting surfaces distinguish:

- the conceptual action, such as `Add`
- the concrete expanded action, such as `Add[delta=2]`
- the concrete `parameter_bindings`, such as `delta=2`

`inspect` reports those identities per action and transition, plus
`parameter_domains` and `expanded_choice_count`. `coverage` reports both
conceptual-action coverage and concrete-choice coverage so expansion does not
inflate business-flow progress. `explain`, `testgen`, and `handoff` preserve
the same concrete parameter choice in their evidence.

## IDE Notes

Current guidance for a smoother IDE experience:

- always write `model Name<State, Action>;`
- prefer declarative `transitions` over `step`
- use `cargo valid readiness <model>` when `rust-analyzer` diagnostics are
  ambiguous
- keep examples and registries small enough that transition intent is obvious

`valid_model!` is currently implemented as a `macro_rules!` frontend. The
active plan is to simplify that grammar for better `rust-analyzer`
compatibility; if A1 cannot meet its acceptance criteria, the fallback is to
restore a function-like proc-macro frontend as described in ADR-0001.

## Model Definition

The model itself is declared with `valid_model!`.

Header form:

```rust
valid_model! {
    model ApprovalModel<ReviewState, Action>;
    // ...
}
```

The generic state and action types are required. The shorthand form without
`<State, Action>` is intentionally not supported, because explicit types give
better diagnostics and IDE behavior.

## Init

`init` defines the initial state set. The current machine IR lowering expects a
single initial state for declarative solver-ready models.

```rust
init [ReviewState {
    score: 0,
    waiver: None,
    approved: false,
}];
```

## Declarative Transitions

This is the preferred form:

```rust
valid_model! {
    model ApprovalModel<ReviewState, Action>;
    init [ReviewState {
        score: 0,
        waiver: None,
        approved: false,
    }];
    transitions {
        transition Approve [tags = ["allow_path", "approval_path"]]
            when |state| state.approved == false
            => [ReviewState {
                score: state.score,
                waiver: state.waiver,
                approved: true,
            }];
    }
    properties {
        invariant P_APPROVAL_BOOLEAN |state|
            state.approved == false || state.approved == true;
    }
}
```

Grouped action syntax is also supported and lowers to the same flat transition
IR:

```rust
transitions {
    on Approve {
        [tags = ["allow_path", "approval_path"]]
        when |state| state.approved == false
        => [ReviewState {
            approved: true,
            ..state
        }];
    }
}
```

When a transition only changes a subset of fields, prefer explicit frame
condition sugar:

```rust
=> [ReviewState {
    approved: true,
    ..state
}];
```

`..state` keeps omitted fields from the current state. Only explicitly listed
fields are recorded as transition updates in the transition metadata and
machine IR. There is no implicit carry-forward of omitted fields; write
`..state` when you want that behavior.

Each transition carries:

- action variant
- optional tags
- guard
- effect/update state

Modulo-based arithmetic is part of the supported declarative subset, so models
like FizzBuzz can stay on the solver-ready path instead of falling back to
`step`.

`on Action { ... }` is currently grouping sugar. It does not yet provide
`otherwise` / `else if` semantics; each `when` still lowers to an ordinary
guarded transition for that action.

Grouped `on Action { ... }` syntax is the preferred surface form when several
guarded branches belong to the same action. It keeps the source readable while
preserving a flat canonical IR for coverage, explain, test generation, and
solver lowering.

### Tags

`tags = [...]` are first-class decision/path annotations. They are used by:

- `graph`
- `coverage`
- `explain`
- `generate-tests --strategy=path`
- `generate-tests --strategy=deadlock`

Examples:

- `"allow_path"`
- `"deny_path"`
- `"approval_path"`
- `"exception_path"`
- `"boundary_path"`
- `"finance_path"`
- `"governance_path"`

Prefer tags that match your business decision structure rather than generic
technical names.

## Step Models

`step` is still allowed:

```rust
valid_model! {
    model PrototypeModel<ReviewState, Action>;
    init [ReviewState { score: 0, waiver: None, approved: false }];
    step |state, action| {
        match action {
            Action::Approve if !state.approved => vec![ReviewState {
                score: state.score,
                waiver: state.waiver,
                approved: true,
            }],
            _ => Vec::new(),
        }
    }
    properties {
        invariant P_APPROVAL_BOOLEAN |state|
            state.approved == false || state.approved == true;
    }
}
```

But this has tradeoffs:

- easier to prototype
- weaker readiness/capability
- opaque to solver lowering
- weaker graph output
- migration likely needed later

Use `cargo valid readiness` and `cargo valid migrate` to move these models
toward declarative transitions.

If a declarative model still contains unsupported lowering constructs,
`cargo valid readiness` now raises them as readiness errors and `cargo valid verify`
prints an explicit warning instead of silently pretending the model is fully
analysis-ready.

## Properties

Properties are declared inside `properties { ... }`.

Current practical first-class property kind:

- `invariant`

In Rust-first models, invariants can now be layered as either:

- `assume`
- `assert`

`assert` is the default guarantee surface. Use `assume` for environment
contracts or fixture boundaries that are expected to hold before the system
guarantee is evaluated.

Example:

```rust
properties {
    invariant P_EXPORT_REQUIRES_APPROVAL |state|
        state.export_enabled == false || state.approved;
    assume ENV_BOUNDARY_ATTACHED |state| state.boundary_attached;
    assert P_EXPORT_REQUIRES_ENTERPRISE |state|
        state.export_enabled == false || state.plan == Plan::Enterprise;
}
```

Multiple properties per model are supported and recommended.

## Worked Example: FizzBuzz

`examples/fizzbuzz.rs` is the smallest arithmetic-heavy declarative model in
the repo. It exercises:

- bounded `u8` state
- modulo guards and updates
- multiple invariants
- graph / inspect / verify on a solver-ready registry

`examples/saas_multi_tenant_registry.rs` is the smallest service-oriented
integration-model example with grouped transitions. It exercises:

- `on Action { ... }` grouped transitions
- enterprise entitlement checks with `FiniteEnumSet`
- multi-tenant isolation properties
- explicit shared-state review across review, entitlement, and isolation
  concerns without requiring full compose syntax
- a safe model and an intentional regression model

`examples/tenant_relation_registry.rs` is the smallest relation/map
integration-model example. It exercises:

- `FiniteRelation<Member, Tenant>`
- `FiniteMap<Tenant, Plan>`
- combined guards using relation and map membership
- shared-state checks that cross membership and plan domains in one focused
  review surface
- strict counterexample generation for cross-tenant regressions

Run it with:

```sh
cargo valid --registry examples/fizzbuzz.rs inspect fizzbuzz
cargo valid --registry examples/fizzbuzz.rs verify fizzbuzz --property=P_FIZZBUZZ_DIVISIBLE_BY_BOTH
cargo valid --registry examples/fizzbuzz.rs graph fizzbuzz

cargo valid --registry examples/tenant_relation_registry.rs inspect tenant-relation-safe
cargo valid --registry examples/tenant_relation_registry.rs verify tenant-relation-regression --property=P_NO_CROSS_TENANT_ACCESS
```

## Registry

A registry exposes one or more models to the CLI:

```rust
use valid::{registry::run_registry_cli, valid_models};

fn main() {
    run_registry_cli(valid_models![
        "approval-model" => ApprovalModel,
        "refund-control" => RefundControlModel,
    ]);
}
```

In a project, `valid.toml` points `cargo valid` at the registry file.

For long-lived projects, keep the registry file thin and put real model logic
under a dedicated module tree such as `src/models/`. See the
[Project Organization Guide](../project-organization.md) for recommended
layouts for standalone models, integration models, shared types, and generated
artifacts.

Project-level verification targeting can also live in `valid.toml`:

```toml
registry = "examples/valid_models.rs"
suite_models = ["refund-control", "approval-model"]
preferred_backends = ["explicit", "sat-varisat"]
default_suite = "smoke"
minimum_overall_coverage_percent = 80
minimum_business_coverage_percent = 75
minimum_setup_coverage_percent = 100
minimum_requirement_coverage_percent = 70

[critical_properties]
refund-control = ["P_EXPORT_REQUIRES_ENTERPRISE"]

[property_suites.smoke]
entries = [
  { model = "approval-model", properties = ["P_APPROVAL_IS_BOOLEAN"] },
  { model = "refund-control", properties = ["P_EXPORT_REQUIRES_ENTERPRISE"] }
]
```

This turns `valid.toml` into a project-level verification policy file, not only
a registry pointer. Clients can read the effective policy through:

- `cargo valid list --json`
- registry `list --json`
- MCP `valid_list_models`

If `default_suite` is set, `cargo valid suite --json` and MCP
`valid_suite_run` will use that suite when no explicit `--suite` or
`suite_name` is given.

## CLI Workflow

Typical project-first flow:

```sh
cargo valid models
cargo valid inspect refund-control
cargo valid graph refund-control
cargo valid readiness counter
cargo valid migrate counter
cargo valid migrate counter --write
cargo valid migrate counter --check
cargo valid verify refund-control
cargo valid explain breakglass-access-regression
cargo valid coverage refund-control
cargo valid generate-tests refund-control --strategy=path
cargo valid benchmark --baseline=compare
cargo valid suite
cargo valid suite --critical
cargo valid suite --suite=smoke
```

## What Each Command Means

- `models`
  Lists exported models in the active registry.
- `inspect`
  Shows model structure, field metadata, action metadata, transitions,
  properties, and capability readiness.
- `graph`
  Renders a model graph in Mermaid, DOT, SVG, text, or JSON form.
- `readiness`
  Shows migration and capability findings.
- `migrate`
  Generates declarative transition snippets for step-based models.
- `migrate --check`
  Runs a migration audit. This is not a formal equivalence proof. It reports
  action coverage and whether manual review is still required.
- `verify`
  Runs the selected property or default property and returns `PASS`, `FAIL`, or
  `UNKNOWN`.
- `explain`
  Summarizes likely failure causes and next steps.
- `coverage`
  Reports action, guard, path-tag, and requirement-tag coverage, including
  business/setup splits plus conceptual-action versus concrete-choice splits
  for bounded parameterized actions.
- `generate-tests`
  Produces regression-oriented Rust tests under `generated-tests/`. JSON output
  reports `strictness` and `derivation` for each vector so review tooling can
  tell strict trace-backed vectors from heuristic or synthetic ones. It also
  adds `priority`, `selection_reason`, `novelty_key`, and bounded-choice
  action metadata so review tooling can keep the highest-value vectors first.
- `benchmark`
  Runs repeated verification and compares against committed baselines.
- `suite`
  Runs verification across the configured suite. `cargo valid suite
  --critical` runs only `critical_properties`, and `cargo valid suite
  --suite=<name>` runs one named entry from `property_suites`.

## Readiness and Capability Model

`inspect --json` and `readiness --json` report capabilities such as:

- `parse_ready`
- `explicit_ready`
- `ir_ready`
- `solver_ready`
- `coverage_ready`
- `explain_ready`
- `testgen_ready`

Typical reasons for degraded readiness:

- `opaque_step_closure`
- `missing_declarative_transitions`
- `unsupported_machine_guard_expr`
- `unsupported_machine_update_expr`
- `unsupported_machine_property_expr`

This is the intended way to decide whether a model is still a prototype or
ready for solver-backed analysis.

## Migration Workflow

For a step-based model:

```sh
cargo valid readiness counter
cargo valid migrate counter
cargo valid migrate counter --write
cargo valid migrate counter --check
```

Recommended interpretation:

1. `readiness`
   See why the model is not solver-ready.
2. `migrate`
   Inspect generated transition candidates.
3. `migrate --write`
   Persist the snippet so it can be edited into the registry.
4. `migrate --check`
   Audit coverage of suggested actions and confirm whether manual review is
   still needed.

`--check` is intentionally conservative. It does not claim formal equivalence
between step and declarative forms unless the model is already declarative.

## Benchmark Workflow

Baselines are tracked in `benchmarks/baselines/`.

Typical flow:

```sh
cargo valid benchmark --baseline=record
cargo valid benchmark --baseline=compare
```

Comparison currently gates on:

- status profile
- explored state count
- explored transition count
- elapsed time, but only when the baseline is large enough to avoid
  millisecond-level noise

This makes the benchmark command suitable for CI regression detection.

## Current Practical Limits

The DSL is already useful for business workflow and policy-style finite-state
models, but it still has limits:

- solver lowering is still a bounded subset
- bounded parameterized actions are implemented, but only for small explicit
  finite domains rather than arbitrary runtime payloads
- richer collections and set-membership abstractions are limited
- declarative transitions are the strongest path; step models are intentionally
  second-class for analysis
- `migrate --check` is an audit tool, not a full semantic equivalence checker
- `testgen` is now prioritized and deduped, but it is still not a complete
  scenario-planning system

## Recommended Modeling Style

- Prefer declarative `transitions` over `step` for production models.
- Use bounded integers aggressively.
- Use finite enums, or `Option<FiniteEnum>`, for workflow phase and reason
  fields.
- Add `reads`, `writes`, and path tags early.
- Split large business policies into multiple transitions and multiple
  invariants rather than one giant condition.
