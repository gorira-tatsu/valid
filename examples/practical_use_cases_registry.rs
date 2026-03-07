use valid::{
    registry::run_registry_cli, valid_model, valid_models, ValidAction, ValidState,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash, ValidState)]
struct DeploymentState {
    #[valid(range = "0..=2")]
    approval_count: u8,
    qa_passed: bool,
    freeze_window: bool,
    incident_open: bool,
    prod_deployed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidAction)]
enum DeploymentAction {
    #[valid(action_id = "APPROVE", reads = ["approval_count"], writes = ["approval_count"])]
    Approve,
    #[valid(action_id = "PASS_QA", reads = ["qa_passed"], writes = ["qa_passed"])]
    PassQa,
    #[valid(action_id = "OPEN_FREEZE", reads = ["freeze_window"], writes = ["freeze_window"])]
    OpenFreeze,
    #[valid(action_id = "CLEAR_FREEZE", reads = ["freeze_window"], writes = ["freeze_window"])]
    ClearFreeze,
    #[valid(action_id = "OPEN_INCIDENT", reads = ["incident_open"], writes = ["incident_open"])]
    OpenIncident,
    #[valid(action_id = "RESOLVE_INCIDENT", reads = ["incident_open"], writes = ["incident_open"])]
    ResolveIncident,
    #[valid(
        action_id = "DEPLOY_PROD",
        reads = ["approval_count", "qa_passed", "freeze_window", "incident_open"],
        writes = ["prod_deployed"]
    )]
    DeployProd,
}

valid_model! {
    model ProdDeploySafeModel<DeploymentState, DeploymentAction>;
    init [DeploymentState {
        approval_count: 0,
        qa_passed: false,
        freeze_window: false,
        incident_open: false,
        prod_deployed: false,
    }];
    transitions {
        transition Approve [tags = ["approval_path"]] when |state| state.approval_count <= 1 => [DeploymentState {
            approval_count: state.approval_count + 1,
            qa_passed: state.qa_passed,
            freeze_window: state.freeze_window,
            incident_open: state.incident_open,
            prod_deployed: state.prod_deployed,
        }];
        transition PassQa [tags = ["approval_path"]] when |state| state.qa_passed == false => [DeploymentState {
            approval_count: state.approval_count,
            qa_passed: true,
            freeze_window: state.freeze_window,
            incident_open: state.incident_open,
            prod_deployed: state.prod_deployed,
        }];
        transition OpenFreeze [tags = ["deny_path"]] when |state| state.freeze_window == false => [DeploymentState {
            approval_count: state.approval_count,
            qa_passed: state.qa_passed,
            freeze_window: true,
            incident_open: state.incident_open,
            prod_deployed: state.prod_deployed,
        }];
        transition ClearFreeze [tags = ["recovery_path"]] when |state| state.freeze_window => [DeploymentState {
            approval_count: state.approval_count,
            qa_passed: state.qa_passed,
            freeze_window: false,
            incident_open: state.incident_open,
            prod_deployed: state.prod_deployed,
        }];
        transition OpenIncident [tags = ["incident_path", "deny_path"]] when |state| state.incident_open == false => [DeploymentState {
            approval_count: state.approval_count,
            qa_passed: state.qa_passed,
            freeze_window: state.freeze_window,
            incident_open: true,
            prod_deployed: state.prod_deployed,
        }];
        transition ResolveIncident [tags = ["incident_path", "recovery_path"]] when |state| state.incident_open => [DeploymentState {
            approval_count: state.approval_count,
            qa_passed: state.qa_passed,
            freeze_window: state.freeze_window,
            incident_open: false,
            prod_deployed: state.prod_deployed,
        }];
        transition DeployProd [tags = ["allow_path", "approval_path"]] when |state| state.approval_count == 2 && state.qa_passed && state.freeze_window == false && state.incident_open == false => [DeploymentState {
            approval_count: state.approval_count,
            qa_passed: state.qa_passed,
            freeze_window: state.freeze_window,
            incident_open: state.incident_open,
            prod_deployed: true,
        }];
    }
    properties {
        invariant P_DEPLOY_REQUIRES_APPROVALS |state| state.prod_deployed == false || state.approval_count == 2;
        invariant P_DEPLOY_REQUIRES_QA |state| state.prod_deployed == false || state.qa_passed;
        invariant P_DEPLOY_BLOCKED_BY_FREEZE_OR_INCIDENT |state| state.prod_deployed == false || (state.freeze_window == false && state.incident_open == false);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, ValidState)]
