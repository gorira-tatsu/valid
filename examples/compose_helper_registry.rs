/*
Compose helper example

Purpose:
  - show the current supported helper-based composition story without claiming a
    first-class compose DSL
  - keep the example small enough to review in one screen
  - demonstrate shared-field synchronization plus ordinary inspect/check APIs

First command to try:
  cargo run --example compose_helper_registry
*/
use valid::{
    api::{check_model, inspect_model, render_inspect_json, CheckRequest},
    compose::compose_models,
    frontend::compile_model,
};

fn main() {
    let approval = compile_model(
        "model Approval\nstate:\n  shared: bool\n  approved: bool\ninit:\n  shared = false\n  approved = false\naction Approve:\n  pre: shared == false\n  post:\n    shared = true\n    approved = true\nproperty P_APPROVED_IMPLIES_SHARED:\n  invariant: approved == false || shared == true\n",
    )
    .expect("approval model should compile");
    let fulfillment = compile_model(
        "model Fulfillment\nstate:\n  shared: bool\n  shipped: bool\ninit:\n  shared = false\n  shipped = false\naction Ship:\n  pre: shared == true\n  post:\n    shipped = true\nproperty P_SHIP_REQUIRES_SHARED:\n  invariant: shipped == false || shared == true\n",
    )
    .expect("fulfillment model should compile");
    let composed =
        compose_models(&approval, &fulfillment, &["shared".to_string()]).expect("compose");

    let inspect = inspect_model("compose-helper-example", &composed);
    println!("{}", render_inspect_json(&inspect));

    let outcome = check_model(
        &CheckRequest {
            request_id: "compose-helper-check".to_string(),
            source_name: "compose-helper".to_string(),
            source: "compose helper".to_string(),
            property_id: Some("Fulfillment::P_SHIP_REQUIRES_SHARED".to_string()),
            profile_id: None,
            scenario_id: None,
            seed: None,
            backend: Some("explicit".to_string()),
            solver_executable: None,
            solver_args: Vec::new(),
        },
        &composed,
        "sha256:compose-helper".to_string(),
    );
    println!("{outcome:?}");
}
