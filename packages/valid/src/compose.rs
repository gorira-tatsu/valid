use std::collections::{BTreeMap, BTreeSet};

use crate::ir::{
    ActionIr, ExprIr, InitAssignment, ModelIr, PredicateIr, PropertyIr, ScenarioIr, StateField,
    UpdateIr,
};

fn rename_expr(expr: ExprIr, field_names: &BTreeMap<String, String>) -> ExprIr {
    match expr {
        ExprIr::Literal(value) => ExprIr::Literal(value),
        ExprIr::FieldRef(field) => {
            ExprIr::FieldRef(field_names.get(&field).cloned().unwrap_or(field))
        }
        ExprIr::Unary { op, expr } => ExprIr::Unary {
            op,
            expr: Box::new(rename_expr(*expr, field_names)),
        },
        ExprIr::Binary { op, left, right } => ExprIr::Binary {
            op,
            left: Box::new(rename_expr(*left, field_names)),
            right: Box::new(rename_expr(*right, field_names)),
        },
    }
}

fn rename_model(
    model: &ModelIr,
    sync_fields: &BTreeSet<String>,
) -> (ModelIr, BTreeMap<String, String>) {
    let prefix = model.model_id.replace('-', "_");
    let field_names = model
        .state_fields
        .iter()
        .map(|field| {
            let renamed = if sync_fields.contains(&field.name) {
                field.name.clone()
            } else {
                format!("{prefix}__{}", field.name)
            };
            (field.name.clone(), renamed)
        })
        .collect::<BTreeMap<_, _>>();
    let rename_field = |field: &str| {
        field_names
            .get(field)
            .cloned()
            .unwrap_or_else(|| field.to_string())
    };
    let rename_action = |action_id: &str| format!("{}::{action_id}", model.model_id);
    let rename_property = |property_id: &str| format!("{}::{property_id}", model.model_id);
    let rename_named = |name: &str| format!("{}::{name}", model.model_id);

    (
        ModelIr {
            model_id: model.model_id.clone(),
            state_fields: model
                .state_fields
                .iter()
                .map(|field| StateField {
                    id: rename_field(&field.id),
                    name: rename_field(&field.name),
                    ty: field.ty.clone(),
                    span: field.span.clone(),
                })
                .collect(),
            init: model
                .init
                .iter()
                .map(|assignment| InitAssignment {
                    field: rename_field(&assignment.field),
                    value: assignment.value.clone(),
                    span: assignment.span.clone(),
                })
                .collect(),
            actions: model
                .actions
                .iter()
                .map(|action| ActionIr {
                    action_id: rename_action(&action.action_id),
                    label: format!("{}::{}", model.model_id, action.label),
                    role: action.role,
                    reads: action
                        .reads
                        .iter()
                        .map(|field| rename_field(field))
                        .collect(),
                    writes: action
                        .writes
                        .iter()
                        .map(|field| rename_field(field))
                        .collect(),
                    path_tags: action.path_tags.clone(),
                    guard: rename_expr(action.guard.clone(), &field_names),
                    updates: action
                        .updates
                        .iter()
                        .map(|update| UpdateIr {
                            field: rename_field(&update.field),
                            value: rename_expr(update.value.clone(), &field_names),
                        })
                        .collect(),
                })
                .collect(),
            predicates: model
                .predicates
                .iter()
                .map(|predicate| PredicateIr {
                    predicate_id: rename_named(&predicate.predicate_id),
                    expr: rename_expr(predicate.expr.clone(), &field_names),
                })
                .collect(),
            scenarios: model
                .scenarios
                .iter()
                .map(|scenario| ScenarioIr {
                    scenario_id: rename_named(&scenario.scenario_id),
                    expr: rename_expr(scenario.expr.clone(), &field_names),
                })
                .collect(),
            properties: model
                .properties
                .iter()
                .map(|property| PropertyIr {
                    property_id: rename_property(&property.property_id),
                    kind: property.kind,
                    layer: property.layer,
                    expr: rename_expr(property.expr.clone(), &field_names),
                    scope: property
                        .scope
                        .clone()
                        .map(|expr| rename_expr(expr, &field_names)),
                    action_filter: property
                        .action_filter
                        .as_ref()
                        .map(|action_id| rename_action(action_id)),
                })
                .collect(),
        },
        field_names,
    )
}

