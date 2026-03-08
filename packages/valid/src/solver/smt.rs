use crate::ir::{BinaryOp, ExprIr, FieldType, ModelIr, PropertyKind, StateField, UnaryOp, Value};
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtCliDialect {
    Cvc5,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmtQuery {
    pub check_smtlib: String,
    pub model_smtlib: String,
    pub action_symbols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmtSolveStatus {
    Sat(Vec<String>),
    Unsat,
    Unknown,
}

pub fn run_bounded_invariant_check(
    executable: &str,
    args: &[String],
    run_id: &str,
    dialect: SmtCliDialect,
    model: &ModelIr,
    target_property_ids: &[String],
    horizon: usize,
) -> Result<SmtSolveStatus, String> {
    for depth in 0..=horizon {
        let query = build_invariant_bmc_query(model, target_property_ids, depth)?;
        let body = run_smt_query(executable, args, run_id, &query.check_smtlib)?;
        match parse_check_sat_status(dialect, &body)? {
            SmtSolveStatus::Sat(_) => {
                let model_body = run_smt_query(executable, args, run_id, &query.model_smtlib)?;
                let actions = parse_sat_model(dialect, &model_body, model, &query.action_symbols)?;
                return Ok(SmtSolveStatus::Sat(actions));
            }
            SmtSolveStatus::Unsat => continue,
            SmtSolveStatus::Unknown => return Ok(SmtSolveStatus::Unknown),
        }
    }
    Ok(SmtSolveStatus::Unsat)
}

fn run_smt_query(
    executable: &str,
    args: &[String],
    run_id: &str,
    smtlib: &str,
) -> Result<String, String> {
    let mut child = Command::new(executable)
        .args(args)
        .env("VALID_RUN_ID", run_id)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("failed to execute SMT solver command: {err}"))?;
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "failed to open stdin for SMT solver command".to_string())?;
        stdin
            .write_all(smtlib.as_bytes())
            .map_err(|err| format!("failed to write SMT-LIB to solver stdin: {err}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("failed to read SMT solver output: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "SMT solver command failed with status {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|err| format!("SMT solver output was not utf8: {err}"))
}

pub fn build_invariant_bmc_query(
    model: &ModelIr,
    target_property_ids: &[String],
    depth: usize,
) -> Result<SmtQuery, String> {
    let property_id = target_property_ids
        .first()
        .ok_or_else(|| "missing target property for SMT query".to_string())?;
    let property = model
        .properties
        .iter()
        .find(|property| &property.property_id == property_id)
        .ok_or_else(|| format!("unknown property `{property_id}`"))?;
    if property.kind != PropertyKind::Invariant {
        return Err(format!(
            "SMT adapter only supports invariant properties, got `{}`",
            property.kind
        ));
    }

    if depth > 0 && model.actions.is_empty() {
        return Err(
            "SMT adapter cannot build a transition query for a model without actions".to_string(),
        );
    }
    if model
        .state_fields
        .iter()
        .any(|field| matches!(field.ty, FieldType::String { .. }))
    {
        return Err(
            "SMT adapter does not yet support string fields; use explicit backend".to_string(),
        );
    }

    let mut smtlib = String::new();
    smtlib.push_str("(set-logic QF_LIA)\n");
    smtlib.push_str("(set-option :produce-models true)\n");

    for step in 0..=depth {
        for field in &model.state_fields {
            declare_state_symbols(&mut smtlib, field, step);
        }
    }

    let action_symbols = (0..depth).map(action_symbol).collect::<Vec<_>>();
    for symbol in &action_symbols {
        smtlib.push_str(&format!("(declare-fun {symbol} () Int)\n"));
    }

    for step in 0..=depth {
        for field in &model.state_fields {
            assert_state_bounds(&mut smtlib, field, step);
        }
    }

    for init in &model.init {
        let field = model
            .state_fields
            .iter()
            .find(|field| field.id == init.field)
            .ok_or_else(|| format!("unknown init field `{}`", init.field))?;
        for constraint in render_init_constraints(field, &init.value, 0)? {
            smtlib.push_str(&format!("(assert {constraint})\n"));
        }
    }

    for step in 0..depth {
        let selector = action_symbol(step);
        smtlib.push_str(&format!(
            "(assert (and (<= 0 {selector}) (<= {selector} {})))\n",
            model.actions.len() - 1
        ));
        let transitions = model
            .actions
            .iter()
            .enumerate()
            .map(|(index, action)| {
                let mut conjuncts = vec![
                    format!("(= {selector} {index})"),
                    render_expr(model, &action.guard, step)?,
                ];
                for field in &model.state_fields {
                    let default_expr = ExprIr::FieldRef(field.id.clone());
                    let next_expr = action
                        .updates
                        .iter()
                        .find(|update| update.field == field.id)
                        .map(|update| &update.value)
                        .unwrap_or(&default_expr);
                    conjuncts.extend(render_field_assignment_constraints(
                        model,
                        field,
                        next_expr,
                        step,
                        step + 1,
                    )?);
                }
                Ok::<_, String>(format!("(and {})", conjuncts.join(" ")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        smtlib.push_str(&format!("(assert (or {}))\n", transitions.join(" ")));
    }

    for step in 0..depth {
        smtlib.push_str(&format!(
            "(assert {})\n",
            render_expr(model, &property.expr, step)?
        ));
    }
    smtlib.push_str(&format!(
        "(assert (not {}))\n",
        render_expr(model, &property.expr, depth)?
    ));

    let mut check_smtlib = smtlib.clone();
    check_smtlib.push_str("(check-sat)\n");
    check_smtlib.push_str("(exit)\n");

    smtlib.push_str("(check-sat)\n");
    for symbol in &action_symbols {
        smtlib.push_str(&format!("(get-value ({symbol}))\n"));
    }
    smtlib.push_str("(exit)\n");

    Ok(SmtQuery {
        check_smtlib,
        model_smtlib: smtlib,
        action_symbols,
    })
}

fn parse_check_sat_status(dialect: SmtCliDialect, body: &str) -> Result<SmtSolveStatus, String> {
    match dialect {
        SmtCliDialect::Cvc5 => {
            let status = body
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .ok_or_else(|| "solver produced empty output".to_string())?;
            match status {
                "sat" => Ok(SmtSolveStatus::Sat(Vec::new())),
                "unsat" => Ok(SmtSolveStatus::Unsat),
                "unknown" => Ok(SmtSolveStatus::Unknown),
                other => Err(format!("unsupported SMT status `{other}`")),
            }
        }
    }
}

fn parse_sat_model(
    dialect: SmtCliDialect,
    body: &str,
    model: &ModelIr,
    action_symbols: &[String],
) -> Result<Vec<String>, String> {
    match dialect {
        SmtCliDialect::Cvc5 => parse_cvc5_sat_model(body, model, action_symbols),
    }
}

fn parse_cvc5_sat_model(
    body: &str,
    model: &ModelIr,
    action_symbols: &[String],
) -> Result<Vec<String>, String> {
    let mut lines = body.lines().map(str::trim).filter(|line| !line.is_empty());
    let status = lines
        .next()
        .ok_or_else(|| "solver produced empty output".to_string())?;
    if status != "sat" {
        return Err(format!("expected sat before parsing model, got `{status}`"));
    }

    let action_indexes = action_symbols
        .iter()
        .map(|symbol| {
            let value_line = lines
                .next()
                .ok_or_else(|| format!("missing get-value output for `{symbol}`"))?;
            let (name, value) = parse_cvc5_get_value_line(value_line)?;
            if name != *symbol {
                return Err(format!("expected get-value for `{symbol}`, got `{name}`"));
            }
            value
                .parse::<usize>()
                .map_err(|err| format!("invalid solver action index `{value}`: {err}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    action_indexes
        .into_iter()
        .map(|index| {
            model
                .actions
                .get(index)
                .map(|action| action.action_id.clone())
                .ok_or_else(|| format!("solver returned unknown action index `{index}`"))
        })
        .collect()
}

fn parse_cvc5_get_value_line(line: &str) -> Result<(String, String), String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("((") || !trimmed.ends_with("))") {
        return Err(format!("unsupported cvc5 get-value output `{trimmed}`"));
    }
    let inner = &trimmed[2..trimmed.len() - 2];
    let mut parts = inner.split_whitespace();
    let name = parts
        .next()
        .ok_or_else(|| format!("missing symbol in get-value output `{trimmed}`"))?;
    let value = parts
        .next()
        .ok_or_else(|| format!("missing value in get-value output `{trimmed}`"))?;
    Ok((name.to_string(), value.to_string()))
}

fn smt_sort(ty: &FieldType) -> &'static str {
    match ty {
        FieldType::Bool => "Bool",
        FieldType::String { .. } => "String",
        FieldType::BoundedU8 { .. }
        | FieldType::BoundedU16 { .. }
        | FieldType::BoundedU32 { .. }
        | FieldType::Enum { .. }
        | FieldType::EnumSet { .. } => "Int",
        FieldType::EnumRelation { .. } | FieldType::EnumMap { .. } => {
            panic!("relation/map fields expand into dedicated SMT symbols")
        }
    }
}

fn integer_bounds(ty: &FieldType) -> Option<(u64, u64)> {
    match ty {
        FieldType::Bool => None,
        FieldType::String { .. } => None,
        FieldType::BoundedU8 { min, max } => Some((*min as u64, *max as u64)),
        FieldType::BoundedU16 { min, max } => Some((*min as u64, *max as u64)),
        FieldType::BoundedU32 { min, max } => Some((*min as u64, *max as u64)),
        FieldType::Enum { variants } => variants.len().checked_sub(1).map(|max| (0, max as u64)),
        FieldType::EnumSet { variants } => {
            if variants.len() > 64 {
                None
            } else if variants.len() == 64 {
                Some((0, u64::MAX))
            } else {
                Some((0, (1u64 << variants.len()) - 1))
            }
        }
        FieldType::EnumRelation { .. } | FieldType::EnumMap { .. } => None,
    }
}

fn state_symbol(field_name: &str, step: usize) -> String {
    format!("{field_name}_{step}")
}

fn relation_slot_symbol(
    field_name: &str,
    step: usize,
    left_index: usize,
    right_index: usize,
) -> String {
    format!("{field_name}_{step}_pair_{left_index}_{right_index}")
}

fn map_present_symbol(field_name: &str, step: usize, key_index: usize) -> String {
    format!("{field_name}_{step}_key_{key_index}_present")
}

fn map_value_symbol(field_name: &str, step: usize, key_index: usize) -> String {
    format!("{field_name}_{step}_key_{key_index}_value")
}

fn action_symbol(step: usize) -> String {
    format!("action_{step}")
}

fn declare_state_symbols(smtlib: &mut String, field: &StateField, step: usize) {
    match &field.ty {
        FieldType::EnumRelation {
            left_variants,
            right_variants,
        } => {
            for left_index in 0..left_variants.len() {
                for right_index in 0..right_variants.len() {
                    smtlib.push_str(&format!(
                        "(declare-fun {} () Bool)\n",
                        relation_slot_symbol(field.name.as_str(), step, left_index, right_index)
                    ));
                }
            }
        }
        FieldType::EnumMap {
            key_variants,
            value_variants: _,
        } => {
            for key_index in 0..key_variants.len() {
                smtlib.push_str(&format!(
                    "(declare-fun {} () Bool)\n",
                    map_present_symbol(field.name.as_str(), step, key_index)
                ));
                smtlib.push_str(&format!(
                    "(declare-fun {} () Int)\n",
                    map_value_symbol(field.name.as_str(), step, key_index)
                ));
            }
        }
        _ => {
            smtlib.push_str(&format!(
                "(declare-fun {} () {})\n",
                state_symbol(field.name.as_str(), step),
                smt_sort(&field.ty)
            ));
        }
    }
}

fn assert_state_bounds(smtlib: &mut String, field: &StateField, step: usize) {
    match &field.ty {
        FieldType::EnumMap {
            key_variants,
            value_variants,
        } => {
            for key_index in 0..key_variants.len() {
                if let Some(max) = value_variants.len().checked_sub(1) {
                    let value_symbol = map_value_symbol(field.name.as_str(), step, key_index);
                    smtlib.push_str(&format!("(assert (<= 0 {value_symbol}))\n"));
                    smtlib.push_str(&format!("(assert (<= {value_symbol} {}))\n", max));
                }
                let present_symbol = map_present_symbol(field.name.as_str(), step, key_index);
                let value_symbol = map_value_symbol(field.name.as_str(), step, key_index);
                smtlib.push_str(&format!(
                    "(assert (=> (not {present_symbol}) (= {value_symbol} 0)))\n"
                ));
            }
        }
        _ => {
            if let Some((min, max)) = integer_bounds(&field.ty) {
                let symbol = state_symbol(field.name.as_str(), step);
                smtlib.push_str(&format!("(assert (<= {} {}))\n", min, symbol));
                smtlib.push_str(&format!("(assert (<= {} {}))\n", symbol, max));
            }
        }
    }
}

fn render_literal(value: &Value, expected_ty: Option<&FieldType>) -> Result<String, String> {
    match value {
        Value::Bool(value) => Ok(value.to_string()),
        Value::UInt(value) => match expected_ty {
            Some(FieldType::EnumRelation { .. }) | Some(FieldType::EnumMap { .. }) => Err(
                "relation/map literals must be decomposed through dedicated render helpers"
                    .to_string(),
            ),
            _ => Ok(value.to_string()),
        },
        Value::String(value) => Ok(format!("{value:?}")),
        Value::EnumVariant { index, .. } => Ok(index.to_string()),
        Value::PairVariant { .. } => Err(
            "pair literals are only supported inside relation/map helper expressions".to_string(),
        ),
    }
}

fn render_expr(model: &ModelIr, expr: &ExprIr, step: usize) -> Result<String, String> {
    render_expr_with_expected_type(model, expr, step, None)
}

fn render_expr_with_expected_type(
    model: &ModelIr,
    expr: &ExprIr,
    step: usize,
    expected_ty: Option<&FieldType>,
) -> Result<String, String> {
    match expr {
        ExprIr::Literal(value) => render_literal(value, expected_ty),
        ExprIr::FieldRef(field) => {
            let field = field_for_id(model, field)?;
            match field.ty {
                FieldType::EnumRelation { .. } | FieldType::EnumMap { .. } => Err(format!(
                    "relation/map field `{}` must be used through dedicated relation/map operators",
                    field.name
                )),
                _ => Ok(state_symbol(field.name.as_str(), step)),
            }
        }
        ExprIr::Unary { op, expr } => match op {
            UnaryOp::Not => Ok(format!(
                "(not {})",
                render_expr_with_expected_type(model, expr, step, expected_ty)?
            )),
            UnaryOp::SetIsEmpty => match field_type_for_expr(model, expr) {
                Some(FieldType::EnumRelation {
                    left_variants,
                    right_variants,
                }) => render_all(
                    (0..left_variants.len()).flat_map(|left_index| {
                        (0..right_variants.len()).map(move |right_index| {
                            Ok(format!(
                                "(not {})",
                                render_relation_slot_expr(
                                    model,
                                    expr,
                                    step,
                                    left_index,
                                    right_index,
                                    None,
                                )?
                            ))
                        })
                    }),
                    "and",
                    "true",
                ),
                Some(FieldType::EnumMap { key_variants, .. }) => render_all(
                    (0..key_variants.len()).map(|key_index| {
                        Ok(format!(
                            "(not {})",
                            render_map_presence_expr(model, expr, step, key_index, None)?
                        ))
                    }),
                    "and",
                    "true",
                ),
                _ => Ok(format!(
                    "(= {} 0)",
                    render_expr_with_expected_type(model, expr, step, expected_ty)?
                )),
            },
            UnaryOp::StringLen => Err(
                "SMT adapter does not yet support string length expressions; use explicit backend"
                    .to_string(),
            ),
            UnaryOp::TemporalAlways
            | UnaryOp::TemporalEventually
            | UnaryOp::TemporalNext => Err(
                "SMT adapter does not yet support temporal expressions; use explicit backend"
                    .to_string(),
            ),
        },
        ExprIr::Binary { op, left, right } => match op {
            BinaryOp::StringContains => Err(
                "SMT adapter does not yet support string contains expressions; use explicit backend"
                    .to_string(),
            ),
            BinaryOp::RegexMatch => Err(
                "SMT adapter does not yet support regex_match expressions; use explicit backend"
                    .to_string(),
            ),
            BinaryOp::TemporalUntil => Err(
                "SMT adapter does not yet support temporal expressions; use explicit backend"
                    .to_string(),
            ),
            BinaryOp::RelationContains => {
                let (left_index, right_index) = extract_pair_indexes(right.as_ref(), expr)?;
                render_relation_slot_expr(
                    model,
                    left,
                    step,
                    left_index as usize,
                    right_index as usize,
                    None,
                )
            }
            BinaryOp::RelationInsert | BinaryOp::RelationRemove => Err(
                "relation value expressions must appear in field assignments or equality checks"
                    .to_string(),
            ),
            BinaryOp::RelationIntersects => {
                match relation_type_for_expr(model, left, None)? {
                    FieldType::EnumRelation {
                        left_variants,
                        right_variants,
                    } => render_any(
                        (0..left_variants.len()).flat_map(|left_index| {
                            (0..right_variants.len()).map(move |right_index| {
                                Ok(format!(
                                    "(and {} {})",
                                    render_relation_slot_expr(
                                        model,
                                        left,
                                        step,
                                        left_index,
                                        right_index,
                                        None,
                                    )?,
                                    render_relation_slot_expr(
                                        model,
                                        right,
                                        step,
                                        left_index,
                                        right_index,
                                        None,
                                    )?
                                ))
                            })
                        }),
                        "false",
                    ),
                    _ => unreachable!(),
                }
            }
            BinaryOp::MapContainsKey => {
                let key_index = extract_enum_index_from_expr(right.as_ref(), expr)?;
                render_map_presence_expr(model, left, step, key_index as usize, None)
            }
            BinaryOp::MapContainsEntry => {
                let (key_index, value_index) = extract_pair_indexes(right.as_ref(), expr)?;
                let key_index = key_index as usize;
                Ok(format!(
                    "(and {} (= {} {}))",
                    render_map_presence_expr(model, left, step, key_index, None)?,
                    render_map_value_expr(model, left, step, key_index, None)?,
                    value_index
                ))
            }
            BinaryOp::MapPut | BinaryOp::MapRemoveKey => Err(
                "map value expressions must appear in field assignments or equality checks"
                    .to_string(),
            ),
            BinaryOp::Add => {
                let left = render_expr_with_expected_type(model, left, step, expected_ty)?;
                let right = render_expr_with_expected_type(model, right, step, expected_ty)?;
                Ok(format!("(+ {left} {right})"))
            }
            BinaryOp::Sub => {
                let left = render_expr_with_expected_type(model, left, step, expected_ty)?;
                let right = render_expr_with_expected_type(model, right, step, expected_ty)?;
                Ok(format!("(- {left} {right})"))
            }
            BinaryOp::Mod => {
                let left = render_expr_with_expected_type(model, left, step, expected_ty)?;
                let right = render_expr_with_expected_type(model, right, step, expected_ty)?;
                Ok(format!("(mod {left} {right})"))
            }
            BinaryOp::SetContains => {
                let left = render_expr_with_expected_type(model, left, step, expected_ty)?;
                let right = render_expr_with_expected_type(model, right, step, expected_ty)?;
                let index = extract_enum_index(right.as_str(), expr)?;
                Ok(format!(
                    "(= (mod (div {left} {}) 2) 1)",
                    enum_variant_mask(index)
                ))
            }
            BinaryOp::SetInsert => {
                let left = render_expr_with_expected_type(model, left, step, expected_ty)?;
                let right = render_expr_with_expected_type(model, right, step, expected_ty)?;
                let index = extract_enum_index(right.as_str(), expr)?;
                let mask = enum_variant_mask(index);
                Ok(format!(
                    "(+ {left} (* (- 1 (mod (div {left} {mask}) 2)) {mask}))"
                ))
            }
            BinaryOp::SetRemove => {
                let left = render_expr_with_expected_type(model, left, step, expected_ty)?;
                let right = render_expr_with_expected_type(model, right, step, expected_ty)?;
                let index = extract_enum_index(right.as_str(), expr)?;
                let mask = enum_variant_mask(index);
                Ok(format!("(- {left} (* (mod (div {left} {mask}) 2) {mask}))"))
            }
            BinaryOp::LessThan => {
                let left = render_expr_with_expected_type(model, left, step, expected_ty)?;
                let right = render_expr_with_expected_type(model, right, step, expected_ty)?;
                Ok(format!("(< {left} {right})"))
            }
            BinaryOp::LessThanOrEqual => {
                let left = render_expr_with_expected_type(model, left, step, expected_ty)?;
                let right = render_expr_with_expected_type(model, right, step, expected_ty)?;
                Ok(format!("(<= {left} {right})"))
            }
            BinaryOp::GreaterThan => {
                let left = render_expr_with_expected_type(model, left, step, expected_ty)?;
                let right = render_expr_with_expected_type(model, right, step, expected_ty)?;
                Ok(format!("(> {left} {right})"))
            }
            BinaryOp::GreaterThanOrEqual => {
                let left = render_expr_with_expected_type(model, left, step, expected_ty)?;
                let right = render_expr_with_expected_type(model, right, step, expected_ty)?;
                Ok(format!("(>= {left} {right})"))
            }
            BinaryOp::Equal => {
                if let Some(ty) = composite_type_for_exprs(model, left, right) {
                    render_equality_expr(model, left, right, step, ty, false)
                } else {
                    let left =
                        render_expr_with_expected_type(model, left, step, expected_ty)?;
                    let right =
                        render_expr_with_expected_type(model, right, step, expected_ty)?;
                    Ok(format!("(= {left} {right})"))
                }
            }
            BinaryOp::NotEqual => {
                if let Some(ty) = composite_type_for_exprs(model, left, right) {
                    render_equality_expr(model, left, right, step, ty, true)
                } else {
                    let left =
                        render_expr_with_expected_type(model, left, step, expected_ty)?;
                    let right =
                        render_expr_with_expected_type(model, right, step, expected_ty)?;
                    Ok(format!("(not (= {left} {right}))"))
                }
            }
            BinaryOp::And => {
                let left = render_expr_with_expected_type(model, left, step, expected_ty)?;
                let right = render_expr_with_expected_type(model, right, step, expected_ty)?;
                Ok(format!("(and {left} {right})"))
            }
            BinaryOp::Or => {
                let left = render_expr_with_expected_type(model, left, step, expected_ty)?;
                let right = render_expr_with_expected_type(model, right, step, expected_ty)?;
                Ok(format!("(or {left} {right})"))
            }
        },
    }
}

fn render_init_constraints(
    field: &StateField,
    value: &Value,
    step: usize,
) -> Result<Vec<String>, String> {
    match (&field.ty, value) {
        (
            FieldType::EnumRelation {
                left_variants,
                right_variants,
            },
            Value::UInt(bits),
        ) => {
            let mut constraints = Vec::new();
            for left_index in 0..left_variants.len() {
                for right_index in 0..right_variants.len() {
                    constraints.push(format!(
                        "(= {} {})",
                        relation_slot_symbol(field.name.as_str(), step, left_index, right_index),
                        relation_literal_contains(
                            *bits,
                            right_variants.len(),
                            left_index,
                            right_index
                        )
                    ));
                }
            }
            Ok(constraints)
        }
        (
            FieldType::EnumMap {
                key_variants,
                value_variants,
            },
            Value::UInt(bits),
        ) => {
            let mut constraints = Vec::new();
            for key_index in 0..key_variants.len() {
                let decoded = decode_map_literal(*bits, value_variants.len(), key_index)?;
                constraints.push(format!(
                    "(= {} {})",
                    map_present_symbol(field.name.as_str(), step, key_index),
                    bool_literal(decoded.is_some())
                ));
                constraints.push(format!(
                    "(= {} {})",
                    map_value_symbol(field.name.as_str(), step, key_index),
                    decoded.unwrap_or(0)
                ));
            }
            Ok(constraints)
        }
        _ => Ok(vec![format!(
            "(= {} {})",
            state_symbol(field.name.as_str(), step),
            render_literal(value, Some(&field.ty))?
        )]),
    }
}

fn render_field_assignment_constraints(
    model: &ModelIr,
    field: &StateField,
    expr: &ExprIr,
    source_step: usize,
    target_step: usize,
) -> Result<Vec<String>, String> {
    match &field.ty {
        FieldType::EnumRelation {
            left_variants,
            right_variants,
        } => {
            let mut constraints = Vec::new();
            for left_index in 0..left_variants.len() {
                for right_index in 0..right_variants.len() {
                    constraints.push(format!(
                        "(= {} {})",
                        relation_slot_symbol(
                            field.name.as_str(),
                            target_step,
                            left_index,
                            right_index
                        ),
                        render_relation_slot_expr(
                            model,
                            expr,
                            source_step,
                            left_index,
                            right_index,
                            Some(&field.ty),
                        )?
                    ));
                }
            }
            Ok(constraints)
        }
        FieldType::EnumMap {
            key_variants,
            value_variants: _,
        } => {
            let mut constraints = Vec::new();
            for key_index in 0..key_variants.len() {
                constraints.push(format!(
                    "(= {} {})",
                    map_present_symbol(field.name.as_str(), target_step, key_index),
                    render_map_presence_expr(model, expr, source_step, key_index, Some(&field.ty))?
                ));
                constraints.push(format!(
                    "(= {} {})",
                    map_value_symbol(field.name.as_str(), target_step, key_index),
                    render_map_value_expr(model, expr, source_step, key_index, Some(&field.ty))?
                ));
            }
            Ok(constraints)
        }
        _ => Ok(vec![format!(
            "(= {} {})",
            state_symbol(field.name.as_str(), target_step),
            render_expr_with_expected_type(model, expr, source_step, Some(&field.ty))?
        )]),
    }
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

fn field_for_id<'a>(model: &'a ModelIr, field_id: &str) -> Result<&'a StateField, String> {
    model
        .state_fields
        .iter()
        .find(|item| item.id == field_id)
        .ok_or_else(|| format!("unknown field reference `{field_id}`"))
}

fn composite_type_for_exprs<'a>(
    model: &'a ModelIr,
    left: &'a ExprIr,
    right: &'a ExprIr,
) -> Option<&'a FieldType> {
    field_type_for_expr(model, left)
        .or_else(|| field_type_for_expr(model, right))
        .filter(|ty| {
            matches!(
                ty,
                FieldType::EnumRelation { .. } | FieldType::EnumMap { .. }
            )
        })
}

fn relation_type_for_expr<'a>(
    model: &'a ModelIr,
    expr: &'a ExprIr,
    expected_ty: Option<&'a FieldType>,
) -> Result<&'a FieldType, String> {
    match expected_ty.or_else(|| field_type_for_expr(model, expr)) {
        Some(ty @ FieldType::EnumRelation { .. }) => Ok(ty),
        other => Err(format!(
            "relation operation requires a relation field, got `{other:?}`"
        )),
    }
}

fn map_type_for_expr<'a>(
    model: &'a ModelIr,
    expr: &'a ExprIr,
    expected_ty: Option<&'a FieldType>,
) -> Result<&'a FieldType, String> {
    match expected_ty.or_else(|| field_type_for_expr(model, expr)) {
        Some(ty @ FieldType::EnumMap { .. }) => Ok(ty),
        other => Err(format!(
            "map operation requires a map field, got `{other:?}`"
        )),
    }
}

fn render_relation_slot_expr(
    model: &ModelIr,
    expr: &ExprIr,
    step: usize,
    left_index: usize,
    right_index: usize,
    expected_ty: Option<&FieldType>,
) -> Result<String, String> {
    match expr {
        ExprIr::Literal(Value::UInt(bits)) => {
            match relation_type_for_expr(model, expr, expected_ty)? {
                FieldType::EnumRelation { right_variants, .. } => Ok(bool_literal(
                    relation_literal_contains(*bits, right_variants.len(), left_index, right_index),
                )
                .to_string()),
                _ => unreachable!(),
            }
        }
        ExprIr::FieldRef(field) => {
            let field = field_for_id(model, field)?;
            match field.ty {
                FieldType::EnumRelation { .. } => Ok(relation_slot_symbol(
                    field.name.as_str(),
                    step,
                    left_index,
                    right_index,
                )),
                _ => Err(format!(
                    "relation operation requires a relation field, got `{}`",
                    field.name
                )),
            }
        }
        ExprIr::Binary {
            op: BinaryOp::RelationInsert,
            left,
            right,
        } => {
            let (target_left, target_right) = extract_pair_indexes(right.as_ref(), expr)?;
            if left_index == target_left as usize && right_index == target_right as usize {
                Ok("true".to_string())
            } else {
                render_relation_slot_expr(model, left, step, left_index, right_index, expected_ty)
            }
        }
        ExprIr::Binary {
            op: BinaryOp::RelationRemove,
            left,
            right,
        } => {
            let (target_left, target_right) = extract_pair_indexes(right.as_ref(), expr)?;
            if left_index == target_left as usize && right_index == target_right as usize {
                Ok("false".to_string())
            } else {
                render_relation_slot_expr(model, left, step, left_index, right_index, expected_ty)
            }
        }
        other => Err(format!(
            "unsupported relation value expression `{other:?}` in SMT encoding"
        )),
    }
}

fn render_map_presence_expr(
    model: &ModelIr,
    expr: &ExprIr,
    step: usize,
    key_index: usize,
    expected_ty: Option<&FieldType>,
) -> Result<String, String> {
    match expr {
        ExprIr::Literal(Value::UInt(bits)) => match map_type_for_expr(model, expr, expected_ty)? {
            FieldType::EnumMap { value_variants, .. } => Ok(bool_literal(
                decode_map_literal(*bits, value_variants.len(), key_index)?.is_some(),
            )
            .to_string()),
            _ => unreachable!(),
        },
        ExprIr::FieldRef(field) => {
            let field = field_for_id(model, field)?;
            match field.ty {
                FieldType::EnumMap { .. } => {
                    Ok(map_present_symbol(field.name.as_str(), step, key_index))
                }
                _ => Err(format!(
                    "map operation requires a map field, got `{}`",
                    field.name
                )),
            }
        }
        ExprIr::Binary {
            op: BinaryOp::MapPut,
            left,
            right,
        } => {
            let (target_key, _) = extract_pair_indexes(right.as_ref(), expr)?;
            if key_index == target_key as usize {
                Ok("true".to_string())
            } else {
                render_map_presence_expr(model, left, step, key_index, expected_ty)
            }
        }
        ExprIr::Binary {
            op: BinaryOp::MapRemoveKey,
            left,
            right,
        } => {
            let target_key = extract_enum_index_from_expr(right.as_ref(), expr)?;
            if key_index == target_key as usize {
                Ok("false".to_string())
            } else {
                render_map_presence_expr(model, left, step, key_index, expected_ty)
            }
        }
        other => Err(format!(
            "unsupported map value expression `{other:?}` in SMT encoding"
        )),
    }
}

fn render_map_value_expr(
    model: &ModelIr,
    expr: &ExprIr,
    step: usize,
    key_index: usize,
    expected_ty: Option<&FieldType>,
) -> Result<String, String> {
    match expr {
        ExprIr::Literal(Value::UInt(bits)) => match map_type_for_expr(model, expr, expected_ty)? {
            FieldType::EnumMap { value_variants, .. } => {
                Ok(decode_map_literal(*bits, value_variants.len(), key_index)?
                    .unwrap_or(0)
                    .to_string())
            }
            _ => unreachable!(),
        },
        ExprIr::FieldRef(field) => {
            let field = field_for_id(model, field)?;
            match field.ty {
                FieldType::EnumMap { .. } => {
                    Ok(map_value_symbol(field.name.as_str(), step, key_index))
                }
                _ => Err(format!(
                    "map operation requires a map field, got `{}`",
                    field.name
                )),
            }
        }
        ExprIr::Binary {
            op: BinaryOp::MapPut,
            left,
            right,
        } => {
            let (target_key, target_value) = extract_pair_indexes(right.as_ref(), expr)?;
            if key_index == target_key as usize {
                Ok(target_value.to_string())
            } else {
                render_map_value_expr(model, left, step, key_index, expected_ty)
            }
        }
        ExprIr::Binary {
            op: BinaryOp::MapRemoveKey,
            left,
            right,
        } => {
            let target_key = extract_enum_index_from_expr(right.as_ref(), expr)?;
            if key_index == target_key as usize {
                Ok("0".to_string())
            } else {
                render_map_value_expr(model, left, step, key_index, expected_ty)
            }
        }
        other => Err(format!(
            "unsupported map value expression `{other:?}` in SMT encoding"
        )),
    }
}

fn render_equality_expr(
    model: &ModelIr,
    left: &ExprIr,
    right: &ExprIr,
    step: usize,
    ty: &FieldType,
    negate: bool,
) -> Result<String, String> {
    let equality = match ty {
        FieldType::EnumRelation {
            left_variants,
            right_variants,
        } => render_all(
            (0..left_variants.len()).flat_map(|left_index| {
                (0..right_variants.len()).map(move |right_index| {
                    Ok(format!(
                        "(= {} {})",
                        render_relation_slot_expr(
                            model,
                            left,
                            step,
                            left_index,
                            right_index,
                            Some(ty)
                        )?,
                        render_relation_slot_expr(
                            model,
                            right,
                            step,
                            left_index,
                            right_index,
                            Some(ty)
                        )?,
                    ))
                })
            }),
            "and",
            "true",
        )?,
        FieldType::EnumMap {
            key_variants,
            value_variants: _,
        } => {
            let mut parts = Vec::new();
            for key_index in 0..key_variants.len() {
                parts.push(format!(
                    "(= {} {})",
                    render_map_presence_expr(model, left, step, key_index, Some(ty))?,
                    render_map_presence_expr(model, right, step, key_index, Some(ty))?,
                ));
                parts.push(format!(
                    "(= {} {})",
                    render_map_value_expr(model, left, step, key_index, Some(ty))?,
                    render_map_value_expr(model, right, step, key_index, Some(ty))?,
                ));
            }
            if parts.is_empty() {
                "true".to_string()
            } else {
                format!("(and {})", parts.join(" "))
            }
        }
        _ => {
            return Err(format!(
                "composite equality requires a relation/map field type, got `{ty:?}`"
            ))
        }
    };
    if negate {
        Ok(format!("(not {equality})"))
    } else {
        Ok(equality)
    }
}

fn extract_enum_index(rendered_right: &str, expr: &ExprIr) -> Result<u64, String> {
    match expr {
        ExprIr::Binary { right, .. } => match &**right {
            ExprIr::Literal(Value::EnumVariant { index, .. }) => Ok(*index),
            other => Err(format!(
                "set operation requires a finite enum literal on the right-hand side, got `{other:?}` / `{rendered_right}`"
            )),
        },
        _ => Err("internal error extracting enum index for set operation".to_string()),
    }
}

fn extract_enum_index_from_expr(expr: &ExprIr, parent: &ExprIr) -> Result<u64, String> {
    match expr {
        ExprIr::Literal(Value::EnumVariant { index, .. }) => Ok(*index),
        other => Err(format!(
            "operation requires a finite enum literal, got `{other:?}` in `{parent:?}`"
        )),
    }
}

fn extract_pair_indexes(expr: &ExprIr, parent: &ExprIr) -> Result<(u64, u64), String> {
    match expr {
        ExprIr::Literal(Value::PairVariant {
            left_index,
            right_index,
            ..
        }) => Ok((*left_index, *right_index)),
        other => Err(format!(
            "relation/map operation requires a finite pair literal, got `{other:?}` in `{parent:?}`"
        )),
    }
}

fn enum_variant_mask(index: u64) -> u64 {
    1u64.checked_shl(index as u32).unwrap_or(0)
}

fn relation_literal_contains(
    bits: u64,
    right_len: usize,
    left_index: usize,
    right_index: usize,
) -> bool {
    let bit_index = left_index
        .checked_mul(right_len)
        .and_then(|value| value.checked_add(right_index))
        .unwrap_or(usize::MAX);
    bits & enum_variant_mask(bit_index as u64) != 0
}

fn decode_map_literal(
    bits: u64,
    value_len: usize,
    key_index: usize,
) -> Result<Option<u64>, String> {
    let mut found = None;
    for value_index in 0..value_len {
        if relation_literal_contains(bits, value_len, key_index, value_index) {
            if found.replace(value_index as u64).is_some() {
                return Err(format!(
                    "map literal encoded multiple values for key index `{key_index}`"
                ));
            }
        }
    }
    Ok(found)
}

fn render_all<I>(parts: I, op: &str, empty: &str) -> Result<String, String>
where
    I: IntoIterator<Item = Result<String, String>>,
{
    let rendered = parts.into_iter().collect::<Result<Vec<_>, _>>()?;
    if rendered.is_empty() {
        Ok(empty.to_string())
    } else {
        Ok(format!("({op} {})", rendered.join(" ")))
    }
}

fn render_any<I>(parts: I, empty: &str) -> Result<String, String>
where
    I: IntoIterator<Item = Result<String, String>>,
{
    render_all(parts, "or", empty)
}

fn bool_literal(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

#[cfg(test)]
mod tests {
    use super::{build_invariant_bmc_query, SmtCliDialect, SmtSolveStatus};
    use crate::{
        frontend::compile_model,
        ir::{
            ActionIr, BinaryOp, ExprIr, FieldType, InitAssignment, ModelIr, PropertyIr,
            PropertyKind, SourceSpan, StateField, UpdateIr, Value,
        },
    };

    fn relation_map_model() -> ModelIr {
        let span = SourceSpan { line: 1, column: 1 };
        let memberships = "memberships".to_string();
        let pending = "pending".to_string();
        let plans = "plans".to_string();
        let alpha_registered = "alpha_registered".to_string();
        let enterprise_alpha = "enterprise_alpha".to_string();
        let pair_alice_alpha = ExprIr::Literal(Value::PairVariant {
            left_label: "Alice".to_string(),
            left_index: 0,
            right_label: "Alpha".to_string(),
            right_index: 0,
        });
        let pair_beta_free = ExprIr::Literal(Value::PairVariant {
            left_label: "Beta".to_string(),
            left_index: 1,
            right_label: "Free".to_string(),
            right_index: 0,
        });
        let pair_alpha_enterprise = ExprIr::Literal(Value::PairVariant {
            left_label: "Alpha".to_string(),
            left_index: 0,
            right_label: "Enterprise".to_string(),
            right_index: 1,
        });
        let alpha = ExprIr::Literal(Value::EnumVariant {
            label: "Alpha".to_string(),
            index: 0,
        });
        let beta = ExprIr::Literal(Value::EnumVariant {
            label: "Beta".to_string(),
            index: 1,
        });

        ModelIr {
            model_id: "RelationMapOps".to_string(),
            state_fields: vec![
                StateField {
                    id: memberships.clone(),
                    name: memberships.clone(),
                    ty: FieldType::EnumRelation {
                        left_variants: vec!["Alice".to_string(), "Bob".to_string()],
                        right_variants: vec!["Alpha".to_string(), "Beta".to_string()],
                    },
                    span: span.clone(),
                },
                StateField {
                    id: pending.clone(),
                    name: pending.clone(),
                    ty: FieldType::EnumRelation {
                        left_variants: vec!["Alice".to_string(), "Bob".to_string()],
                        right_variants: vec!["Alpha".to_string(), "Beta".to_string()],
                    },
                    span: span.clone(),
                },
                StateField {
                    id: plans.clone(),
                    name: plans.clone(),
                    ty: FieldType::EnumMap {
                        key_variants: vec!["Alpha".to_string(), "Beta".to_string()],
                        value_variants: vec!["Free".to_string(), "Enterprise".to_string()],
                    },
                    span: span.clone(),
                },
                StateField {
                    id: alpha_registered.clone(),
                    name: alpha_registered.clone(),
                    ty: FieldType::Bool,
                    span: span.clone(),
                },
                StateField {
                    id: enterprise_alpha.clone(),
                    name: enterprise_alpha.clone(),
                    ty: FieldType::Bool,
                    span: span.clone(),
                },
            ],
            init: vec![
                InitAssignment {
                    field: memberships.clone(),
                    value: Value::UInt(0),
                    span: span.clone(),
                },
                InitAssignment {
                    field: pending.clone(),
                    value: Value::UInt(1),
                    span: span.clone(),
                },
                InitAssignment {
                    field: plans.clone(),
                    value: Value::UInt(5),
                    span: span.clone(),
                },
                InitAssignment {
                    field: alpha_registered.clone(),
                    value: Value::Bool(false),
                    span: span.clone(),
                },
                InitAssignment {
                    field: enterprise_alpha.clone(),
                    value: Value::Bool(false),
                    span: span.clone(),
                },
            ],
            actions: vec![
                ActionIr {
                    action_id: "SYNC".to_string(),
                    label: "SYNC".to_string(),
                    role: crate::ir::action::ActionRole::Business,
                    reads: vec![memberships.clone(), pending.clone(), plans.clone()],
                    writes: vec![
                        memberships.clone(),
                        pending.clone(),
                        alpha_registered.clone(),
                        enterprise_alpha.clone(),
                    ],
                    path_tags: Vec::new(),
                    guard: ExprIr::Binary {
                        op: BinaryOp::RelationIntersects,
                        left: Box::new(ExprIr::FieldRef(pending.clone())),
                        right: Box::new(ExprIr::FieldRef(pending.clone())),
                    },
                    updates: vec![
                        UpdateIr {
                            field: memberships.clone(),
                            value: ExprIr::Binary {
                                op: BinaryOp::RelationInsert,
                                left: Box::new(ExprIr::FieldRef(memberships.clone())),
                                right: Box::new(pair_alice_alpha.clone()),
                            },
                        },
                        UpdateIr {
                            field: pending.clone(),
                            value: ExprIr::Binary {
                                op: BinaryOp::RelationRemove,
                                left: Box::new(ExprIr::FieldRef(pending.clone())),
                                right: Box::new(pair_alice_alpha.clone()),
                            },
                        },
                        UpdateIr {
                            field: alpha_registered.clone(),
                            value: ExprIr::Binary {
                                op: BinaryOp::MapContainsKey,
                                left: Box::new(ExprIr::FieldRef(plans.clone())),
                                right: Box::new(alpha.clone()),
                            },
                        },
                        UpdateIr {
                            field: enterprise_alpha.clone(),
                            value: ExprIr::Binary {
                                op: BinaryOp::MapContainsEntry,
                                left: Box::new(ExprIr::FieldRef(plans.clone())),
                                right: Box::new(pair_beta_free.clone()),
                            },
                        },
                    ],
                },
                ActionIr {
                    action_id: "UPGRADE".to_string(),
                    label: "UPGRADE".to_string(),
                    role: crate::ir::action::ActionRole::Business,
                    reads: vec![plans.clone(), memberships.clone()],
                    writes: vec![
                        plans.clone(),
                        alpha_registered.clone(),
                        enterprise_alpha.clone(),
                    ],
                    path_tags: Vec::new(),
                    guard: ExprIr::Binary {
                        op: BinaryOp::And,
                        left: Box::new(ExprIr::Binary {
                            op: BinaryOp::MapContainsKey,
                            left: Box::new(ExprIr::FieldRef(plans.clone())),
                            right: Box::new(alpha.clone()),
                        }),
                        right: Box::new(ExprIr::Unary {
                            op: crate::ir::UnaryOp::Not,
                            expr: Box::new(ExprIr::Binary {
                                op: BinaryOp::MapContainsEntry,
                                left: Box::new(ExprIr::FieldRef(plans.clone())),
                                right: Box::new(pair_alpha_enterprise.clone()),
                            }),
                        }),
                    },
                    updates: vec![
                        UpdateIr {
                            field: plans.clone(),
                            value: ExprIr::Binary {
                                op: BinaryOp::MapPut,
                                left: Box::new(ExprIr::Binary {
                                    op: BinaryOp::MapRemoveKey,
                                    left: Box::new(ExprIr::FieldRef(plans.clone())),
                                    right: Box::new(beta.clone()),
                                }),
                                right: Box::new(pair_alpha_enterprise.clone()),
                            },
                        },
                        UpdateIr {
                            field: alpha_registered.clone(),
                            value: ExprIr::Literal(Value::Bool(true)),
                        },
                        UpdateIr {
                            field: enterprise_alpha.clone(),
                            value: ExprIr::Binary {
                                op: BinaryOp::RelationContains,
                                left: Box::new(ExprIr::FieldRef(memberships.clone())),
                                right: Box::new(pair_alice_alpha.clone()),
                            },
                        },
                    ],
                },
            ],
            properties: vec![PropertyIr {
                property_id: "P_SAFE".to_string(),
                kind: PropertyKind::Invariant,
                expr: ExprIr::Binary {
                    op: BinaryOp::Or,
                    left: Box::new(ExprIr::Binary {
                        op: BinaryOp::RelationContains,
                        left: Box::new(ExprIr::FieldRef(memberships)),
                        right: Box::new(pair_alice_alpha),
                    }),
                    right: Box::new(ExprIr::Binary {
                        op: BinaryOp::MapContainsEntry,
                        left: Box::new(ExprIr::FieldRef(plans)),
                        right: Box::new(pair_alpha_enterprise),
                    }),
                },
            }],
        }
    }

    #[test]
    fn bmc_query_declares_actions_and_negates_terminal_property() {
        let model = compile_model(
            "model A\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Inc:\n  pre: true\n  post:\n    x = x + 1\nproperty P_SAFE:\n  invariant: x <= 1\n",
        )
        .unwrap();
        let query = build_invariant_bmc_query(&model, &["P_SAFE".to_string()], 2).unwrap();
        assert!(query.check_smtlib.contains("(declare-fun action_0 () Int)"));
        assert!(query.check_smtlib.contains("(declare-fun action_1 () Int)"));
        assert!(query.check_smtlib.contains("(assert (not (<= x_2 1)))"));
        assert!(query.model_smtlib.contains("(get-value (action_0))"));
    }

    #[test]
    fn render_expr_supports_or_and_equality() {
        let model = compile_model(
            "model A\nstate:\n  x: u8[0..7]\n  enabled: bool\ninit:\n  x = 0\n  enabled = false\naction Inc:\n  pre: enabled == false || x <= 1\n  post:\n    enabled = true\nproperty P_SAFE:\n  invariant: enabled == true || x <= 7\n",
        )
        .unwrap();
        let query = build_invariant_bmc_query(&model, &["P_SAFE".to_string()], 1).unwrap();
        assert!(query
            .check_smtlib
            .contains("(or (= enabled_0 false) (<= x_0 1))"));
        assert!(query
            .check_smtlib
            .contains("(not (or (= enabled_1 true) (<= x_1 7)))"));
    }

    #[test]
    fn render_expr_supports_extended_numeric_ops() {
        let model = compile_model(
            "model A\nstate:\n  x: u8[0..7]\n  enabled: bool\ninit:\n  x = 2\n  enabled = false\naction Inc:\n  pre: x - 1 > 0 && x >= 1 && x < 7 && enabled != true\n  post:\n    x = x - 1\nproperty P_SAFE:\n  invariant: x >= 0 && enabled != true\n",
        )
        .unwrap();
        let query = build_invariant_bmc_query(&model, &["P_SAFE".to_string()], 1).unwrap();
        assert!(query.check_smtlib.contains("(- x_0 1)"));
        assert!(query.check_smtlib.contains("(> (- x_0 1) 0)"));
        assert!(query.check_smtlib.contains("(>= x_0 1)"));
        assert!(query.check_smtlib.contains("(< x_0 7)"));
        assert!(query.check_smtlib.contains("(not (= enabled_0 true))"));
    }

    #[test]
    fn render_expr_supports_modulo() {
        let model = compile_model(
            "model A\nstate:\n  x: u8[0..15]\ninit:\n  x = 0\naction Inc:\n  pre: x % 3 != 2\n  post:\n    x = x + 1\nproperty P_SAFE:\n  invariant: x % 3 != 2\n",
        )
        .unwrap();
        let query = build_invariant_bmc_query(&model, &["P_SAFE".to_string()], 1).unwrap();
        assert!(query.check_smtlib.contains("(mod x_0 3)"));
        assert!(query.check_smtlib.contains("(mod x_1 3)"));
    }

    #[test]
    fn bmc_query_supports_u16_bounds() {
        let model = compile_model(
            "model Budget\nstate:\n  spend: u16[0..5000]\ninit:\n  spend = 0\naction Raise:\n  pre: spend <= 4500\n  post:\n    spend = spend + 500\nproperty P_SAFE:\n  invariant: spend <= 5000\n",
        )
        .unwrap();
        let query = build_invariant_bmc_query(&model, &["P_SAFE".to_string()], 1).unwrap();
        assert!(query.check_smtlib.contains("(assert (<= 0 spend_0))"));
        assert!(query.check_smtlib.contains("(assert (<= spend_0 5000))"));
        assert!(query.check_smtlib.contains("(assert (<= spend_1 5000))"));
    }

    #[test]
    fn relation_map_encoding_uses_slot_symbols_and_constraints() {
        let query = build_invariant_bmc_query(&relation_map_model(), &["P_SAFE".to_string()], 1)
            .expect("relation/map model should encode");
        assert!(query
            .check_smtlib
            .contains("(declare-fun memberships_0_pair_0_0 () Bool)"));
        assert!(query
            .check_smtlib
            .contains("(declare-fun plans_0_key_0_present () Bool)"));
        assert!(query
            .check_smtlib
            .contains("(declare-fun plans_0_key_0_value () Int)"));
        assert!(query
            .check_smtlib
            .contains("(assert (=> (not plans_0_key_0_present) (= plans_0_key_0_value 0)))"));
        assert!(query
            .check_smtlib
            .contains("(= memberships_0_pair_0_0 false)"));
        assert!(query.check_smtlib.contains("(= pending_0_pair_0_0 true)"));
        assert!(query
            .check_smtlib
            .contains("(= plans_0_key_0_present true)"));
        assert!(query
            .check_smtlib
            .contains("(= plans_0_key_1_present true)"));
        assert!(query.check_smtlib.contains("(= plans_0_key_1_value 0)"));
        assert!(query
            .check_smtlib
            .contains("(or (and pending_0_pair_0_0 pending_0_pair_0_0)"));
        assert!(query
            .check_smtlib
            .contains("(and plans_0_key_1_present (= plans_0_key_1_value 0))"));
        assert!(query
            .check_smtlib
            .contains("(= memberships_1_pair_0_0 true)"));
        assert!(query.check_smtlib.contains("(= pending_1_pair_0_0 false)"));
        assert!(query
            .check_smtlib
            .contains("(= plans_1_key_0_present true)"));
        assert!(query.check_smtlib.contains("(= plans_1_key_0_value 1)"));
        assert!(query
            .check_smtlib
            .contains("(= plans_1_key_1_present false)"));
        assert!(query.check_smtlib.contains("(= plans_1_key_1_value 0)"));
    }

    #[test]
    fn cvc5_status_parser_handles_sat_unsat_unknown() {
        assert!(matches!(
            super::parse_check_sat_status(SmtCliDialect::Cvc5, "sat\n").unwrap(),
            SmtSolveStatus::Sat(_)
        ));
        assert!(matches!(
            super::parse_check_sat_status(SmtCliDialect::Cvc5, "unsat\n").unwrap(),
            SmtSolveStatus::Unsat
        ));
        assert!(matches!(
            super::parse_check_sat_status(SmtCliDialect::Cvc5, "unknown\n").unwrap(),
            SmtSolveStatus::Unknown
        ));
    }
}
