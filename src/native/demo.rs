//! Built-in Rust-native demonstration reports for realistic verification targets.

use super::{
    authz::{
        collect_authorization_coverage, evaluate_request, explain_request,
        find_newly_allowed_requests, AuthorizationRequest, Matcher, PolicyDomain, PolicyEffect,
        PolicySet, PolicyStatement, RequestContext,
    },
    entitlements::{
        collect_entitlement_coverage, evaluate_entitlement,
        verify_free_plan_never_gets_enterprise_features, verify_member_never_gets_admin_api,
        ActorRole, EntitlementRequest, Feature, Plan,
    },
    fare::{
        calculate_fare, collect_fare_coverage, explain_fare,
        verify_child_never_costs_more_than_adult, verify_day_pass_is_zero,
        verify_longer_distance_is_not_cheaper, FareRequest, RiderCategory, StationZone, TicketKind,
        TransferWindow,
    },
    Finite,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeDemoKind {
    IamAuthz,
    IamPolicyDiff,
    TrainFare,
    SaasEntitlements,
}

impl NativeDemoKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "iam-authz" => Some(Self::IamAuthz),
            "iam-policy-diff" => Some(Self::IamPolicyDiff),
            "train-fare" => Some(Self::TrainFare),
            "saas-entitlements" => Some(Self::SaasEntitlements),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::IamAuthz => "iam-authz",
            Self::IamPolicyDiff => "iam-policy-diff",
            Self::TrainFare => "train-fare",
            Self::SaasEntitlements => "saas-entitlements",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDemoReport {
    pub schema_version: String,
    pub demo_id: String,
    pub summary: String,
    pub checks: Vec<String>,
    pub highlights: Vec<String>,
}

pub fn run_demo(kind: NativeDemoKind) -> NativeDemoReport {
    match kind {
        NativeDemoKind::IamAuthz => iam_authz_report(),
        NativeDemoKind::IamPolicyDiff => iam_policy_diff_report(),
        NativeDemoKind::TrainFare => train_fare_report(),
        NativeDemoKind::SaasEntitlements => saas_entitlements_report(),
    }
}

pub fn render_demo_text(report: &NativeDemoReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("demo_id: {}\n", report.demo_id));
    out.push_str(&format!("summary: {}\n", report.summary));
    if !report.checks.is_empty() {
        out.push_str("checks:\n");
        for check in &report.checks {
            out.push_str(&format!("- {}\n", check));
        }
    }
    if !report.highlights.is_empty() {
        out.push_str("highlights:\n");
        for line in &report.highlights {
            out.push_str(&format!("- {}\n", line));
        }
    }
    out
}

