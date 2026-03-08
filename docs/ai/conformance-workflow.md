# Conformance Workflow

Use this page when the model has already been reviewed and the next question
is whether an implementation still matches it.

This is the bridge between formal model review and implementation-facing test
or runner workflows.

## Conformance order

1. Confirm that the model itself is understandable and reviewed.
2. Inspect or lint the model if the capability picture is unclear.
3. Generate or collect the evidence vectors you want to run against the SUT.
4. Run conformance and classify the mismatch surface.
5. Decide whether the next fix belongs in:
   - the model
   - the implementation
   - the requirement

## Useful commands

```sh
cargo valid inspect <model>
cargo valid handoff <model> --write
cargo valid testgen <model>
cargo valid conformance <model> --runner <runner>
```

Useful MCP tools:

- `valid_inspect`
- `valid_handoff`
- `valid_testgen`
- `valid_contract_check`
- `valid_explain`

## What a good conformance review records

- which model and property set were used
- which generated vectors or traces were exercised
- whether the mismatch is state, output, property, or harness-oriented
- which requirement or scenario the mismatch belongs to

## Common conformance mistakes

- treating generated tests as proof that the model is already right
- skipping model review before blaming the implementation
- using one large replay trace when a smaller witness would isolate the issue
- mixing contract drift and runtime mismatch without recording both

## Next read

- [Review Workflow](./review-workflow.md)
- [Modeling Checklist](./modeling-checklist.md)
- [Examples Curriculum](./examples-curriculum.md)
