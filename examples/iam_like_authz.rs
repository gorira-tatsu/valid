use valid::native::{
    authz::{
        collect_authorization_coverage, evaluate_request, explain_request, AuthorizationRequest,
        Matcher, PolicyDomain, PolicyEffect, PolicySet, PolicyStatement, RequestContext,
    },
    Finite,
};

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

fn main() {
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
    println!("decision: {:?}", trace.decision);
    println!("matched policies: {:?}", trace.matched_policy_ids);
    println!("denying policies: {:?}", trace.denying_policy_ids);
    println!("summary: {}", explanation.summary);
    println!("repair hints: {:?}", explanation.repair_hints);
    println!("coverage allow_count: {}", coverage.allow_count);
    println!("coverage explicit_deny_count: {}", coverage.explicit_deny_count);
}
