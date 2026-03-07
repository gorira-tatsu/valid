# ADR-0001: `valid_model!` Frontend Decision

- Status: Accepted
- Date: 2026-03-07
- Related issues: `#4` (A1), `#15` (A3)

## Context

`valid_model!` is currently implemented as `macro_rules!` in
`packages/valid/src/modeling/mod.rs`.

Issue `#4` defines the acceptance criteria for keeping that direction:

- reduce the macro to 5 arms or fewer
- eliminate self-recursion
- eliminate nested optional repetition
- restore `rust-analyzer` compatibility for normal authoring flows

As of this ADR, those criteria are not yet met. The current macro still carries
syntax-normalization arms, recursive rewrites, and compatibility branches for
multiple surface forms. That means A1 has not produced the evidence needed to
declare the `macro_rules!` frontend successful.

At the same time, `packages/valid_derive` already uses handwritten
`proc_macro::TokenTree` parsing for derive macros and does not currently depend
on `syn` or `quote`. Reverting `valid_model!` to a function-like proc-macro is
therefore technically feasible, but it would reintroduce a second frontend
implementation while A1 is still unresolved.

## Decision

We do not restore `valid_model!` to a function-like proc-macro now.

Instead:

1. `valid_model!` stays on the `macro_rules!` track while A1 remains active.
2. A3 is resolved as a fallback decision, not an immediate implementation task.
3. If A1 fails to satisfy its acceptance criteria, we will restore a
   function-like proc-macro frontend in `packages/valid_derive`.
4. If that fallback is triggered, we will prefer the existing lightweight
   `TokenTree` parsing style first and add `syn` / `quote` only if handwritten
   parsing becomes the maintenance bottleneck.

## Rationale

- Shipping the proc-macro rollback before A1 concludes would abandon the
  grammar-simplification path without measuring it.
- Keeping the current implementation avoids extra dependency churn and preserves
  current build characteristics.
- Recording the fallback explicitly removes ambiguity: if `rust-analyzer`
  compatibility cannot be recovered with a simplified `macro_rules!` surface,
  the project should prefer authoring correctness and recoverable diagnostics
  over macro purity.
- The existing derive proc-macro crate lowers the implementation risk of the
  fallback because the repository already has a proc-macro boundary and a
  custom parser style.

## Consequences

- No immediate code migration from `macro_rules!` to proc-macro is performed by
  this ADR.
- Documentation must describe `valid_model!` as a `macro_rules!` frontend today,
  not as an already-restored proc-macro.
- The next concrete implementation step remains A1: simplify the grammar until
  either the acceptance criteria pass or failure is clear enough to trigger the
  proc-macro fallback.
