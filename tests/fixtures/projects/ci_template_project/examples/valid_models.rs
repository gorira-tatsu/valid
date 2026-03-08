use valid::{registry::run_registry_cli, valid_actions, valid_model, valid_models, valid_state};

valid_state! {
    struct State {
        approved: bool,
    }
}

valid_actions! {
    enum Action {
        Approve => "APPROVE" [reads = ["approved"], writes = ["approved"]],
    }
}

valid_model! {
    model ApprovalModel<State, Action>;
    init [State { approved: false }];
    transitions {
        transition Approve [tags = ["allow_path"]] when |state| state.approved == false => [State { approved: true }];
    }
    properties {
        invariant P_APPROVAL_IS_BOOLEAN |state| state.approved == false || state.approved == true;
    }
}

fn main() {
    run_registry_cli(valid_models![
        "approval-model" => ApprovalModel,
    ]);
}
