/*
テナント関係・マップ例

目的:
  - FiniteRelation と FiniteMap を使って、テナント所属とテナント別プランを自然に書く
  - SaaS の cross-tenant access を小さいモデルで検証する

含まれるモデル:
  - tenant-relation-safe
    membership と tenant plan の両方を見て export を許可する
  - tenant-relation-regression
    plan だけで export を許可してしまい、cross-tenant access が起きる

主な性質:
  - P_EXPORT_REQUIRES_MEMBERSHIP
  - P_EXPORT_REQUIRES_ENTERPRISE
  - P_NO_CROSS_TENANT_ACCESS

最初に試すコマンド:
  cargo valid --registry examples/tenant_relation_registry.rs inspect tenant-relation-safe
  cargo valid --registry examples/tenant_relation_registry.rs verify tenant-relation-regression --property=P_NO_CROSS_TENANT_ACCESS
*/
use valid::{
    map_contains_entry, map_put, registry::run_registry_cli, rel_contains, rel_insert,
    valid_actions, valid_model, valid_models, valid_state, FiniteMap, FiniteRelation, ValidEnum,
};

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidEnum)]
enum Member {
    Alice,
    Bob,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidEnum)]
enum Tenant {
    Alpha,
    Beta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidEnum)]
enum Plan {
    Free,
    Enterprise,
}

valid_state! {
    struct TenantRelationState {
        memberships: FiniteRelation<Member, Tenant> [relation],
        plans: FiniteMap<Tenant, Plan> [map],
        export_enabled: bool,
        cross_tenant_access: bool,
    }
}

valid_actions! {
    enum TenantRelationAction {
        AttachAliceAlpha => "ATTACH_ALICE_ALPHA" [reads = ["memberships"], writes = ["memberships"]],
        UpgradeAlphaEnterprise => "UPGRADE_ALPHA_ENTERPRISE" [reads = ["plans"], writes = ["plans"]],
        EnableAlphaExport => "ENABLE_ALPHA_EXPORT" [reads = ["memberships", "plans"], writes = ["export_enabled", "cross_tenant_access"]],
        EnableCrossTenantExport => "ENABLE_CROSS_TENANT_EXPORT" [reads = ["plans"], writes = ["export_enabled", "cross_tenant_access"]],
    }
}

valid_model! {
    model TenantRelationSafeModel<TenantRelationState, TenantRelationAction>;
    init [TenantRelationState {
        memberships: FiniteRelation::empty(),
        plans: map_put(FiniteMap::empty(), Tenant::Alpha, Plan::Free),
        export_enabled: false,
        cross_tenant_access: false,
    }];
    transitions {
        transition AttachAliceAlpha [tags = ["membership_path", "tenant_isolation_path"]]
        when |state| !rel_contains(state.memberships, Member::Alice, Tenant::Alpha)
        => [TenantRelationState {
            memberships: rel_insert(state.memberships, Member::Alice, Tenant::Alpha),
            plans: state.plans,
            export_enabled: state.export_enabled,
            cross_tenant_access: state.cross_tenant_access,
        }];
        transition UpgradeAlphaEnterprise [tags = ["entitlement_path", "allow_path"]]
        when |state| !map_contains_entry(state.plans, Tenant::Alpha, Plan::Enterprise)
        => [TenantRelationState {
            memberships: state.memberships,
            plans: map_put(state.plans, Tenant::Alpha, Plan::Enterprise),
            export_enabled: state.export_enabled,
            cross_tenant_access: state.cross_tenant_access,
        }];
        transition EnableAlphaExport [tags = ["allow_path", "membership_path", "tenant_isolation_path"]]
        when |state|
            rel_contains(state.memberships, Member::Alice, Tenant::Alpha)
            && map_contains_entry(state.plans, Tenant::Alpha, Plan::Enterprise)
        => [TenantRelationState {
            memberships: state.memberships,
            plans: state.plans,
            export_enabled: true,
            cross_tenant_access: false,
        }];
    }
    properties {
        invariant P_EXPORT_REQUIRES_MEMBERSHIP |state|
            state.export_enabled == false || rel_contains(state.memberships, Member::Alice, Tenant::Alpha);
        invariant P_EXPORT_REQUIRES_ENTERPRISE |state|
            state.export_enabled == false || map_contains_entry(state.plans, Tenant::Alpha, Plan::Enterprise);
        invariant P_NO_CROSS_TENANT_ACCESS |state|
            state.cross_tenant_access == false;
    }
}

valid_model! {
    model TenantRelationRegressionModel<TenantRelationState, TenantRelationAction>;
    init [TenantRelationState {
        memberships: FiniteRelation::empty(),
        plans: map_put(FiniteMap::empty(), Tenant::Beta, Plan::Enterprise),
        export_enabled: false,
        cross_tenant_access: false,
    }];
    transitions {
        transition EnableCrossTenantExport [tags = ["exception_path", "tenant_isolation_path"]]
        when |state| map_contains_entry(state.plans, Tenant::Beta, Plan::Enterprise)
        => [TenantRelationState {
            memberships: state.memberships,
            plans: state.plans,
            export_enabled: true,
            cross_tenant_access: true,
        }];
    }
    properties {
        invariant P_EXPORT_REQUIRES_MEMBERSHIP |state|
            state.export_enabled == false || rel_contains(state.memberships, Member::Alice, Tenant::Alpha);
        invariant P_NO_CROSS_TENANT_ACCESS |state|
            state.cross_tenant_access == false;
    }
}

fn main() {
    run_registry_cli(valid_models![
        "tenant-relation-safe" => TenantRelationSafeModel,
        "tenant-relation-regression" => TenantRelationRegressionModel,
    ]);
}
