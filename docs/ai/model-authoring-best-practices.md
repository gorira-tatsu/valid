# Model Authoring Best Practices

This guide explains what model authors should document close to the source so
that `valid` models stay reviewable for humans and AI over time.

Use it together with:

- [AI Authoring Guide](./authoring-guide.md)
- [Modeling Checklist](./modeling-checklist.md)
- [Rust DSL Guide](../dsl/README.md)

This page is about authoring discipline, not new language features.

## Why this matters

A model can be formally valid and still be hard to maintain.

The most common failure mode is not a parser or solver error. It is a model
that technically runs, but leaves reviewers guessing about:

- what real requirement it represents
- what is intentionally in scope or out of scope
- which assumptions are environmental vs business rules
- which properties are critical enough to gate CI
- whether a scenario or setup path is modeling runtime behavior or only a test
  slice

That confusion gets worse as projects add more models, more scenarios, and more
AI-generated edits.

## What every model should explain

Keep a short comment block immediately above each long-lived model. The comment
should answer these questions in plain language:

1. What business behavior does this model represent?
2. What is in scope?
3. What is intentionally out of scope?
4. Which assumptions does the model rely on?
5. Which properties are the critical review/CI targets?
6. Which scenarios are just focused slices rather than full runtime stories?

The comment does not need to be long. It needs to be explicit.

## Recommended source-adjacent comment template

Use a short Rust doc comment block or line comments directly above the model:

```rust
/// Model: PostDetailModel
/// Summary: Detail-page behavior for viewing a post that may be deleted.
/// In scope: load result, not-found behavior, deleted-post behavior, retry path.
/// Out of scope: authoring flow, comment pagination, moderator actions.
/// Assumptions: storage lookup itself is atomic; API shape already passed contract checks.
/// Critical properties: P_DELETED_POST_IS_NOT_FOUND, P_RETRY_EVENTUALLY_RECOVERS.
/// Scenarios: DeletedPostDetail is a focused review slice, not a standalone user journey.
valid_model! {
    model PostDetailModel<State, Action>;
    // ...
}
```

Use line comments if that fits the file style better. The important part is
that the intent lives next to the model, not only in an external design doc.

## What to document for scenarios

If you define `scenarios`, explain why they exist.

Scenarios should usually document:

- which state slice they isolate
- why this slice matters for review
- whether it represents a real user-facing phase or only a focused verification
  slice

Example:

```rust
// DeletedPostDetail isolates the "resource exists but is deleted" slice.
// It is used to review NotFound handling without modeling the full delete flow here.
scenario DeletedPostDetail |state| state.post_exists && state.post_deleted;
```

If a scenario is only there to make a property or failure easier to review,
say that directly.

## What to document for predicates

Predicates should encode domain vocabulary, not anonymous boolean soup.

Good predicate names reflect business meaning:

- `valid_post_input`
- `tenant_can_access_resource`
- `unavailable_post`
- `is_terminal_error`

Add a short comment when the name alone is not enough:

```rust
// "Unavailable" means the UI must not show the detail view, whether the post
// is missing or soft-deleted.
predicate unavailable_post |state| !state.post_exists || state.post_deleted;
```

If a condition is repeated in guards and properties, extract it and name it.
That makes later requirement drift local instead of duplicated.

## What to document for properties

Properties need intent, not only identifiers.

At minimum, make the property id readable and add a comment when the rule is
subtle or requirement-derived.

Document:

- what user-visible or business rule the property protects
- whether the property is a critical CI target
- whether the property only matters under a scenario or scope

Example:

```rust
// Critical: deleted posts must never render the detail screen.
invariant P_DELETED_POST_IS_NOT_FOUND |state|
    unavailable_post(state) implies state.screen == Screen::NotFound;
```

For transition properties, explain the before/after rule:

```rust
// Critical: a successful create must increase the visible count by one.
transition P_CREATE_SUCCESS_INCREMENTS_COUNT on CreateValidPost |prev, next|
    next.post_count == prev.post_count + 1;
```

## Keep models reviewable in size and responsibility

Prefer models that have one clear purpose.

Signs a model should be split or paired with an integration model:

- the header comment needs multiple unrelated summaries
- properties belong to different user journeys or subdomains
- actions are mostly setup/bootstrap noise
- predicates start encoding multiple disconnected concepts

As a rule of thumb:

- use one model per business concern when possible
- use shared predicates and shared types for common vocabulary
- use an integration model when the real review question is cross-boundary

## Keep registries thin

Registry files should mostly register models.

Avoid turning a registry into:

- the place where model intent is explained
- a dumping ground for shared types
- a substitute for per-model source comments

Intent should live next to the model definition. Registry files should stay
small enough that the model list is obvious.

## What AI-generated edits must preserve

When an AI updates a model, it should preserve or improve:

- the model summary
- in-scope / out-of-scope notes
- scenario intent comments
- critical property notes
- domain predicate names

If the AI changes the requirement shape, it should update the adjacent comments
too. A correct code edit with stale intent comments is still a bad review
artifact.

## Review checklist for maintainers

Before accepting a model change, check:

- Does the model explain what it represents?
- Is the scope boundary explicit?
- Are assumptions written down?
- Are critical properties called out?
- Are scenarios and predicates named in business language?
- Is the model small enough to review as one concern?

If the answer to several of these is "no", fix the authoring quality before
expanding the model further.
