use crate::{
    frontend::typecheck::TypedModel,
    ir::{
        ActionIr, ActionRole, BinaryOp, ExprIr, FieldType, InitAssignment, ModelIr, PredicateIr,
        PropertyIr, PropertyKind, ScenarioIr, SourceSpan, StateField, UpdateIr, Value,
    },
    support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode, Span},
};
use std::collections::{BTreeMap, BTreeSet};

pub fn lower_model(typed: TypedModel) -> Result<ModelIr, Vec<Diagnostic>> {
    let parsed = &typed.resolved.parsed;
    let mut errors = Vec::new();
    let predicate_defs = parsed
        .predicates
        .iter()
        .map(|predicate| (predicate.name.clone(), predicate.expr.clone()))
        .collect::<BTreeMap<_, _>>();

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
            Some(expr) => match lower_expr(expr, &predicate_defs, ExprMode::Current) {
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
            match lower_expr(&post.expr, &predicate_defs, ExprMode::Current) {
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
            role: ActionRole::parse(&action.role).unwrap_or(ActionRole::Business),
            reads,
            writes,
            path_tags: crate::modeling::decision_path_tags(
                &[],
                &action.name,
                action.posts.iter().map(|post| post.field.as_str()),
                action.posts.iter().map(|post| post.field.as_str()),
                action.pre.as_deref(),
                Some(
                    &action
                        .posts
                        .iter()
                        .map(|post| format!("{}={}", post.field, post.expr))
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
            ),
            guard,
            updates,
        });
    }

    let predicates = parsed
        .predicates
        .iter()
        .filter_map(|predicate| {
            match lower_expr(&predicate.expr, &predicate_defs, ExprMode::Current) {
                Some(expr) => Some(PredicateIr {
                    predicate_id: predicate.name.clone(),
                    expr,
                }),
                None => {
                    errors.push(lowering_error(
                        format!("unsupported predicate expression `{}`", predicate.expr),
                        predicate.line,
                    ));
                    None
                }
            }
        })
        .collect::<Vec<_>>();

    let scenarios = parsed
        .scenarios
        .iter()
        .filter_map(|scenario| {
            match lower_expr(&scenario.expr, &predicate_defs, ExprMode::Current) {
                Some(expr) => Some(ScenarioIr {
                    scenario_id: scenario.name.clone(),
                    expr,
                }),
                None => {
                    errors.push(lowering_error(
                        format!("unsupported scenario expression `{}`", scenario.expr),
                        scenario.line,
                    ));
                    None
                }
            }
        })
        .collect::<Vec<_>>();

    let mut properties = Vec::new();
    for property in &parsed.properties {
        match lower_property_kind(&property.kind) {
            Some(PropertyKind::Invariant)
            | Some(PropertyKind::Reachability)
            | Some(PropertyKind::Cover)
            | Some(PropertyKind::Temporal) => {
                match lower_expr(&property.expr, &predicate_defs, ExprMode::Current) {
                    Some(expr) => {
                        if let Some(scope_expr) = &property.scope_expr {
                            if let Some(scope) =
                                lower_expr(scope_expr, &predicate_defs, ExprMode::Current)
                            {
                                properties.push(PropertyIr {
                                    property_id: property.name.clone(),
                                    kind: PropertyKind::parse(&property.kind).unwrap(),
                                    expr,
                                    scope: Some(scope),
                                    action_filter: None,
                                });
                            } else {
                                errors.push(lowering_error(
                                    format!(
                                        "unsupported property scope expression `{}`",
                                        property.scope_expr.as_deref().unwrap_or_default()
                                    ),
                                    property.line,
                                ));
                            }
                        } else {
                            properties.push(PropertyIr {
                                property_id: property.name.clone(),
                                kind: PropertyKind::parse(&property.kind).unwrap(),
                                expr,
                                scope: None,
                                action_filter: None,
                            });
                        }
                    }
                    None => errors.push(lowering_error(
                        format!("unsupported property expression `{}`", property.expr),
                        property.line,
                    )),
                }
            }
            Some(PropertyKind::Transition) => {
                let expr = lower_expr(&property.expr, &predicate_defs, ExprMode::Transition);
                let scope = property
                    .scope_expr
                    .as_deref()
                    .map(|scope| lower_expr(scope, &predicate_defs, ExprMode::Transition));
                match (expr, scope) {
                    (Some(expr), Some(Some(scope))) => properties.push(PropertyIr {
                        property_id: property.name.clone(),
                        kind: PropertyKind::Transition,
                        expr,
                        scope: Some(scope),
                        action_filter: property.action_filter.clone(),
                    }),
                    (Some(expr), None) => properties.push(PropertyIr {
                        property_id: property.name.clone(),
                        kind: PropertyKind::Transition,
                        expr,
                        scope: None,
                        action_filter: property.action_filter.clone(),
                    }),
                    (None, _) => errors.push(lowering_error(
                        format!("unsupported property expression `{}`", property.expr),
                        property.line,
                    )),
                    (_, Some(None)) => errors.push(lowering_error(
                        format!(
                            "unsupported property scope expression `{}`",
                            property.scope_expr.as_deref().unwrap_or_default()
                        ),
                        property.line,
                    )),
                }
            }
            Some(PropertyKind::DeadlockFreedom) => properties.push(PropertyIr {
                property_id: property.name.clone(),
                kind: PropertyKind::DeadlockFreedom,
                expr: ExprIr::Literal(Value::Bool(true)),
                scope: property
                    .scope_expr
                    .as_deref()
                    .and_then(|scope| lower_expr(scope, &predicate_defs, ExprMode::Current)),
                action_filter: None,
            }),
            None => errors.push(lowering_error(
                format!("unsupported property kind `{}`", property.kind),
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
            predicates,
            scenarios,
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

    if input.starts_with("u16[") && input.ends_with(']') {
        let range = &input[4..input.len() - 1];
        let (min, max) = range.split_once("..")?;
        return Some(FieldType::BoundedU16 {
            min: min.parse().ok()?,
            max: max.parse().ok()?,
        });
    }

    if input.starts_with("u32[") && input.ends_with(']') {
        let range = &input[4..input.len() - 1];
        let (min, max) = range.split_once("..")?;
        return Some(FieldType::BoundedU32 {
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

fn lower_property_kind(input: &str) -> Option<PropertyKind> {
    PropertyKind::parse(input)
}

#[derive(Clone, Copy)]
enum ExprMode {
    Current,
    Transition,
}

fn lower_expr(
    input: &str,
    predicates: &BTreeMap<String, String>,
    mode: ExprMode,
) -> Option<ExprIr> {
    lower_expr_inner(input, predicates, mode, &mut BTreeSet::new())
}

fn lower_expr_inner(
    input: &str,
    predicates: &BTreeMap<String, String>,
    mode: ExprMode,
    seen_predicates: &mut BTreeSet<String>,
) -> Option<ExprIr> {
    let trimmed = strip_wrapping_parens(input.trim());
    if let Some(value) = lower_value(trimmed) {
        return Some(ExprIr::Literal(value));
    }
    if let Some(expr) = lower_predicate_ref(trimmed, predicates, mode, seen_predicates) {
        return Some(expr);
    }
    if let Some([left, right]) = function_args(trimmed, "implies") {
        return Some(ExprIr::Binary {
            op: BinaryOp::Or,
            left: Box::new(ExprIr::Unary {
                op: crate::ir::UnaryOp::Not,
                expr: Box::new(lower_expr_inner(
                    left.trim(),
                    predicates,
                    mode,
                    seen_predicates,
                )?),
            }),
            right: Box::new(lower_expr_inner(
                right.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some([left, right]) = function_args(trimmed, "iff") {
        let left_expr = lower_expr_inner(left.trim(), predicates, mode, seen_predicates)?;
        let right_expr = lower_expr_inner(right.trim(), predicates, mode, seen_predicates)?;
        let both = ExprIr::Binary {
            op: BinaryOp::And,
            left: Box::new(left_expr.clone()),
            right: Box::new(right_expr.clone()),
        };
        let neither = ExprIr::Binary {
            op: BinaryOp::And,
            left: Box::new(ExprIr::Unary {
                op: crate::ir::UnaryOp::Not,
                expr: Box::new(left_expr),
            }),
            right: Box::new(ExprIr::Unary {
                op: crate::ir::UnaryOp::Not,
                expr: Box::new(right_expr),
            }),
        };
        return Some(ExprIr::Binary {
            op: BinaryOp::Or,
            left: Box::new(both),
            right: Box::new(neither),
        });
    }
    if let Some([left, right]) = function_args(trimmed, "xor") {
        let left_expr = lower_expr_inner(left.trim(), predicates, mode, seen_predicates)?;
        let right_expr = lower_expr_inner(right.trim(), predicates, mode, seen_predicates)?;
        let either = ExprIr::Binary {
            op: BinaryOp::Or,
            left: Box::new(left_expr.clone()),
            right: Box::new(right_expr.clone()),
        };
        let both = ExprIr::Binary {
            op: BinaryOp::And,
            left: Box::new(left_expr),
            right: Box::new(right_expr),
        };
        return Some(ExprIr::Binary {
            op: BinaryOp::And,
            left: Box::new(either),
            right: Box::new(ExprIr::Unary {
                op: crate::ir::UnaryOp::Not,
                expr: Box::new(both),
            }),
        });
    }
    if let Some([inner]) = function_args(trimmed, "always") {
        return Some(ExprIr::Unary {
            op: crate::ir::UnaryOp::TemporalAlways,
            expr: Box::new(lower_expr_inner(
                inner.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some([inner]) = function_args(trimmed, "eventually") {
        return Some(ExprIr::Unary {
            op: crate::ir::UnaryOp::TemporalEventually,
            expr: Box::new(lower_expr_inner(
                inner.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some([inner]) = function_args(trimmed, "next") {
        return Some(ExprIr::Unary {
            op: crate::ir::UnaryOp::TemporalNext,
            expr: Box::new(lower_expr_inner(
                inner.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some([left, right]) = function_args(trimmed, "until") {
        return Some(ExprIr::Binary {
            op: BinaryOp::TemporalUntil,
            left: Box::new(lower_expr_inner(
                left.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
            right: Box::new(lower_expr_inner(
                right.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some(rest) = trimmed.strip_prefix('!') {
        return Some(ExprIr::Unary {
            op: crate::ir::UnaryOp::Not,
            expr: Box::new(lower_expr_inner(
                rest.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some((left, right)) = split_top_level(trimmed, "||") {
        return Some(ExprIr::Binary {
            op: BinaryOp::Or,
            left: Box::new(lower_expr_inner(
                left.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
            right: Box::new(lower_expr_inner(
                right.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some((left, right)) = split_top_level(trimmed, "&&") {
        return Some(ExprIr::Binary {
            op: BinaryOp::And,
            left: Box::new(lower_expr_inner(
                left.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
            right: Box::new(lower_expr_inner(
                right.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some((left, right)) = split_top_level(trimmed, "!=") {
        return Some(ExprIr::Binary {
            op: BinaryOp::NotEqual,
            left: Box::new(lower_expr_inner(
                left.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
            right: Box::new(lower_expr_inner(
                right.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some((left, right)) = split_top_level(trimmed, ">=") {
        return Some(ExprIr::Binary {
            op: BinaryOp::GreaterThanOrEqual,
            left: Box::new(lower_expr_inner(
                left.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
            right: Box::new(lower_expr_inner(
                right.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some((left, right)) = split_top_level(trimmed, "<=") {
        return Some(ExprIr::Binary {
            op: BinaryOp::LessThanOrEqual,
            left: Box::new(lower_expr_inner(
                left.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
            right: Box::new(lower_expr_inner(
                right.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some((left, right)) = split_top_level(trimmed, ">") {
        return Some(ExprIr::Binary {
            op: BinaryOp::GreaterThan,
            left: Box::new(lower_expr_inner(
                left.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
            right: Box::new(lower_expr_inner(
                right.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some((left, right)) = split_top_level(trimmed, "<") {
        return Some(ExprIr::Binary {
            op: BinaryOp::LessThan,
            left: Box::new(lower_expr_inner(
                left.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
            right: Box::new(lower_expr_inner(
                right.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some((left, right)) = split_top_level(trimmed, "==") {
        return Some(ExprIr::Binary {
            op: BinaryOp::Equal,
            left: Box::new(lower_expr_inner(
                left.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
            right: Box::new(lower_expr_inner(
                right.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some((left, right)) = split_top_level(trimmed, "-") {
        return Some(ExprIr::Binary {
            op: BinaryOp::Sub,
            left: Box::new(lower_expr_inner(
                left.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
            right: Box::new(lower_expr_inner(
                right.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some((left, right)) = split_top_level(trimmed, "%") {
        return Some(ExprIr::Binary {
            op: BinaryOp::Mod,
            left: Box::new(lower_expr_inner(
                left.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
            right: Box::new(lower_expr_inner(
                right.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some((left, right)) = split_top_level(trimmed, "+") {
        return Some(ExprIr::Binary {
            op: BinaryOp::Add,
            left: Box::new(lower_expr_inner(
                left.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
            right: Box::new(lower_expr_inner(
                right.trim(),
                predicates,
                mode,
                seen_predicates,
            )?),
        });
    }
    if let Some(field_ref) = lower_field_ref(trimmed, mode) {
        return Some(field_ref);
    }
    None
}

fn lower_predicate_ref(
    input: &str,
    predicates: &BTreeMap<String, String>,
    mode: ExprMode,
    seen_predicates: &mut BTreeSet<String>,
) -> Option<ExprIr> {
    let expr = predicates.get(input)?;
    if !seen_predicates.insert(input.to_string()) {
        return None;
    }
    let lowered = lower_expr_inner(expr, predicates, mode, seen_predicates);
    seen_predicates.remove(input);
    lowered
}

fn lower_field_ref(input: &str, mode: ExprMode) -> Option<ExprIr> {
    if input
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Some(ExprIr::FieldRef(input.to_string()));
    }
    if matches!(mode, ExprMode::Transition) {
        if let Some(field) = input.strip_prefix("prev.") {
            return Some(ExprIr::FieldRef(format!("prev_{}", field.trim())));
        }
        if let Some(field) = input.strip_prefix("next.") {
            return Some(ExprIr::FieldRef(format!("next_{}", field.trim())));
        }
    }
    None
}

fn function_args<'a, const N: usize>(input: &'a str, name: &str) -> Option<[&'a str; N]> {
    let call = input
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix('('))
        .and_then(|rest| rest.strip_suffix(')'))?;
    let parts = split_top_level_args(call);
    if parts.len() != N {
        return None;
    }
    Some(std::array::from_fn(|index| parts[index].trim()))
}

fn strip_wrapping_parens(input: &str) -> &str {
    let mut current = input.trim();
    loop {
        if !(current.starts_with('(') && current.ends_with(')')) {
            return current;
        }
        let mut depth = 0usize;
        let mut wraps = true;
        for (index, ch) in current.char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 && index != current.len() - 1 {
                        wraps = false;
                        break;
                    }
                }
                _ => {}
            }
        }
        if wraps {
            current = current[1..current.len() - 1].trim();
        } else {
            return current;
        }
    }
}

fn split_top_level<'a>(input: &'a str, needle: &str) -> Option<(&'a str, &'a str)> {
    let mut depth = 0usize;
    let bytes = input.as_bytes();
    let needle_bytes = needle.as_bytes();
    let mut index = 0usize;
    while index + needle_bytes.len() <= bytes.len() {
        match bytes[index] as char {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        if depth == 0 && bytes[index..].starts_with(needle_bytes) {
            let left = &input[..index];
            let right = &input[index + needle.len()..];
            return Some((left, right));
        }
        index += 1;
    }
    None
}

fn split_top_level_args(input: &str) -> Vec<&str> {
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut parts = Vec::new();
    for (index, ch) in input.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(input[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(input[start..].trim());
    parts
}

fn lowering_error(message: String, line: usize) -> Diagnostic {
    Diagnostic::new(
        ErrorCode::UnsupportedExpr,
        DiagnosticSegment::FrontendLowering,
        message,
    )
    .with_span(Span::new(line, 1))
    .with_help("rewrite the expression into the MVP expression subset")
    .with_best_practice("keep expressions explicit and within the MVP bool/arithmetic subset")
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
    fn lowers_modulo_expr_in_property() {
        let source = r#"
model Fizz
state:
  x: u8[0..15]
init:
  x = 0
action Step:
  pre: x < 15
  post:
    x = x + 1
property P_MOD:
  invariant: x % 3 != 1
"#;

        let model = compile_model(source).expect("compile");
        let debug = format!("{:?}", model.properties[0].expr);
        assert!(debug.contains("Mod"));
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

    #[test]
    fn lowers_extended_comparison_expressions() {
        let source = r#"
model RefundLike
state:
  risk: u8[0..7]
  approved: bool
init:
  risk = 1
  approved = false
action Escalate:
  pre: risk - 1 > 0 && approved != true && risk >= 1 && risk < 5
  post:
    risk = risk - 1
property P_SAFE:
  invariant: risk >= 0 && approved != true
"#;
        let model = compile_model(source).expect("compile");
        let guard = format!("{:?}", model.actions[0].guard);
        assert!(guard.contains("Sub"));
        assert!(guard.contains("GreaterThan"));
        assert!(guard.contains("NotEqual"));
        assert!(guard.contains("GreaterThanOrEqual"));
        assert!(guard.contains("LessThan"));
    }

    #[test]
    fn lowers_u16_state_fields() {
        let source = r#"
model BudgetControl
state:
  spend: u16[0..5000]
  approved: bool
init:
  spend = 0
  approved = false
action Raise:
  pre: spend <= 4500
  post:
    spend = spend + 500
property P_SAFE:
  invariant: spend <= 5000
"#;
        let model = compile_model(source).expect("compile");
        let field = &model.state_fields[0];
        assert_eq!(field.name, "spend");
        assert!(matches!(
            field.ty,
            crate::ir::FieldType::BoundedU16 { min: 0, max: 5000 }
        ));
    }

    #[test]
    fn lowers_u32_state_fields() {
        let source = r#"
model CostControl
state:
  spend: u32[0..500000]
  approved: bool
init:
  spend = 0
  approved = false
action Raise:
  pre: spend <= 499000
  post:
    spend = spend + 1000
property P_SAFE:
  invariant: spend <= 500000
"#;
        let model = compile_model(source).expect("compile");
        let field = &model.state_fields[0];
        assert_eq!(field.name, "spend");
        assert!(matches!(
            field.ty,
            crate::ir::FieldType::BoundedU32 {
                min: 0,
                max: 500000
            }
        ));
    }

    #[test]
    fn lowers_reachability_properties() {
        let source = r#"
model DoorControl
state:
  open: bool
init:
  open = false
property P_OPEN:
  reachability: open == true
"#;

        let model = compile_model(source).expect("compile");
        assert_eq!(
            model.properties[0].kind,
            crate::ir::PropertyKind::Reachability
        );
    }
    #[test]
    fn lowers_deadlock_freedom_property_kind() {
        let source = "model CounterLock\nstate:\n  x: u8[0..1]\ninit:\n  x = 0\nproperty P_LIVE: deadlock_freedom\n";
        let model = compile_model(source).expect("compile");
        assert_eq!(model.properties[0].property_id, "P_LIVE");
        assert_eq!(
            model.properties[0].kind,
            crate::ir::PropertyKind::DeadlockFreedom
        );
        assert_eq!(
            model.properties[0].expr,
            crate::ir::ExprIr::Literal(crate::ir::Value::Bool(true))
        );
    }

    #[test]
    fn lowers_temporal_properties() {
        let source = r#"
model DoorControl
state:
  open: bool
init:
  open = false
property P_TEMP:
  temporal: until(!open, open == true)
"#;
        let model = compile_model(source).expect("compile");
        assert_eq!(model.properties[0].kind, crate::ir::PropertyKind::Temporal);
        let debug = format!("{:?}", model.properties[0].expr);
        assert!(debug.contains("TemporalUntil"));
    }
}
