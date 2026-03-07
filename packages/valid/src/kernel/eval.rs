use crate::{
    ir::{BinaryOp, ExprIr, FieldType, ModelIr, UnaryOp, Value},
    support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode},
};
use std::sync::{Mutex, OnceLock};

use super::MachineState;

pub fn eval_expr(
    model: &ModelIr,
    state: &MachineState,
    expr: &ExprIr,
) -> Result<Value, Diagnostic> {
    match expr {
        ExprIr::Literal(value) => Ok(value.clone()),
        ExprIr::FieldRef(field) => state
            .get(model, field)
            .cloned()
            .ok_or_else(|| eval_error(format!("unknown field `{field}` during evaluation"))),
        ExprIr::Unary { op, expr } => {
            let value = eval_expr(model, state, expr)?;
            match (op, value) {
                (UnaryOp::Not, Value::Bool(inner)) => Ok(Value::Bool(!inner)),
                (UnaryOp::SetIsEmpty, Value::UInt(bits)) => Ok(Value::Bool(bits == 0)),
                (UnaryOp::StringLen, Value::String(value)) => {
                    Ok(Value::UInt(value.chars().count() as u64))
                }
                _ => Err(eval_error("invalid unary operand type".to_string())),
            }
        }
        ExprIr::Binary { op, left, right } => {
            let left_value = eval_expr(model, state, left)?;
            let right_value = eval_expr(model, state, right)?;
            match (op, left_value, right_value) {
                (BinaryOp::Add, Value::UInt(left), Value::UInt(right)) => {
                    Ok(Value::UInt(left.saturating_add(right)))
                }
                (BinaryOp::Sub, Value::UInt(left), Value::UInt(right)) => {
                    Ok(Value::UInt(left.saturating_sub(right)))
                }
                (BinaryOp::Mod, Value::UInt(left), Value::UInt(right)) => {
                    if right == 0 {
                        Err(eval_error("modulo by zero".to_string()))
                    } else {
                        Ok(Value::UInt(left % right))
                    }
                }
                (BinaryOp::StringContains, Value::String(left), Value::String(right)) => {
                    Ok(Value::Bool(left.contains(&right)))
                }
                (BinaryOp::RegexMatch, Value::String(left), Value::String(right)) => {
                    Ok(Value::Bool(regex_match_cached(&left, &right)?))
                }
                (BinaryOp::SetContains, Value::UInt(bits), Value::EnumVariant { index, .. }) => {
                    Ok(Value::Bool(bits & enum_variant_mask(index) != 0))
                }
                (BinaryOp::SetInsert, Value::UInt(bits), Value::EnumVariant { index, .. }) => {
                    Ok(Value::UInt(bits | enum_variant_mask(index)))
                }
                (BinaryOp::SetRemove, Value::UInt(bits), Value::EnumVariant { index, .. }) => {
                    Ok(Value::UInt(bits & !enum_variant_mask(index)))
                }
                (
                    BinaryOp::RelationContains,
                    Value::UInt(bits),
                    Value::PairVariant {
                        left_index,
                        right_index,
                        ..
                    },
                ) => Ok(Value::Bool(
                    relation_mask_for_expr(model, left.as_ref(), left_index, right_index)?
                        .map(|mask| bits & mask != 0)
                        .unwrap_or(false),
                )),
                (
                    BinaryOp::RelationInsert,
                    Value::UInt(bits),
                    Value::PairVariant {
                        left_index,
                        right_index,
                        ..
                    },
                ) => Ok(Value::UInt(
                    relation_mask_for_expr(model, left.as_ref(), left_index, right_index)?
                        .map(|mask| bits | mask)
                        .unwrap_or(bits),
                )),
                (
                    BinaryOp::RelationRemove,
                    Value::UInt(bits),
                    Value::PairVariant {
                        left_index,
                        right_index,
                        ..
                    },
                ) => Ok(Value::UInt(
                    relation_mask_for_expr(model, left.as_ref(), left_index, right_index)?
                        .map(|mask| bits & !mask)
                        .unwrap_or(bits),
                )),
                (BinaryOp::RelationIntersects, Value::UInt(left), Value::UInt(right)) => {
                    Ok(Value::Bool(left & right != 0))
                }
                (BinaryOp::MapContainsKey, Value::UInt(bits), Value::EnumVariant { index, .. }) => {
                    Ok(Value::Bool(
                        map_group_bits(model, left.as_ref(), bits, index)? != 0,
                    ))
                }
                (
                    BinaryOp::MapContainsEntry,
                    Value::UInt(bits),
                    Value::PairVariant {
                        left_index,
                        right_index,
                        ..
                    },
                ) => Ok(Value::Bool(
                    relation_mask_for_expr(model, left.as_ref(), left_index, right_index)?
                        .map(|mask| bits & mask != 0)
                        .unwrap_or(false),
                )),
                (
                    BinaryOp::MapPut,
                    Value::UInt(bits),
                    Value::PairVariant {
                        left_index,
                        right_index,
                        ..
                    },
                ) => {
                    let cleared = clear_map_group(model, left.as_ref(), bits, left_index)?;
                    Ok(Value::UInt(
                        relation_mask_for_expr(model, left.as_ref(), left_index, right_index)?
                            .map(|mask| cleared | mask)
                            .unwrap_or(cleared),
                    ))
                }
                (BinaryOp::MapRemoveKey, Value::UInt(bits), Value::EnumVariant { index, .. }) => {
                    Ok(Value::UInt(clear_map_group(
                        model,
                        left.as_ref(),
                        bits,
                        index,
                    )?))
                }
                (BinaryOp::LessThan, Value::UInt(left), Value::UInt(right)) => {
                    Ok(Value::Bool(left < right))
                }
                (BinaryOp::LessThanOrEqual, Value::UInt(left), Value::UInt(right)) => {
                    Ok(Value::Bool(left <= right))
                }
                (BinaryOp::GreaterThan, Value::UInt(left), Value::UInt(right)) => {
                    Ok(Value::Bool(left > right))
                }
                (BinaryOp::GreaterThanOrEqual, Value::UInt(left), Value::UInt(right)) => {
                    Ok(Value::Bool(left >= right))
                }
                (BinaryOp::Equal, left, right) => Ok(Value::Bool(left == right)),
                (BinaryOp::NotEqual, left, right) => Ok(Value::Bool(left != right)),
                (BinaryOp::And, Value::Bool(left), Value::Bool(right)) => {
                    Ok(Value::Bool(left && right))
                }
                (BinaryOp::Or, Value::Bool(left), Value::Bool(right)) => {
                    Ok(Value::Bool(left || right))
                }
                _ => Err(eval_error("invalid binary operand types".to_string())),
            }
        }
    }
}

