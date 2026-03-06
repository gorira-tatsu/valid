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
    model CounterModel<State, Action>;
    property P_RANGE;
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
    invariant |state| state.x <= 3;
}

valid_model! {
    model FailingCounterModel<State, Action>;
    property P_FAIL;
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
    invariant |state| state.x <= 1;
}

fn main() {
    run_registry_cli(valid_models![
        "counter" => CounterModel,
        "failing-counter" => FailingCounterModel,
    ]);
}
