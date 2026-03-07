use crate::ir::{BinaryOp, ExprIr, FieldType, ModelIr, PropertyKind, UnaryOp, Value};
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
            property.property_id
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
            smtlib.push_str(&format!(
                "(declare-fun {} () {})\n",
                state_symbol(field.name.as_str(), step),
                smt_sort(&field.ty)
            ));
        }
    }

    let action_symbols = (0..depth).map(action_symbol).collect::<Vec<_>>();
    for symbol in &action_symbols {
        smtlib.push_str(&format!("(declare-fun {symbol} () Int)\n"));
    }

    for step in 0..=depth {
        for field in &model.state_fields {
            if let Some((min, max)) = integer_bounds(&field.ty) {
                let symbol = state_symbol(field.name.as_str(), step);
                smtlib.push_str(&format!("(assert (<= {} {}))\n", min, symbol));
                smtlib.push_str(&format!("(assert (<= {} {}))\n", symbol, max));
            }
        }
    }

    for init in &model.init {
        let field = model
            .state_fields
            .iter()
            .find(|field| field.id == init.field)
            .ok_or_else(|| format!("unknown init field `{}`", init.field))?;
        smtlib.push_str(&format!(
            "(assert (= {} {}))\n",
            state_symbol(field.name.as_str(), 0),
            literal(&init.value)
        ));
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
                    let next_symbol = state_symbol(field.name.as_str(), step + 1);
                    let next_expr = action
                        .updates
                        .iter()
                        .find(|update| update.field == field.id)
                        .map(|update| render_expr(model, &update.value, step))
                        .transpose()?
                        .unwrap_or_else(|| state_symbol(field.name.as_str(), step));
                    conjuncts.push(format!("(= {next_symbol} {next_expr})"));
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
        | FieldType::EnumSet { .. }
        | FieldType::EnumRelation { .. }
        | FieldType::EnumMap { .. } => "Int",
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
        FieldType::EnumRelation {
            left_variants,
            right_variants,
        }
        | FieldType::EnumMap {
            key_variants: left_variants,
            value_variants: right_variants,
        } => {
            let slots = left_variants.len().saturating_mul(right_variants.len());
            if slots > 64 {
                None
            } else if slots == 64 {
                Some((0, u64::MAX))
            } else {
                Some((0, (1u64 << slots) - 1))
            }
        }
    }
}

fn state_symbol(field_name: &str, step: usize) -> String {
    format!("{field_name}_{step}")
}

fn action_symbol(step: usize) -> String {
    format!("action_{step}")
}

fn literal(value: &Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::UInt(value) => value.to_string(),
        Value::String(value) => format!("{value:?}"),
        Value::EnumVariant { index, .. } => index.to_string(),
        Value::PairVariant { .. } => {
            panic!("pair literals must be rendered through relation/map operators")
        }
    }
}

