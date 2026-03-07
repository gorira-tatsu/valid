#![cfg(feature = "varisat-backend")]

use valid::{
    engine::{CheckOutcome, PropertySelection, RunPlan, RunStatus},
    map_contains_entry, map_contains_key, map_put, map_remove,
    modeling::{lower_machine_model, VerifiedMachine},
    rel_contains, rel_insert, rel_intersects, rel_remove,
    solver::{run_with_adapter, AdapterConfig},
    valid_actions, valid_model, valid_state, FiniteMap, FiniteRelation, ValidEnum,
};

fn run_model<M: VerifiedMachine>(
    property_id: &str,
    backend: AdapterConfig,
) -> (RunStatus, Vec<String>) {
    let model = lower_machine_model::<M>().expect("machine model should lower");
    let mut plan = RunPlan::default();
    plan.property_selection = PropertySelection::ExactlyOne(property_id.to_string());
    plan.search_bounds.max_depth = Some(4);
    plan.detect_deadlocks = false;
    let normalized = run_with_adapter(&model, &plan, &backend).expect("adapter should run");
    let actions = normalized
        .trace
        .as_ref()
        .map(|trace| {
            trace
                .steps
                .iter()
                .filter_map(|step| step.action_id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    match normalized.outcome {
        CheckOutcome::Completed(result) => (result.status, actions),
        CheckOutcome::Errored(error) => panic!("unexpected error: {:?}", error.diagnostics),
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidEnum)]
enum Member {
    Alice,
    Bob,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidEnum)]
enum Tenant {
    Alpha,
    Beta,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidEnum)]
enum Plan {
    Free,
    Enterprise,
}

valid_state! {
    struct RelationMapState {
        memberships: FiniteRelation<Member, Tenant> [relation],
        pending: FiniteRelation<Member, Tenant> [relation],
        plans: FiniteMap<Tenant, Plan> [map],
        export_enabled: bool,
        cross_tenant_access: bool,
    }
}

valid_actions! {
    enum RelationMapAction {
        GrantMembership => "GRANT_MEMBERSHIP" [reads = ["pending", "memberships"], writes = ["pending", "memberships"]],
        UpgradeAlpha => "UPGRADE_ALPHA" [reads = ["plans"], writes = ["plans"]],
        RetireBeta => "RETIRE_BETA" [reads = ["plans"], writes = ["plans"]],
        EnableExport => "ENABLE_EXPORT" [reads = ["memberships", "plans"], writes = ["export_enabled", "cross_tenant_access"]],
        OpenLeak => "OPEN_LEAK" [reads = ["memberships", "plans"], writes = ["export_enabled", "cross_tenant_access"]],
    }
}

valid_model! {
    model RelationMapSafeModel<RelationMapState, RelationMapAction>;
    init [RelationMapState {
        memberships: FiniteRelation::empty(),
        pending: rel_insert(FiniteRelation::empty(), Member::Alice, Tenant::Alpha),
        plans: map_put(
            map_put(FiniteMap::empty(), Tenant::Alpha, Plan::Free),
            Tenant::Beta,
            Plan::Free,
        ),
        export_enabled: false,
        cross_tenant_access: false,
    }];
    transitions {
        transition GrantMembership
        when |state| rel_intersects(state.pending, state.pending)
        => [RelationMapState {
            memberships: rel_insert(state.memberships, Member::Alice, Tenant::Alpha),
            pending: rel_remove(state.pending, Member::Alice, Tenant::Alpha),
            plans: state.plans,
            export_enabled: state.export_enabled,
            cross_tenant_access: state.cross_tenant_access,
        }];
        transition UpgradeAlpha
        when |state|
            map_contains_key(state.plans, Tenant::Alpha)
            && !map_contains_entry(state.plans, Tenant::Alpha, Plan::Enterprise)
        => [RelationMapState {
            memberships: state.memberships,
            pending: state.pending,
            plans: map_put(state.plans, Tenant::Alpha, Plan::Enterprise),
            export_enabled: state.export_enabled,
            cross_tenant_access: state.cross_tenant_access,
        }];
        transition RetireBeta
        when |state| map_contains_entry(state.plans, Tenant::Beta, Plan::Free)
        => [RelationMapState {
            memberships: state.memberships,
            pending: state.pending,
            plans: map_remove(state.plans, Tenant::Beta),
            export_enabled: state.export_enabled,
            cross_tenant_access: state.cross_tenant_access,
        }];
        transition EnableExport
        when |state|
            rel_contains(state.memberships, Member::Alice, Tenant::Alpha)
            && map_contains_entry(state.plans, Tenant::Alpha, Plan::Enterprise)
        => [RelationMapState {
            memberships: state.memberships,
            pending: state.pending,
            plans: state.plans,
            export_enabled: true,
            cross_tenant_access: false,
        }];
    }
    properties {
        invariant P_NO_CROSS_TENANT_ACCESS |state| state.cross_tenant_access == false;
    }
}

valid_model! {
    model RelationMapRegressionModel<RelationMapState, RelationMapAction>;
    init [RelationMapState {
        memberships: FiniteRelation::empty(),
        pending: FiniteRelation::empty(),
        plans: map_put(FiniteMap::empty(), Tenant::Beta, Plan::Enterprise),
        export_enabled: false,
        cross_tenant_access: false,
    }];
    transitions {
        transition OpenLeak
        when |state|
            map_contains_key(state.plans, Tenant::Beta)
            && !rel_contains(state.memberships, Member::Alice, Tenant::Alpha)
        => [RelationMapState {
            memberships: state.memberships,
            pending: state.pending,
            plans: state.plans,
            export_enabled: true,
            cross_tenant_access: true,
        }];
    }
    properties {
        invariant P_NO_CROSS_TENANT_ACCESS |state| state.cross_tenant_access == false;
    }
}

#[test]
fn relation_map_safe_model_matches_explicit_and_varisat() {
    let explicit =
        run_model::<RelationMapSafeModel>("P_NO_CROSS_TENANT_ACCESS", AdapterConfig::Explicit);
    let sat =
        run_model::<RelationMapSafeModel>("P_NO_CROSS_TENANT_ACCESS", AdapterConfig::SatVarisat);

    assert_eq!(explicit.0, RunStatus::Pass);
    assert_eq!(explicit, sat);
}

#[test]
fn relation_map_regression_matches_explicit_and_varisat() {
    let explicit = run_model::<RelationMapRegressionModel>(
        "P_NO_CROSS_TENANT_ACCESS",
        AdapterConfig::Explicit,
    );
    let sat = run_model::<RelationMapRegressionModel>(
        "P_NO_CROSS_TENANT_ACCESS",
        AdapterConfig::SatVarisat,
    );

    assert_eq!(explicit.0, RunStatus::Fail);
    assert_eq!(explicit.1, vec!["OPEN_LEAK".to_string()]);
    assert_eq!(explicit, sat);
}
