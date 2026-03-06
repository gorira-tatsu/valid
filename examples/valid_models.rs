use valid::{
    registry::run_registry_cli,
    valid_actions, valid_model, valid_models, valid_state,
};

valid_state! {
    struct State {
        x: u8,
        locked: bool,
    }
}

valid_actions! {
    enum Action {
        Inc => "INC",
        Lock => "LOCK",
        Unlock => "UNLOCK",
    }
}

valid_model! {
    model CounterModel;
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

valid_model! {
    model FailingCounterModel;
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