fn enum_variant_mask(index: u64) -> u64 {
    1u64.checked_shl(index as u32).unwrap_or(0)
}

fn relation_mask_for_expr(
    model: &ModelIr,
    expr: &ExprIr,
    left_index: u64,
    right_index: u64,
) -> Result<Option<u64>, Diagnostic> {
    let right_len = match field_type_for_expr(model, expr) {
        Some(FieldType::EnumRelation { right_variants, .. }) => right_variants.len() as u64,
        Some(FieldType::EnumMap { value_variants, .. }) => value_variants.len() as u64,
        _ => {
            return Err(eval_error(
                "relation/map operation requires a relation- or map-typed field".to_string(),
            ))
        }
    };
    let bit_index = left_index
        .checked_mul(right_len)
        .and_then(|value| value.checked_add(right_index))
        .ok_or_else(|| eval_error("relation/map index overflow".to_string()))?;
    Ok(Some(enum_variant_mask(bit_index)))
}

fn map_group_bits(
    model: &ModelIr,
    expr: &ExprIr,
    bits: u64,
    key_index: u64,
) -> Result<u64, Diagnostic> {
    let value_len = match field_type_for_expr(model, expr) {
        Some(FieldType::EnumMap { value_variants, .. }) => value_variants.len() as u64,
        _ => {
            return Err(eval_error(
                "map operation requires a finite map field".to_string(),
            ))
        }
    };
    let mut found = 0u64;
    for value_index in 0..value_len {
        if let Some(mask) = relation_mask_for_expr(model, expr, key_index, value_index)? {
            if bits & mask != 0 {
                found |= mask;
            }
        }
    }
    Ok(found)
}

