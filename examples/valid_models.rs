/*
Minimal counter example

Purpose:
  - provide the smallest readable entry point into the Rust DSL
  - act as the baseline fixture before explaining the tradeoffs between
    `step` models and declarative transition models

Included models:
  - `counter`
    Does not increment while locked and keeps `x` within `0..=3`.
  - `failing-counter`
    Uses an intentionally weak invariant so counterexample, explain, and
    testgen flows have a small failing fixture.

First commands to try:
  cargo valid --registry examples/valid_models.rs inspect counter
  cargo valid --registry examples/valid_models.rs verify failing-counter
*/
use valid::{
    registry::run_registry_cli, valid_actions, valid_models, valid_state, valid_step_model,
};

valid_state! {
    struct State {
        x: u8 [range = "0..=3"],
        locked: bool,
    }
}

valid_actions! {
    enum Action {
        Inc => "INC" [reads = ["x", "locked"], writes = ["x"]],
        Lock => "LOCK" [reads = ["locked"], writes = ["locked"]],
        Unlock => "UNLOCK" [reads = ["locked"], writes = ["locked"]],
    }
}

valid_step_model! {
    model CounterModel<State, Action>;
    init [State {
        x: 0,
        locked: false,
    }];
    step |state, action| {
        match action {
            Action::Inc if !state.locked && state.x < 3 => vec![State {
                x: state.x + 1,
                locked: state.locked,
            }],
            Action::Lock => vec![State {
                x: state.x,
                locked: true,
            }],
            Action::Unlock => vec![State {
                x: state.x,
                locked: false,
            }],
            _ => Vec::new(),
        }
    }
    properties {
        invariant P_RANGE |state| state.x <= 3;
        invariant P_LOCKED_RANGE |state| !state.locked || state.x <= 3;
    }
}

valid_step_model! {
    model FailingCounterModel<State, Action>;
    init [State {
        x: 0,
        locked: false,
    }];
    step |state, action| {
        match action {
            Action::Inc if !state.locked && state.x < 3 => vec![State {
                x: state.x + 1,
                locked: state.locked,
            }],
            Action::Lock => vec![State {
                x: state.x,
                locked: true,
            }],
            Action::Unlock => vec![State {
                x: state.x,
                locked: false,
            }],
            _ => Vec::new(),
        }
    }
    properties {
        invariant P_FAIL |state| state.x <= 1;
    }
}

fn main() {
    run_registry_cli(valid_models![
        "counter" => CounterModel,
        "failing-counter" => FailingCounterModel,
    ]);
}
