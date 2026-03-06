use crate::{
    ir::{FieldType, ModelIr, Value},
    support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode},
};

use super::{eval::eval_expr, guard::evaluate_guard, MachineState};

pub fn build_initial_state(model: &ModelIr) -> Result<MachineState, Diagnostic> {
    let mut values = Vec::with_capacity(model.state_fields.len());
    for field in &model.state_fields {
        let assignment = model
            .init
            .iter()
            .find(|item| item.field == field.id)
            .ok_or_else(|| {
                Diagnostic::new(
                    ErrorCode::InvalidState,
                    DiagnosticSegment::KernelTransition,
                    format!("missing init assignment for field `{}`", field.name),
                )
                .with_help("assign every declared state field in the init block")
                .with_best_practice("keep the MVP init section fully explicit")
            })?;
        validate_value_for_type(&field.ty, &assignment.value, &field.name)?;
        values.push(assignment.value.clone());
    }
    Ok(MachineState::new(values))
}

pub fn apply_action(
    model: &ModelIr,
    state: &MachineState,
    action_id: &str,
) -> Result<Option<MachineState>, Diagnostic> {
    let action = model
        .actions
        .iter()
        .find(|item| item.action_id == action_id)
        .ok_or_else(|| {
            Diagnostic::new(
                ErrorCode::InvalidTransitionUpdate,
                DiagnosticSegment::KernelTransition,
                format!("unknown action `{action_id}`"),
            )
            .with_help("select an action id that exists in the model")
        })?;

    if !evaluate_guard(model, state, action)? {
        return Ok(None);
    }

    let mut next_values = state.values.clone();
    for update in &action.updates {
        let index = model
            .state_fields
            .iter()
            .position(|field| field.id == update.field)
            .ok_or_else(|| {
                Diagnostic::new(
                    ErrorCode::InvalidTransitionUpdate,
                    DiagnosticSegment::KernelTransition,
                    format!("unknown update target `{}`", update.field),
                )
                .with_help("lowering must only emit updates to declared fields")
            })?;
        let next_value = eval_expr(model, state, &update.value)?;
        validate_value_for_type(&model.state_fields[index].ty, &next_value, &update.field)?;
        next_values[index] = next_value;
    }

    Ok(Some(MachineState::new(next_values)))
}

fn validate_value_for_type(
    ty: &FieldType,
    value: &Value,
    field_name: &str,
) -> Result<(), Diagnostic> {
    match (ty, value) {
        (FieldType::Bool, Value::Bool(_)) => Ok(()),
        (FieldType::BoundedU8 { min, max }, Value::UInt(value))
            if *value >= *min as u64 && *value <= *max as u64 =>
        {
            Ok(())
        }
        (FieldType::BoundedU8 { .. }, Value::UInt(_)) => Err(Diagnostic::new(
            ErrorCode::InvalidState,
            DiagnosticSegment::KernelTransition,
            format!("value for `{field_name}` exceeds declared range"),
        )
        .with_help("keep update results inside the bounded integer range")
        .with_best_practice("encode saturation or guards explicitly in the model")),
        _ => Err(Diagnostic::new(
            ErrorCode::InvalidState,
            DiagnosticSegment::KernelTransition,
            format!("value for `{field_name}` does not match the declared field type"),
        )
        .with_help("keep init assignments and updates type-consistent")
        .with_best_practice("prefer bool for predicates and bounded u8 for counters in the MVP")),
    }
}

#[cfg(test)]
mod tests {
    use crate::ir::{
        ActionIr, BinaryOp, ExprIr, FieldType, InitAssignment, ModelIr, SourceSpan, StateField,
        UpdateIr, Value,
    };

    use super::{apply_action, build_initial_state};

    fn swap_model() -> ModelIr {
        ModelIr {
            model_id: "Swap".to_string(),
            state_fields: vec![
                StateField {
                    id: "x".to_string(),
                    name: "x".to_string(),
                    ty: FieldType::BoundedU8 { min: 0, max: 7 },
                    span: SourceSpan { line: 1, column: 1 },
                },
                StateField {
                    id: "y".to_string(),
                    name: "y".to_string(),
                    ty: FieldType::BoundedU8 { min: 0, max: 7 },
                    span: SourceSpan { line: 2, column: 1 },
                },
            ],
            init: vec![
                InitAssignment {
                    field: "x".to_string(),
                    value: Value::UInt(1),
                    span: SourceSpan { line: 3, column: 1 },
                },
                InitAssignment {
                    field: "y".to_string(),
                    value: Value::UInt(2),
                    span: SourceSpan { line: 4, column: 1 },
                },
            ],
            actions: vec![ActionIr {
                action_id: "Swap".to_string(),
                label: "Swap".to_string(),
                reads: vec!["x".to_string(), "y".to_string()],
                writes: vec!["x".to_string(), "y".to_string()],
                guard: ExprIr::Literal(Value::Bool(true)),
                updates: vec![
                    UpdateIr {
                        field: "x".to_string(),
                        value: ExprIr::FieldRef("y".to_string()),
                    },
                    UpdateIr {
                        field: "y".to_string(),
                        value: ExprIr::FieldRef("x".to_string()),
                    },
                ],
            }],
            properties: vec![],
        }
    }

    #[test]
    fn applies_updates_simultaneously() {
        let model = swap_model();
        let initial = build_initial_state(&model).unwrap();
        let next = apply_action(&model, &initial, "Swap").unwrap().unwrap();
        assert_eq!(next.values, vec![Value::UInt(2), Value::UInt(1)]);
    }

    #[test]
    fn rejects_out_of_range_update() {
        let mut model = swap_model();
        model.actions[0].updates = vec![UpdateIr {
            field: "x".to_string(),
            value: ExprIr::Binary {
                op: BinaryOp::Add,
                left: Box::new(ExprIr::Literal(Value::UInt(7))),
                right: Box::new(ExprIr::Literal(Value::UInt(1))),
            },
        }];
        let initial = build_initial_state(&model).unwrap();
        assert!(apply_action(&model, &initial, "Swap").is_err());
    }
}
