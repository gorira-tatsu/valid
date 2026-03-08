# Candidate Comparison Workflow

Use this workflow when two plausible models or two plausible property
interpretations both seem defensible and you need reviewable evidence before
committing to one of them.

## Goal

Turn "these both sound reasonable" into:

- the concrete assumption that differs
- the shortest trace where the difference becomes observable
- the property or state slice affected by that difference
- the follow-up requirement question or model decision that resolves it

## Workflow

1. Write down the shared requirement brief first.
2. Name the competing candidates explicitly.
3. Inspect both candidates and verify that they are comparable at the same
   state/action boundary.
4. Run `valid_distinguish` to find the shortest shared-prefix divergence.
5. Review the last checkpoint to identify:
   - which assumption changed
   - which field values differ
   - whether the divergence is in state, guard enablement, or property value
6. Summarize the requirement decision in product language, not only model
   language.

## Minimum Inputs

- one left candidate model
- one right candidate model
- optional property ids when the comparison should focus on one requirement
- a short note describing the requirement ambiguity under review

## MCP-Friendly Order

1. `valid_docs_get` for this workflow and the review workflow
2. `valid_inspect` on both candidates
3. `valid_distinguish`
4. `valid_explain` on the affected property if the divergence still needs more
   context

## What To Record

- shared requirement brief
- differing assumption
- divergence kind
- checkpoint summary
- final product decision
- follow-up model edits or property edits

## Example Questions

- Should retry preserve the draft or reset it?
- Is a deleted object hidden immediately or only after confirmation?
- Does rate limiting block the current request or only future requests?

## Exit Criteria

You are done when the comparison produces:

- a named ambiguity
- a shortest distinguishing trace
- a clear decision or a focused product question
