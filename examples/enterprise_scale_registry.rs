use valid::{registry::run_registry_cli, valid_model, valid_models, ValidAction, ValidEnum, ValidState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidEnum)]
enum ReviewStage {
    Draft,
    Investigating,
    Approved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidEnum)]
enum ExportWaiverReason {
    Budget,
    Legal,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, ValidState)]
struct AccessReviewState {
    #[valid(range = "0..=12")]
    open_findings: u16,
    #[valid(range = "0..=3")]
    approved_exceptions: u8,
    #[valid(enum)]
    review_stage: ReviewStage,
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
        reads = ["scp_locked", "approved_exceptions", "open_findings", "review_stage"],
        writes = ["scp_locked", "review_stage"]
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
        reads = ["scp_locked", "breakglass_used", "open_findings", "approved_exceptions", "review_stage"],
        writes = ["privileged_access_enabled"]
    )]
    EnablePrivilegedAccess,
}

valid_model! {
    model AccessReviewScaleModel<AccessReviewState, AccessReviewAction>;
    init [AccessReviewState {
        open_findings: 0,
        approved_exceptions: 0,
        review_stage: ReviewStage::Draft,
        scp_locked: false,
        breakglass_used: false,
        privileged_access_enabled: false,
    }];
    transitions {
        transition AddFinding [tags = ["risk_path", "review_path"]] when |state| state.open_findings <= 9 => [AccessReviewState {
            open_findings: state.open_findings + 3,
            approved_exceptions: state.approved_exceptions,
            review_stage: ReviewStage::Investigating,
            scp_locked: state.scp_locked,
            breakglass_used: state.breakglass_used,
            privileged_access_enabled: state.privileged_access_enabled,
        }];
        transition ApproveException [tags = ["approval_path", "review_path"]] when |state| state.approved_exceptions <= 2 => [AccessReviewState {
            open_findings: state.open_findings,
            approved_exceptions: state.approved_exceptions + 1,
            review_stage: state.review_stage,
            scp_locked: state.scp_locked,
            breakglass_used: state.breakglass_used,
            privileged_access_enabled: state.privileged_access_enabled,
        }];
        transition LockScp [tags = ["deny_path", "scp_path"]] when |state| state.scp_locked == false => [AccessReviewState {
            open_findings: state.open_findings,
            approved_exceptions: state.approved_exceptions,
            review_stage: ReviewStage::Investigating,
            scp_locked: true,
            breakglass_used: state.breakglass_used,
            privileged_access_enabled: false,
        }];
        transition UnlockScp [tags = ["recovery_path", "scp_path"]] when |state| state.scp_locked && (state.approved_exceptions >= 2 || state.open_findings <= 3) => [AccessReviewState {
            open_findings: state.open_findings,
            approved_exceptions: state.approved_exceptions,
            review_stage: ReviewStage::Approved,
            scp_locked: false,
            breakglass_used: state.breakglass_used,
            privileged_access_enabled: state.privileged_access_enabled,
        }];
        transition UseBreakglass [tags = ["exception_path"]] when |state| state.breakglass_used == false => [AccessReviewState {
            open_findings: state.open_findings,
            approved_exceptions: state.approved_exceptions,
            review_stage: state.review_stage,
            scp_locked: state.scp_locked,
            breakglass_used: true,
            privileged_access_enabled: state.privileged_access_enabled,
        }];
        transition EnablePrivilegedAccess [tags = ["allow_path", "approval_path", "exception_path"]] when |state| state.scp_locked == false && state.breakglass_used != false && state.review_stage == ReviewStage::Approved && (state.open_findings <= 2 || state.approved_exceptions >= 2) => [AccessReviewState {
            open_findings: state.open_findings,
            approved_exceptions: state.approved_exceptions,
            review_stage: state.review_stage,
            scp_locked: state.scp_locked,
            breakglass_used: state.breakglass_used,
            privileged_access_enabled: true,
        }];
    }
    properties {
        invariant P_PRIV_ACCESS_REQUIRES_REVIEW |state| state.privileged_access_enabled == false || (state.open_findings <= 2 || state.approved_exceptions >= 2);
        invariant P_SCP_LOCK_BLOCKS_PRIV_ACCESS |state| state.privileged_access_enabled == false || state.scp_locked == false;
        invariant P_PRIV_ACCESS_REQUIRES_APPROVED_STAGE |state| state.privileged_access_enabled == false || state.review_stage == ReviewStage::Approved;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, ValidState)]
