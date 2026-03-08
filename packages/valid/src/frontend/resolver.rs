use std::collections::HashSet;

use crate::frontend::parser::{ParsedAssignment, ParsedChoice, ParsedModel, ParsedNamedExpr};
use crate::support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModel {
    pub parsed: ParsedModel,
}

pub fn resolve_model(parsed: ParsedModel) -> Result<ResolvedModel, Vec<Diagnostic>> {
    let mut errors = Vec::new();
    let mut fields = HashSet::new();
    let mut actions = HashSet::new();
    let mut predicates = HashSet::new();
    let mut scenarios = HashSet::new();
    let mut properties = HashSet::new();

    for field in &parsed.state_fields {
        if !fields.insert(field.name.clone()) {
            errors.push(resolve_error(
                format!("duplicate state field `{}`", field.name),
                field.line,
            ));
        }
    }

    for assignment in &parsed.init_assignments {
        check_assignment_target(assignment, &fields, &mut errors);
    }

    for action in &parsed.actions {
        if !actions.insert(action.name.clone()) {
            errors.push(resolve_error(
                format!("duplicate action `{}`", action.name),
                action.line,
            ));
        }
        let mut choice_names = HashSet::new();
        for choice in &action.choices {
            ensure_unique_choice(choice, &mut choice_names, &mut errors);
        }
        for post in &action.posts {
            check_assignment_target(post, &fields, &mut errors);
        }
    }

    for predicate in &parsed.predicates {
        ensure_unique_named_expr("predicate", predicate, &mut predicates, &mut errors);
    }

    for scenario in &parsed.scenarios {
        ensure_unique_named_expr("scenario", scenario, &mut scenarios, &mut errors);
    }

    for property in &parsed.properties {
        if !properties.insert(property.name.clone()) {
            errors.push(resolve_error(
                format!("duplicate property `{}`", property.name),
                property.line,
            ));
        }
    }

    if errors.is_empty() {
        Ok(ResolvedModel { parsed })
    } else {
        Err(errors)
    }
}

fn ensure_unique_choice(
    choice: &ParsedChoice,
    seen: &mut HashSet<String>,
    errors: &mut Vec<Diagnostic>,
) {
    if choice.name.is_empty() {
        errors.push(resolve_error(
            "bounded choice name must not be empty".to_string(),
            choice.line,
        ));
    } else if !seen.insert(choice.name.clone()) {
        errors.push(resolve_error(
            format!("duplicate bounded choice `{}`", choice.name),
            choice.line,
        ));
    }
}

fn check_assignment_target(
    assignment: &ParsedAssignment,
    fields: &HashSet<String>,
    errors: &mut Vec<Diagnostic>,
) {
    if !fields.contains(&assignment.field) {
        errors.push(resolve_error(
            format!("unresolved state field `{}`", assignment.field),
            assignment.line,
        ));
    }
}

fn ensure_unique_named_expr(
    kind: &str,
    expr: &ParsedNamedExpr,
    seen: &mut HashSet<String>,
    errors: &mut Vec<Diagnostic>,
) {
    if !seen.insert(expr.name.clone()) {
        errors.push(resolve_error(
            format!("duplicate {kind} `{}`", expr.name),
            expr.line,
        ));
    }
}

fn resolve_error(message: String, line: usize) -> Diagnostic {
    Diagnostic::new(
        ErrorCode::NameResolutionError,
        DiagnosticSegment::FrontendResolve,
        message,
    )
    .with_span(Span::new(line, 1))
    .with_help("declare the symbol before referencing it")
    .with_best_practice("keep state field names unique within a model")
}

#[cfg(test)]
mod tests {
    use crate::frontend::parser::parse_model;

    use super::resolve_model;

    #[test]
    fn reports_unresolved_init_field() {
        let source = r#"
model CounterLock
state:
  x: u8[0..7]
init:
  y = 0
"#;

        let parsed = parse_model(source).expect("parse should succeed");
        let errors = resolve_model(parsed).expect_err("must fail");
        assert_eq!(errors[0].error_code.as_str(), "NAME_RESOLUTION_ERROR");
    }
}
