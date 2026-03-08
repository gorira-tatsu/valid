# valid Language Evolution

This document is a non-normative collection of design notes and candidate
features for the `valid` DSL. It describes where the language may evolve next,
not what is currently implemented.

## Goal

`valid` aims for a middle ground between:

- SPIN-style state transitions, counterexamples, and deadlock reasoning
- Alloy-style relations, sets, and maps
- Dafny-style contract and property readability

The target is a practical finite-state formal verification platform, not just a
generic model checker or theorem prover.

## Candidate 1: relational action semantics

The current canonical path is guarded updates.

In the future, actions may also be expressible more directly as pre/post
relations:

```rust
on Inc {
    require |pre| !pre.locked && pre.x < 3;
    ensure |pre, post| {
        post.x == pre.x + 1 &&
        post.locked == pre.locked
    };
}
```

The internal representation is still expected to lower to flat transition
lists.

## Candidate 2: richer property kinds

The current surface only supports `Invariant`. Candidates include:

- `DeadlockFreedom`
- `Reachability`
- future contract/assertion-oriented property kinds

## Candidate 3: Decision / Path IR

Rather than evolving `explain`, `coverage`, and `generate-tests` separately, a
shared abstraction is desirable around:

- action
- guard
- guard outcome
- write-set
- path tags
- property branch

This would make the following easier:

- policy-path coverage
- consistent `explain`
- path-based test generation
- unified witnesses across solvers and exploration modes

## Candidate 4: richer finite data model

Current surface:

- finite enum
- `Option<FiniteEnum>`
- `FiniteEnumSet`
- `FiniteRelation`
- `FiniteMap`
- `String` with explicit regex helpers

Possible future additions:

- `FiniteTuple`
- richer relation/map sugar
- finite multiset-style representations
- bounded text abstraction for policy/password use cases instead of raw string
  theory

## Candidate 5: transition update sugar (`..state`)

### Background

Declarative `transitions` is the canonical path, but even small updates used to
require fully spelled-out state literals:

```rust
on Approve {
    when |state| !state.approved
    => [ReviewState {
        score: state.score,
        waiver: state.waiver,
        approved: true,
    }];
}
```

That has several downsides:

- frame conditions become verbose
- retaining non-`Copy` fields such as `String` is awkward
- falling back to arbitrary expression lists weakens transition metadata

### Decision

Declarative transition literals support explicit frame-condition sugar with
`..state`:

```rust
on Approve {
    when |state| !state.approved
    => [ReviewState {
        approved: true,
        ..state
    }];
}
```

Semantics:

- only explicitly listed fields are updates
- `..state` retains every omitted field
- there is no implicit retention

If unchanged fields should stay unchanged, `..state` must be written
explicitly.

### Lowering and IR policy

- generated `step` code expands to something like
  `State { approved: true, ..state.clone() }`
- `TransitionUpdateDescriptor` / machine IR updates keep only explicitly
  updated fields
- `effect` strings preserve the source-level `..state` shape
- `reads` / `writes` continue to treat action metadata as the primary source

This keeps the core IR flat and guarded-update-oriented.

### `macro_rules!` constraints

The chosen syntax stays within what `macro_rules!` can handle well.

- allowed: identifier-based struct update such as `..state`
- rejected: arbitrary expressions such as `..state.clone()`

Allowing the latter would make it easier to fall into opaque expression paths
and lose transition metadata.

### Backward compatibility

- fully explicit `field: expr` state literals continue to work
- the generic `=> [expr, ...];` form continues to work
- the new sugar is only an ergonomic improvement for declarative transition
  literals

### Non-goals

- implicit frame-condition inference
- nested or multiple spreads
- changes to write-set inference policy

## Candidate 6: logic sugar

Possible future additions:

- `all_of(...)`
- `any_of(...)`
- `none_of(...)`
- more ergonomic grouped transitions
- `otherwise` sugar

The core IR should still stay small.

## Candidate 7: text / regex story

Current text support is explicit-first:

- `String`
- `len`
- `str_contains`
- `regex_match`

Possible future directions:

1. keep text support explicit-only
2. lower a restricted regex fragment to SAT/SMT
3. introduce higher-level predicates for passwords, tokens, and identifiers

Option 3 currently looks the most practical.

Examples:

- `has_uppercase(password)`
- `has_digit(password)`
- `has_symbol(password)`
- `min_length(password, 12)`

These may be easier to keep backend-neutral.

## Candidate 8: the role of `step`

There is no current plan to remove `step`, but its role should remain clear.

- `step`
  - prototype-oriented
  - explicit-first
  - migration source
- `transitions`
  - canonical specification
  - solver-visible
  - canonical input for `graph`, `coverage`, `testgen`, and `explain`

## Candidate 9: IDE and diagnostics

Areas to strengthen:

- better parser span diagnostics for `valid_model!`
- more `trybuild` UI tests
- fewer `rust-analyzer` false diagnostics
- more precise `cargo valid readiness` / `cargo valid migrate` suggestions

## Candidate 10: packaging

Goals:

- binary-only users should still be able to use the tool
- Rust model authors should get a smooth Cargo workflow
- solver backends should remain pluggable

Policy:

- keep the core solver-neutral
- prefer pure-Rust embedded backends where practical
- keep external solver backends optional

## Near-term candidate tasks

- make `DeadlockFreedom` a first-class property kind
- make `Reachability` a first-class property kind
- extend solver encoding for `FiniteRelation` / `FiniteMap`
- add text abstraction for passwords, tokens, and policy use cases
- improve migration hints in the capability matrix
