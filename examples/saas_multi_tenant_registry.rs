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
        on OnboardTenant {
            [tags = ["growth_path"]]
            when |state| state.tenant_count < 8
            => [TenantState {
                tenant_count: state.tenant_count + 1,
                plan: state.plan,
                entitlements: state.entitlements,
                isolation_reviewed: state.isolation_reviewed,
                shared_search_enabled: state.shared_search_enabled,
                cross_tenant_access: state.cross_tenant_access,
            }];
        }
        on UpgradeEnterprise {
            [tags = ["entitlement_path", "allow_path"]]
            when |state| state.plan != TenantPlan::Enterprise
            => [TenantState {
                tenant_count: state.tenant_count,
                plan: TenantPlan::Enterprise,
                entitlements: state.entitlements,
                isolation_reviewed: state.isolation_reviewed,
                shared_search_enabled: state.shared_search_enabled,
                cross_tenant_access: state.cross_tenant_access,
            }];
        }
        on ReviewIsolation {
            [tags = ["approval_path", "governance_path"]]
            when |state| state.isolation_reviewed == false
            => [TenantState {
                tenant_count: state.tenant_count,
                plan: state.plan,
                entitlements: state.entitlements,
                isolation_reviewed: true,
                shared_search_enabled: state.shared_search_enabled,
                cross_tenant_access: state.cross_tenant_access,
            }];
        }
        on GrantExportApi {
            [tags = ["allow_path", "entitlement_path"]]
            when |state| state.plan == TenantPlan::Enterprise && !contains(state.entitlements, Feature::ExportApi)
            => [TenantState {
                tenant_count: state.tenant_count,
                plan: state.plan,
                entitlements: insert(state.entitlements, Feature::ExportApi),
                isolation_reviewed: state.isolation_reviewed,
                shared_search_enabled: state.shared_search_enabled,
                cross_tenant_access: state.cross_tenant_access,
            }];
        }
        on EnableSharedSearch {
            [tags = ["allow_path", "approval_path", "tenant_isolation_path"]]
            when |state| state.shared_search_enabled == false && state.isolation_reviewed && state.tenant_count >= 2
            => [TenantState {
                tenant_count: state.tenant_count,
                plan: state.plan,
                entitlements: state.entitlements,
                isolation_reviewed: state.isolation_reviewed,
                shared_search_enabled: true,
                cross_tenant_access: state.cross_tenant_access,
            }];
        }
        on ServeCrossTenantQuery {
            [tags = ["allow_path", "tenant_isolation_path"]]
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
        on EnableSharedSearch {
            [tags = ["allow_path", "approval_path", "tenant_isolation_path"]]
            when |state| state.shared_search_enabled == false && state.isolation_reviewed
            => [TenantState {
                tenant_count: state.tenant_count,
                plan: state.plan,
                entitlements: state.entitlements,
                isolation_reviewed: state.isolation_reviewed,
                shared_search_enabled: true,
                cross_tenant_access: state.cross_tenant_access,
            }];

            [tags = ["exception_path", "tenant_isolation_path"]]
            when |state| state.shared_search_enabled == false && state.plan == TenantPlan::Enterprise && state.isolation_reviewed == false
            => [TenantState {
                tenant_count: state.tenant_count,
                plan: state.plan,
                entitlements: state.entitlements,
                isolation_reviewed: state.isolation_reviewed,
                shared_search_enabled: true,
                cross_tenant_access: state.cross_tenant_access,
            }];
        }
        on ServeCrossTenantQuery {
            [tags = ["deny_path", "exception_path", "tenant_isolation_path"]]
            when |state| state.shared_search_enabled && state.isolation_reviewed == false
            => [TenantState {
                tenant_count: state.tenant_count,
                plan: state.plan,
                entitlements: state.entitlements,
                isolation_reviewed: state.isolation_reviewed,
                shared_search_enabled: state.shared_search_enabled,
                cross_tenant_access: true,
            }];

            [tags = ["allow_path", "tenant_isolation_path"]]
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
