use crate::{
    ir::{ActionIr, FieldType, ModelIr, Value},
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
    let candidates = apply_action_variants(model, state, action_id)?;
    match candidates.len() {
        0 => Ok(None),
        1 => Ok(candidates.into_iter().next()),
        _ => Err(Diagnostic::new(
            ErrorCode::InvalidTransitionUpdate,
            DiagnosticSegment::KernelTransition,
            format!(
                "action `{action_id}` is ambiguous during replay because multiple guarded transitions are enabled"
            ),
        )
        .with_help(
            "use mutually exclusive guards for duplicate declarative transitions or replay with a more specific branch identifier",
        )
        .with_best_practice(
            "treat repeated action ids as branch families whose guards should be disjoint in executable traces",
        )),
    }
}

pub fn apply_action_transition(
    model: &ModelIr,
    state: &MachineState,
    action: &ActionIr,
) -> Result<Option<MachineState>, Diagnostic> {
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

pub fn apply_action_variants(
    model: &ModelIr,
    state: &MachineState,
    action_id: &str,
) -> Result<Vec<MachineState>, Diagnostic> {
    let matching = model
        .actions
        .iter()
        .filter(|item| item.action_id == action_id)
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return Err(Diagnostic::new(
            ErrorCode::InvalidTransitionUpdate,
            DiagnosticSegment::KernelTransition,
            format!("unknown action `{action_id}`"),
        )
        .with_help("select an action id that exists in the model"));
    }

    let mut candidates = Vec::new();
    for action in matching {
        if let Some(next_state) = apply_action_transition(model, state, action)? {
            if !candidates.iter().any(|existing| existing == &next_state) {
                candidates.push(next_state);
            }
        }
    }
    Ok(candidates)
}

