use valid::native::entitlements::{
    collect_entitlement_coverage, evaluate_entitlement,
    verify_free_plan_never_gets_enterprise_features, verify_member_never_gets_admin_api, ActorRole,
    EntitlementRequest, Feature, Plan,
};

fn main() {
    let request = EntitlementRequest {
        plan: Plan::Enterprise,
        feature: Feature::Sso,
        role: ActorRole::Admin,
    };
    let decision = evaluate_entitlement(request);
    let coverage = collect_entitlement_coverage();

    println!("allowed: {}", decision.allowed);
    println!("reasons: {:?}", decision.reasons);
    println!(
        "invariants: free_enterprise_features={} member_admin_api={}",
        verify_free_plan_never_gets_enterprise_features().is_empty(),
        verify_member_never_gets_admin_api().is_empty()
    );
    println!("coverage total_requests: {}", coverage.total_requests);
    println!("coverage reason_counts: {:?}", coverage.reason_counts);
}
