/*
IAM エンタープライズ例

目的:
  - boundary, session, MFA, SCP, explicit deny を合わせた IAM 風 access model を置く
  - OR 条件や deny 優先の policy-path を確認する benchmark fixture にする

主な性質:
  - P_BILLING_READ_REQUIRES_SESSION
  - P_BILLING_READ_REQUIRES_BOUNDARY_OR_MFA
  - P_DENY_BLOCKS_ACCESS

最初に試すコマンド:
  cargo valid --registry benchmarks/registries/iam_enterprise_registry.rs inspect iam-enterprise
  cargo valid --registry benchmarks/registries/iam_enterprise_registry.rs verify iam-enterprise --property=P_DENY_BLOCKS_ACCESS
*/
use valid::{registry::run_registry_cli, valid_actions, valid_model, valid_models, valid_state};

valid_state! {
    struct EnterpriseState {
        boundary_attached: bool,
        session_active: bool,
        mfa_present: bool,
        explicit_deny: bool,
        scp_allows_billing: bool,
        billing_read_allowed: bool,
        logs_read_allowed: bool,
    }
}

valid_actions! {
    enum EnterpriseAction {
        AttachBoundary => "ATTACH_BOUNDARY" [reads = ["boundary_attached"], writes = ["boundary_attached"]],
        AssumeSession => "ASSUME_SESSION" [reads = ["boundary_attached", "session_active"], writes = ["session_active"]],
        EnableMfa => "ENABLE_MFA" [reads = ["mfa_present"], writes = ["mfa_present"]],
        SetExplicitDeny => "SET_EXPLICIT_DENY" [reads = ["explicit_deny"], writes = ["explicit_deny"]],
        EvaluateBillingRead => "EVAL_BILLING_READ" [reads = ["boundary_attached", "session_active", "mfa_present", "explicit_deny", "scp_allows_billing"], writes = ["billing_read_allowed"]],
        EvaluateLogsRead => "EVAL_LOGS_READ" [reads = ["session_active", "explicit_deny"], writes = ["logs_read_allowed"]],
    }
}

valid_model! {
    model EnterpriseIamModel<EnterpriseState, EnterpriseAction>;
    init [EnterpriseState {
        boundary_attached: false,
        session_active: false,
        mfa_present: false,
        explicit_deny: false,
        scp_allows_billing: true,
        billing_read_allowed: false,
        logs_read_allowed: false,
    }];
    transitions {
        transition AttachBoundary [tags = ["boundary_path"]] when |state| state.boundary_attached == false => [EnterpriseState {
            boundary_attached: true,
            session_active: state.session_active,
            mfa_present: state.mfa_present,
            explicit_deny: state.explicit_deny,
            scp_allows_billing: state.scp_allows_billing,
            billing_read_allowed: state.billing_read_allowed,
            logs_read_allowed: state.logs_read_allowed,
        }];
        transition AssumeSession [tags = ["session_path"]] when |state| state.boundary_attached && state.session_active == false => [EnterpriseState {
            boundary_attached: state.boundary_attached,
            session_active: true,
            mfa_present: state.mfa_present,
            explicit_deny: state.explicit_deny,
            scp_allows_billing: state.scp_allows_billing,
            billing_read_allowed: state.billing_read_allowed,
            logs_read_allowed: state.logs_read_allowed,
        }];
        transition EnableMfa [tags = ["session_path"]] when |state| state.mfa_present == false => [EnterpriseState {
            boundary_attached: state.boundary_attached,
            session_active: state.session_active,
            mfa_present: true,
            explicit_deny: state.explicit_deny,
            scp_allows_billing: state.scp_allows_billing,
            billing_read_allowed: state.billing_read_allowed,
            logs_read_allowed: state.logs_read_allowed,
        }];
        transition SetExplicitDeny [tags = ["deny_path"]] when |state| state.explicit_deny == false => [EnterpriseState {
            boundary_attached: state.boundary_attached,
            session_active: state.session_active,
            mfa_present: state.mfa_present,
            explicit_deny: true,
            scp_allows_billing: state.scp_allows_billing,
            billing_read_allowed: false,
            logs_read_allowed: false,
        }];
        transition EvaluateBillingRead [tags = ["allow_path", "boundary_path", "session_path"]] when |state| state.explicit_deny == false && state.scp_allows_billing && state.session_active && (state.boundary_attached || state.mfa_present) => [EnterpriseState {
            boundary_attached: state.boundary_attached,
            session_active: state.session_active,
            mfa_present: state.mfa_present,
            explicit_deny: state.explicit_deny,
            scp_allows_billing: state.scp_allows_billing,
            billing_read_allowed: true,
            logs_read_allowed: state.logs_read_allowed,
        }];
        transition EvaluateLogsRead [tags = ["allow_path", "session_path"]] when |state| state.explicit_deny == false && state.session_active => [EnterpriseState {
            boundary_attached: state.boundary_attached,
            session_active: state.session_active,
            mfa_present: state.mfa_present,
            explicit_deny: state.explicit_deny,
            scp_allows_billing: state.scp_allows_billing,
            billing_read_allowed: state.billing_read_allowed,
            logs_read_allowed: true,
        }];
    }
    properties {
        invariant P_BILLING_READ_REQUIRES_SESSION |state| state.billing_read_allowed == false || state.session_active;
        invariant P_BILLING_READ_REQUIRES_BOUNDARY_OR_MFA |state| state.billing_read_allowed == false || state.boundary_attached || state.mfa_present;
        invariant P_DENY_BLOCKS_ACCESS |state| state.explicit_deny == false || (state.billing_read_allowed == false && state.logs_read_allowed == false);
    }
}

fn main() {
    run_registry_cli(valid_models![
        "iam-enterprise" => EnterpriseIamModel,
    ]);
}
