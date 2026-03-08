# Requirement Refinement Workflow

Use this page when the requirement is still moving, or when model evidence
shows that the current brief is underspecified.

This workflow exists to keep `valid` product-oriented. The goal is not to ask
generic questions forever. The goal is to turn ambiguity into a stable
modeling brief, then use verification evidence to refine only the parts of the
requirement that matter.

## When to use it

Start here when any of these are true:

- the request is written in feature language rather than state/action language
- the team disagrees about success, failure, retry, or recovery behavior
- a counterexample exposes a behavior nobody explicitly decided
- one or more actions stay dead because the requirement omitted an enabling path
- a property or cover looks vacuous because the intended state slice was never
  made concrete
- conformance shows a mismatch and it is unclear whether the model or product
  expectation is wrong

## Refinement loop

1. Read the [AI Authoring Guide](./authoring-guide.md) for the current
   supported modeling surface.
2. Start with the raw requirement, user story, incident note, or policy text.
3. Ask the minimum follow-up questions needed to lock down:
   - actors and authoritative state
   - happy path and rejection path
   - retries, recovery, and terminal failure
   - side effects, audit/notification expectations, and out-of-scope behavior
4. Write a compact modeling brief before editing the model.
5. Author or review the model.
6. Run evidence-producing tools such as `valid_inspect`, `valid_lint`,
   `valid_check`, `valid_explain`, `valid_coverage`, or conformance.
7. If new evidence exposes ambiguity, ask targeted follow-up questions and
   update the brief instead of patching the model blindly.

## What the modeling brief should contain

Keep the brief short and concrete. It should usually fit on one screen.

- product requirement summary in plain language
- authoritative state and business actors
- allowed actions and their preconditions
- failure, retry, timeout, cancellation, or recovery paths
- explicit assumptions
- in-scope and out-of-scope behavior
- likely predicates, scenarios, and critical properties
- expected verification mode: solver-ready, explicit-ready, or mixed

## Evidence-driven follow-up

Different evidence types should trigger different questions.

### Counterexample

Ask:

- Is the failing path actually forbidden, or was the property too strong?
- Which guard, assumption, or prerequisite was missing from the brief?
- Should this become a business-facing property, scenario, or path tag?

### Dead action

Ask:

- Is the action truly reachable in the product, or is it intentionally staged
  for a later milestone?
- Which enabling state or transition is missing?
- Is the action mislabeled, or is the requirement itself incomplete?

### Vacuity or empty coverage

Ask:

- Did we define the target state slice concretely enough?
- Is the property checking setup behavior instead of the real product path?
- Do we need a scenario, predicate, or path tag that matches the business
  intent more directly?

### Conformance mismatch

Ask:

- Did the product expectation change after the model was accepted?
- Is the implementation violating a stable rule, or did the model miss a real
  exception path?
- What requirement statement should be changed so future traces classify the
  mismatch correctly?

## Recommended MCP flow

1. `valid_docs_index`
2. `valid_docs_get` for this page and the [AI Authoring Guide](./authoring-guide.md)
3. `refine_requirement` for the first clarification pass
4. `author_model` or `review_model`
5. `valid_inspect` and `valid_lint`
6. `valid_check`, `valid_explain`, `valid_coverage`, or conformance-oriented
   tooling
7. `refine_requirement_from_evidence` when the evidence shows requirement drift
   or an underspecified path

## Example session: ambiguous feature request

Input:

> Users can retry failed exports after approval.

Clarifying questions:

- Does approval lock the export payload, or can users edit and retry?
- Are retries unlimited, or capped?
- What counts as a failed export: transport failure, validation failure, or
  downstream rejection?

Resulting brief:

- Approved exports are immutable.
- Users may retry transport failures up to three times.
- Validation failures require a new approval.
- Critical properties:
  - approved payloads do not change during retry
  - retries stop after the configured cap

## Example session: evidence triggers refinement

Evidence:

- `valid_explain` shows a counterexample where an approved export retries after
  a validation failure
- `valid_coverage` shows the manual-review recovery path is never reached

Follow-up questions:

- Should validation failure revoke approval immediately?
- Is manual review mandatory after repeated transport failures, or only after
  validation failure?

Updated brief:

- Validation failure revokes approval.
- Three transport failures escalate to manual review.
- Add a manual-review scenario and separate properties for retry cap vs
  approval revocation.

## Exit criteria

You are ready to leave refinement mode when:

- the brief distinguishes business ambiguity from modeling ambiguity
- the next model edit is obvious from the brief
- each failing trace maps to a named requirement or assumption
- open questions are explicit and limited, not hidden in the model

## Next read

- [AI Authoring Guide](./authoring-guide.md)
- [Modeling Checklist](./modeling-checklist.md)
- [Review Workflow](./review-workflow.md)
- [Conformance Workflow](./conformance-workflow.md)
