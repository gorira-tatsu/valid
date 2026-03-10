/*
SaaS multi-tenant isolation example

Purpose:
  - model tenant isolation, shared-service access, and entitlements in one
    grouped-transition integration model
  - provide a compact example of SaaS / multi-tenant service guarantees
  - show the shared-state cross-domain style that works today without a full
    compose DSL

Concerns combined here:
  - isolation review
  - entitlement checks
  - shared service access

Included models:
  - `tenant-isolation-safe`
    Allows only reviewed shared search and blocks cross-tenant access.
  - `tenant-isolation-regression`
    Allows an unreviewed path and produces an isolation counterexample.

Key properties:
  - `P_SHARED_SEARCH_REQUIRES_REVIEW`
  - `P_EXPORT_API_REQUIRES_ENTERPRISE`
  - `P_NO_CROSS_TENANT_ACCESS`

First commands to try:
  cargo valid --registry examples/saas_multi_tenant_registry.rs inspect tenant-isolation-safe
  cargo valid --registry examples/saas_multi_tenant_registry.rs verify tenant-isolation-regression --property=P_NO_CROSS_TENANT_ACCESS
*/
use valid::{
    contains, insert, registry::run_registry_cli, valid_actions, valid_model, valid_models,
    valid_state, FiniteEnumSet, ValidEnum,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidEnum)]
enum TenantPlan {
    Pro,
    Enterprise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidEnum)]
enum Feature {
    ExportApi,
}

valid_state! {
    struct TenantState {
        tenant_count: u8 [range = "1..=8"],
        plan: TenantPlan [enum],
        entitlements: FiniteEnumSet<Feature> [set],
        isolation_reviewed: bool,
        shared_search_enabled: bool,
        cross_tenant_access: bool,
    }
}

valid_actions! {
    enum TenantAction {
        OnboardTenant => "ONBOARD_TENANT" [reads = ["tenant_count"], writes = ["tenant_count"]],
        UpgradeEnterprise => "UPGRADE_ENTERPRISE" [reads = ["plan"], writes = ["plan"]],
        ReviewIsolation => "REVIEW_ISOLATION" [reads = ["isolation_reviewed"], writes = ["isolation_reviewed"]],
        GrantExportApi => "GRANT_EXPORT_API" [reads = ["plan", "entitlements"], writes = ["entitlements"]],
        EnableSharedSearch => "ENABLE_SHARED_SEARCH" [reads = ["tenant_count", "plan", "isolation_reviewed", "shared_search_enabled"], writes = ["shared_search_enabled"]],
        ServeCrossTenantQuery => "SERVE_CROSS_TENANT_QUERY" [reads = ["shared_search_enabled", "isolation_reviewed", "cross_tenant_access"], writes = ["cross_tenant_access"]],
    }
}

valid_model! {
    /// Model: TenantIsolationSafeModel
    /// Summary: Shared-state integration model for tenant isolation, review state, and enterprise entitlement checks.
    /// In scope: the contract between isolation review, shared search enablement, and enterprise-only export entitlement.
    /// Out of scope: full onboarding, billing, and moderation workflows beyond the shared-state slice under review.
    /// Assumptions: local subdomain rules still own their internal transitions; this model restates only the shared fields needed for cross-domain checks.
    /// Critical properties: P_SHARED_SEARCH_REQUIRES_REVIEW, P_EXPORT_API_REQUIRES_ENTERPRISE, P_NO_CROSS_TENANT_ACCESS.
    model TenantIsolationSafeModel<TenantState, TenantAction>;
    init [TenantState {
        tenant_count: 1,
        plan: TenantPlan::Pro,
        entitlements: FiniteEnumSet::empty(),
        isolation_reviewed: false,
        shared_search_enabled: false,
        cross_tenant_access: false,
    }];
    transitions {
        transition OnboardTenant [tags = ["growth_path"]]
        when |state| state.tenant_count < 8
        => [TenantState {
            tenant_count: state.tenant_count + 1,
            plan: state.plan,
            entitlements: state.entitlements,
            isolation_reviewed: state.isolation_reviewed,
            shared_search_enabled: state.shared_search_enabled,
            cross_tenant_access: state.cross_tenant_access,
        }];
        transition UpgradeEnterprise [tags = ["entitlement_path", "allow_path"]]
        when |state| state.plan != TenantPlan::Enterprise
        => [TenantState {
            tenant_count: state.tenant_count,
            plan: TenantPlan::Enterprise,
            entitlements: state.entitlements,
            isolation_reviewed: state.isolation_reviewed,
            shared_search_enabled: state.shared_search_enabled,
            cross_tenant_access: state.cross_tenant_access,
        }];
        transition ReviewIsolation [tags = ["approval_path", "governance_path"]]
        when |state| state.isolation_reviewed == false
        => [TenantState {
            tenant_count: state.tenant_count,
            plan: state.plan,
            entitlements: state.entitlements,
            isolation_reviewed: true,
            shared_search_enabled: state.shared_search_enabled,
            cross_tenant_access: state.cross_tenant_access,
        }];
        transition GrantExportApi [tags = ["allow_path", "entitlement_path"]]
        when |state| state.plan == TenantPlan::Enterprise && !contains(state.entitlements, Feature::ExportApi)
        => [TenantState {
            tenant_count: state.tenant_count,
            plan: state.plan,
            entitlements: insert(state.entitlements, Feature::ExportApi),
            isolation_reviewed: state.isolation_reviewed,
            shared_search_enabled: state.shared_search_enabled,
            cross_tenant_access: state.cross_tenant_access,
        }];
        transition EnableSharedSearch [tags = ["allow_path", "approval_path", "tenant_isolation_path"]]
        when |state| state.shared_search_enabled == false && state.isolation_reviewed && state.tenant_count >= 2
        => [TenantState {
            tenant_count: state.tenant_count,
            plan: state.plan,
            entitlements: state.entitlements,
            isolation_reviewed: state.isolation_reviewed,
            shared_search_enabled: true,
            cross_tenant_access: state.cross_tenant_access,
        }];
        transition ServeCrossTenantQuery [tags = ["allow_path", "tenant_isolation_path"]]
        when |state| state.shared_search_enabled && state.isolation_reviewed
        => [TenantState {
            tenant_count: state.tenant_count,
            plan: state.plan,
            entitlements: state.entitlements,
            isolation_reviewed: state.isolation_reviewed,
            shared_search_enabled: state.shared_search_enabled,
            cross_tenant_access: false,
        }];
    }
    properties {
        invariant P_SHARED_SEARCH_REQUIRES_REVIEW |state|
            state.shared_search_enabled == false || state.isolation_reviewed;
        invariant P_EXPORT_API_REQUIRES_ENTERPRISE |state|
            !contains(state.entitlements, Feature::ExportApi) || state.plan == TenantPlan::Enterprise;
        invariant P_NO_CROSS_TENANT_ACCESS |state|
            state.cross_tenant_access == false;
    }
}