pub fn compose_models(
    left: &ModelIr,
    right: &ModelIr,
    sync_fields: &[String],
) -> Result<ModelIr, String> {
    let sync_fields = sync_fields.iter().cloned().collect::<BTreeSet<_>>();
    for field in &sync_fields {
        let left_field = left.state_fields.iter().find(|entry| &entry.name == field);
        let right_field = right.state_fields.iter().find(|entry| &entry.name == field);
        match (left_field, right_field) {
            (Some(left_field), Some(right_field)) if left_field.ty == right_field.ty => {}
            (Some(_), Some(_)) => {
                return Err(format!(
                    "sync field `{field}` has incompatible types across composed models"
                ));
            }
            _ => return Err(format!("sync field `{field}` must exist in both models")),
        }
    }

    let (left, _) = rename_model(left, &sync_fields);
    let (right, _) = rename_model(right, &sync_fields);

    let mut state_fields = left.state_fields.clone();
    for field in right.state_fields {
        if sync_fields.contains(&field.name) {
            continue;
        }
        state_fields.push(field);
    }

    let mut init_by_field = BTreeMap::<String, InitAssignment>::new();
    for assignment in left.init.into_iter().chain(right.init.into_iter()) {
        if let Some(existing) = init_by_field.get(&assignment.field) {
            if existing.value != assignment.value {
                return Err(format!(
                    "sync field `{}` has conflicting init assignments across composed models",
                    assignment.field
                ));
            }
            continue;
        }
        init_by_field.insert(assignment.field.clone(), assignment);
    }

    Ok(ModelIr {
        model_id: format!("{}+{}", left.model_id, right.model_id),
        state_fields,
        init: init_by_field.into_values().collect(),
        actions: left.actions.into_iter().chain(right.actions).collect(),
        predicates: left
            .predicates
            .into_iter()
            .chain(right.predicates)
            .collect(),
        scenarios: left.scenarios.into_iter().chain(right.scenarios).collect(),
        properties: left
            .properties
            .into_iter()
            .chain(right.properties)
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        api::{check_model, inspect_model, CheckRequest},
        engine::RunStatus,
        frontend::compile_model,
    };

    use super::compose_models;

    #[test]
    fn compose_models_namespaces_non_sync_fields_and_ids() {
        let left = compile_model(
            "model Left\nstate:\n  shared: bool\n  local_left: bool\ninit:\n  shared = false\n  local_left = false\naction EnableLeft:\n  pre: shared == false\n  post:\n    local_left = true\nproperty P_LEFT:\n  invariant: local_left == false\n",
        )
        .unwrap();
        let right = compile_model(
            "model Right\nstate:\n  shared: bool\n  local_right: bool\ninit:\n  shared = false\n  local_right = false\naction EnableRight:\n  pre: shared == false\n  post:\n    local_right = true\nproperty P_RIGHT:\n  invariant: local_right == false\n",
        )
        .unwrap();
        let composed = compose_models(&left, &right, &["shared".to_string()]).unwrap();
        let inspect = inspect_model("req-compose", &composed);
        assert_eq!(inspect.model_id, "Left+Right");
        assert!(inspect.state_fields.contains(&"shared".to_string()));
        assert!(inspect
            .state_fields
            .contains(&"Left__local_left".to_string()));
        assert!(inspect
            .state_fields
            .contains(&"Right__local_right".to_string()));
        assert!(inspect.actions.contains(&"Left::EnableLeft".to_string()));
        assert!(inspect.actions.contains(&"Right::EnableRight".to_string()));
        assert!(inspect.properties.contains(&"Left::P_LEFT".to_string()));
        assert!(inspect.properties.contains(&"Right::P_RIGHT".to_string()));
    }

    #[test]
    fn compose_models_can_fail_cross_model_property_checks() {
        let left = compile_model(
            "model Left\nstate:\n  shared: bool\ninit:\n  shared = false\nproperty P_SHARED_STAYS_FALSE:\n  invariant: shared == false\n",
        )
        .unwrap();
        let right = compile_model(
            "model Right\nstate:\n  shared: bool\ninit:\n  shared = false\naction Flip:\n  pre: shared == false\n  post:\n    shared = true\nproperty P_RIGHT:\n  invariant: shared == true || shared == false\n",
        )
        .unwrap();
        let composed = compose_models(&left, &right, &["shared".to_string()]).unwrap();
        let outcome = check_model(
            &CheckRequest {
                request_id: "req-compose-check".to_string(),
                source_name: "compose".to_string(),
                source: String::new(),
                property_id: Some("Left::P_SHARED_STAYS_FALSE".to_string()),
                scenario_id: None,
                seed: None,
                backend: Some("explicit".to_string()),
                solver_executable: None,
                solver_args: vec![],
            },
            &composed,
            "compose".to_string(),
        );
        let crate::engine::CheckOutcome::Completed(result) = outcome else {
            panic!("composed check should complete");
        };
        assert_eq!(result.status, RunStatus::Fail);
        assert_eq!(
            result
                .trace
                .as_ref()
                .and_then(|trace| trace.steps.last())
                .and_then(|step| step.action_id.as_deref()),
            Some("Right::Flip")
        );
    }
}
