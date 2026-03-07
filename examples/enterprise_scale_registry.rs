use valid::{registry::run_registry_cli, valid_model, valid_models, ValidAction, ValidState};

#[derive(Clone, Debug, PartialEq, Eq, Hash, ValidState)]
struct AccessReviewState {
    #[valid(range = "0..=12")]
    open_findings: u16,
    #[valid(range = "0..=3")]
    approved_exceptions: u8,
    scp_locked: bool,
    breakglass_used: bool,
    privileged_access_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidAction)]
enum AccessReviewAction {
    #[valid(
        action_id = "ADD_FINDING",
        reads = ["open_findings"],
        writes = ["open_findings"]
    )]
    AddFinding,
    #[valid(
        action_id = "APPROVE_EXCEPTION",
        reads = ["approved_exceptions"],
        writes = ["approved_exceptions"]
    )]
    ApproveException,
    #[valid(action_id = "LOCK_SCP", reads = ["scp_locked"], writes = ["scp_locked"])]
    LockScp,
    #[valid(
        action_id = "UNLOCK_SCP",
        reads = ["scp_locked", "approved_exceptions", "open_findings"],
        writes = ["scp_locked"]
    )]
    UnlockScp,
    #[valid(
        action_id = "USE_BREAKGLASS",
        reads = ["breakglass_used"],
        writes = ["breakglass_used"]
    )]
    UseBreakglass,
    #[valid(
        action_id = "ENABLE_PRIVILEGED_ACCESS",
        reads = ["scp_locked", "breakglass_used", "open_findings", "approved_exceptions"],
        writes = ["privileged_access_enabled"]
    )]
    EnablePrivilegedAccess,
}

valid_model! {
    model AccessReviewScaleModel<AccessReviewState, AccessReviewAction>;
    init [AccessReviewState {
        open_findings: 0,
        approved_exceptions: 0,
        scp_locked: false,
        breakglass_used: false,
        privileged_access_enabled: false,
    }];
    transitions {
        transition AddFinding [tags = ["risk_path", "review_path"]] when |state| state.open_findings <= 9 => [AccessReviewState {
            open_findings: state.open_findings + 3,
            approved_exceptions: state.approved_exceptions,
            scp_locked: state.scp_locked,
            breakglass_used: state.breakglass_used,
            privileged_access_enabled: state.privileged_access_enabled,
        }];
        transition ApproveException [tags = ["approval_path", "review_path"]] when |state| state.approved_exceptions <= 2 => [AccessReviewState {
            open_findings: state.open_findings,
            approved_exceptions: state.approved_exceptions + 1,
            scp_locked: state.scp_locked,
            breakglass_used: state.breakglass_used,
            privileged_access_enabled: state.privileged_access_enabled,
        }];
        transition LockScp [tags = ["deny_path", "scp_path"]] when |state| state.scp_locked == false => [AccessReviewState {
            open_findings: state.open_findings,
            approved_exceptions: state.approved_exceptions,
            scp_locked: true,
            breakglass_used: state.breakglass_used,
            privileged_access_enabled: false,
        }];
        transition UnlockScp [tags = ["recovery_path", "scp_path"]] when |state| state.scp_locked && (state.approved_exceptions >= 2 || state.open_findings - 2 <= 1) => [AccessReviewState {
            open_findings: state.open_findings,
            approved_exceptions: state.approved_exceptions,
            scp_locked: false,
            breakglass_used: state.breakglass_used,
            privileged_access_enabled: state.privileged_access_enabled,
        }];
        transition UseBreakglass [tags = ["exception_path"]] when |state| state.breakglass_used == false => [AccessReviewState {
            open_findings: state.open_findings,
            approved_exceptions: state.approved_exceptions,
            scp_locked: state.scp_locked,
            breakglass_used: true,
            privileged_access_enabled: state.privileged_access_enabled,
        }];
        transition EnablePrivilegedAccess [tags = ["allow_path", "approval_path", "exception_path"]] when |state| state.scp_locked == false && state.breakglass_used != false && (state.open_findings <= 2 || state.approved_exceptions >= 2) => [AccessReviewState {
            open_findings: state.open_findings,
            approved_exceptions: state.approved_exceptions,
            scp_locked: state.scp_locked,
            breakglass_used: state.breakglass_used,
            privileged_access_enabled: true,
        }];
    }
    properties {
        invariant P_PRIV_ACCESS_REQUIRES_REVIEW |state| state.privileged_access_enabled == false || (state.open_findings <= 2 || state.approved_exceptions >= 2);
        invariant P_SCP_LOCK_BLOCKS_PRIV_ACCESS |state| state.privileged_access_enabled == false || state.scp_locked == false;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, ValidState)]
struct QuotaGuardrailState {
    #[valid(range = "0..=5000")]
    monthly_spend_cents: u16,
    approved_budget_increase: bool,
    dpa_signed: bool,
    region_aligned: bool,
    waiver_active: bool,
    export_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidAction)]