struct BreakglassState {
    incident_open: bool,
    manager_approved: bool,
    exception_enabled: bool,
    access_granted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidAction)]
enum BreakglassAction {
    #[valid(action_id = "OPEN_INCIDENT", reads = ["incident_open"], writes = ["incident_open"])]
    OpenIncident,
    #[valid(
        action_id = "APPROVE_ACCESS",
        reads = ["incident_open", "manager_approved"],
        writes = ["manager_approved"]
    )]
    ApproveAccess,
    #[valid(
        action_id = "ENABLE_EXCEPTION",
        reads = ["exception_enabled"],
        writes = ["exception_enabled"]
    )]
    EnableException,
    #[valid(
        action_id = "GRANT_ACCESS",
        reads = ["incident_open", "manager_approved"],
        writes = ["access_granted"]
    )]
    GrantAccess,
    #[valid(
        action_id = "FORCE_GRANT",
        reads = ["exception_enabled"],
        writes = ["access_granted"]
    )]
    ForceGrant,
}

valid_model! {
    model BreakglassAccessRegressionModel<BreakglassState, BreakglassAction>;
    init [BreakglassState {
        incident_open: false,
        manager_approved: false,
        exception_enabled: false,
        access_granted: false,
    }];
    transitions {
        transition OpenIncident [tags = ["incident_path"]] when |state| state.incident_open == false => [BreakglassState {
            incident_open: true,
            manager_approved: state.manager_approved,
            exception_enabled: state.exception_enabled,
            access_granted: state.access_granted,
        }];
        transition ApproveAccess [tags = ["approval_path"]] when |state| state.incident_open && state.manager_approved == false => [BreakglassState {
            incident_open: state.incident_open,
            manager_approved: true,
            exception_enabled: state.exception_enabled,
            access_granted: state.access_granted,
        }];
        transition EnableException [tags = ["exception_path"]] when |state| state.exception_enabled == false => [BreakglassState {
            incident_open: state.incident_open,
            manager_approved: state.manager_approved,
            exception_enabled: true,
            access_granted: state.access_granted,
        }];
        transition GrantAccess [tags = ["allow_path", "approval_path"]] when |state| state.incident_open && state.manager_approved => [BreakglassState {
            incident_open: state.incident_open,
            manager_approved: state.manager_approved,
            exception_enabled: state.exception_enabled,
            access_granted: true,
        }];
        transition ForceGrant [tags = ["exception_path", "deny_path"]] when |state| state.exception_enabled => [BreakglassState {
            incident_open: state.incident_open,
            manager_approved: state.manager_approved,
            exception_enabled: state.exception_enabled,
            access_granted: true,
        }];
    }
    properties {
        invariant P_ACCESS_REQUIRES_INCIDENT |state| state.access_granted == false || state.incident_open;
        invariant P_ACCESS_REQUIRES_APPROVAL |state| state.access_granted == false || state.manager_approved;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, ValidState)]
struct RefundState {
    #[valid(range = "0..=3")]
    risk_score: u8,
    fraud_cleared: bool,
    manager_approved: bool,
    refund_issued: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidAction)]
enum RefundAction {
    #[valid(action_id = "ESCALATE_RISK", reads = ["risk_score"], writes = ["risk_score"])]
    EscalateRisk,
    #[valid(action_id = "CLEAR_FRAUD", reads = ["fraud_cleared"], writes = ["fraud_cleared"])]
    ClearFraud,
    #[valid(
        action_id = "APPROVE_MANAGER",
        reads = ["manager_approved"],
        writes = ["manager_approved"]
    )]
    ApproveManager,
    #[valid(
        action_id = "ISSUE_REFUND",
        reads = ["risk_score", "fraud_cleared", "manager_approved"],
        writes = ["refund_issued"]
    )]
    IssueRefund,
}

