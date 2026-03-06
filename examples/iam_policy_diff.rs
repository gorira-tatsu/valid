#[path = "support/authz.rs"]
mod authz;

use authz::{
    find_newly_allowed_requests, Matcher, PolicyDomain, PolicyEffect, PolicySet, PolicyStatement,
};
use valid::modeling::Finite;

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

fn main() {
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
    println!("newly allowed requests: {}", deltas.len());
    for delta in deltas {
        println!(
            "- principal={:?} action={:?} resource={:?} mfa={} before={:?} after={:?}",
            delta.request.principal,
            delta.request.action,
            delta.request.resource,
            delta.request.context.mfa_present,
            delta.before,
            delta.after
        );
    }
}