fn clear_map_group(
    model: &ModelIr,
    expr: &ExprIr,
    bits: u64,
    key_index: u64,
) -> Result<u64, Diagnostic> {
    let value_len = match field_type_for_expr(model, expr) {
        Some(FieldType::EnumMap { value_variants, .. }) => value_variants.len() as u64,
        _ => {
            return Err(eval_error(
                "map operation requires a finite map field".to_string(),
            ))
        }
    };
    let mut cleared = bits;
    for value_index in 0..value_len {
        if let Some(mask) = relation_mask_for_expr(model, expr, key_index, value_index)? {
            cleared &= !mask;
        }
    }
    Ok(cleared)
}

fn field_type_for_expr<'a>(model: &'a ModelIr, expr: &ExprIr) -> Option<&'a FieldType> {
    match expr {
        ExprIr::FieldRef(field) => model
            .state_fields
            .iter()
            .find(|state_field| state_field.id == *field)
            .map(|state_field| &state_field.ty),
        ExprIr::Binary { op, left, .. } => match op {
            BinaryOp::SetInsert
            | BinaryOp::SetRemove
            | BinaryOp::RelationInsert
            | BinaryOp::RelationRemove
            | BinaryOp::MapPut
            | BinaryOp::MapRemoveKey => field_type_for_expr(model, left),
            _ => None,
        },
        _ => None,
    }
}

fn eval_error(message: String) -> Diagnostic {
    Diagnostic::new(ErrorCode::EvalError, DiagnosticSegment::KernelEval, message)
        .with_help("check field names and operand types in the lowered IR")
        .with_best_practice(
            "keep MVP expressions within bool, finite sets, u64, !, &&, ||, +, -, %, membership, and numeric comparisons",
        )
}

fn regex_match_cached(value: &str, pattern: &str) -> Result<bool, Diagnostic> {
    static CACHE: OnceLock<Mutex<std::collections::BTreeMap<String, regex::Regex>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::BTreeMap::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let compiled = if let Some(compiled) = cache.get(pattern) {
        compiled.clone()
    } else {
        let compiled = regex::Regex::new(pattern)
            .map_err(|error| eval_error(format!("invalid regex pattern `{pattern}`: {error}")))?;
        cache.insert(pattern.to_string(), compiled.clone());
        compiled
    };
    Ok(compiled.is_match(value))
}

#[cfg(test)]
mod tests {
    use crate::{
        ir::{BinaryOp, ExprIr, FieldType, ModelIr, SourceSpan, StateField, UnaryOp, Value},
        kernel::MachineState,
    };

    use super::eval_expr;

    fn model() -> ModelIr {
        ModelIr {
            model_id: "Eval".to_string(),
            state_fields: vec![
                StateField {
                    id: "x".to_string(),
                    name: "x".to_string(),
                    ty: FieldType::BoundedU8 { min: 0, max: 7 },
                    span: SourceSpan { line: 1, column: 1 },
                },
                StateField {
                    id: "locked".to_string(),
                    name: "locked".to_string(),
                    ty: FieldType::Bool,
                    span: SourceSpan { line: 2, column: 1 },
                },
            ],
            init: vec![],
            actions: vec![],
            properties: vec![],
        }
    }

    fn string_model() -> ModelIr {
        ModelIr {
            model_id: "StringEval".to_string(),
            state_fields: vec![StateField {
                id: "password".to_string(),
                name: "password".to_string(),
                ty: FieldType::String {
                    min_len: Some(0),
                    max_len: Some(64),
                },
                span: SourceSpan { line: 1, column: 1 },
            }],
            init: vec![],
            actions: vec![],
            properties: vec![],
        }
    }

    #[test]
    fn evaluates_boolean_and_arithmetic_expr() {
        let model = model();
        let state = MachineState::new(vec![Value::UInt(3), Value::Bool(false)]);
        let expr = ExprIr::Binary {
            op: BinaryOp::LessThanOrEqual,
            left: Box::new(ExprIr::Binary {
                op: BinaryOp::Add,
                left: Box::new(ExprIr::FieldRef("x".to_string())),
                right: Box::new(ExprIr::Literal(Value::UInt(1))),
            }),
            right: Box::new(ExprIr::Literal(Value::UInt(4))),
        };
        assert_eq!(eval_expr(&model, &state, &expr).unwrap(), Value::Bool(true));
    }

