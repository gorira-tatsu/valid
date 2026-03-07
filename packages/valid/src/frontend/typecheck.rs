use crate::frontend::resolver::ResolvedModel;
use crate::support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedModel {
    pub resolved: ResolvedModel,
}

pub fn typecheck_model(resolved: ResolvedModel) -> Result<TypedModel, Vec<Diagnostic>> {
    let mut errors = Vec::new();

    for field in &resolved.parsed.state_fields {
        if field.ty != "bool"
            && !is_bounded_u8(&field.ty)
            && !is_bounded_u16(&field.ty)
            && !is_bounded_u32(&field.ty)
        {
            errors.push(type_error(
                format!("unknown type `{}`", field.ty),
                field.line,
            ));
        }
    }

    for assignment in &resolved.parsed.init_assignments {
        if assignment.expr.contains('<') || assignment.expr.contains('>') {
            errors.push(type_error(
                "init assignments cannot contain comparison expressions".to_string(),
                assignment.line,
            ));
        }
    }

    for action in &resolved.parsed.actions {
        if let Some(pre) = &action.pre {
            if pre.contains(" + ") {
                errors.push(type_error(
                    "guard expression must be boolean".to_string(),
                    action.line,
                ));
            }
        }
        for post in &action.posts {
            if post.expr == "unknown_expr" {
                errors.push(
                    Diagnostic::new(
                        ErrorCode::UnsupportedExpr,
                        DiagnosticSegment::FrontendTypecheck,
                        "unsupported expression shape",
                    )
                    .with_span(Span::new(post.line, 1))
                    .with_help("rewrite the expression using literals, field refs, !, +, and <=")
                    .with_best_practice(
                        "introduce complex expression forms only after IR support exists",
                    ),
                );
            }
        }
    }

    for property in &resolved.parsed.properties {
        if crate::ir::PropertyKind::parse(&property.kind).is_none() {
            errors.push(type_error(
match property.kind.as_str() {
            "invariant" => {
                if property.expr.is_empty() {
                        "invariant property requires a boolean expression".to_string(),
                        property.line,
                    ));
                }
            }
            "deadlock_freedom" => {
                if !property.expr.is_empty() {
                        "deadlock_freedom property does not accept an expression".to_string(),
                        property.line,
                    ));
                }
            }
            _ => errors.push(type_error(
                format!("unsupported property kind `{}`", property.kind),
                property.line,
            )),
        }
    }

    if errors.is_empty() {
        Ok(TypedModel { resolved })
    } else {
        Err(errors)
    }
}

fn is_bounded_u8(ty: &str) -> bool {
    ty.starts_with("u8[") && ty.ends_with(']') && ty.contains("..")
}

fn is_bounded_u16(ty: &str) -> bool {
    ty.starts_with("u16[") && ty.ends_with(']') && ty.contains("..")
}

fn is_bounded_u32(ty: &str) -> bool {
    ty.starts_with("u32[") && ty.ends_with(']') && ty.contains("..")
}

fn type_error(message: String, line: usize) -> Diagnostic {
    Diagnostic::new(
        ErrorCode::TypecheckError,
        DiagnosticSegment::FrontendTypecheck,
        message,
    )
    .with_span(Span::new(line, 1))
    .with_help("review the field type and expression shape")
    .with_best_practice("keep guard expressions boolean and updates value-producing")
}

#[cfg(test)]
mod tests {
    use crate::frontend::{parser::parse_model, resolver::resolve_model};

    use super::typecheck_model;

    #[test]
    fn rejects_non_boolean_guard() {
        let source = r#"
model CounterLock
state:
  x: u8[0..7]
init:
  x = 0
action Inc:
  pre: x + 1
  post:
    x = x + 1
"#;
        let parsed = parse_model(source).expect("parse");
        let resolved = resolve_model(parsed).expect("resolve");
        let errors = typecheck_model(resolved).expect_err("must fail");
        assert_eq!(errors[0].error_code.as_str(), "TYPECHECK_ERROR");
    }

    #[test]
    fn accepts_bounded_u16_fields() {
        let source = r#"
model BudgetControl
state:
  spend: u16[0..5000]
init:
  spend = 0
property P_SAFE:
  invariant: spend <= 5000
"#;
        let parsed = parse_model(source).expect("parse");
        let resolved = resolve_model(parsed).expect("resolve");
        typecheck_model(resolved).expect("u16 fields should typecheck");
    }

    #[test]
    fn accepts_bounded_u32_fields() {
        let source = r#"
model CostControl
state:
  spend: u32[0..500000]
init:
  spend = 0
property P_SAFE:
  invariant: spend <= 500000
"#;
        let parsed = parse_model(source).expect("parse");
        let resolved = resolve_model(parsed).expect("resolve");
        typecheck_model(resolved).expect("u32 fields should typecheck");
    }

    #[test]
    fn accepts_reachability_properties() {
        let source = r#"
model DoorControl
state:
  open: bool
init:
  open = false
property P_OPEN:
  reachability: open == true
"#;
        let parsed = parse_model(source).expect("parse");
        let resolved = resolve_model(parsed).expect("resolve");
        typecheck_model(resolved).expect("reachability properties should typecheck");
    }
    fn accepts_deadlock_freedom_property() {
model CounterLock
  locked: bool
  locked = false
property P_LIVE: deadlock_freedom
        typecheck_model(resolved).expect("deadlock_freedom should typecheck");
}