valid_model! {
    model RefundControlModel<RefundState, RefundAction>;
    init [RefundState {
        risk_score: 0,
        fraud_cleared: false,
        manager_approved: false,
        refund_issued: false,
    }];
    transitions {
        transition EscalateRisk [tags = ["risk_path"]] when |state| state.risk_score <= 2 => [RefundState {
            risk_score: state.risk_score + 1,
            fraud_cleared: state.fraud_cleared,
            manager_approved: state.manager_approved,
            refund_issued: state.refund_issued,
        }];
        transition ClearFraud [tags = ["risk_path"]] when |state| state.fraud_cleared == false => [RefundState {
            risk_score: state.risk_score,
            fraud_cleared: true,
            manager_approved: state.manager_approved,
            refund_issued: state.refund_issued,
        }];
        transition ApproveManager [tags = ["approval_path", "finance_path"]] when |state| state.manager_approved == false => [RefundState {
            risk_score: state.risk_score,
            fraud_cleared: state.fraud_cleared,
            manager_approved: true,
            refund_issued: state.refund_issued,
        }];
        transition IssueRefund [tags = ["allow_path", "finance_path"]] when |state| state.fraud_cleared && (state.risk_score == 0 || state.manager_approved) => [RefundState {
            risk_score: state.risk_score,
            fraud_cleared: state.fraud_cleared,
            manager_approved: state.manager_approved,
            refund_issued: true,
        }];
    }
    properties {
        invariant P_REFUND_REQUIRES_FRAUD_CLEAR |state| state.refund_issued == false || state.fraud_cleared;
        invariant P_HIGH_RISK_REFUND_REQUIRES_APPROVAL |state| state.refund_issued == false || state.risk_score == 0 || state.manager_approved;
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, ValidState)]
struct ExportState {
    contract_active: bool,
    dpa_signed: bool,
    region_aligned: bool,
    export_started: bool,
    export_completed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidAction)]
enum ExportAction {
    #[valid(action_id = "ACTIVATE_CONTRACT", reads = ["contract_active"], writes = ["contract_active"])]
    ActivateContract,
    #[valid(action_id = "SIGN_DPA", reads = ["dpa_signed"], writes = ["dpa_signed"])]
    SignDpa,
    #[valid(action_id = "ALIGN_REGION", reads = ["region_aligned"], writes = ["region_aligned"])]
    AlignRegion,
    #[valid(
        action_id = "START_EXPORT",
        reads = ["contract_active", "dpa_signed", "region_aligned"],
        writes = ["export_started"]
    )]
    StartExport,
    #[valid(action_id = "COMPLETE_EXPORT", reads = ["export_started"], writes = ["export_completed"])]
    CompleteExport,
}

valid_model! {
    model DataExportControlModel<ExportState, ExportAction>;
    init [ExportState {
        contract_active: false,
        dpa_signed: false,
        region_aligned: false,
        export_started: false,
        export_completed: false,
    }];
    transitions {
        transition ActivateContract [tags = ["compliance_path"]] when |state| state.contract_active == false => [ExportState {
            contract_active: true,
            dpa_signed: state.dpa_signed,
            region_aligned: state.region_aligned,
            export_started: state.export_started,
            export_completed: state.export_completed,
        }];
        transition SignDpa [tags = ["compliance_path"]] when |state| state.dpa_signed == false => [ExportState {
            contract_active: state.contract_active,
            dpa_signed: true,
            region_aligned: state.region_aligned,
            export_started: state.export_started,
            export_completed: state.export_completed,
        }];
        transition AlignRegion [tags = ["boundary_path", "compliance_path"]] when |state| state.region_aligned == false => [ExportState {
            contract_active: state.contract_active,
            dpa_signed: state.dpa_signed,
            region_aligned: true,
            export_started: state.export_started,
            export_completed: state.export_completed,
        }];
        transition StartExport [tags = ["allow_path", "compliance_path"]] when |state| state.contract_active && state.dpa_signed && state.region_aligned => [ExportState {
            contract_active: state.contract_active,
            dpa_signed: state.dpa_signed,
            region_aligned: state.region_aligned,
            export_started: true,
            export_completed: state.export_completed,
        }];
        transition CompleteExport [tags = ["allow_path", "compliance_path"]] when |state| state.export_started => [ExportState {
            contract_active: state.contract_active,
            dpa_signed: state.dpa_signed,
            region_aligned: state.region_aligned,
            export_started: state.export_started,
            export_completed: true,
        }];
    }
    properties {
        invariant P_EXPORT_REQUIRES_CONTRACT_DPA_AND_REGION |state| state.export_started == false || (state.contract_active && state.dpa_signed && state.region_aligned);
        invariant P_COMPLETE_REQUIRES_START |state| state.export_completed == false || state.export_started;
    }
}

fn main() {
    run_registry_cli(valid_models![
        "prod-deploy-safe" => ProdDeploySafeModel,
        "breakglass-access-regression" => BreakglassAccessRegressionModel,
        "refund-control" => RefundControlModel,
        "data-export-control" => DataExportControlModel,
    ]);
}
