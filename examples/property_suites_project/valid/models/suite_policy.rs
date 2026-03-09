use valid::{valid_actions, valid_model, valid_state};

valid_state! {
    struct SuitePolicyState {
        ready: bool,
        retries: u8 [range = "0..=2"],
    }
}

valid_actions! {
    enum SuitePolicyAction {
        Start => "START" [reads = ["ready"], writes = ["ready"]],
        Retry => "RETRY" [reads = ["ready", "retries"], writes = ["retries"]],
    }
}

valid_model! {
    /// Minimal project-first suite example.
    model SuitePolicyModel<SuitePolicyState, SuitePolicyAction>;
    init [SuitePolicyState {
        ready: false,
        retries: 0,
    }];
    transitions {
        transition Start [tags = ["review_path"]] when |state| state.ready == false => [SuitePolicyState {
            ready: true,
            retries: state.retries,
        }];
        transition Retry [tags = ["risk_path"]] when |state| state.ready && state.retries < 2 => [SuitePolicyState {
            ready: state.ready,
            retries: state.retries + 1,
        }];
    }
    properties {
        invariant P_READY_BOOLEAN |state| state.ready == false || state.ready == true;
        invariant P_RETRIES_BOUNDED |state| state.retries <= 2;
    }
}