    #[test]
    fn evaluates_or_and_equality_expr() {
        let model = model();
        let state = MachineState::new(vec![Value::UInt(3), Value::Bool(false)]);
        let expr = ExprIr::Binary {
            op: BinaryOp::Or,
            left: Box::new(ExprIr::Binary {
                op: BinaryOp::Equal,
                left: Box::new(ExprIr::FieldRef("x".to_string())),
                right: Box::new(ExprIr::Literal(Value::UInt(2))),
            }),
            right: Box::new(ExprIr::Binary {
                op: BinaryOp::Equal,
                left: Box::new(ExprIr::FieldRef("locked".to_string())),
                right: Box::new(ExprIr::Literal(Value::Bool(false))),
            }),
        };
        assert_eq!(eval_expr(&model, &state, &expr).unwrap(), Value::Bool(true));
    }

    #[test]
    fn evaluates_extended_numeric_comparisons() {
        let model = model();
        let state = MachineState::new(vec![Value::UInt(3), Value::Bool(false)]);
        let expr = ExprIr::Binary {
            op: BinaryOp::And,
            left: Box::new(ExprIr::Binary {
                op: BinaryOp::GreaterThan,
                left: Box::new(ExprIr::Binary {
                    op: BinaryOp::Sub,
                    left: Box::new(ExprIr::FieldRef("x".to_string())),
                    right: Box::new(ExprIr::Literal(Value::UInt(1))),
                }),
                right: Box::new(ExprIr::Literal(Value::UInt(1))),
            }),
            right: Box::new(ExprIr::Binary {
                op: BinaryOp::NotEqual,
                left: Box::new(ExprIr::FieldRef("locked".to_string())),
                right: Box::new(ExprIr::Literal(Value::Bool(true))),
            }),
        };
        assert_eq!(eval_expr(&model, &state, &expr).unwrap(), Value::Bool(true));
    }

    #[test]
    fn evaluates_modulo_expr() {
        let model = model();
        let state = MachineState::new(vec![Value::UInt(7), Value::Bool(false)]);
        let expr = ExprIr::Binary {
            op: BinaryOp::Equal,
            left: Box::new(ExprIr::Binary {
                op: BinaryOp::Mod,
                left: Box::new(ExprIr::FieldRef("x".to_string())),
                right: Box::new(ExprIr::Literal(Value::UInt(3))),
            }),
            right: Box::new(ExprIr::Literal(Value::UInt(1))),
        };
        assert_eq!(eval_expr(&model, &state, &expr).unwrap(), Value::Bool(true));
    }

    #[test]
    fn evaluates_string_length_contains_and_regex_exprs() {
        let model = string_model();
        let state = MachineState::new(vec![Value::String("Str0ngPass!".to_string())]);

        let len_expr = ExprIr::Binary {
            op: BinaryOp::GreaterThanOrEqual,
            left: Box::new(ExprIr::Unary {
                op: UnaryOp::StringLen,
                expr: Box::new(ExprIr::FieldRef("password".to_string())),
            }),
            right: Box::new(ExprIr::Literal(Value::UInt(10))),
        };
        assert_eq!(
            eval_expr(&model, &state, &len_expr).unwrap(),
            Value::Bool(true)
        );

        let contains_expr = ExprIr::Binary {
            op: BinaryOp::StringContains,
            left: Box::new(ExprIr::FieldRef("password".to_string())),
            right: Box::new(ExprIr::Literal(Value::String("Pass".to_string()))),
        };
        assert_eq!(
            eval_expr(&model, &state, &contains_expr).unwrap(),
            Value::Bool(true)
        );

        let regex_expr = ExprIr::Binary {
            op: BinaryOp::RegexMatch,
            left: Box::new(ExprIr::FieldRef("password".to_string())),
            right: Box::new(ExprIr::Literal(Value::String("[A-Z]".to_string()))),
        };
        assert_eq!(
            eval_expr(&model, &state, &regex_expr).unwrap(),
            Value::Bool(true)
        );
    }
}
