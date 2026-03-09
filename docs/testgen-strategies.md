# Testgen Strategies Guide

Use this guide when you need to choose the right generated evidence for review,
debugging, or implementation handoff.

Not every strategy answers the same question. Some are replay-oriented, some
are evidence-oriented, and some are specifically meant to unblock dead actions
or implementation-facing tests.

## Strategy map

### Replay and regression

- `counterexample`
  Use when a property currently fails and you want the shortest actionable
  reproduction.
- `witness`
  Use when you want a passing trace that demonstrates the intended rule.
- `path`
  Use when you care about a tagged path or a known business flow.

### Transition and guard exploration

- `transition`
  Use when you want one vector per concrete transition shape.
- `guard`
  Use when you want to exercise both guard outcomes and coverage gaps.
- `boundary`
  Use when the interesting behavior is around bounded numeric or categorical
  edges.

### Search and unblock strategies

- `deadlock`
  Use when you want a shortest route to a terminal or stuck state.
- `enablement`
  Use when an action is blocked and you want the shortest trace that enables
  it. This strategy is typically paired with `--focus-action=<id>`.
- `random`
  Use for ad hoc exploration, not as the primary review surface.

## Grouping and prioritization

Recent vectors can also carry grouped metadata.

- `requirement_clusters`
  Use these to map vectors back to product-facing requirement areas.
- `risk_clusters`
  Use these to prioritize vectors that exercise higher-risk behavior.

This is useful when one model supports both a feature-level review and a more
operational "what should we test first?" question.

## Practical examples

```sh
cargo valid testgen <model> --strategy=counterexample
cargo valid testgen <model> --strategy=deadlock
cargo valid testgen <model> --strategy=enablement --focus-action=<action-id>
cargo valid testgen <model> --strategy=guard
```

Use `--json` when you want the complete machine-readable vector contract.

## Which strategy should I use first?

- Failing property: `counterexample`
- Deadlock review: `deadlock`
- Blocked action: `enablement`
- Coverage improvement: `guard` or `transition`
- Implementation handoff: start with `counterexample` or `witness`, then keep
  the highest-value grouped vectors from `handoff`

## Important limits

- `random` is not a substitute for a reviewable regression asset
- `enablement` only makes sense when you know which action you want to unblock
- `deadlock` and `enablement` are strong review tools, but they still depend on
  the explored finite state space

## Next read

- [Testgen and Handoff Guide](./testgen-and-handoff.md)
- [Graph and Review Guide](./graph-and-review.md)
- [Artifact Inventory and Run History](./artifacts.md)
