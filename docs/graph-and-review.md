# Graph and Review Guide

Use this guide when a model already exists and you need review-oriented output
rather than raw exploration data.

The main rule is simple: do not start from the biggest graph unless the review
question is truly global.

## Review surfaces

- `inspect`
  Use first to understand capability, properties, scenarios, and action
  metadata.
  `inspect` now also reports bounded state-field domains and analysis profiles.
- `explain`
  Use when you need failure context, field diffs, and candidate causes.
- `trace`
  Use when you need the concrete sequence that led to a state.
- `graph`
  Use when the review question is about structure, not only one failing step.
  `graph --format=json` now returns a `graph_snapshot` with reduction metadata,
  node/edge summaries, focus indices, and deadlock identifiers.

## Graph views

`graph --view` is now task-oriented.

- `overview`
  Best first read for a small model.
- `logic`
  Best when you want to study the full transition structure.
- `failure`
  Best when a specific property failure is the review anchor.
- `deadlock`
  Best when the problem is terminal behavior or a stuck state.
- `scc`
  Best when the graph is large and you need cycle or condensation structure.

The reduction metadata is intentionally review-oriented:

- `full`
  Keep the main structure for overview or logic review.
- `boundary_paths`
  Focus around failure-oriented or scoped review paths.
- `focus_states`
  Keep profile-driven state/property focus when a review profile narrows the
  graph surface.
- `deadlock_focus`
  Keep terminal/deadlock-oriented summaries.
- `scc_condensation`
  Keep the condensed cyclic structure.

When a project defines `valid.toml` analysis profiles, `graph --format=json`
and generated docs now use the selected profile's `doc_graph_policy` for the
overview snapshot. In practice that means profile-driven review artifacts can
default to `boundary_paths` or `focus_states` without changing the top-level
command shape.

## Review workflow

### Property failure

1. `inspect` the model and note the property, scenario, and capability surface.
2. `explain` the failing property to see the field diff and candidate causes.
3. `trace` the failure if you need the replayable sequence.
4. `graph --view=failure` if the structural neighborhood matters.

### Deadlock or stuck state

1. `testgen --strategy=deadlock`
2. `trace`
3. `graph --view=deadlock`
4. `graph --view=scc` if the state space is large or cyclical

## Field-diff first reading

Recent explain and traceback output is intentionally field-diff oriented.
Prefer asking:

- which fields changed at the break point
- which guard or scope allowed the step
- which scenario or property scope mattered

instead of scanning raw full-state dumps first.

## Example commands

```sh
valid inspect examples/scenario_focus.valid --json
valid check examples/scenario_focus.valid --profile=DeletedPost --json
valid explain examples/scenario_focus.valid --property=P_NOT_FOUND_WHEN_DELETED
valid graph examples/scenario_focus.valid --format=json
valid graph examples/scenario_focus.valid --view=failure
```

For standalone Rust registry files rather than `.valid` models or project-root
models, use `cargo valid --registry ...`:

```sh
cargo valid --registry examples/iam_transition_registry.rs inspect iam-access
cargo valid --registry examples/iam_transition_registry.rs graph iam-access --view=logic
```

## When not to use a graph first

Skip graph-first review when:

- the failure is already isolated by one short counterexample
- the issue is clearly an implementation conformance mismatch
- the real problem is requirement ambiguity rather than model structure

In those cases, start with `explain`, `handoff`, or the AI review workflows.

## Next read

- [Testgen Strategies Guide](./testgen-strategies.md)
- [Testgen and Handoff Guide](./testgen-and-handoff.md)
- [AI Review Workflow](./ai/review-workflow.md)
