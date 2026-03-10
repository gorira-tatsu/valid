/*
Handoff and testgen example

Purpose:
  - show a model whose implementation handoff clearly points at generated test
    vectors
  - keep requirement and risk grouping visible in one small registry

First commands to try:
  cargo valid --registry examples/handoff_testgen_registry.rs handoff review-gate-regression --json
  cargo valid --registry examples/handoff_testgen_registry.rs testgen review-gate-regression --strategy=counterexample --json

What to look for:
  - failing traces and vectors now keep `counterexample_kind`
  - generated vectors are ranked with `priority` and `selection_reason`
  - handoff output echoes the same review target instead of inventing a second
    selection model
*/
use valid::{registry::run_registry_cli, valid_actions, valid_model, valid_models, valid_state};

valid_state! {
    struct ReviewGateState {
        reviewed: bool,
        published: bool,
    }
}

valid_actions! {
    enum ReviewGateAction {
        Review => "REVIEW" [reads = ["reviewed"], writes = ["reviewed"]],
        Publish => "PUBLISH" [reads = ["reviewed", "published"], writes = ["published"]],
    }
}

valid_model! {
    model ReviewGateSafeModel<ReviewGateState, ReviewGateAction>;
    init [ReviewGateState {
        reviewed: false,
        published: false,
    }];
    transitions {
        transition Review [tags = ["review_path"]] when |state| state.reviewed == false => [ReviewGateState {
            reviewed: true,
            published: state.published,
        }];
        transition Publish [tags = ["allow_path", "review_path", "risk_path"]] when |state| state.reviewed && state.published == false => [ReviewGateState {
            reviewed: state.reviewed,
            published: true,
        }];
    }
    properties {
        invariant P_PUBLISH_REQUIRES_REVIEW |state| state.published == false || state.reviewed;
    }
}

valid_model! {
    model ReviewGateRegressionModel<ReviewGateState, ReviewGateAction>;
    init [ReviewGateState {
        reviewed: false,
        published: false,
    }];
    transitions {
        transition Publish [tags = ["review_path", "risk_path"]] when |state| state.published == false => [ReviewGateState {
            reviewed: state.reviewed,
            published: true,
        }];
    }
    properties {
        invariant P_PUBLISH_REQUIRES_REVIEW |state| state.published == false || state.reviewed;
    }
}

fn main() {
    run_registry_cli(valid_models![
        "review-gate-safe" => ReviewGateSafeModel,
        "review-gate-regression" => ReviewGateRegressionModel,
    ]);
}
