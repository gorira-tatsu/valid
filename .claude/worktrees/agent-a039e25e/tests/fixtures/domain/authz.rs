#![allow(dead_code)]

//! IAM-like authorization semantics as an example Rust model.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    hash::Hash,
};

use valid::modeling::Finite;

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
        vec![Self { mfa_present: false }, Self { mfa_present: true }]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationExplanation {
    pub decision: AuthorizationDecision,
    pub summary: String,
    pub decisive_policy_ids: Vec<String>,
    pub failed_domains: Vec<String>,
    pub failed_conditions: Vec<String>,
    pub repair_hints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationCoverageReport {
    pub total_requests: usize,
    pub allow_count: usize,
    pub explicit_deny_count: usize,
    pub implicit_deny_count: usize,
    pub matched_policy_ids: BTreeSet<String>,
    pub unmatched_policy_ids: BTreeSet<String>,
    pub mfa_true_count: usize,
    pub mfa_false_count: usize,
    pub domain_block_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationDelta<P, A, R> {
    pub request: AuthorizationRequest<P, A, R>,
    pub before: AuthorizationDecision,
    pub after: AuthorizationDecision,
    pub summary: String,
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
    domain_map.insert(
        "identity_or_resource".to_string(),
        identity_or_resource_allow,
    );
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

pub fn explain_request<P, A, R>(
    policies: &PolicySet<P, A, R>,
    request: &AuthorizationRequest<P, A, R>,
) -> AuthorizationExplanation
where
    P: Clone + Debug + PartialEq,
    A: Clone + Debug + PartialEq,
    R: Clone + Debug + PartialEq,
{
    let trace = evaluate_request(policies, request);
    let failed_domains = trace
        .domain_allows
        .iter()
        .filter_map(|(domain, allowed)| if *allowed { None } else { Some(domain.clone()) })
        .collect::<Vec<_>>();
    let failed_conditions = policies
        .statements
        .iter()
        .filter(|statement| {
            statement.principal.matches(&request.principal)
                && statement.action.matches(&request.action)
                && statement.resource.matches(&request.resource)
                && statement
                    .condition
                    .as_ref()
                    .map(|condition| !condition.matches(&request.context))
                    .unwrap_or(false)
        })
        .map(|statement| statement.id.clone())
        .collect::<Vec<_>>();

    let (summary, decisive_policy_ids) = match trace.decision {
        AuthorizationDecision::ExplicitDeny => (
            "request denied because an explicit deny statement matched".to_string(),
            trace.denying_policy_ids.clone(),
        ),
        AuthorizationDecision::Allow => (
            "request allowed because an allow path exists and all limiting domains permit it"
                .to_string(),
            trace.allowing_policy_ids.clone(),
        ),
        AuthorizationDecision::ImplicitDeny => (
            "request denied because no complete allow path survived all limiting domains"
                .to_string(),
            trace.allowing_policy_ids.clone(),
        ),
    };

    let mut repair_hints = Vec::new();
    if matches!(trace.decision, AuthorizationDecision::ImplicitDeny) {
        if !failed_conditions.is_empty() {
            repair_hints.push(format!(
                "review unmet conditions in policies [{}]",
                failed_conditions.join(", ")
            ));
        }
        if !failed_domains.is_empty() {
            repair_hints.push(format!(
                "review limiting domains [{}] for missing allows or unexpected restrictions",
                failed_domains.join(", ")
            ));
        }
        if trace.allowing_policy_ids.is_empty() {
            repair_hints.push(
                "no identity/resource allow matched; verify principal, action, and resource scope"
                    .to_string(),
            );
        }
    }
    if matches!(trace.decision, AuthorizationDecision::ExplicitDeny) {
        repair_hints.push(format!(
            "inspect explicit deny statements [{}] before changing any allow policy",
            trace.denying_policy_ids.join(", ")
        ));
    }

    AuthorizationExplanation {
        decision: trace.decision,
        summary,
        decisive_policy_ids,
        failed_domains,
        failed_conditions,
        repair_hints,
    }
}

fn domain_allows<P, A, R>(
    all_statements: &[PolicyStatement<P, A, R>],
    applicable: &[&PolicyStatement<P, A, R>],
    domain: PolicyDomain,
) -> bool {
    let domain_defined = all_statements
        .iter()
        .any(|statement| statement.domain == domain);
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

pub fn collect_authorization_coverage<P, A, R>(
    policies: &PolicySet<P, A, R>,
) -> AuthorizationCoverageReport
where
    P: Clone + Debug + Eq + Hash + Finite + Ord + PartialEq,
    A: Clone + Debug + Eq + Hash + Finite + Ord + PartialEq,
    R: Clone + Debug + Eq + Hash + Finite + Ord + PartialEq,
{
    let requests = enumerate_requests::<P, A, R>();
    let mut allow_count = 0usize;
    let mut explicit_deny_count = 0usize;
    let mut implicit_deny_count = 0usize;
    let mut matched_policy_ids = BTreeSet::new();
    let mut domain_block_counts = BTreeMap::new();
    let mut mfa_true_count = 0usize;
    let mut mfa_false_count = 0usize;

    for request in &requests {
        if request.context.mfa_present {
            mfa_true_count += 1;
        } else {
            mfa_false_count += 1;
        }
        let trace = evaluate_request(policies, request);
        for policy_id in &trace.matched_policy_ids {
            matched_policy_ids.insert(policy_id.clone());
        }
        match trace.decision {
            AuthorizationDecision::Allow => allow_count += 1,
            AuthorizationDecision::ExplicitDeny => explicit_deny_count += 1,
            AuthorizationDecision::ImplicitDeny => implicit_deny_count += 1,
        }
        for (domain, allowed) in &trace.domain_allows {
            if !allowed {
                *domain_block_counts.entry(domain.clone()).or_insert(0) += 1;
            }
        }
    }

    let all_policy_ids = policies
        .statements
        .iter()
        .map(|statement| statement.id.clone())
        .collect::<BTreeSet<_>>();
    let unmatched_policy_ids = all_policy_ids
        .difference(&matched_policy_ids)
        .cloned()
        .collect::<BTreeSet<_>>();

    AuthorizationCoverageReport {
        total_requests: requests.len(),
        allow_count,
        explicit_deny_count,
        implicit_deny_count,
        matched_policy_ids,
        unmatched_policy_ids,
        mfa_true_count,
        mfa_false_count,
        domain_block_counts,
    }
}

pub fn find_newly_allowed_requests<P, A, R>(
    before: &PolicySet<P, A, R>,
    after: &PolicySet<P, A, R>,
) -> Vec<AuthorizationDelta<P, A, R>>
where
    P: Clone + Debug + Eq + Hash + Finite + Ord + PartialEq,
    A: Clone + Debug + Eq + Hash + Finite + Ord + PartialEq,
    R: Clone + Debug + Eq + Hash + Finite + Ord + PartialEq,
{
    enumerate_requests::<P, A, R>()
        .into_iter()
        .filter_map(|request| {
            let before_trace = evaluate_request(before, &request);
            let after_trace = evaluate_request(after, &request);
            if before_trace.decision != AuthorizationDecision::Allow
                && after_trace.decision == AuthorizationDecision::Allow
            {
                Some(AuthorizationDelta {
                    request,
                    before: before_trace.decision,
                    after: after_trace.decision,
                    summary: "request became newly allowed after the policy change".to_string(),
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
        collect_authorization_coverage, evaluate_request, explain_request,
        find_newly_allowed_requests, verify_never_allows, AuthorizationDecision,
        AuthorizationRequest, Matcher, PolicyCondition, PolicyDomain, PolicyEffect, PolicySet,
        PolicyStatement, RequestContext,
    };
    use valid::modeling::Finite;

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

    fn request(
        principal: Principal,
        action: Action,
        resource: Resource,
    ) -> AuthorizationRequest<Principal, Action, Resource> {
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

        let trace = evaluate_request(
            &policies,
            &request(Principal::Alice, Action::Read, Resource::Billing),
        );
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

        let trace = evaluate_request(
            &policies,
            &request(Principal::Alice, Action::Write, Resource::Billing),
        );
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

        let trace = evaluate_request(
            &policies,
            &request(Principal::Bob, Action::Read, Resource::Logs),
        );
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

    #[test]
    fn explanation_reports_failed_domains_and_conditions() {
        let policies = PolicySet {
            statements: vec![
                PolicyStatement {
                    id: "allow-write-with-mfa".to_string(),
                    domain: PolicyDomain::Identity,
                    effect: PolicyEffect::Allow,
                    principal: Matcher::Exact(Principal::Alice),
                    action: Matcher::Exact(Action::Write),
                    resource: Matcher::Exact(Resource::Logs),
                    condition: Some(PolicyCondition { require_mfa: true }),
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

        let explanation = explain_request(
            &policies,
            &AuthorizationRequest {
                principal: Principal::Alice,
                action: Action::Write,
                resource: Resource::Logs,
                context: RequestContext { mfa_present: false },
            },
        );

        assert_eq!(explanation.decision, AuthorizationDecision::ImplicitDeny);
        assert!(explanation.failed_domains.contains(&"boundary".to_string()));
        assert!(explanation
            .failed_conditions
            .contains(&"allow-write-with-mfa".to_string()));
        assert!(!explanation.repair_hints.is_empty());
    }

    #[test]
    fn authorization_coverage_reports_unmatched_policies_and_domain_blocks() {
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
                    id: "allow-logs-write-with-mfa".to_string(),
                    domain: PolicyDomain::Resource,
                    effect: PolicyEffect::Allow,
                    principal: Matcher::Exact(Principal::Alice),
                    action: Matcher::Exact(Action::Write),
                    resource: Matcher::Exact(Resource::Logs),
                    condition: Some(PolicyCondition { require_mfa: true }),
                },
                PolicyStatement {
                    id: "scp-deny-write".to_string(),
                    domain: PolicyDomain::Scp,
                    effect: PolicyEffect::Deny,
                    principal: Matcher::Any,
                    action: Matcher::Exact(Action::Write),
                    resource: Matcher::Exact(Resource::Billing),
                    condition: None,
                },
            ],
        };

        let report = collect_authorization_coverage(&policies);
        assert!(report.total_requests > 0);
        assert!(report.matched_policy_ids.contains("allow-read"));
        assert!(report.matched_policy_ids.contains("scp-deny-write"));
        assert!(report
            .matched_policy_ids
            .contains("allow-logs-write-with-mfa"));
        assert_eq!(report.unmatched_policy_ids.len(), 0);
        assert!(report.explicit_deny_count > 0);
    }

    #[test]
    fn policy_diff_finds_newly_allowed_requests() {
        let before = PolicySet { statements: vec![] };
        let after = PolicySet {
            statements: vec![PolicyStatement {
                id: "allow-alice-logs-read".to_string(),
                domain: PolicyDomain::Identity,
                effect: PolicyEffect::Allow,
                principal: Matcher::Exact(Principal::Alice),
                action: Matcher::Exact(Action::Read),
                resource: Matcher::Exact(Resource::Logs),
                condition: None,
            }],
        };

        let deltas = find_newly_allowed_requests(&before, &after);
        assert_eq!(deltas.len(), 2);
        assert!(deltas.iter().any(|delta| {
            delta.request.principal == Principal::Alice
                && delta.request.action == Action::Read
                && delta.request.resource == Resource::Logs
                && !delta.request.context.mfa_present
        }));
        assert!(deltas
            .iter()
            .all(|delta| delta.after == AuthorizationDecision::Allow));
    }
}
