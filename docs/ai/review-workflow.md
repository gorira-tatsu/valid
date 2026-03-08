# Review Workflow

Use this page when the task is to review an existing model, explain a failing
result, or propose the next change.

This is different from authoring: the goal is not to write from scratch, but
to separate requirement problems, model problems, and implementation problems.

## Review order

1. Read the [AI Authoring Guide](./authoring-guide.md) to confirm the current
   supported surface.
2. Run `inspect` to see state, actions, properties, predicates, scenarios, and
   capability metadata.
3. Run `lint` or readiness-oriented checks before claiming that a failure is a
   semantic bug.
4. Read one explanation path or traceback.
5. Decide whether the likely repair surface is:
   - requirement clarification
   - model revision
   - implementation/conformance follow-up

## Commands

Registry mode:

```sh
cargo valid inspect <model>
cargo valid lint <model>
cargo valid explain <model>
```

MCP mode:

1. `valid_docs_get` for this page
2. `refine_requirement_from_evidence` if the failure suggests requirement
   ambiguity
3. `valid_inspect`
4. `valid_lint`
5. `valid_explain`

## What to look for

- unsupported expressions or capability downgrades
- scenario or scope choices that make the result narrower than expected
- vacuous properties or covers that never reach the intended state slice
- repeated conditions that should become predicates
- oversized models that should be split or supported by an integration model

## Review checklist

- Does the model explain its in-scope and out-of-scope behavior?
- Are the action ids stable and meaningful?
- Are `reads` and `writes` metadata present where they improve explanation?
- Is the model solver-ready, explicit-ready, or mixed?
- Are the reported failures attached to the right requirement?

## Anti-patterns during review

- treating parse success as proof of solver support
- rewriting the model before understanding the failing trace
- changing action ids casually during cleanup
- fixing a symptom without confirming the intended requirement

## Next read

- [Requirement Refinement Workflow](./requirement-refinement-workflow.md)
- [Common Pitfalls](./common-pitfalls.md)
- [Modeling Checklist](./modeling-checklist.md)
- [Conformance Workflow](./conformance-workflow.md)