fn render_expr(model: &ModelIr, expr: &ExprIr, step: usize) -> Result<String, String> {
    match expr {
        ExprIr::Literal(Value::PairVariant { .. }) => Err(
            "pair literals are only supported inside relation/map helper expressions".to_string(),
        ),
        ExprIr::Literal(value) => Ok(literal(value)),
        ExprIr::FieldRef(field) => {
            let field_name = model
                .state_fields
                .iter()
                .find(|item| item.id == *field)
                .map(|item| item.name.as_str())
                .ok_or_else(|| format!("unknown field reference `{field}`"))?;
            Ok(state_symbol(field_name, step))
        }
        ExprIr::Unary { op, expr } => match op {
            UnaryOp::Not => Ok(format!("(not {})", render_expr(model, expr, step)?)),
            UnaryOp::SetIsEmpty => Ok(format!("(= {} 0)", render_expr(model, expr, step)?)),
            UnaryOp::StringLen => Err(
                "SMT adapter does not yet support string length expressions; use explicit backend"
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
            BinaryOp::RelationContains => {
                let left_rendered = render_expr(model, left, step)?;
                let (left_index, right_index) = extract_pair_indexes(right.as_ref(), expr)?;
                let mask = relation_mask_for_expr(model, left.as_ref(), left_index, right_index)?;
                Ok(format!("(= (mod (div {left_rendered} {mask}) 2) 1)"))
            }
            BinaryOp::RelationInsert => {
                let left_rendered = render_expr(model, left, step)?;
                let (left_index, right_index) = extract_pair_indexes(right.as_ref(), expr)?;
                let mask = relation_mask_for_expr(model, left.as_ref(), left_index, right_index)?;
                Ok(format!(
                    "(+ {left_rendered} (* (- 1 (mod (div {left_rendered} {mask}) 2)) {mask}))"
                ))
            }
            BinaryOp::RelationRemove => {
                let left_rendered = render_expr(model, left, step)?;
                let (left_index, right_index) = extract_pair_indexes(right.as_ref(), expr)?;
                let mask = relation_mask_for_expr(model, left.as_ref(), left_index, right_index)?;
                Ok(format!(
                    "(- {left_rendered} (* (mod (div {left_rendered} {mask}) 2) {mask}))"
                ))
            }
            BinaryOp::RelationIntersects => {
                let left_rendered = render_expr(model, left, step)?;
                let right_rendered = render_expr(model, right, step)?;
                let masks = relation_masks_for_expr(model, left.as_ref())?;
                if masks.is_empty() {
                    Ok("false".to_string())
                } else {
                    Ok(format!(
                            "(or {})",
                            masks
                                .iter()
                                .map(|mask| format!(
                                    "(and (= (mod (div {left_rendered} {mask}) 2) 1) (= (mod (div {right_rendered} {mask}) 2) 1))"
                                ))
                                .collect::<Vec<_>>()
                                .join(" ")
                        ))
                }
            }
            BinaryOp::MapContainsKey => {
                let left_rendered = render_expr(model, left, step)?;
                let key_index = extract_enum_index_from_expr(right.as_ref(), expr)?;
                let masks = map_masks_for_key(model, left.as_ref(), key_index)?;
                if masks.is_empty() {
                    Ok("false".to_string())
                } else {
                    Ok(format!(
                        "(or {})",
                        masks
                            .iter()
                            .map(|mask| format!("(= (mod (div {left_rendered} {mask}) 2) 1)"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    ))
                }
            }
            BinaryOp::MapContainsEntry => {
                let left_rendered = render_expr(model, left, step)?;
                let (key_index, value_index) = extract_pair_indexes(right.as_ref(), expr)?;
                let mask = relation_mask_for_expr(model, left.as_ref(), key_index, value_index)?;
                Ok(format!("(= (mod (div {left_rendered} {mask}) 2) 1)"))
            }
            BinaryOp::MapPut => {
                let left_rendered = render_expr(model, left, step)?;
                let (key_index, value_index) = extract_pair_indexes(right.as_ref(), expr)?;
                let clear = render_map_clear_expr(model, left.as_ref(), &left_rendered, key_index)?;
                let mask = relation_mask_for_expr(model, left.as_ref(), key_index, value_index)?;
                Ok(format!(
                    "(+ {clear} (* (- 1 (mod (div {clear} {mask}) 2)) {mask}))"
                ))
            }
            BinaryOp::MapRemoveKey => {
                let left_rendered = render_expr(model, left, step)?;
                let key_index = extract_enum_index_from_expr(right.as_ref(), expr)?;
                render_map_clear_expr(model, left.as_ref(), &left_rendered, key_index)
            }
            BinaryOp::Add => {
                let left = render_expr(model, left, step)?;
                let right = render_expr(model, right, step)?;
                Ok(format!("(+ {left} {right})"))
            }
            BinaryOp::Sub => {
                let left = render_expr(model, left, step)?;
                let right = render_expr(model, right, step)?;
                Ok(format!("(- {left} {right})"))
            }
            BinaryOp::Mod => {
                let left = render_expr(model, left, step)?;
                let right = render_expr(model, right, step)?;
                Ok(format!("(mod {left} {right})"))
            }
            BinaryOp::SetContains => {
                let left = render_expr(model, left, step)?;
                let right = render_expr(model, right, step)?;
                let index = extract_enum_index(right.as_str(), expr)?;
                Ok(format!(
                    "(= (mod (div {left} {}) 2) 1)",
                    enum_variant_mask(index)
                ))
            }
            BinaryOp::SetInsert => {
                let left = render_expr(model, left, step)?;
                let right = render_expr(model, right, step)?;
                let index = extract_enum_index(right.as_str(), expr)?;
                let mask = enum_variant_mask(index);
                Ok(format!(
                    "(+ {left} (* (- 1 (mod (div {left} {mask}) 2)) {mask}))"
                ))
            }
            BinaryOp::SetRemove => {
                let left = render_expr(model, left, step)?;
                let right = render_expr(model, right, step)?;
                let index = extract_enum_index(right.as_str(), expr)?;
                let mask = enum_variant_mask(index);
                Ok(format!("(- {left} (* (mod (div {left} {mask}) 2) {mask}))"))
            }
            BinaryOp::LessThan => {
                let left = render_expr(model, left, step)?;
                let right = render_expr(model, right, step)?;
                Ok(format!("(< {left} {right})"))
            }
            BinaryOp::LessThanOrEqual => {
                let left = render_expr(model, left, step)?;
                let right = render_expr(model, right, step)?;
                Ok(format!("(<= {left} {right})"))
            }
            BinaryOp::GreaterThan => {
                let left = render_expr(model, left, step)?;
                let right = render_expr(model, right, step)?;
                Ok(format!("(> {left} {right})"))
            }
            BinaryOp::GreaterThanOrEqual => {
                let left = render_expr(model, left, step)?;
                let right = render_expr(model, right, step)?;
                Ok(format!("(>= {left} {right})"))
            }
            BinaryOp::Equal => {
                let left = render_expr(model, left, step)?;
                let right = render_expr(model, right, step)?;
                Ok(format!("(= {left} {right})"))
            }
            BinaryOp::NotEqual => {
                let left = render_expr(model, left, step)?;
                let right = render_expr(model, right, step)?;
                Ok(format!("(not (= {left} {right}))"))
            }
            BinaryOp::And => {
                let left = render_expr(model, left, step)?;
                let right = render_expr(model, right, step)?;
                Ok(format!("(and {left} {right})"))
            }
            BinaryOp::Or => {
                let left = render_expr(model, left, step)?;
                let right = render_expr(model, right, step)?;
                Ok(format!("(or {left} {right})"))
            }
        },
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

fn relation_mask_for_expr(
    model: &ModelIr,
    expr: &ExprIr,
    left_index: u64,
    right_index: u64,
) -> Result<u64, String> {
    let right_len = match field_type_for_expr(model, expr) {
        Some(FieldType::EnumRelation { right_variants, .. }) => right_variants.len() as u64,
        Some(FieldType::EnumMap { value_variants, .. }) => value_variants.len() as u64,
        other => {
            return Err(format!(
                "relation/map operation requires a relation or map field, got `{other:?}`"
            ))
        }
    };
    let bit_index = left_index
        .checked_mul(right_len)
        .and_then(|value| value.checked_add(right_index))
        .ok_or_else(|| "relation/map bit index overflow".to_string())?;
    Ok(enum_variant_mask(bit_index))
}

fn relation_masks_for_expr(model: &ModelIr, expr: &ExprIr) -> Result<Vec<u64>, String> {
    match field_type_for_expr(model, expr) {
        Some(FieldType::EnumRelation {
            left_variants,
            right_variants,
        })
        | Some(FieldType::EnumMap {
            key_variants: left_variants,
            value_variants: right_variants,
        }) => Ok((0..left_variants.len() as u64)
            .flat_map(|left_index| {
                (0..right_variants.len() as u64).map(move |right_index| {
                    enum_variant_mask(left_index * right_variants.len() as u64 + right_index)
                })
            })
            .collect()),
        other => Err(format!(
            "relation/map operation requires a relation or map field, got `{other:?}`"
        )),
    }
}

fn map_masks_for_key(model: &ModelIr, expr: &ExprIr, key_index: u64) -> Result<Vec<u64>, String> {
    match field_type_for_expr(model, expr) {
        Some(FieldType::EnumMap { value_variants, .. }) => Ok((0..value_variants.len() as u64)
            .map(|value_index| relation_mask_for_expr(model, expr, key_index, value_index))
            .collect::<Result<Vec<_>, _>>()?),
        other => Err(format!(
            "map operation requires a map field, got `{other:?}`"
        )),
    }
}

fn render_map_clear_expr(
    model: &ModelIr,
    expr: &ExprIr,
    rendered_left: &str,
    key_index: u64,
) -> Result<String, String> {
    let masks = map_masks_for_key(model, expr, key_index)?;
    if masks.is_empty() {
        return Ok(rendered_left.to_string());
    }
    Ok(format!(
        "(- {rendered_left} (+ {}))",
        masks
            .iter()
            .map(|mask| format!("(* (mod (div {rendered_left} {mask}) 2) {mask})"))
            .collect::<Vec<_>>()
            .join(" ")
    ))
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

#[cfg(test)]
mod tests {
    use super::{build_invariant_bmc_query, SmtCliDialect, SmtSolveStatus};
    use crate::frontend::compile_model;

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