pub fn render_demo_json(report: &NativeDemoReport) -> String {
    format!(
        "{{\"schema_version\":\"{}\",\"demo_id\":\"{}\",\"summary\":\"{}\",\"checks\":[{}],\"highlights\":[{}]}}",
        report.schema_version,
        escape_json(&report.demo_id),
        escape_json(&report.summary),
        report
            .checks
            .iter()
            .map(|item| format!("\"{}\"", escape_json(item)))
            .collect::<Vec<_>>()
            .join(","),
        report
            .highlights
            .iter()
            .map(|item| format!("\"{}\"", escape_json(item)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn iam_authz_report() -> NativeDemoReport {
    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    enum Principal {
        PlatformAdmin,
        Analyst,
    }
    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    enum Action {
        Read,
        Write,
    }
    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    enum Resource {
        Billing,
        AuditLog,
    }
    impl Finite for Principal {
        fn all() -> Vec<Self> {
            vec![Self::PlatformAdmin, Self::Analyst]
        }
    }
    impl Finite for Action {
        fn all() -> Vec<Self> {
            vec![Self::Read, Self::Write]
        }
    }
    impl Finite for Resource {
        fn all() -> Vec<Self> {
            vec![Self::Billing, Self::AuditLog]
        }
    }

    let policies = PolicySet {
        statements: vec![
            PolicyStatement {
                id: "identity-allow-billing-read".to_string(),
                domain: PolicyDomain::Identity,
                effect: PolicyEffect::Allow,
                principal: Matcher::Exact(Principal::Analyst),
                action: Matcher::Exact(Action::Read),
                resource: Matcher::Exact(Resource::Billing),
                condition: None,
            },
            PolicyStatement {
                id: "scp-deny-billing-write".to_string(),
                domain: PolicyDomain::Scp,
                effect: PolicyEffect::Deny,
                principal: Matcher::Any,
                action: Matcher::Exact(Action::Write),
                resource: Matcher::Exact(Resource::Billing),
                condition: None,
            },
        ],
    };

    let request = AuthorizationRequest {
        principal: Principal::Analyst,
        action: Action::Write,
        resource: Resource::Billing,
        context: RequestContext { mfa_present: true },
    };
    let trace = evaluate_request(&policies, &request);
    let explanation = explain_request(&policies, &request);
    let coverage = collect_authorization_coverage(&policies);

    NativeDemoReport {
        schema_version: "1.0.0".to_string(),
        demo_id: NativeDemoKind::IamAuthz.as_str().to_string(),
        summary: format!(
            "decision={:?} summary={}",
            trace.decision, explanation.summary
        ),
        checks: vec![
            "explicit deny overrides allows".to_string(),
            "coverage counts all request combinations".to_string(),
        ],
        highlights: vec![
            format!("matched_policies={:?}", trace.matched_policy_ids),
            format!("repair_hints={:?}", explanation.repair_hints),
            format!("allow_count={}", coverage.allow_count),
            format!("explicit_deny_count={}", coverage.explicit_deny_count),
        ],
    }
}

fn iam_policy_diff_report() -> NativeDemoReport {
    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    enum Principal {
        Analyst,
        Auditor,
    }
    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    enum Action {
        Read,
        Write,
    }
    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    enum Resource {
        Billing,
        AuditLog,
    }
    impl Finite for Principal {
        fn all() -> Vec<Self> {
            vec![Self::Analyst, Self::Auditor]
        }
    }
    impl Finite for Action {
        fn all() -> Vec<Self> {
            vec![Self::Read, Self::Write]
        }
    }
    impl Finite for Resource {
        fn all() -> Vec<Self> {
            vec![Self::Billing, Self::AuditLog]
        }
    }

    let before = PolicySet {
        statements: vec![PolicyStatement {
            id: "analyst-read-audit".to_string(),
            domain: PolicyDomain::Identity,
            effect: PolicyEffect::Allow,
            principal: Matcher::Exact(Principal::Analyst),
            action: Matcher::Exact(Action::Read),
            resource: Matcher::Exact(Resource::AuditLog),
            condition: None,
        }],
    };
    let after = PolicySet {
        statements: vec![
            PolicyStatement {
                id: "analyst-read-audit".to_string(),
                domain: PolicyDomain::Identity,
                effect: PolicyEffect::Allow,
                principal: Matcher::Exact(Principal::Analyst),
                action: Matcher::Exact(Action::Read),
                resource: Matcher::Exact(Resource::AuditLog),
                condition: None,
            },
            PolicyStatement {
                id: "auditor-billing-read".to_string(),
                domain: PolicyDomain::Identity,
                effect: PolicyEffect::Allow,
                principal: Matcher::Exact(Principal::Auditor),
                action: Matcher::Exact(Action::Read),
                resource: Matcher::Exact(Resource::Billing),
                condition: None,
            },
        ],
    };
    let deltas = find_newly_allowed_requests(&before, &after);
    NativeDemoReport {
        schema_version: "1.0.0".to_string(),
        demo_id: NativeDemoKind::IamPolicyDiff.as_str().to_string(),
        summary: format!("newly_allowed_requests={}", deltas.len()),
        checks: vec!["policy changes should not widen access unexpectedly".to_string()],
        highlights: deltas
            .into_iter()
            .map(|delta| {
                format!(
                    "principal={:?} action={:?} resource={:?} mfa={} before={:?} after={:?}",
                    delta.request.principal,
                    delta.request.action,
                    delta.request.resource,
                    delta.request.context.mfa_present,
                    delta.before,
                    delta.after
                )
            })
            .collect(),
    }
}

fn train_fare_report() -> NativeDemoReport {
    let request = FareRequest {
        origin: StationZone::Zone1,
        destination: StationZone::Zone3,
        rider: RiderCategory::Child,
        ticket: TicketKind::SingleRide,
        transfer: TransferWindow::Within90Minutes,
    };
    let decision = calculate_fare(request);
    let coverage = collect_fare_coverage();
    NativeDemoReport {
        schema_version: "1.0.0".to_string(),
        demo_id: NativeDemoKind::TrainFare.as_str().to_string(),
        summary: explain_fare(request),
        checks: vec![
            format!(
                "child_never_costs_more_than_adult={}",
                verify_child_never_costs_more_than_adult().is_empty()
            ),
            format!("day_pass_is_zero={}", verify_day_pass_is_zero().is_empty()),
            format!(
                "longer_distance_is_not_cheaper={}",
                verify_longer_distance_is_not_cheaper().is_empty()
            ),
        ],
        highlights: vec![
            format!("fare_yen={}", decision.total_yen),
            format!("rules={:?}", decision.applied_rules),
            format!("coverage_total_requests={}", coverage.total_requests),
            format!("coverage_rule_counts={:?}", coverage.rule_counts),
        ],
    }
}

fn saas_entitlements_report() -> NativeDemoReport {
    let request = EntitlementRequest {
        plan: Plan::Enterprise,
        feature: Feature::Sso,
        role: ActorRole::Admin,
    };
    let decision = evaluate_entitlement(request);
    let coverage = collect_entitlement_coverage();
    NativeDemoReport {
        schema_version: "1.0.0".to_string(),
        demo_id: NativeDemoKind::SaasEntitlements.as_str().to_string(),
        summary: format!(
            "allowed={} reasons={:?}",
            decision.allowed, decision.reasons
        ),
        checks: vec![
            format!(
                "free_plan_never_gets_enterprise_features={}",
                verify_free_plan_never_gets_enterprise_features().is_empty()
            ),
            format!(
                "member_never_gets_admin_api={}",
                verify_member_never_gets_admin_api().is_empty()
            ),
        ],
        highlights: vec![
            format!("coverage_total_requests={}", coverage.total_requests),
            format!("allowed_count={}", coverage.allowed_count),
            format!("denied_count={}", coverage.denied_count),
            format!("reason_counts={:?}", coverage.reason_counts),
        ],
    }
}

fn escape_json(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::{render_demo_json, run_demo, NativeDemoKind};

    #[test]
    fn all_demo_reports_render() {
        for kind in [
            NativeDemoKind::IamAuthz,
            NativeDemoKind::IamPolicyDiff,
            NativeDemoKind::TrainFare,
            NativeDemoKind::SaasEntitlements,
        ] {
            let report = run_demo(kind);
            let json = render_demo_json(&report);
            assert!(json.contains("\"schema_version\""));
            assert!(!report.summary.is_empty());
        }
    }
}
