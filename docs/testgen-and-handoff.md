# Testgen and Handoff Guide

Use this guide when the question is no longer "does the model verify?" but
"what should implementation-facing tests look like?"

`valid` now treats generated vectors as language-agnostic test specs. The core
contract is the input sequence plus the expected observations and oracle
targets, not framework-specific Rust, Jest, or pytest code.

## What each surface is for

- `testgen`
  Generates the full test-spec artifacts and JSON vectors.
- `handoff`
  Produces a model-level implementation brief and a shortlist of recommended
  vectors.
- `conformance`
  Runs a SUT against those vectors and classifies the mismatch surface.

Treat them as one workflow:

1. `inspect` or `lint` the model if the capability picture is unclear.
2. `testgen` to get full vectors or artifacts.
3. `handoff` to get the model summary plus recommended vectors.
4. `conformance` to compare the real implementation against the accepted model.

## Core test-spec contract

The current public vector contract is observation-first.

- `actions`
  The replayable input sequence.
- `expected_observations`
  The primary observable oracle.
- `observation_layers`
  Which layers matter for this vector, such as `output`, `state`, or
  `side_effect`.
- `oracle_targets`
  What the vector expects the consumer to compare, such as `observations` or
  `property_holds`.
- `implementation_hints`
  Hints for where the vector best fits, such as `api`, `ui`, or `handler`.
- `expected_states`
  Debug or projection help, not the only source of truth.

This is why `valid` should own the spec, while the target repository or AI
agent owns the final test body, mocks, hooks, and assertion style.

## Handoff summaries

`valid handoff` and `cargo valid handoff` now include a `testgen_summary`
section. That summary is intentionally smaller than the full vector set.

It tells you:

- which strategy the summary came from
- how many vectors were generated
- which files were written
- which vectors are most worth implementing first

Each recommended vector includes:

- `vector_id`
- `property_id`
- `strategy`
- `grouping`
- `observation_layers`
- `oracle_targets`
- `suggested_surface`
- `state_visibility`
- `why_this_vector_matters`

Use `handoff` when you want a brief. Use `testgen` when you need the full JSON
or generated artifacts.

## Recommended command flow

```sh
cargo valid inspect <model>
cargo valid handoff <model> --json
cargo valid testgen <model> --json
cargo valid conformance <model> --runner <runner>
```

If you are working through MCP:

- `valid_handoff`
- `valid_testgen`
- `valid_conformance`
- `valid_explain`

## Choosing the right implementation surface

Use the vector metadata to decide where the test should live.

- `suggested_surface=api`
  Good fit for HTTP or RPC request/response tests.
- `suggested_surface=handler`
  Good fit for in-process function or service-layer conformance.
- `suggested_surface=ui`
  Good fit when the visible behavior is the important oracle.

When internal state is hard to observe, prefer `expected_observations` and
`observation_layers=output` over wiring deep state mocks just to satisfy a
vector.

## Next read

- [Testgen Strategies Guide](./testgen-strategies.md)
- [Conformance Workflow](./ai/conformance-workflow.md)
- [Graph and Review Guide](./graph-and-review.md)
