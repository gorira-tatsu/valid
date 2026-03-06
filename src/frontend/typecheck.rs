use crate::frontend::resolver::ResolvedModel;
use crate::support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedModel {
    pub resolved: ResolvedModel,
}

pub fn typecheck_model(resolved: ResolvedModel) -> Result<TypedModel, Vec<Diagnostic>> {
    let mut errors = Vec::new();

    for field in &resolved.parsed.state_fields {
        if field.ty != "bool" && !is_bounded_u8(&field.ty) {
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
                    .with_best_practice("introduce complex expression forms only after IR support exists"),
                );
            }
        }
    }

    for property in &resolved.parsed.properties {
        if property.kind != "invariant" {
            errors.push(type_error(
                format!("unsupported property kind `{}`", property.kind),
                property.line,
            ));
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
}
