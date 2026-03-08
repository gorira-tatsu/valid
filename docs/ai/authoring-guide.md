# AI Authoring Guide

This is the shortest canonical entrypoint for LLM agents that need to write or
review `valid` models.

Use it before the full DSL guide or language spec. It tells you what to do,
what not to do, and which commands/tools to reach for next.

Related documents:

- [Modeling Checklist](./modeling-checklist.md)
- [Common Pitfalls](./common-pitfalls.md)
- [Examples Curriculum](./examples-curriculum.md)
- [Project Organization Guide](../project-organization.md)
- [Rust DSL Guide](../dsl/README.md)
- [DSL Language Spec](../dsl/language-spec.md)

## What `valid` is

`valid` is a Rust-first finite-state verification tool for business-rule
models. The canonical path is:

1. Define state and actions in Rust.
2. Define the machine with `valid_model!`.
3. Export the model through a registry.
4. Run `cargo valid` or the MCP tools.

For compatibility fixtures, `.valid` files still work through the `valid`
binary and the DSL-mode MCP tools.

## Preferred modeling path

When generating new models:

- prefer declarative `transitions { ... }`
- treat `step |state, action| { ... }` as explicit-first or migration-oriented
- always write `model Name<State, Action>;`
- prefer small finite domains with explicit metadata

Why:

- `transitions` is the canonical analysis path
- it gives better `inspect`, `graph`, `readiness`, `explain`, `coverage`, and
  `testgen`
- `step` is still supported, but carries weaker structural information

## Minimal registry model skeleton

```rust
use valid::{
    registry::run_registry_cli, valid_actions, valid_model, valid_models, valid_state,
};

valid_state! {
    struct State {
        x: u8 [range = "0..=3"],
        locked: bool,
    }
}

valid_actions! {
    enum Action {
        Inc => "INC" [reads = ["x", "locked"], writes = ["x"]],
        Lock => "LOCK" [reads = ["locked"], writes = ["locked"]],
    }
}

valid_model! {
    model CounterModel<State, Action>;
    init [State {
        x: 0,
        locked: false,
    }];
    transitions {
        transition Inc [tags = ["allow_path"]]
        when |state| state.locked == false && state.x < 3
        => [State {
            x: state.x + 1,
            ..state
        }];

        transition Lock [tags = ["governance_path"]]
        when |state| state.locked == false
        => [State {
            locked: true,
            ..state
        }];
    }
    properties {
        invariant P_RANGE |state| state.x <= 3;
    }
}

fn main() {
    run_registry_cli(valid_models![
        "counter" => CounterModel,
    ]);
}
```

## Supported building blocks

Use these first:

- `#[derive(ValidState)]`, `#[derive(ValidAction)]`, `#[derive(ValidEnum)]`
- `valid_state!`, `valid_actions!`, `valid_model!`, `valid_models!`
- state fields:
  - `bool`
  - bounded `u8`, `u16`, `u32`
  - finite enums
  - `Option<FiniteEnum>`
  - `FiniteEnumSet<T>`
  - `FiniteRelation<A, B>`
  - `FiniteMap<K, V>`
  - `String` with explicit-first helpers
- property kind:
  - `invariant`
  - `reachability`
  - `deadlock_freedom`
  - `cover`
  - action-scoped `transition`

Useful expressions:

- boolean and arithmetic: `!`, `&&`, `||`, `==`, `!=`, `<`, `<=`, `>`, `>=`,
  `+`, `-`, `%`
- logic sugar: `implies`, `iff`, `xor`
- finite collections: `contains`, `insert`, `remove`, `is_empty`
- relations/maps: `rel_contains`, `rel_insert`, `rel_remove`,
  `rel_intersects`, `map_contains_key`, `map_contains_entry`, `map_put`,
  `map_remove`
- string helpers: `len`, `str_contains`, `regex_match`

## Capability constraints you must respect

- `String`, `str_contains`, and `regex_match` are explicit-first today
- `reachability` and `deadlock_freedom` are supported over the finite state
  space
- `cover`, `transition`, and scenario-scoped checks are explicit-first today
- a model can be `explicit_ready` but not `solver_ready`
- `readiness` / `lint` is the authority for migration hints and degraded
  capability reasons
- declarative models with unsupported expressions will be flagged by readiness
  and may fail solver-backed verification

Do not assume that parse success means solver-ready support.

## Commands and MCP workflow

For local authoring:

```sh
cargo valid inspect <model>
cargo valid readiness <model>
cargo valid verify <model>
```

For MCP-driven authoring:

1. Call `valid_docs_index`
2. Read this guide with `valid_docs_get`
3. Read one example with `valid_example_get`
4. Inspect or lint the concrete model
5. Use `valid_suite_run` when the project declares critical properties or
   named suites
6. Verify only after capability/readiness is understood

## Rules to follow

- Prefer registry mode for new work.
- Use `.valid` mode only for compatibility fixtures or frontend tests.
- Always give bounded integer ranges.
- Add `reads` and `writes` metadata to every action variant when possible.
- Mark bootstrap/fixture transitions with `role = setup` so coverage and
  generated vectors do not overstate business-flow coverage.
- Prefer `scenarios:` over large fixture-only transition ladders when you need
  a focused deleted/error/recovered state slice.
- Extract repeated guard/property conditions into `predicates:` so drift stays
  local and inspect output stays readable.
- Keep project-level `critical_properties` and `property_suites` small and
  reviewable. Treat them as CI targeting contracts, not a dump of every
  property in the model.
- Keep registry files thin. Prefer one model per file and move shared enums or
  reusable domain vocabulary into a dedicated shared module instead of copying
  them across models.
- Use `..state` only as explicit frame-condition sugar.
- Keep domains finite and obvious.
- Choose declarative transitions unless you are intentionally staying
  explicit-first.

## Do not do these things

- do not write `model Name;`
- do not rely on implicit field retention in declarative transitions
- do not invent unsupported property kinds
- do not assume general `Vec`, `HashMap`, `HashSet`, or infinite strings are
  available
- do not treat `step` as the canonical solver-visible form
- do not skip `readiness` when using string/regex-heavy models

## Next read

- If you need a generation checklist: [Modeling Checklist](./modeling-checklist.md)
- If you need anti-patterns: [Common Pitfalls](./common-pitfalls.md)
- If you need examples in learning order: [Examples Curriculum](./examples-curriculum.md)
- If you need project layout guidance: [Project Organization Guide](../project-organization.md)
- If you need the full supported surface: [DSL Language Spec](../dsl/language-spec.md)
