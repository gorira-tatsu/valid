# valid Language Spec

This document is the normative description of the currently implemented
`valid` Rust DSL surface. Unlike [docs/dsl/README.md](./README.md), which is a
user guide, this file is about the supported syntax, semantics, and current
capability boundaries.

## Positioning

`valid` is a Rust-first finite-state specification DSL.

- Main use cases are business-rule verification, approval flows, IAM and
  multi-tenant policy, pricing, and entitlements
- The canonical path is declarative `transitions { ... }`
- `step` is still supported, but should be treated as explicit-first and
  migration-oriented

## Canonical modeling rules

The standard model shape is:

1. state type
2. action type
3. `valid_model!`
4. registry via `valid_models!`

Rules:

- model headers must be explicit: `model Name<State, Action>;`
- shorthand `model Name;` is not supported
- declarative solver-oriented models should use a single `init [ ... ];`
- new long-lived models should prefer declarative `transitions`

## Supported syntax

### State types

State is expressed as a Rust struct. The current field classes that lower to
machine IR are:

- `bool`
- `String`
  - `#[valid(range = "8..=64")]` or `password: String [range = "8..=64"]`
    constrains string length
- bounded unsigned integers
  - `u8`
  - `u16`
  - `u32`
- finite enum
- `Option<FiniteEnum>`
- `FiniteEnumSet<T>`
- `FiniteRelation<A, B>`
- `FiniteMap<K, V>`

### State field metadata

The current field metadata is:

- `range = "..."`
  - numeric range for integers
  - length range for `String`
- `enum`
- `set`
- `relation`
- `map`

### Actions

Actions are finite enums. Each variant must have an `action_id`, and should
declare `reads` and `writes` metadata when possible.

Declarative transitions can also declare:

- `role = business` (default)
- `role = setup`

This metadata feeds:

- `inspect`
- `graph`
- `readiness`
- `explain`
- `coverage`
- `generate-tests`

Role metadata is reporting-oriented. It does not change the transition
semantics, but it does change how `coverage`, `inspect`, `explain`, and
`testgen` classify the transition.

### Model definition

`valid_model!` requires an explicit header.

```rust
valid_model! {
    model PasswordPolicyModel<PasswordState, Action>;
    // ...
}
```

### Init

Use `init [ ... ];` to define the initial state set.

Declarative IR lowering currently assumes a single initial state for
solver-oriented models.

### Declarative transitions

This is the canonical path.

```rust
transitions {
    on SetStrongPassword {
        [role = business]
        [tags = ["password_policy_path"]]
        when |state| state.password_set == false
        => [PasswordState {
            password: "Str0ngPass!".to_string(),
            password_set: true,
        }];
    }
}
```

If unchanged fields should be retained explicitly, use struct-update syntax
with `..state`.

```rust
=> [ReviewState {
    approved: true,
    ..state
}];
```

`..state` is frame-condition sugar. The lowering keeps only explicitly updated
fields in the flat guarded transition IR.

Use `role = setup` for fixture/bootstrap transitions that prepare a state space
but should not inflate business-action coverage. Coverage reporting separates
overall action coverage, business/setup coverage, and requirement-tag coverage.

### Step

```rust
step |state, action| {
    match action {
        // ...
    }
}
```

This is supported, but not canonical.

- usable for explicit exploration
- weaker for solver lowering
- weaker for `graph`, `coverage`, and `testgen` structure

### Properties

The current `PropertyKind` surface contains:

- `Invariant`
- `Reachability`
- `DeadlockFreedom`
- `Cover`
- `Transition`

```rust
properties {
    invariant P_EXPORT_REQUIRES_ENTERPRISE |state|
        state.export_enabled == false || state.plan == Plan::Enterprise;
}
```

```rust
properties {
    reachability P_RECOVERED |state| state.retry_scheduled == false;
    deadlock_freedom P_NO_DEADLOCK;
}
```

```rust
properties {
    cover C_DELETED_VIEW |state| state.deleted == true;
    transition P_DELETE_POST on Delete |prev, next|
        prev.visible == true && next.deleted == true;
}
```

`Transition` properties support:

- `prev.<field>`
- `next.<field>`
- `on: <ActionId>`
- optional `when: <expr>` scope

`Cover` checks whether a state predicate is reachable in the explored state
space. It is reported as an explicit-first property today.

### Predicates

Use `predicates:` to name repeated boolean expressions.

```text
predicates:
  valid_post_input: title_len >= 1 && title_len <= 100
```

Predicates are pure expression aliases. They are non-recursive and expand into
the current expression IR.

### Scenarios

Use `scenarios:` to define named initial-state restrictions for focused
verification.

```text
scenarios:
  DeletedPost: deleted == true
```

`check`, `explain`, `trace`, and `coverage` can select a scenario and explore
only the reachable states that satisfy it.

Properties are semantic constraints over reachable states, not Rust type-level
claims.

### Expressions

Boolean and arithmetic:

- `!`
- `&&`
- `||`
- `implies(a, b)`
- `iff(a, b)`
- `xor(a, b)`
- `==`, `!=`, `<`, `<=`, `>`, `>=`
- `+`, `-`, `%`

Finite collections:

- `contains(set, item)`
- `insert(set, item)`
- `remove(set, item)`
- `is_empty(set)`
- `rel_contains(rel, left, right)`
- `rel_insert(rel, left, right)`
- `rel_remove(rel, left, right)`
- `rel_intersects(left, right)`
- `map_contains_key(map, key)`
- `map_contains_entry(map, key, value)`
- `map_put(map, key, value)`
- `map_remove(map, key)`

String and password-oriented helpers:

- `len(&state.password)`
- `str_contains(&state.password, "@")`
- `regex_match(&state.password, r"[A-Z]")`

String literals currently supported by lowering:

- `"abc"`
- `r"[A-Z]"`
- `r#"... "#`
- `"abc".to_string()`
- `String::from("abc")`

## Capability and backend constraints

Current capability matrix fields are:

- `parse_ready`
- `explicit_ready`
- `ir_ready`
- `solver_ready`
- `coverage_ready`
- `explain_ready`
- `testgen_ready`
- `reasons`

Interpretation:

- `ir_ready = true` does not imply `solver_ready = true`
- string- and regex-heavy models are the common example
- `readiness` and `lint` are the authority for degraded capability reasons

Current explicit-first text constraints:

- string helpers evaluate on the current explicit backend
- current SAT/SMT paths are not solver-ready for these string helpers
- `readiness` / `lint` can return:
  - `string_fields_require_explicit_backend`
  - `string_ops_require_explicit_backend`
  - `regex_match_requires_explicit_backend`

## Unsupported and non-goals

This spec does not currently target:

- general `Vec`, `HashMap`, `HashSet`
- infinite string theory
- general regex-theory solver encoding
- higher-order logic
- general-purpose program proofs

Anything outside this supported surface should be treated as unsupported until
the implementation and this spec are both updated.