struct QuotaGuardrailState {
    #[valid(range = "0..=500000")]
    monthly_spend_cents: u32,
    approved_budget_increase: bool,
    dpa_signed: bool,
    region_aligned: bool,
    #[valid(enum)]
    waiver_reason: Option<ExportWaiverReason>,
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
        action_id = "GRANT_BUDGET_WAIVER",
        reads = ["waiver_reason"],
        writes = ["waiver_reason"]
    )]
    GrantBudgetWaiver,
    #[valid(
        action_id = "GRANT_LEGAL_WAIVER",
        reads = ["waiver_reason"],
        writes = ["waiver_reason"]
    )]
    GrantLegalWaiver,
    #[valid(
        action_id = "ENABLE_EXPORT",
        reads = ["monthly_spend_cents", "approved_budget_increase", "dpa_signed", "region_aligned", "waiver_reason"],
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
        waiver_reason: None,
        export_enabled: false,
    }];
    transitions {
        transition RaiseSpend [tags = ["finance_path", "quota_path"]] when |state| state.monthly_spend_cents <= 450000 => [QuotaGuardrailState {
            monthly_spend_cents: state.monthly_spend_cents + 50000,
            approved_budget_increase: state.approved_budget_increase,
            dpa_signed: state.dpa_signed,
            region_aligned: state.region_aligned,
            waiver_reason: state.waiver_reason,
            export_enabled: state.export_enabled,
        }];
        transition ApproveBudgetIncrease [tags = ["approval_path", "finance_path"]] when |state| state.monthly_spend_cents >= 200000 && state.approved_budget_increase == false => [QuotaGuardrailState {
            monthly_spend_cents: state.monthly_spend_cents,
            approved_budget_increase: true,
            dpa_signed: state.dpa_signed,
            region_aligned: state.region_aligned,
            waiver_reason: state.waiver_reason,
            export_enabled: state.export_enabled,
        }];
        transition SignDpa [tags = ["governance_path"]] when |state| state.dpa_signed == false => [QuotaGuardrailState {
            monthly_spend_cents: state.monthly_spend_cents,
            approved_budget_increase: state.approved_budget_increase,
            dpa_signed: true,
            region_aligned: state.region_aligned,
            waiver_reason: state.waiver_reason,
            export_enabled: state.export_enabled,
        }];
        transition AlignRegion [tags = ["governance_path"]] when |state| state.region_aligned == false => [QuotaGuardrailState {
            monthly_spend_cents: state.monthly_spend_cents,
            approved_budget_increase: state.approved_budget_increase,
            dpa_signed: state.dpa_signed,
            region_aligned: true,
            waiver_reason: state.waiver_reason,
            export_enabled: state.export_enabled,
        }];
        transition GrantBudgetWaiver [tags = ["exception_path", "finance_path"]] when |state| state.waiver_reason == None => [QuotaGuardrailState {
            monthly_spend_cents: state.monthly_spend_cents,
            approved_budget_increase: state.approved_budget_increase,
            dpa_signed: state.dpa_signed,
            region_aligned: state.region_aligned,
            waiver_reason: Some(ExportWaiverReason::Budget),
            export_enabled: state.export_enabled,
        }];
        transition GrantLegalWaiver [tags = ["exception_path", "governance_path"]] when |state| state.waiver_reason == None => [QuotaGuardrailState {
            monthly_spend_cents: state.monthly_spend_cents,
            approved_budget_increase: state.approved_budget_increase,
            dpa_signed: state.dpa_signed,
            region_aligned: state.region_aligned,
            waiver_reason: Some(ExportWaiverReason::Legal),
            export_enabled: state.export_enabled,
        }];
        transition EnableExport [tags = ["allow_path", "exception_path", "finance_path"]] when |state| state.dpa_signed && state.region_aligned && (state.monthly_spend_cents < 200000 || state.approved_budget_increase || state.waiver_reason == Some(ExportWaiverReason::Budget)) => [QuotaGuardrailState {
            monthly_spend_cents: state.monthly_spend_cents,
            approved_budget_increase: state.approved_budget_increase,
            dpa_signed: state.dpa_signed,
            region_aligned: state.region_aligned,
            waiver_reason: state.waiver_reason,
            export_enabled: true,
        }];
    }
    properties {
        invariant P_EXPORT_REQUIRES_GOVERNANCE |state| state.export_enabled == false || (state.dpa_signed && state.region_aligned);
        invariant P_EXPORT_REQUIRES_BUDGET_DISCIPLINE |state| state.export_enabled == false || (state.monthly_spend_cents < 200000 || state.approved_budget_increase);
    }
}

fn main() {
    run_registry_cli(valid_models![
        "access-review-scale" => AccessReviewScaleModel,
        "quota-guardrail-regression" => QuotaGuardrailRegressionModel,
    ]);
}