enum QuotaGuardrailAction {
    #[valid(
        action_id = "RAISE_SPEND",
        reads = ["monthly_spend_cents"],
        writes = ["monthly_spend_cents"]
    )]
    RaiseSpend,
    #[valid(
        action_id = "APPROVE_BUDGET_INCREASE",
        reads = ["monthly_spend_cents", "approved_budget_increase"],
        writes = ["approved_budget_increase"]
    )]
    ApproveBudgetIncrease,
    #[valid(action_id = "SIGN_DPA", reads = ["dpa_signed"], writes = ["dpa_signed"])]
    SignDpa,
    #[valid(
        action_id = "ALIGN_REGION",
        reads = ["region_aligned"],
        writes = ["region_aligned"]
    )]
    AlignRegion,
    #[valid(
        action_id = "ACTIVATE_WAIVER",
        reads = ["waiver_active"],
        writes = ["waiver_active"]
    )]
    ActivateWaiver,
    #[valid(
        action_id = "ENABLE_EXPORT",
        reads = ["monthly_spend_cents", "approved_budget_increase", "dpa_signed", "region_aligned", "waiver_active"],
        writes = ["export_enabled"]
    )]
    EnableExport,
}

valid_model! {
    model QuotaGuardrailRegressionModel<QuotaGuardrailState, QuotaGuardrailAction>;
    init [QuotaGuardrailState {
        monthly_spend_cents: 0,
        approved_budget_increase: false,
        dpa_signed: false,
        region_aligned: false,
        waiver_active: false,
        export_enabled: false,
    }];
    transitions {
        transition RaiseSpend [tags = ["finance_path", "quota_path"]] when |state| state.monthly_spend_cents <= 4500 => [QuotaGuardrailState {
            monthly_spend_cents: state.monthly_spend_cents + 500,
            approved_budget_increase: state.approved_budget_increase,
            dpa_signed: state.dpa_signed,
            region_aligned: state.region_aligned,
            waiver_active: state.waiver_active,
            export_enabled: state.export_enabled,
        }];
        transition ApproveBudgetIncrease [tags = ["approval_path", "finance_path"]] when |state| state.monthly_spend_cents >= 2000 && state.approved_budget_increase == false => [QuotaGuardrailState {
            monthly_spend_cents: state.monthly_spend_cents,
            approved_budget_increase: true,
            dpa_signed: state.dpa_signed,
            region_aligned: state.region_aligned,
            waiver_active: state.waiver_active,
            export_enabled: state.export_enabled,
        }];
        transition SignDpa [tags = ["governance_path"]] when |state| state.dpa_signed == false => [QuotaGuardrailState {
            monthly_spend_cents: state.monthly_spend_cents,
            approved_budget_increase: state.approved_budget_increase,
            dpa_signed: true,
            region_aligned: state.region_aligned,
            waiver_active: state.waiver_active,
            export_enabled: state.export_enabled,
        }];
        transition AlignRegion [tags = ["governance_path"]] when |state| state.region_aligned == false => [QuotaGuardrailState {
            monthly_spend_cents: state.monthly_spend_cents,
            approved_budget_increase: state.approved_budget_increase,
            dpa_signed: state.dpa_signed,
            region_aligned: true,
            waiver_active: state.waiver_active,
            export_enabled: state.export_enabled,
        }];
        transition ActivateWaiver [tags = ["exception_path", "deny_path"]] when |state| state.waiver_active == false => [QuotaGuardrailState {
            monthly_spend_cents: state.monthly_spend_cents,
            approved_budget_increase: state.approved_budget_increase,
            dpa_signed: state.dpa_signed,
            region_aligned: state.region_aligned,
            waiver_active: true,
            export_enabled: state.export_enabled,
        }];
        transition EnableExport [tags = ["allow_path", "exception_path", "finance_path"]] when |state| state.dpa_signed && state.region_aligned && (state.monthly_spend_cents < 2000 || state.approved_budget_increase || state.waiver_active != false) => [QuotaGuardrailState {
            monthly_spend_cents: state.monthly_spend_cents,
            approved_budget_increase: state.approved_budget_increase,
            dpa_signed: state.dpa_signed,
            region_aligned: state.region_aligned,
            waiver_active: state.waiver_active,
            export_enabled: true,
        }];
    }
    properties {
        invariant P_EXPORT_REQUIRES_GOVERNANCE |state| state.export_enabled == false || (state.dpa_signed && state.region_aligned);
        invariant P_EXPORT_REQUIRES_BUDGET_DISCIPLINE |state| state.export_enabled == false || (state.monthly_spend_cents < 2000 || state.approved_budget_increase);
    }
}

fn main() {
    run_registry_cli(valid_models![
        "access-review-scale" => AccessReviewScaleModel,
        "quota-guardrail-regression" => QuotaGuardrailRegressionModel,
    ]);
}