fn validate_value_for_type(
    ty: &FieldType,
    value: &Value,
    field_name: &str,
) -> Result<(), Diagnostic> {
    match (ty, value) {
        (FieldType::Bool, Value::Bool(_)) => Ok(()),
        (FieldType::String { min_len, max_len }, Value::String(value)) => {
            let len = value.chars().count() as u64;
            if let Some(min_len) = min_len {
                if len < *min_len as u64 {
                    return Err(Diagnostic::new(
                        ErrorCode::InvalidState,
                        DiagnosticSegment::KernelTransition,
                        format!("value for `{field_name}` is shorter than the declared minimum length"),
                    )
                    .with_help("keep string updates inside the declared length range")
                    .with_best_practice(
                        "declare password and token lengths explicitly with #[valid(range = \"min..=max\")]",
                    ));
                }
            }
            if let Some(max_len) = max_len {
                if len > *max_len as u64 {
                    return Err(Diagnostic::new(
                        ErrorCode::InvalidState,
                        DiagnosticSegment::KernelTransition,
                        format!("value for `{field_name}` exceeds the declared maximum length"),
                    )
                    .with_help("keep string updates inside the declared length range")
                    .with_best_practice(
                        "declare password and token lengths explicitly with #[valid(range = \"min..=max\")]",
                    ));
                }
            }
            Ok(())
        }
        (FieldType::BoundedU8 { min, max }, Value::UInt(value))
            if *value >= *min as u64 && *value <= *max as u64 =>
        {
            Ok(())
        }
        (FieldType::BoundedU16 { min, max }, Value::UInt(value))
            if *value >= *min as u64 && *value <= *max as u64 =>
        {
            Ok(())
        }
        (FieldType::BoundedU32 { min, max }, Value::UInt(value))
            if *value >= *min as u64 && *value <= *max as u64 =>
        {
            Ok(())
        }
        (FieldType::Enum { variants }, Value::EnumVariant { label, index })
            if variants
                .get(*index as usize)
                .map(|variant| variant == label)
                .unwrap_or(false) =>
        {
            Ok(())
        }
        (FieldType::EnumSet { variants }, Value::UInt(value))
            if within_enum_set_bounds(variants, *value) =>
        {
            Ok(())
        }
        (
            FieldType::EnumRelation {
                left_variants,
                right_variants,
            },
            Value::UInt(value),
        ) if within_relation_bounds(left_variants, right_variants, *value) => Ok(()),
        (
            FieldType::EnumMap {
                key_variants,
                value_variants,
            },
            Value::UInt(value),
        ) if within_relation_bounds(key_variants, value_variants, *value) => Ok(()),
        (FieldType::BoundedU8 { .. }, Value::UInt(_)) => Err(Diagnostic::new(
            ErrorCode::InvalidState,
            DiagnosticSegment::KernelTransition,
            format!("value for `{field_name}` exceeds declared range"),
        )
        .with_help("keep update results inside the bounded integer range")
        .with_best_practice("encode saturation or guards explicitly in the model")),
        (FieldType::BoundedU16 { .. }, Value::UInt(_)) => Err(Diagnostic::new(
            ErrorCode::InvalidState,
            DiagnosticSegment::KernelTransition,
            format!("value for `{field_name}` exceeds declared range"),
        )
        .with_help("keep update results inside the bounded integer range")
        .with_best_practice("encode saturation or guards explicitly in the model")),
        (FieldType::BoundedU32 { .. }, Value::UInt(_)) => Err(Diagnostic::new(
            ErrorCode::InvalidState,
            DiagnosticSegment::KernelTransition,
            format!("value for `{field_name}` exceeds declared range"),
        )
        .with_help("keep update results inside the bounded integer range")
        .with_best_practice("encode saturation or guards explicitly in the model")),
        (FieldType::Enum { .. }, Value::EnumVariant { .. }) => Err(Diagnostic::new(
            ErrorCode::InvalidState,
            DiagnosticSegment::KernelTransition,
            format!("value for `{field_name}` is not one of the declared enum variants"),
        )
        .with_help("keep enum updates within the derived finite variant set")
        .with_best_practice("prefer unit enums with stable variant sets for symbolic state")),
        (FieldType::EnumSet { .. }, Value::UInt(_)) => Err(Diagnostic::new(
            ErrorCode::InvalidState,
            DiagnosticSegment::KernelTransition,
            format!("value for `{field_name}` exceeds the declared finite-set bounds"),
        )
        .with_help("keep finite-set updates within the declared enum universe")
        .with_best_practice("model set membership with FiniteEnumSet<T> and finite enum variants")),
        (FieldType::EnumRelation { .. }, Value::UInt(_)) => Err(Diagnostic::new(
            ErrorCode::InvalidState,
            DiagnosticSegment::KernelTransition,
            format!("value for `{field_name}` exceeds the declared finite-relation bounds"),
        )
        .with_help("keep finite-relation updates within the declared pair universe")
        .with_best_practice(
            "model relation membership with FiniteRelation<A, B> and finite enums",
        )),
        (FieldType::EnumMap { .. }, Value::UInt(_)) => Err(Diagnostic::new(
            ErrorCode::InvalidState,
            DiagnosticSegment::KernelTransition,
            format!("value for `{field_name}` exceeds the declared finite-map bounds"),
        )
        .with_help("keep finite-map updates within the declared key/value universe")
        .with_best_practice("model key/value assignments with FiniteMap<K, V> and finite enums")),
        _ => Err(Diagnostic::new(
            ErrorCode::InvalidState,
            DiagnosticSegment::KernelTransition,
            format!("value for `{field_name}` does not match the declared field type"),
        )
        .with_help("keep init assignments and updates type-consistent")
        .with_best_practice(
            "prefer bool for predicates and bounded unsigned integers for counters in the MVP",
        )),
    }
}

fn within_enum_set_bounds(variants: &[String], value: u64) -> bool {
    if variants.len() > 64 {
        return false;
    }
    if variants.len() == 64 {
        return true;
    }
    value <= ((1u64 << variants.len()) - 1)
}

fn within_relation_bounds(left_variants: &[String], right_variants: &[String], value: u64) -> bool {
    let slots = left_variants.len().saturating_mul(right_variants.len());
    if slots > 64 {
        return false;
    }
    if slots == 64 {
        return true;
    }
    value <= ((1u64 << slots) - 1)
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
                path_tags: vec!["read_path".to_string(), "write_path".to_string()],
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
