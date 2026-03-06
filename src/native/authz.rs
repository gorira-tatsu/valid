//! IAM-like authorization semantics for mission-critical policy verification.

use std::{collections::BTreeMap, fmt::Debug, hash::Hash};

use super::Finite;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyEffect {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDomain {
    Identity,
    Resource,
    Boundary,
    Session,
    Scp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Matcher<T> {
    Any,
    Exact(T),
}

impl<T: PartialEq> Matcher<T> {
    fn matches(&self, value: &T) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(expected) => expected == value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct RequestContext {
    pub mfa_present: bool,
}

impl Finite for RequestContext {
    fn all() -> Vec<Self> {
        vec![
            Self { mfa_present: false },
            Self { mfa_present: true },
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyCondition {
    pub require_mfa: bool,
}

impl PolicyCondition {
    fn matches(&self, context: &RequestContext) -> bool {
        !self.require_mfa || context.mfa_present
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyStatement<P, A, R> {
    pub id: String,
    pub domain: PolicyDomain,
    pub effect: PolicyEffect,
    pub principal: Matcher<P>,
    pub action: Matcher<A>,
    pub resource: Matcher<R>,
    pub condition: Option<PolicyCondition>,
}

impl<P: PartialEq, A: PartialEq, R: PartialEq> PolicyStatement<P, A, R> {
    fn applies_to(&self, request: &AuthorizationRequest<P, A, R>) -> bool {
        self.principal.matches(&request.principal)
            && self.action.matches(&request.action)
            && self.resource.matches(&request.resource)
            && self
                .condition
                .as_ref()
                .map(|condition| condition.matches(&request.context))
                .unwrap_or(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PolicySet<P, A, R> {
    pub statements: Vec<PolicyStatement<P, A, R>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AuthorizationRequest<P, A, R> {
    pub principal: P,
    pub action: A,
    pub resource: R,
    pub context: RequestContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationDecision {
    Allow,
    ExplicitDeny,
    ImplicitDeny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionTrace {
    pub decision: AuthorizationDecision,
    pub matched_policy_ids: Vec<String>,
    pub allowing_policy_ids: Vec<String>,
    pub denying_policy_ids: Vec<String>,
    pub domain_allows: BTreeMap<String, bool>,
}

pub fn evaluate_request<P, A, R>(
    policies: &PolicySet<P, A, R>,
    request: &AuthorizationRequest<P, A, R>,
) -> DecisionTrace
where
    P: Clone + PartialEq,
    A: Clone + PartialEq,
    R: Clone + PartialEq,
{
    let applicable = policies
        .statements
        .iter()
        .filter(|statement| statement.applies_to(request))
        .collect::<Vec<_>>();

    let matched_policy_ids = applicable
        .iter()
        .map(|statement| statement.id.clone())
        .collect::<Vec<_>>();
    let denying_policy_ids = applicable
        .iter()
        .filter(|statement| statement.effect == PolicyEffect::Deny)
        .map(|statement| statement.id.clone())
        .collect::<Vec<_>>();
    let allowing_policy_ids = applicable
        .iter()
        .filter(|statement| statement.effect == PolicyEffect::Allow)
        .map(|statement| statement.id.clone())
        .collect::<Vec<_>>();

    if !denying_policy_ids.is_empty() {
        return DecisionTrace {
            decision: AuthorizationDecision::ExplicitDeny,
            matched_policy_ids,
            allowing_policy_ids,
            denying_policy_ids,
            domain_allows: BTreeMap::new(),
        };
    }

    let identity_or_resource_allow = applicable.iter().any(|statement| {
        statement.effect == PolicyEffect::Allow
            && matches!(
                statement.domain,
                PolicyDomain::Identity | PolicyDomain::Resource
            )
    });

    let boundary_allow = domain_allows(&policies.statements, &applicable, PolicyDomain::Boundary);
    let session_allow = domain_allows(&policies.statements, &applicable, PolicyDomain::Session);
    let scp_allow = domain_allows(&policies.statements, &applicable, PolicyDomain::Scp);

    let mut domain_map = BTreeMap::new();
    domain_map.insert("identity_or_resource".to_string(), identity_or_resource_allow);
    domain_map.insert("boundary".to_string(), boundary_allow);
    domain_map.insert("session".to_string(), session_allow);
    domain_map.insert("scp".to_string(), scp_allow);

    let decision = if identity_or_resource_allow && boundary_allow && session_allow && scp_allow {
        AuthorizationDecision::Allow
    } else {
        AuthorizationDecision::ImplicitDeny
    };

    DecisionTrace {
        decision,
        matched_policy_ids,
        allowing_policy_ids,
        denying_policy_ids,
        domain_allows: domain_map,
    }
}

fn domain_allows<P, A, R>(
    all_statements: &[PolicyStatement<P, A, R>],
    applicable: &[&PolicyStatement<P, A, R>],
    domain: PolicyDomain,
) -> bool {
    let domain_defined = all_statements.iter().any(|statement| statement.domain == domain);
    let domain_policies = applicable
        .iter()
        .copied()
        .filter(|statement| statement.domain == domain)
        .collect::<Vec<_>>();
    if !domain_defined {
        true
    } else {
        domain_policies
            .iter()
            .any(|statement| statement.effect == PolicyEffect::Allow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationViolation<P, A, R> {
    pub request: AuthorizationRequest<P, A, R>,
    pub trace: DecisionTrace,
    pub message: String,
}

pub fn verify_never_allows<P, A, R, F>(
    policies: &PolicySet<P, A, R>,
    predicate: F,
) -> Vec<AuthorizationViolation<P, A, R>>
where
    P: Clone + Debug + Eq + Hash + Finite + Ord,
    A: Clone + Debug + Eq + Hash + Finite + Ord,
    R: Clone + Debug + Eq + Hash + Finite + Ord,
    F: Fn(&AuthorizationRequest<P, A, R>) -> bool,
{
    enumerate_requests::<P, A, R>()
        .into_iter()
        .filter_map(|request| {
            if !predicate(&request) {
                return None;
            }
            let trace = evaluate_request(policies, &request);
            if trace.decision == AuthorizationDecision::Allow {
                Some(AuthorizationViolation {
                    message: "request was allowed but should have been denied".to_string(),
                    request,
                    trace,
                })
            } else {
                None
            }
        })
        .collect()
}

pub fn enumerate_requests<P, A, R>() -> Vec<AuthorizationRequest<P, A, R>>
where
    P: Clone + Finite,
    A: Clone + Finite,
    R: Clone + Finite,
{
    let mut requests = Vec::new();
    for principal in P::all() {
        for action in A::all() {
            for resource in R::all() {
                for context in RequestContext::all() {
                    requests.push(AuthorizationRequest {
                        principal: principal.clone(),
                        action: action.clone(),
                        resource: resource.clone(),
                        context,
                    });
                }
            }
        }
    }
    requests
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_request, verify_never_allows, AuthorizationDecision, AuthorizationRequest,
        Matcher, PolicyCondition, PolicyDomain, PolicyEffect, PolicySet, PolicyStatement,
        RequestContext,
    };
    use crate::native::Finite;

    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    enum Principal {
        Alice,
        Bob,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    enum Action {
        Read,
        Write,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
    enum Resource {
        Billing,
        Logs,
    }

    impl Finite for Principal {
        fn all() -> Vec<Self> {
            vec![Self::Alice, Self::Bob]
        }
    }

    impl Finite for Action {
        fn all() -> Vec<Self> {
            vec![Self::Read, Self::Write]
        }
    }

    impl Finite for Resource {
        fn all() -> Vec<Self> {
            vec![Self::Billing, Self::Logs]
        }
    }

    fn request(principal: Principal, action: Action, resource: Resource) -> AuthorizationRequest<Principal, Action, Resource> {
        AuthorizationRequest {
            principal,
            action,
            resource,
            context: RequestContext::default(),
        }
    }

    #[test]
    fn explicit_deny_wins() {
        let policies = PolicySet {
            statements: vec![
                PolicyStatement {
                    id: "allow-read".to_string(),
                    domain: PolicyDomain::Identity,
                    effect: PolicyEffect::Allow,
                    principal: Matcher::Exact(Principal::Alice),
                    action: Matcher::Exact(Action::Read),
                    resource: Matcher::Exact(Resource::Billing),
                    condition: None,
                },
                PolicyStatement {
                    id: "deny-read".to_string(),
                    domain: PolicyDomain::Scp,
                    effect: PolicyEffect::Deny,
                    principal: Matcher::Exact(Principal::Alice),
                    action: Matcher::Exact(Action::Read),
                    resource: Matcher::Exact(Resource::Billing),
                    condition: None,
                },
            ],
        };

        let trace = evaluate_request(&policies, &request(Principal::Alice, Action::Read, Resource::Billing));
        assert_eq!(trace.decision, AuthorizationDecision::ExplicitDeny);
        assert_eq!(trace.denying_policy_ids, vec!["deny-read".to_string()]);
    }

    #[test]
    fn boundary_intersection_blocks_identity_allow() {
        let policies = PolicySet {
            statements: vec![
                PolicyStatement {
                    id: "identity-allow-write".to_string(),
                    domain: PolicyDomain::Identity,
                    effect: PolicyEffect::Allow,
                    principal: Matcher::Exact(Principal::Alice),
                    action: Matcher::Exact(Action::Write),
                    resource: Matcher::Exact(Resource::Billing),
                    condition: None,
                },
                PolicyStatement {
                    id: "boundary-read-only".to_string(),
                    domain: PolicyDomain::Boundary,
                    effect: PolicyEffect::Allow,
                    principal: Matcher::Exact(Principal::Alice),
                    action: Matcher::Exact(Action::Read),
                    resource: Matcher::Any,
                    condition: None,
                },
            ],
        };

        let trace = evaluate_request(&policies, &request(Principal::Alice, Action::Write, Resource::Billing));
        assert_eq!(trace.decision, AuthorizationDecision::ImplicitDeny);
        assert_eq!(trace.domain_allows.get("boundary"), Some(&false));
    }

    #[test]
    fn resource_policy_can_grant_when_identity_does_not() {
        let policies = PolicySet {
            statements: vec![PolicyStatement {
                id: "resource-allow-read".to_string(),
                domain: PolicyDomain::Resource,
                effect: PolicyEffect::Allow,
                principal: Matcher::Exact(Principal::Bob),
                action: Matcher::Exact(Action::Read),
                resource: Matcher::Exact(Resource::Logs),
                condition: None,
            }],
        };

        let trace = evaluate_request(&policies, &request(Principal::Bob, Action::Read, Resource::Logs));
        assert_eq!(trace.decision, AuthorizationDecision::Allow);
    }

    #[test]
    fn mfa_condition_is_enforced() {
        let policies = PolicySet {
            statements: vec![PolicyStatement {
                id: "allow-with-mfa".to_string(),
                domain: PolicyDomain::Identity,
                effect: PolicyEffect::Allow,
                principal: Matcher::Exact(Principal::Alice),
                action: Matcher::Exact(Action::Write),
                resource: Matcher::Exact(Resource::Logs),
                condition: Some(PolicyCondition { require_mfa: true }),
            }],
        };

        let without_mfa = evaluate_request(
            &policies,
            &AuthorizationRequest {
                principal: Principal::Alice,
                action: Action::Write,
                resource: Resource::Logs,
                context: RequestContext { mfa_present: false },
            },
        );
        assert_eq!(without_mfa.decision, AuthorizationDecision::ImplicitDeny);

        let with_mfa = evaluate_request(
            &policies,
            &AuthorizationRequest {
                principal: Principal::Alice,
                action: Action::Write,
                resource: Resource::Logs,
                context: RequestContext { mfa_present: true },
            },
        );
        assert_eq!(with_mfa.decision, AuthorizationDecision::Allow);
    }

    #[test]
    fn exhaustive_verification_can_prove_no_billing_write_for_bob() {
        let policies = PolicySet {
            statements: vec![
                PolicyStatement {
                    id: "alice-read-billing".to_string(),
                    domain: PolicyDomain::Identity,
                    effect: PolicyEffect::Allow,
                    principal: Matcher::Exact(Principal::Alice),
                    action: Matcher::Exact(Action::Read),
                    resource: Matcher::Exact(Resource::Billing),
                    condition: None,
                },
                PolicyStatement {
                    id: "bob-logs-read".to_string(),
                    domain: PolicyDomain::Identity,
                    effect: PolicyEffect::Allow,
                    principal: Matcher::Exact(Principal::Bob),
                    action: Matcher::Exact(Action::Read),
                    resource: Matcher::Exact(Resource::Logs),
                    condition: None,
                },
            ],
        };

        let violations = verify_never_allows(&policies, |request| {
            request.principal == Principal::Bob
                && request.action == Action::Write
                && request.resource == Resource::Billing
        });

        assert!(violations.is_empty());
    }
}
