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
cargo valid testgen <model>
cargo valid conformance <model> --runner <runner>
```

Useful MCP tools:

- `valid_inspect`
- `valid_testgen`
- `valid_contract_check`
- `valid_explain`

## Rust-native harnesses

If your SUT already lives in Rust, you do not need to shell out to an external
runner. `valid::conformance` now exposes a `RustConformanceHarness` trait and
`run_rust_conformance(...)` helper so you can drive a `TestVector` directly in
process.

Minimal shape:

```rust
use std::collections::BTreeMap;
use valid::{
    conformance::{run_rust_conformance, RustConformanceHarness},
    ir::Value,
};

struct CounterHarness {
    x: u64,
}

impl RustConformanceHarness for CounterHarness {
    fn harness_name(&self) -> &'static str {
        "counter-harness"
    }

    fn apply_action(
        &mut self,
        step: &valid::testgen::VectorActionStep,
    ) -> Result<BTreeMap<String, Value>, String> {
        match step.action_id.as_str() {
            "Inc" => {
                self.x += 1;
                Ok(BTreeMap::from([("x".into(), Value::UInt(self.x))]))
            }
            other => Err(format!("unknown action `{other}`")),
        }
    }

    fn property_holds(&self, property_id: &str) -> Result<Option<bool>, String> {
        match property_id {
            "P_SAFE" => Ok(Some(self.x <= 2)),
            _ => Ok(None),
        }
    }
}

let report = run_rust_conformance(&vector, &mut CounterHarness { x: 0 });
assert_eq!(report.status, "PASS");
```

Use the external `--runner` flow when:

- the SUT is not written in Rust
- the comparison boundary is a separate process or service
- you want stdin/stdout compatibility across languages

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
