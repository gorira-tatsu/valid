# Common Pitfalls

These are the mistakes LLMs are most likely to make when generating `valid`
models.

## 1. Using the wrong primary path

Wrong:

- treating `.valid` files as the main authoring experience
- using `valid` instead of `cargo valid` for Rust registries

Correct:

- use registry mode and `cargo valid` for new Rust-first work
- use `.valid` files only for compatibility fixtures or frontend/kernel tests

## 2. Writing shorthand model headers

Wrong:

```rust
valid_model! {
    model CounterModel;
}
```

Correct:

```rust
valid_model! {
    model CounterModel<State, Action>;
}
```

## 3. Treating `step` as the canonical form

`step` is supported, but it is not the preferred solver-visible form.

Use `step` only when:

- staying explicit-first on purpose
- prototyping quickly
- migrating an older model

Prefer `transitions` for long-lived models.

## 4. Assuming implicit field retention

Wrong:

```rust
=> [State {
    locked: true,
}];
```

Correct:

```rust
=> [State {
    locked: true,
    ..state
}];
```

## 5. Overclaiming solver support

Common mistake:

- model parses successfully
- therefore the agent assumes it is solver-ready

This is false for current string/regex-heavy models. Use `readiness` or
`valid_lint` before making solver claims.

## 6. Inventing unsupported containers or property kinds

Do not invent:

- general `Vec`
- general `HashMap`
- general `HashSet`
- arbitrary theorem-prover style constructs
- property kinds other than `invariant`

Use the currently supported finite containers instead.

## 7. Omitting action metadata

Models may still run, but `inspect`, `graph`, `explain`, `coverage`, and
`generate-tests` become weaker when `reads` and `writes` are omitted.

## 8. Mixing Rust semantics and model semantics

`valid` properties express semantic constraints over reachable states. They are
not ordinary Rust type-level guarantees or runtime assertions.

## 9. Reading docs in the wrong order

Common mistake:

- jumping straight to the full spec
- skipping examples and review flow
- treating one checklist as the whole curriculum

Correct:

- start with [AI Authoring Guide](./authoring-guide.md)
- use [AI Docs Curriculum](./curriculum.md) to pick the next page by task
- switch to the review, migration, or conformance page when the task changes
