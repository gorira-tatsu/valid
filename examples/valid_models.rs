/*
最小カウンタ例

目的:
  - Rust DSL の最小構成を読むための入口にする
  - `step` モデルと declarative モデルの違いを説明する前の最小 fixture にする

含まれるモデル:
  - counter
    ロック中は増えず、x は常に 0..=3 に収まる
  - failing-counter
    わざと弱い不変条件を置き、反例・explain・testgen の入口にする

最初に試すコマンド:
  cargo valid --registry examples/valid_models.rs inspect counter
  cargo valid --registry examples/valid_models.rs verify failing-counter
*/
use valid::{registry::run_registry_cli, valid_actions, valid_model, valid_models, valid_state};

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

valid_model! {
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

valid_model! {
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
