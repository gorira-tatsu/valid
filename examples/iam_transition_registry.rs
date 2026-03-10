/*
IAM 風アクセス評価例

目的:
  - declarative `transitions` と path tags を使った solver-ready 例を示す
  - embedded SAT / SMT / command backend で扱いやすい bool 中心モデルを用意する

含まれるモデル:
  - iam-access
    Billing read は boundary と session の両方を要求する安全な仕様

主な性質:
  - P_BILLING_READ_REQUIRES_BOUNDARY
  - P_BILLING_READ_REQUIRES_SESSION

最初に試すコマンド:
  cargo valid --registry examples/iam_transition_registry.rs inspect iam-access
  cargo valid --registry examples/iam_transition_registry.rs graph iam-access --format=json
  cargo valid --registry examples/iam_transition_registry.rs verify iam-access --property=P_BILLING_READ_REQUIRES_SESSION
  cargo valid --registry examples/iam_transition_registry.rs verify iam-access --property=P_BILLING_READ_REQUIRES_SESSION --backend=sat-varisat --json

見どころ:
  - `sat-varisat` で扱いやすい bounded bool surface の確認用
  - `graph --format=json` で review-oriented snapshot を取りやすい
  - path tags を保ったまま backend 比較や explain/testgen の起点にしやすい
*/
use valid::{registry::run_registry_cli, valid_actions, valid_model, valid_models, valid_state};

valid_state! {
    struct AccessState {
        boundary_attached: bool,
        session_active: bool,
        billing_read_allowed: bool,
    }
}

valid_actions! {
    enum AccessAction {
        AttachBoundary => "ATTACH_BOUNDARY" [reads = ["boundary_attached"], writes = ["boundary_attached"]],
        AssumeSession => "ASSUME_SESSION" [reads = ["boundary_attached", "session_active"], writes = ["session_active"]],
        EvaluateBillingRead => "EVAL_BILLING_READ" [reads = ["boundary_attached", "session_active"], writes = ["billing_read_allowed"]],
    }
}

valid_model! {
    model IamAccessModel<AccessState, AccessAction>;
    init [AccessState {
        boundary_attached: false,
        session_active: false,
        billing_read_allowed: false,
    }];
    transitions {
        transition AttachBoundary [tags = ["boundary_path"]] when |state| !state.boundary_attached => [AccessState {
            boundary_attached: true,
            session_active: state.session_active,
            billing_read_allowed: state.billing_read_allowed,
        }];
        transition AssumeSession [tags = ["session_path"]] when |state| state.boundary_attached && !state.session_active => [AccessState {
            boundary_attached: state.boundary_attached,
            session_active: true,
            billing_read_allowed: state.billing_read_allowed,
        }];
        transition EvaluateBillingRead [tags = ["allow_path", "boundary_path", "session_path"]] when |state| state.boundary_attached && state.session_active => [AccessState {
            boundary_attached: state.boundary_attached,
            session_active: state.session_active,
            billing_read_allowed: true,
        }];
    }
    properties {
        invariant P_BILLING_READ_REQUIRES_BOUNDARY |state| !state.billing_read_allowed || state.boundary_attached;
        invariant P_BILLING_READ_REQUIRES_SESSION |state| !state.billing_read_allowed || state.session_active;
    }
}

fn main() {
    run_registry_cli(valid_models![
        "iam-access" => IamAccessModel,
    ]);
}
