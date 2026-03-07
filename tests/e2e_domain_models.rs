use valid::modeling::Finite;
#[path = "../examples/use_cases/authz.rs"]
mod authz;
#[path = "../examples/use_cases/entitlements.rs"]
mod entitlements;
#[path = "../examples/use_cases/fare.rs"]
mod fare;

use authz::{
    collect_authorization_coverage, evaluate_request, explain_request, find_newly_allowed_requests,
    AuthorizationDecision, AuthorizationRequest, Matcher, PolicyDomain, PolicyEffect, PolicySet,
    PolicyStatement, RequestContext,
};
use entitlements::{
    collect_entitlement_coverage, evaluate_entitlement,
    verify_free_plan_never_gets_enterprise_features, verify_member_never_gets_admin_api, ActorRole,
    EntitlementRequest, Feature, Plan,
};
use fare::{
    calculate_fare, collect_fare_coverage, explain_fare, verify_child_never_costs_more_than_adult,
    verify_day_pass_is_zero, verify_longer_distance_is_not_cheaper, FareRequest, RiderCategory,
    StationZone, TicketKind, TransferWindow,
};

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

#[test]
fn authz_explicit_deny_and_diff_are_visible() {
    let before = PolicySet { statements: vec![] };
    let after = PolicySet {
        statements: vec![
            PolicyStatement {
                id: "auditor-billing-read".to_string(),
                domain: PolicyDomain::Identity,
                effect: PolicyEffect::Allow,
                principal: Matcher::Exact(Principal::Auditor),
                action: Matcher::Exact(Action::Read),
                resource: Matcher::Exact(Resource::Billing),
                condition: None,
            },
            PolicyStatement {
                id: "deny-billing-write".to_string(),
                domain: PolicyDomain::Scp,
                effect: PolicyEffect::Deny,
                principal: Matcher::Any,
                action: Matcher::Exact(Action::Write),
                resource: Matcher::Exact(Resource::Billing),
                condition: None,
            },
        ],
    };

    let denied = evaluate_request(
        &after,
        &AuthorizationRequest {
            principal: Principal::Auditor,
            action: Action::Write,
            resource: Resource::Billing,
            context: RequestContext { mfa_present: false },
        },
    );
    assert_eq!(denied.decision, AuthorizationDecision::ExplicitDeny);

    let explanation = explain_request(
        &after,
        &AuthorizationRequest {
            principal: Principal::Auditor,
            action: Action::Write,
            resource: Resource::Billing,
            context: RequestContext { mfa_present: false },
        },
    );
    assert!(!explanation.repair_hints.is_empty());

    let diff_after = PolicySet {
        statements: vec![PolicyStatement {
            id: "auditor-billing-read".to_string(),
            domain: PolicyDomain::Identity,
            effect: PolicyEffect::Allow,
            principal: Matcher::Exact(Principal::Auditor),
            action: Matcher::Exact(Action::Read),
            resource: Matcher::Exact(Resource::Billing),
            condition: None,
        }],
    };
    let deltas = find_newly_allowed_requests(&before, &diff_after);
    assert!(!deltas.is_empty());

    let coverage = collect_authorization_coverage(&after);
    assert!(coverage.explicit_deny_count > 0);
}

#[test]
fn fare_invariants_hold() {
    assert!(verify_child_never_costs_more_than_adult().is_empty());
    assert!(verify_day_pass_is_zero().is_empty());
    assert!(verify_longer_distance_is_not_cheaper().is_empty());

    let decision = calculate_fare(FareRequest {
        origin: StationZone::Zone1,
        destination: StationZone::Zone3,
        rider: RiderCategory::Child,
        ticket: TicketKind::SingleRide,
        transfer: TransferWindow::Within90Minutes,
    });
    assert!(decision.total_yen > 0);
    assert!(explain_fare(FareRequest {
        origin: StationZone::Zone1,
        destination: StationZone::Zone3,
        rider: RiderCategory::Child,
        ticket: TicketKind::SingleRide,
        transfer: TransferWindow::Within90Minutes,
    })
    .contains("transfer_discount"));
    assert!(collect_fare_coverage().total_requests > 0);
}

#[test]
fn entitlement_invariants_hold() {
    assert!(verify_free_plan_never_gets_enterprise_features().is_empty());
    assert!(verify_member_never_gets_admin_api().is_empty());

    let allowed = evaluate_entitlement(EntitlementRequest {
        plan: Plan::Enterprise,
        feature: Feature::Sso,
        role: ActorRole::Admin,
    });
    assert!(allowed.allowed);

    let denied = evaluate_entitlement(EntitlementRequest {
        plan: Plan::Free,
        feature: Feature::Sso,
        role: ActorRole::Admin,
    });
    assert!(!denied.allowed);

    let coverage = collect_entitlement_coverage();
    assert!(coverage.allowed_count > 0);
    assert!(coverage.denied_count > 0);
}