valid_model! {
    /// Model: TenantIsolationRegressionModel
    /// Summary: Regression-oriented integration model showing how an enterprise exception path can bypass isolation review.
    /// In scope: the cross-domain failure path between entitlement state, review state, and shared-search access.
    /// Out of scope: the full standalone workflows that would exist inside separate review or tenant-lifecycle models.
    /// Assumptions: the example stays intentionally thin so the shared-state contract remains reviewable before full compose syntax exists.
    /// Critical properties: P_SHARED_SEARCH_REQUIRES_REVIEW, P_NO_CROSS_TENANT_ACCESS.
    model TenantIsolationRegressionModel<TenantState, TenantAction>;
    init [TenantState {
        tenant_count: 2,
        plan: TenantPlan::Enterprise,
        entitlements: FiniteEnumSet::empty(),
        isolation_reviewed: false,
        shared_search_enabled: false,
        cross_tenant_access: false,
    }];
    transitions {
        transition EnableSharedSearch [tags = ["allow_path", "approval_path", "tenant_isolation_path"]]
        when |state| state.shared_search_enabled == false && state.isolation_reviewed
        => [TenantState {
            tenant_count: state.tenant_count,
            plan: state.plan,
            entitlements: state.entitlements,
            isolation_reviewed: state.isolation_reviewed,
            shared_search_enabled: true,
            cross_tenant_access: state.cross_tenant_access,
        }];

        transition EnableSharedSearch [tags = ["exception_path", "tenant_isolation_path"]]
        when |state| state.shared_search_enabled == false && state.plan == TenantPlan::Enterprise && state.isolation_reviewed == false
        => [TenantState {
            tenant_count: state.tenant_count,
            plan: state.plan,
            entitlements: state.entitlements,
            isolation_reviewed: state.isolation_reviewed,
            shared_search_enabled: true,
            cross_tenant_access: state.cross_tenant_access,
        }];

        transition ServeCrossTenantQuery [tags = ["deny_path", "exception_path", "tenant_isolation_path"]]
        when |state| state.shared_search_enabled && state.isolation_reviewed == false
        => [TenantState {
            tenant_count: state.tenant_count,
            plan: state.plan,
            entitlements: state.entitlements,
            isolation_reviewed: state.isolation_reviewed,
            shared_search_enabled: state.shared_search_enabled,
            cross_tenant_access: true,
        }];

        transition ServeCrossTenantQuery [tags = ["allow_path", "tenant_isolation_path"]]
        when |state| state.shared_search_enabled && state.isolation_reviewed
        => [TenantState {
            tenant_count: state.tenant_count,
            plan: state.plan,
            entitlements: state.entitlements,
            isolation_reviewed: state.isolation_reviewed,
            shared_search_enabled: state.shared_search_enabled,
            cross_tenant_access: false,
        }];
    }
    properties {
        invariant P_SHARED_SEARCH_REQUIRES_REVIEW |state|
            state.shared_search_enabled == false || state.isolation_reviewed;
        invariant P_NO_CROSS_TENANT_ACCESS |state|
            state.cross_tenant_access == false;
    }
}

fn main() {
    run_registry_cli(valid_models![
        "tenant-isolation-safe" => TenantIsolationSafeModel,
        "tenant-isolation-regression" => TenantIsolationRegressionModel,
    ]);
}
