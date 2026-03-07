---
name: valid-model
description: Create or refine a valid model from business rules or existing code. Use when the user wants help designing states, actions, properties, or a starter model implementation.
argument-hint: "[requirements-or-target-file]"
disable-model-invocation: true
---

Help the user create or refine a `valid` model for `$ARGUMENTS`.

## Modeling Defaults

- Prefer the Rust-first path unless the user explicitly asks for `.valid` compatibility syntax.
- Treat the job as model design first, code generation second.
- Favor concrete state fields, explicit actions, and named safety properties over prose-only guidance.

## Discovery Workflow

1. Read the user request and search the repository for adjacent domain logic, policy checks, reducers, state machines, or existing `valid` examples.
2. If the repo already contains related models, inspect them before writing anything new.
3. Extract or infer:
   - State fields and invariants
   - Actions and guards
   - Required properties and failure modes
   - External concepts that should stay outside the model
4. Make assumptions explicit. If a key semantic choice is ambiguous, present the assumption before you write code.

## Implementation Workflow

1. Draft the smallest useful model skeleton that captures the core business rules.
2. When working in Rust-first mode:
   - Prefer adding or updating a registry-backed model file.
   - Keep names stable and property identifiers readable.
3. When working in `.valid` mode:
   - Use syntax that `valid inspect` can parse immediately.
4. After writing or updating the model, validate it:
   - Rust/project mode: `cargo valid inspect <model> --json` with repo-local fallback `cargo run -q --bin cargo-valid -- valid inspect <model> --json`
   - File mode: `cargo run -q --bin valid -- inspect <file> --json`
5. If validation fails, fix structural issues before discussing advanced refinements.

## Required Summary

Always return:

- The modeled state, actions, and properties
- Assumptions or deferred decisions
- What file or registry entry was created or changed
- The validation result from `inspect`
- Recommended next checks, usually `verify`, `coverage`, or `generate-tests`

Keep the result concrete enough that the user can immediately continue with `/valid-check` or `/valid-review`.
