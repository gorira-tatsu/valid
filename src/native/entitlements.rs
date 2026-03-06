//! Rust-native SaaS entitlement verification examples.

use std::collections::BTreeMap;

use super::Finite;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Plan {
    Free,
    Pro,
    Enterprise,
}

impl Finite for Plan {
    fn all() -> Vec<Self> {
        vec![Self::Free, Self::Pro, Self::Enterprise]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Feature {
    ExportCsv,
    AuditLog,
    Sso,
    AdminApi,
}

impl Finite for Feature {
    fn all() -> Vec<Self> {
        vec![Self::ExportCsv, Self::AuditLog, Self::Sso, Self::AdminApi]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActorRole {
    Member,
    Admin,
    BillingAdmin,
}

impl Finite for ActorRole {
    fn all() -> Vec<Self> {
        vec![Self::Member, Self::Admin, Self::BillingAdmin]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntitlementRequest {
    pub plan: Plan,
    pub feature: Feature,
    pub role: ActorRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitlementDecision {
    pub allowed: bool,
    pub reasons: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitlementCoverageReport {
    pub total_requests: usize,
    pub allowed_count: usize,
    pub denied_count: usize,
    pub reason_counts: BTreeMap<&'static str, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitlementViolation {
    pub message: String,
    pub request: EntitlementRequest,
    pub decision: EntitlementDecision,
}

pub fn evaluate_entitlement(request: EntitlementRequest) -> EntitlementDecision {
    let mut reasons = Vec::new();

    let plan_allows = match request.feature {
        Feature::ExportCsv => !matches!(request.plan, Plan::Free),
        Feature::AuditLog => matches!(request.plan, Plan::Enterprise),
        Feature::Sso => matches!(request.plan, Plan::Enterprise),
        Feature::AdminApi => matches!(request.plan, Plan::Pro | Plan::Enterprise),
    };
    if plan_allows {
        reasons.push("plan_allows_feature");
    } else {
        reasons.push("plan_blocks_feature");
    }

    let role_allows = match request.feature {
        Feature::ExportCsv => matches!(request.role, ActorRole::Admin | ActorRole::BillingAdmin),
        Feature::AuditLog | Feature::Sso | Feature::AdminApi => {
            matches!(request.role, ActorRole::Admin)
        }
    };
    if role_allows {
        reasons.push("role_allows_feature");
    } else {
        reasons.push("role_blocks_feature");
    }

    EntitlementDecision {
        allowed: plan_allows && role_allows,
        reasons,
    }
}

pub fn enumerate_requests() -> Vec<EntitlementRequest> {
    let mut requests = Vec::new();
    for plan in Plan::all() {
        for feature in Feature::all() {
            for role in ActorRole::all() {
                requests.push(EntitlementRequest {
                    plan,
                    feature,
                    role,
                });
            }
        }
    }
    requests
}

pub fn collect_entitlement_coverage() -> EntitlementCoverageReport {
    let requests = enumerate_requests();
    let mut allowed_count = 0usize;
    let mut denied_count = 0usize;
    let mut reason_counts = BTreeMap::new();

    for request in requests.iter().copied() {
        let decision = evaluate_entitlement(request);
        if decision.allowed {
            allowed_count += 1;
        } else {
            denied_count += 1;
        }
        for reason in decision.reasons {
            *reason_counts.entry(reason).or_insert(0) += 1;
        }
    }

    EntitlementCoverageReport {
        total_requests: requests.len(),
        allowed_count,
        denied_count,
        reason_counts,
    }
}

pub fn verify_free_plan_never_gets_enterprise_features() -> Vec<EntitlementViolation> {
    enumerate_requests()
        .into_iter()
        .filter(|request| matches!(request.plan, Plan::Free))
        .filter(|request| matches!(request.feature, Feature::AuditLog | Feature::Sso))
        .filter_map(|request| {
            let decision = evaluate_entitlement(request);
            if decision.allowed {
                Some(EntitlementViolation {
                    message: "free plan unexpectedly gained enterprise-only feature".to_string(),
                    request,
                    decision,
                })
            } else {
                None
            }
        })
        .collect()
}

pub fn verify_member_never_gets_admin_api() -> Vec<EntitlementViolation> {
    enumerate_requests()
        .into_iter()
        .filter(|request| {
            matches!(request.role, ActorRole::Member)
                && matches!(request.feature, Feature::AdminApi)
        })
        .filter_map(|request| {
            let decision = evaluate_entitlement(request);
            if decision.allowed {
                Some(EntitlementViolation {
                    message: "member unexpectedly gained admin api access".to_string(),
                    request,
                    decision,
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        collect_entitlement_coverage, evaluate_entitlement,
        verify_free_plan_never_gets_enterprise_features, verify_member_never_gets_admin_api,
        ActorRole, EntitlementRequest, Feature, Plan,
    };

    #[test]
    fn free_plan_never_gets_enterprise_features() {
        assert!(verify_free_plan_never_gets_enterprise_features().is_empty());
    }

    #[test]
    fn member_never_gets_admin_api() {
        assert!(verify_member_never_gets_admin_api().is_empty());
    }

    #[test]
    fn enterprise_admin_gets_sso() {
        let decision = evaluate_entitlement(EntitlementRequest {
            plan: Plan::Enterprise,
            feature: Feature::Sso,
            role: ActorRole::Admin,
        });
        assert!(decision.allowed);
    }

    #[test]
    fn free_plan_admin_still_cannot_use_sso() {
        let decision = evaluate_entitlement(EntitlementRequest {
            plan: Plan::Free,
            feature: Feature::Sso,
            role: ActorRole::Admin,
        });
        assert!(!decision.allowed);
    }

    #[test]
    fn coverage_reports_both_allow_and_deny_paths() {
        let coverage = collect_entitlement_coverage();
        assert!(coverage.allowed_count > 0);
        assert!(coverage.denied_count > 0);
        assert!(coverage.reason_counts.contains_key("plan_allows_feature"));
        assert!(coverage.reason_counts.contains_key("role_blocks_feature"));
    }
}
