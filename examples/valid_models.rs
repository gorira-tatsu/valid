use std::collections::BTreeMap;

use valid::{
    ir::Value,
    modeling::{Finite, ModelingAction, ModelingState, VerifiedMachine},
    registry::run_registry_cli,
    valid_models,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct State {
    x: u8,
    locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Action {
    Inc,
    Lock,
    Unlock,
}

impl Finite for Action {
    fn all() -> Vec<Self> {
        vec![Self::Inc, Self::Lock, Self::Unlock]
    }
}

impl ModelingAction for Action {
    fn action_id(&self) -> String {
        match self {
            Action::Inc => "INC".to_string(),
            Action::Lock => "LOCK".to_string(),
            Action::Unlock => "UNLOCK".to_string(),
        }
    }
}

impl ModelingState for State {
    fn snapshot(&self) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("x".to_string(), Value::UInt(self.x as u64)),
            ("locked".to_string(), Value::Bool(self.locked)),
        ])
    }
}

struct CounterModel;
struct FailingCounterModel;

impl VerifiedMachine for CounterModel {
    type State = State;
    type Action = Action;

    fn model_id() -> &'static str {
        "CounterModel"
    }

    fn property_id() -> &'static str {
        "P_RANGE"
    }

    fn init_states() -> Vec<Self::State> {
        vec![State {
            x: 0,
            locked: false,
        }]
    }

    fn step(state: &Self::State, action: &Self::Action) -> Vec<Self::State> {
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

    fn holds(state: &Self::State) -> bool {
        state.x <= 3
    }
}

impl VerifiedMachine for FailingCounterModel {
    type State = State;
    type Action = Action;

    fn model_id() -> &'static str {
        "FailingCounterModel"
    }

    fn property_id() -> &'static str {
        "P_FAIL"
    }

    fn init_states() -> Vec<Self::State> {
        CounterModel::init_states()
    }

    fn step(state: &Self::State, action: &Self::Action) -> Vec<Self::State> {
        CounterModel::step(state, action)
    }

    fn holds(state: &Self::State) -> bool {
        state.x <= 1
    }
}

fn main() {
    run_registry_cli(valid_models![
        "counter" => CounterModel,
        "failing-counter" => FailingCounterModel,
    ]);
}
