/*
Deadlock and enablement example

Purpose:
  - show one deadlock-oriented review path and one blocked-action enablement path
  - keep the registry small enough that `testgen` strategies are easy to map
    back to the model

First commands to try:
  cargo valid --registry examples/deadlock_enablement_registry.rs testgen deadlock-terminal --strategy=deadlock
  cargo valid --registry examples/deadlock_enablement_registry.rs testgen blocked-recovery --strategy=enablement --focus-action=Recover
*/
use valid::{registry::run_registry_cli, valid_actions, valid_model, valid_models, valid_state};

valid_state! {
    struct DeadlockState {
        stage: u8 [range = "0..=1"],
    }
}

valid_actions! {
    enum DeadlockAction {
        Advance => "ADVANCE" [reads = ["stage"], writes = ["stage"]],
    }
}

valid_model! {
    model DeadlockTerminalModel<DeadlockState, DeadlockAction>;
    init [DeadlockState {
        stage: 0,
    }];
    transitions {
        transition Advance [tags = ["review_path", "risk_path"]] when |state| state.stage == 0 => [DeadlockState {
            stage: 1,
        }];
    }
    properties {
        deadlock_freedom P_NO_DEADLOCK;
    }
}

valid_state! {
    struct EnablementState {
        locked: bool,
        recovered: bool,
    }
}

valid_actions! {
    enum EnablementAction {
        Lock => "LOCK" [reads = ["locked"], writes = ["locked"]],
        Recover => "RECOVER" [reads = ["locked", "recovered"], writes = ["recovered"]],
    }
}

valid_model! {
    model BlockedRecoveryModel<EnablementState, EnablementAction>;
    init [EnablementState {
        locked: false,
        recovered: false,
    }];
    transitions {
        transition Lock [tags = ["allow_path", "risk_path"]] when |state| state.locked == false => [EnablementState {
            locked: true,
            recovered: state.recovered,
        }];
        transition Recover [tags = ["review_path", "risk_path"]] when |state| state.locked && state.recovered == false => [EnablementState {
            locked: state.locked,
            recovered: true,
        }];
    }
    properties {
        invariant P_RECOVERY_IS_BOOLEAN |state| state.recovered == false || state.recovered == true;
    }
}

fn main() {
    run_registry_cli(valid_models![
        "deadlock-terminal" => DeadlockTerminalModel,
        "blocked-recovery" => BlockedRecoveryModel,
    ]);
}
