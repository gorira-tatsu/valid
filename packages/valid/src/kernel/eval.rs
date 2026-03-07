use crate::{
    ir::{BinaryOp, ExprIr, ModelIr, UnaryOp, Value},
    support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode},
};

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
                _ => Err(eval_error("invalid unary operand type".to_string())),
            }
        }
        ExprIr::Binary { op, left, right } => {
            let left = eval_expr(model, state, left)?;
            let right = eval_expr(model, state, right)?;
            match (op, left, right) {
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
                (BinaryOp::SetContains, Value::UInt(bits), Value::EnumVariant { index, .. }) => {
                    Ok(Value::Bool(bits & enum_variant_mask(index) != 0))
                }
                (BinaryOp::SetInsert, Value::UInt(bits), Value::EnumVariant { index, .. }) => {
                    Ok(Value::UInt(bits | enum_variant_mask(index)))
                }
                (BinaryOp::SetRemove, Value::UInt(bits), Value::EnumVariant { index, .. }) => {
                    Ok(Value::UInt(bits & !enum_variant_mask(index)))
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

fn eval_error(message: String) -> Diagnostic {
    Diagnostic::new(ErrorCode::EvalError, DiagnosticSegment::KernelEval, message)
        .with_help("check field names and operand types in the lowered IR")
        .with_best_practice(
            "keep MVP expressions within bool, finite sets, u64, !, &&, ||, +, -, %, membership, and numeric comparisons",
        )
}

#[cfg(test)]
mod tests {
    use crate::{
        ir::{BinaryOp, ExprIr, FieldType, ModelIr, SourceSpan, StateField, Value},
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
}
