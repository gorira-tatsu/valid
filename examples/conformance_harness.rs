/*
Conformance harness example

Purpose:
  - show the smallest in-process RustConformanceHarness flow
  - demonstrate `testgen -> handoff -> conformance` without an external runner

First command to try:
  cargo run --example conformance_harness
*/
use std::collections::BTreeMap;

use valid::{
    conformance::{build_vector_from_actions, run_rust_conformance, RustConformanceHarness},
    frontend::compile_model,
    ir::Value,
    testgen::VectorActionStep,
};

struct CounterHarness {
    x: u64,
}

impl RustConformanceHarness for CounterHarness {
    fn apply_action(&mut self, step: &VectorActionStep) -> Result<BTreeMap<String, Value>, String> {
        match step.action_id.as_str() {
            "Inc" => self.x += 1,
            other => return Err(format!("unsupported action `{other}`")),
        }
        Ok(BTreeMap::from([(String::from("x"), Value::UInt(self.x))]))
    }

    fn property_holds(&self, property_id: &str) -> Result<Option<bool>, String> {
        match property_id {
            "P_SAFE" => Ok(Some(self.x <= 2)),
            _ => Ok(None),
        }
    }
}

fn main() {
    let model = compile_model(
        r#"
model Counter
state:
  x: u8[0..2]
init:
  x = 0
action Inc:
  pre: x <= 1
  post:
    x = x + 1
property P_SAFE:
  invariant: x <= 2
"#,
    )
    .expect("counter model should compile");

    let vector = build_vector_from_actions(
        &model,
        Some("P_SAFE"),
        &[String::from("Inc"), String::from("Inc")],
    )
    .expect("vector should build");
    let report = run_rust_conformance(&vector, &mut CounterHarness { x: 0 });
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("report should serialize")
    );
}
