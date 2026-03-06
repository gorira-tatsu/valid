use crate::{
    frontend::typecheck::TypedModel,
    ir::{
        ActionIr, BinaryOp, ExprIr, FieldType, InitAssignment, ModelIr, PropertyIr, PropertyKind,
        SourceSpan, StateField, UpdateIr, Value,
    },
    support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode, Span},
};

pub fn lower_model(typed: TypedModel) -> Result<ModelIr, Vec<Diagnostic>> {
    let parsed = &typed.resolved.parsed;
    let mut errors = Vec::new();

    let state_fields = parsed
        .state_fields
        .iter()
        .map(|field| StateField {
            id: field.name.clone(),
            name: field.name.clone(),
            ty: lower_type(&field.ty).unwrap_or(FieldType::Bool),
            span: SourceSpan {
                line: field.line,
                column: 1,
            },
        })
        .collect::<Vec<_>>();

    let mut init = Vec::new();
    for assignment in &parsed.init_assignments {
        match lower_value(&assignment.expr) {
            Some(value) => init.push(InitAssignment {
                field: assignment.field.clone(),
                value,
                span: SourceSpan {
                    line: assignment.line,
                    column: 1,
                },
            }),
            None => errors.push(lowering_error(
                format!("unsupported init expression `{}`", assignment.expr),
                assignment.line,
            )),
        }
    }

    let mut actions = Vec::new();
    for action in &parsed.actions {
        let guard = match action.pre.as_deref() {
            Some(expr) => match lower_expr(expr) {
                Some(expr) => expr,
                None => {
                    errors.push(lowering_error(
                        format!("unsupported guard expression `{}`", expr),
                        action.line,
                    ));
                    continue;
                }
            },
            None => ExprIr::Literal(Value::Bool(true)),
        };

        let mut updates = Vec::new();
        let mut reads = Vec::new();
        let mut writes = Vec::new();

        for post in &action.posts {
            writes.push(post.field.clone());
            reads.push(post.field.clone());
            match lower_expr(&post.expr) {
                Some(expr) => updates.push(UpdateIr {
                    field: post.field.clone(),
                    value: expr,
                }),
                None => errors.push(lowering_error(
                    format!("unsupported update expression `{}`", post.expr),
                    post.line,
                )),
            }
        }

        actions.push(ActionIr {
            action_id: action.name.clone(),
            label: action.name.clone(),
            reads,
            writes,
            guard,
            updates,
        });
    }

    let mut properties = Vec::new();
    for property in &parsed.properties {
        match lower_expr(&property.expr) {
            Some(expr) => properties.push(PropertyIr {
                property_id: property.name.clone(),
                kind: PropertyKind::Invariant,
                expr,
            }),
            None => errors.push(lowering_error(
                format!("unsupported property expression `{}`", property.expr),
                property.line,
            )),
        }
    }

    if errors.is_empty() {
        Ok(ModelIr {
            model_id: parsed.model_name.clone(),
            state_fields,
            init,
            actions,
            properties,
        })
    } else {
        Err(errors)
    }
}

fn lower_type(input: &str) -> Option<FieldType> {
    if input == "bool" {
        return Some(FieldType::Bool);
    }

    if input.starts_with("u8[") && input.ends_with(']') {
        let range = &input[3..input.len() - 1];
        let (min, max) = range.split_once("..")?;
        return Some(FieldType::BoundedU8 {
            min: min.parse().ok()?,
            max: max.parse().ok()?,
        });
    }

    None
}

fn lower_value(input: &str) -> Option<Value> {
    if input == "true" {
        return Some(Value::Bool(true));
    }
    if input == "false" {
        return Some(Value::Bool(false));
    }
    input.parse::<u64>().ok().map(Value::UInt)
}

fn lower_expr(input: &str) -> Option<ExprIr> {
    let trimmed = input.trim();
    if let Some(value) = lower_value(trimmed) {
        return Some(ExprIr::Literal(value));
    }
    if let Some(rest) = trimmed.strip_prefix('!') {
        return Some(ExprIr::Unary {
            op: crate::ir::UnaryOp::Not,
            expr: Box::new(lower_expr(rest.trim())?),
        });
    }
    if let Some((left, right)) = trimmed.split_once("<=") {
        return Some(ExprIr::Binary {
            op: BinaryOp::LessThanOrEqual,
            left: Box::new(lower_expr(left.trim())?),
            right: Box::new(lower_expr(right.trim())?),
        });
    }
    if let Some((left, right)) = trimmed.split_once('+') {
        return Some(ExprIr::Binary {
            op: BinaryOp::Add,
            left: Box::new(lower_expr(left.trim())?),
            right: Box::new(lower_expr(right.trim())?),
        });
    }
    if trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Some(ExprIr::FieldRef(trimmed.to_string()));
    }
    None
}

fn lowering_error(message: String, line: usize) -> Diagnostic {
    Diagnostic::new(
        ErrorCode::UnsupportedExpr,
        DiagnosticSegment::FrontendLowering,
        message,
    )
    .with_span(Span::new(line, 1))
    .with_help("rewrite the expression into the MVP expression subset")
    .with_best_practice("keep expressions explicit and avoid implicit coercions")
}

#[cfg(test)]
mod tests {
    use crate::frontend::{
        compile_model, parser::parse_model, resolver::resolve_model, typecheck::typecheck_model,
    };

    #[test]
    fn lowers_minimal_model_to_ir() {
        let source = r#"
model CounterLock
state:
  x: u8[0..7]
  locked: bool
init:
  x = 0
  locked = false
action Inc:
  pre: !locked
  post:
    x = x + 1
property P_SAFE:
  invariant: x <= 7
"#;

        let model = compile_model(source).expect("compile");
        assert_eq!(model.model_id, "CounterLock");
        assert_eq!(model.state_fields.len(), 2);
        assert_eq!(model.actions.len(), 1);
        assert_eq!(model.properties.len(), 1);
    }

    #[test]
    fn lowering_keeps_property_id() {
        let source = r#"
model CounterLock
state:
  x: u8[0..7]
init:
  x = 0
property P_SAFE:
  invariant: x <= 7
"#;
        let parsed = parse_model(source).expect("parse");
        let resolved = resolve_model(parsed).expect("resolve");
        let typed = typecheck_model(resolved).expect("type");
        let model = super::lower_model(typed).expect("lower");
        assert_eq!(model.properties[0].property_id, "P_SAFE");
    }
}
