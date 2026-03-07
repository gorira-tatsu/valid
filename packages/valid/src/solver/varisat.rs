use crate::ir::ModelIr;
#[cfg(feature = "varisat-backend")]
use crate::ir::{BinaryOp, ExprIr, FieldType, PropertyKind, StateField, UnaryOp, Value};

#[cfg(feature = "varisat-backend")]
use std::collections::HashSet;
#[cfg(feature = "varisat-backend")]
use varisat::{ExtendFormula, Lit, Solver, Var};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VarisatSolveStatus {
    Sat(Vec<String>),
    Unsat,
    Unknown,
}

#[cfg(feature = "varisat-backend")]
pub fn run_bounded_invariant_check_varisat(
    model: &ModelIr,
    target_property_ids: &[String],
    horizon: usize,
) -> Result<VarisatSolveStatus, String> {
    validate_varisat_model(model, target_property_ids)?;
    let property_id = target_property_ids
        .first()
        .ok_or_else(|| "missing target property for sat-varisat".to_string())?;
    for depth in 0..=horizon {
        let mut encoder = CnfEncoder::new(model, property_id, depth);
        encoder.encode()?;
        match encoder.solve()? {
            Some(actions) => return Ok(VarisatSolveStatus::Sat(actions)),
            None => continue,
        }
    }
    Ok(VarisatSolveStatus::Unsat)
}

#[cfg(not(feature = "varisat-backend"))]
pub fn run_bounded_invariant_check_varisat(
    _model: &ModelIr,
    _target_property_ids: &[String],
    _horizon: usize,
) -> Result<VarisatSolveStatus, String> {
    Err("backend=sat-varisat requires the `varisat-backend` Cargo feature".to_string())
}

#[cfg(feature = "varisat-backend")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldEncoding {
    Bool,
    UInt {
        bit_width: usize,
        min: u64,
        max: u64,
    },
    Relation {
        slot_count: usize,
    },
    Map {
        slot_count: usize,
    },
    EnumSet {
        width: usize,
    },
}

#[cfg(feature = "varisat-backend")]
#[derive(Clone)]
enum EncodedValue {
    Bool(Lit),
    UInt(Vec<Lit>),
    Relation(Vec<Lit>),
    Map(Vec<Lit>),
    EnumSet(Vec<Lit>),
}

#[cfg(feature = "varisat-backend")]
struct CnfEncoder<'a> {
    model: &'a ModelIr,
    property_id: &'a str,
    depth: usize,
    solver: Solver<'static>,
    next_var_index: usize,
    true_lit: Lit,
    false_lit: Lit,
    field_encodings: Vec<FieldEncoding>,
    state_vars: Vec<Vec<EncodedValue>>,
    action_lits: Vec<Vec<Lit>>,
}

#[cfg(feature = "varisat-backend")]
impl<'a> CnfEncoder<'a> {
    fn new(model: &'a ModelIr, property_id: &'a str, depth: usize) -> Self {
        let field_encodings = model
            .state_fields
            .iter()
            .map(|field| {
                field_encoding(&field.ty)
                    .expect("varisat field encoding should be validated before construction")
            })
            .collect::<Vec<_>>();

        let mut next_var_index = 0usize;
        let mut alloc = || {
            let var = Var::from_index(next_var_index);
            next_var_index += 1;
            Lit::from_var(var, true)
        };

        let true_lit = alloc();
        let false_lit = alloc();
        let state_vars = (0..=depth)
            .map(|_| {
                field_encodings
                    .iter()
                    .map(|encoding| allocate_field(encoding, &mut alloc))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let action_lits = (0..depth)
            .map(|_| model.actions.iter().map(|_| alloc()).collect::<Vec<_>>())
            .collect::<Vec<_>>();

        let mut solver = Solver::new();
        solver.add_clause(&[true_lit]);
        solver.add_clause(&[!false_lit]);

        Self {
            model,
            property_id,
            depth,
            solver,
            next_var_index,
            true_lit,
            false_lit,
            field_encodings,
            state_vars,
            action_lits,
        }
    }

    fn encode(&mut self) -> Result<(), String> {
        self.encode_state_bounds();
        self.encode_state_invariants()?;
        self.encode_init()?;
        self.encode_action_choice();
        self.encode_transitions()?;
        self.encode_property()?;
        Ok(())
    }

    fn solve(mut self) -> Result<Option<Vec<String>>, String> {
        if !self
            .solver
            .solve()
            .map_err(|err| format!("varisat solve failed: {err}"))?
        {
            return Ok(None);
        }
        let model = self
            .solver
            .model()
            .ok_or_else(|| "varisat reported sat but produced no model".to_string())?;
        let positive = model.into_iter().collect::<HashSet<_>>();
        let mut actions = Vec::new();
        for step in 0..self.depth {
            let selected_index = self.action_lits[step]
                .iter()
                .enumerate()
                .find_map(|(index, lit)| positive.contains(lit).then_some(index))
                .ok_or_else(|| format!("varisat model did not select an action for step {step}"))?;
            actions.push(self.model.actions[selected_index].action_id.clone());
        }
        Ok(Some(actions))
    }

    // --- State bounds (UInt min/max) ---

    fn encode_state_bounds(&mut self) {
        for step in 0..=self.depth {
            for field_index in 0..self.field_encodings.len() {
                let encoding = self.field_encodings[field_index];
                let EncodedValue::UInt(bits) = &self.state_vars[step][field_index] else {
                    continue;
                };
                let bits = bits.clone();
                let FieldEncoding::UInt { min, max, .. } = encoding else {
                    continue;
                };
                if min > 0 {
                    let min_bits = self.uint_const(min, bits.len());
                    let lower_violation = self.uint_less_than(&bits, &min_bits);
                    self.solver.add_clause(&[!lower_violation]);
                }
                let max_bits = self.uint_const(max, bits.len());
                let upper_violation = self.uint_less_than(&max_bits, &bits);
                self.solver.add_clause(&[!upper_violation]);
            }
        }
    }

    // --- State invariants (Map at-most-one per key) ---

    fn encode_state_invariants(&mut self) -> Result<(), String> {
        for step in 0..=self.depth {
            for field_index in 0..self.model.state_fields.len() {
                let field = &self.model.state_fields[field_index];
                if let FieldType::EnumMap {
                    key_variants,
                    value_variants,
                } = &field.ty
                {
                    for key_index in 0..key_variants.len() {
                        for left_value in 0..value_variants.len() {
                            let left_lit =
                                self.map_slot_lit(step, field_index, key_index, left_value)?;
                            for right_value in (left_value + 1)..value_variants.len() {
                                let right_lit =
                                    self.map_slot_lit(step, field_index, key_index, right_value)?;
                                self.solver.add_clause(&[!left_lit, !right_lit]);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // --- Init encoding ---

    fn encode_init(&mut self) -> Result<(), String> {
        for (field_index, field) in self.model.state_fields.iter().enumerate() {
            let assignment = self
                .model
                .init
                .iter()
                .find(|assignment| assignment.field == field.id)
                .ok_or_else(|| format!("missing init assignment for field `{}`", field.name))?;
            match (self.state_vars[0][field_index].clone(), &assignment.value) {
                (EncodedValue::Bool(target), Value::Bool(value)) => {
                    self.solver
                        .add_clause(&[if *value { target } else { !target }]);
                }
                (EncodedValue::UInt(bits), Value::UInt(value)) => {
                    self.add_bits_equal_value(&bits, *value);
                }
                (EncodedValue::Relation(slots), Value::UInt(bits)) => {
                    if let FieldType::EnumRelation {
                        left_variants,
                        right_variants,
                    } = &field.ty
                    {
                        for left_index in 0..left_variants.len() {
                            for right_index in 0..right_variants.len() {
                                let slot_idx = relation_slot_index(
                                    left_index,
                                    right_index,
                                    right_variants.len(),
                                );
                                let lit = slots[slot_idx];
                                let present = relation_literal_contains(
                                    *bits,
                                    right_variants.len(),
                                    left_index,
                                    right_index,
                                );
                                self.solver
                                    .add_clause(&[if present { lit } else { !lit }]);
                            }
                        }
                    }
                }
                (EncodedValue::Map(slots), Value::UInt(bits)) => {
                    if let FieldType::EnumMap {
                        key_variants,
                        value_variants,
                    } = &field.ty
                    {
                        for key_index in 0..key_variants.len() {
                            for value_index in 0..value_variants.len() {
                                let slot_idx =
                                    map_slot_index(key_index, value_index, value_variants.len());
                                let lit = slots[slot_idx];
                                let present = relation_literal_contains(
                                    *bits,
                                    value_variants.len(),
                                    key_index,
                                    value_index,
                                );
                                self.solver
                                    .add_clause(&[if present { lit } else { !lit }]);
                            }
                        }
                    }
                }
                (EncodedValue::EnumSet(lits), Value::UInt(bits)) => {
                    for (index, lit) in lits.into_iter().enumerate() {
                        let present = *bits & enum_variant_mask(index as u64) != 0;
                        self.solver.add_clause(&[if present { lit } else { !lit }]);
                    }
                }
                _ => {
                    return Err(format!(
                        "backend=sat-varisat does not support init assignment `{}` for `{}`",
                        assignment.field,
                        rust_type_label(&field.ty)
                    ));
                }
            }
        }
        Ok(())
    }

    // --- Action choice ---

    fn encode_action_choice(&mut self) {
        for step in 0..self.depth {
            let lits = self.action_lits[step].clone();
            self.solver.add_clause(&lits);
            for i in 0..lits.len() {
                for j in (i + 1)..lits.len() {
                    self.solver.add_clause(&[!lits[i], !lits[j]]);
                }
            }
        }
    }

    // --- Transition encoding ---

    fn encode_transitions(&mut self) -> Result<(), String> {
        for step in 0..self.depth {
            for (action_index, action) in self.model.actions.iter().enumerate() {
                let selector = self.action_lits[step][action_index];
                let guard = self.encode_bool_expr(step, &action.guard)?;
                self.solver.add_clause(&[!selector, guard]);
                for (field_index, field) in self.model.state_fields.iter().enumerate() {
                    let next = self.state_vars[step + 1][field_index].clone();
                    let value = match action
                        .updates
                        .iter()
                        .find(|update| update.field == field.id)
                    {
                        Some(update) => self.encode_expr(step, &update.value)?,
                        None => self.state_vars[step][field_index].clone(),
                    };
                    self.add_equivalence_under_expr(selector, &next, value)?;
                }
            }
        }
        Ok(())
    }

    // --- Property encoding ---

    fn encode_property(&mut self) -> Result<(), String> {
        let property = self
            .model
            .properties
            .iter()
            .find(|property| property.property_id == self.property_id)
            .ok_or_else(|| format!("unknown property `{}`", self.property_id))?;
        for step in 0..self.depth {
            let lit = self.encode_bool_expr(step, &property.expr)?;
            self.solver.add_clause(&[lit]);
        }
        let fail = self.encode_bool_expr(self.depth, &property.expr)?;
        self.solver.add_clause(&[!fail]);
        Ok(())
    }

    // --- Expression encoding ---

    fn encode_expr(&mut self, step: usize, expr: &ExprIr) -> Result<EncodedValue, String> {
        match expr {
            ExprIr::Literal(Value::Bool(value)) => Ok(EncodedValue::Bool(self.bool_const(*value))),
            ExprIr::Literal(Value::UInt(value)) => {
                let index = self.field_index_for_context(expr);
                match index.and_then(|i| self.field_encodings.get(i).copied()) {
                    Some(FieldEncoding::EnumSet { width }) => {
                        Ok(EncodedValue::EnumSet(self.uint_to_set_lits(*value, width)))
                    }
                    Some(FieldEncoding::Relation { slot_count }) => {
                        Ok(EncodedValue::Relation(self.uint_to_bit_lits(*value, slot_count)))
                    }
                    Some(FieldEncoding::Map { slot_count }) => {
                        Ok(EncodedValue::Map(self.uint_to_bit_lits(*value, slot_count)))
                    }
                    _ => Ok(EncodedValue::UInt(self.min_uint_const(*value))),
                }
            }
            ExprIr::FieldRef(field_id) => {
                let index = self.field_index(field_id)?;
                Ok(self.state_vars[step][index].clone())
            }
            ExprIr::Unary { op, .. } => match op {
                UnaryOp::Not => Ok(EncodedValue::Bool(self.encode_bool_expr(step, expr)?)),
                UnaryOp::SetIsEmpty => Ok(EncodedValue::Bool(self.encode_set_is_empty(step, expr)?)),
                UnaryOp::StringLen => Err(
                    "backend=sat-varisat does not support string length expressions".to_string(),
                ),
            },
            ExprIr::Binary { op, left, right } => match op {
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mod => {
                    Ok(EncodedValue::UInt(self.encode_uint_expr(step, expr)?))
                }
                BinaryOp::SetInsert => {
                    let set = self.encode_set_expr(step, left)?;
                    let index = extract_enum_index_from_expr(right, expr)? as usize;
                    let mut result = set;
                    if index < result.len() {
                        result[index] = self.bool_const(true);
                    }
                    Ok(EncodedValue::EnumSet(result))
                }
                BinaryOp::SetRemove => {
                    let set = self.encode_set_expr(step, left)?;
                    let index = extract_enum_index_from_expr(right, expr)? as usize;
                    let mut result = set;
                    if index < result.len() {
                        result[index] = self.bool_const(false);
                    }
                    Ok(EncodedValue::EnumSet(result))
                }
                BinaryOp::RelationInsert | BinaryOp::RelationRemove => {
                    self.encode_relation_expr(step, expr)
                }
                BinaryOp::MapPut | BinaryOp::MapRemoveKey => {
                    self.encode_map_expr(step, expr)
                }
                BinaryOp::LessThan
                | BinaryOp::LessThanOrEqual
                | BinaryOp::GreaterThan
                | BinaryOp::GreaterThanOrEqual
                | BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::SetContains
                | BinaryOp::RelationContains
                | BinaryOp::RelationIntersects
                | BinaryOp::MapContainsKey
                | BinaryOp::MapContainsEntry => {
                    Ok(EncodedValue::Bool(self.encode_bool_expr(step, expr)?))
                }
                BinaryOp::StringContains | BinaryOp::RegexMatch => Err(format!(
                    "backend=sat-varisat does not support `{op:?}`"
                )),
            },
            ExprIr::Literal(Value::EnumVariant { .. }) | ExprIr::Literal(Value::PairVariant { .. }) => {
                Err(format!(
                    "backend=sat-varisat does not support bare enum/pair literals in expression position"
                ))
            }
            ExprIr::Literal(other) => Err(format!(
                "backend=sat-varisat does not support literal `{other:?}`"
            )),
        }
    }

    fn encode_bool_expr(&mut self, step: usize, expr: &ExprIr) -> Result<Lit, String> {
        match expr {
            ExprIr::Literal(Value::Bool(value)) => Ok(self.bool_const(*value)),
            ExprIr::FieldRef(field_id) => {
                let index = self.field_index(field_id)?;
                match &self.state_vars[step][index] {
                    EncodedValue::Bool(lit) => Ok(*lit),
                    _ => Err(format!(
                        "backend=sat-varisat cannot use non-boolean field `{field_id}` as a predicate"
                    )),
                }
            }
            ExprIr::Unary { op, expr } => match op {
                UnaryOp::Not => Ok(!self.encode_bool_expr(step, expr)?),
                UnaryOp::SetIsEmpty => self.encode_set_is_empty(step, expr),
                UnaryOp::StringLen => Err(
                    "backend=sat-varisat does not support string length expressions".to_string(),
                ),
            },
            ExprIr::Binary { op, left, right } => match op {
                BinaryOp::And => {
                    let a = self.encode_bool_expr(step, left)?;
                    let b = self.encode_bool_expr(step, right)?;
                    Ok(self.bool_and(a, b))
                }
                BinaryOp::Or => {
                    let a = self.encode_bool_expr(step, left)?;
                    let b = self.encode_bool_expr(step, right)?;
                    Ok(self.bool_or(a, b))
                }
                BinaryOp::Equal => self.encode_equal_expr(step, left, right),
                BinaryOp::NotEqual => Ok(!self.encode_equal_expr(step, left, right)?),
                BinaryOp::LessThan => {
                    let left = self.encode_uint_expr(step, left)?;
                    let right = self.encode_uint_expr(step, right)?;
                    Ok(self.uint_less_than(&left, &right))
                }
                BinaryOp::LessThanOrEqual => {
                    let left = self.encode_uint_expr(step, left)?;
                    let right = self.encode_uint_expr(step, right)?;
                    let less_than = self.uint_less_than(&left, &right);
                    let equal = self.uint_equal(&left, &right);
                    Ok(self.bool_or(less_than, equal))
                }
                BinaryOp::GreaterThan => {
                    let left = self.encode_uint_expr(step, left)?;
                    let right = self.encode_uint_expr(step, right)?;
                    Ok(self.uint_less_than(&right, &left))
                }
                BinaryOp::GreaterThanOrEqual => {
                    let left = self.encode_uint_expr(step, left)?;
                    let right = self.encode_uint_expr(step, right)?;
                    let greater_than = self.uint_less_than(&right, &left);
                    let equal = self.uint_equal(&left, &right);
                    Ok(self.bool_or(greater_than, equal))
                }
                BinaryOp::SetContains => {
                    let set = self.encode_set_expr(step, left)?;
                    let index = extract_enum_index_from_expr(right, expr)? as usize;
                    if index < set.len() {
                        Ok(set[index])
                    } else {
                        Ok(self.bool_const(false))
                    }
                }
                BinaryOp::RelationContains => {
                    let (left_index, right_index) = extract_pair_indexes(right, expr)?;
                    self.encode_relation_slot_check(
                        step,
                        left,
                        left_index as usize,
                        right_index as usize,
                    )
                }
                BinaryOp::RelationIntersects => {
                    self.encode_relation_intersects(step, left, right)
                }
                BinaryOp::MapContainsKey => {
                    let key_index = extract_enum_index_from_expr(right, expr)? as usize;
                    self.encode_map_contains_key(step, left, key_index)
                }
                BinaryOp::MapContainsEntry => {
                    let (key_index, value_index) = extract_pair_indexes(right, expr)?;
                    self.encode_map_slot_check(
                        step,
                        left,
                        key_index as usize,
                        value_index as usize,
                    )
                }
                _ => Err(format!(
                    "backend=sat-varisat does not support `{op:?}` as a boolean expression"
                )),
            },
            ExprIr::Literal(other) => Err(format!(
                "backend=sat-varisat expected a boolean expression, got `{other:?}`"
            )),
        }
    }

    fn encode_equal_expr(
        &mut self,
        step: usize,
        left: &ExprIr,
        right: &ExprIr,
    ) -> Result<Lit, String> {
        match (
            self.encode_expr(step, left)?,
            self.encode_expr(step, right)?,
        ) {
            (EncodedValue::Bool(a), EncodedValue::Bool(b)) => Ok(self.bool_equal(a, b)),
            (EncodedValue::UInt(a), EncodedValue::UInt(b)) => Ok(self.uint_equal(&a, &b)),
            (EncodedValue::Relation(a), EncodedValue::Relation(b))
            | (EncodedValue::Map(a), EncodedValue::Map(b))
            | (EncodedValue::EnumSet(a), EncodedValue::EnumSet(b)) => {
                Ok(self.slot_vec_equal(&a, &b))
            }
            (left, right) => Err(format!(
                "backend=sat-varisat cannot compare mismatched expression kinds `{}` and `{}`",
                encoded_kind_label(&left),
                encoded_kind_label(&right)
            )),
        }
    }

    fn encode_uint_expr(&mut self, step: usize, expr: &ExprIr) -> Result<Vec<Lit>, String> {
        match expr {
            ExprIr::Literal(Value::UInt(value)) => Ok(self.min_uint_const(*value)),
            ExprIr::FieldRef(field_id) => {
                match &self.state_vars[step][self.field_index(field_id)?] {
                    EncodedValue::UInt(bits) => Ok(bits.clone()),
                    _ => Err(format!(
                        "backend=sat-varisat expected a bounded integer, but `{field_id}` is not"
                    )),
                }
            }
            ExprIr::Binary { op, left, right } => match op {
                BinaryOp::Add => {
                    let left = self.encode_uint_expr(step, left)?;
                    let right = self.encode_uint_expr(step, right)?;
                    Ok(self.uint_add(&left, &right))
                }
                BinaryOp::Sub => {
                    let left = self.encode_uint_expr(step, left)?;
                    let right = self.encode_uint_expr(step, right)?;
                    Ok(self.uint_saturating_sub(&left, &right))
                }
                BinaryOp::Mod => {
                    let (divisor_min, divisor_max) = self.uint_expr_bounds(right)?;
                    if divisor_max == 0 || divisor_min == 0 {
                        return Err(
                            "backend=sat-varisat requires modulo divisors with a strictly positive lower bound"
                                .to_string(),
                        );
                    }
                    let dividend = self.encode_uint_expr(step, left)?;
                    let divisor = self.encode_uint_expr(step, right)?;
                    Ok(self.uint_mod(&dividend, &divisor))
                }
                _ => Err(format!(
                    "backend=sat-varisat does not support `{op:?}` as a bounded integer expression"
                )),
            },
            _ => Err(format!(
                "backend=sat-varisat expected a bounded integer expression, got `{expr:?}`"
            )),
        }
    }

    // --- Set encoding ---

    fn encode_set_expr(&mut self, step: usize, expr: &ExprIr) -> Result<Vec<Lit>, String> {
        match expr {
            ExprIr::Literal(Value::UInt(bits)) => {
                let width = self.set_width_for_expr(expr)?;
                Ok(self.uint_to_set_lits(*bits, width))
            }
            ExprIr::FieldRef(field_id) => {
                let index = self.field_index(field_id)?;
                match &self.state_vars[step][index] {
                    EncodedValue::EnumSet(lits) => Ok(lits.clone()),
                    _ => Err(format!(
                        "backend=sat-varisat expected an enum set field, got `{field_id}`"
                    )),
                }
            }
            ExprIr::Binary {
                op: BinaryOp::SetInsert,
                left,
                right,
            } => {
                let mut set = self.encode_set_expr(step, left)?;
                let index = extract_enum_index_from_expr(right, expr)? as usize;
                if index < set.len() {
                    set[index] = self.bool_const(true);
                }
                Ok(set)
            }
            ExprIr::Binary {
                op: BinaryOp::SetRemove,
                left,
                right,
            } => {
                let mut set = self.encode_set_expr(step, left)?;
                let index = extract_enum_index_from_expr(right, expr)? as usize;
                if index < set.len() {
                    set[index] = self.bool_const(false);
                }
                Ok(set)
            }
            other => Err(format!(
                "backend=sat-varisat does not support set expression `{other:?}`"
            )),
        }
    }

    fn encode_set_is_empty(&mut self, step: usize, expr: &ExprIr) -> Result<Lit, String> {
        match field_type_for_expr(self.model, expr) {
            Some(FieldType::EnumSet { .. }) => {
                let set = self.encode_set_expr(step, expr)?;
                Ok(!self.bool_or_many(set))
            }
            Some(FieldType::EnumRelation {
                left_variants,
                right_variants,
            }) => {
                let mut slots = Vec::new();
                for left_index in 0..left_variants.len() {
                    for right_index in 0..right_variants.len() {
                        slots.push(self.encode_relation_slot_check(
                            step,
                            expr,
                            left_index,
                            right_index,
                        )?);
                    }
                }
                Ok(!self.bool_or_many(slots))
            }
            Some(FieldType::EnumMap {
                key_variants,
                value_variants,
            }) => {
                let mut slots = Vec::new();
                for key_index in 0..key_variants.len() {
                    for value_index in 0..value_variants.len() {
                        slots.push(self.encode_map_slot_check(
                            step,
                            expr,
                            key_index,
                            value_index,
                        )?);
                    }
                }
                Ok(!self.bool_or_many(slots))
            }
            other => Err(format!(
                "backend=sat-varisat cannot evaluate is_empty over `{other:?}`"
            )),
        }
    }

    fn set_width_for_expr(&self, expr: &ExprIr) -> Result<usize, String> {
        match field_type_for_expr(self.model, expr) {
            Some(FieldType::EnumSet { variants }) => Ok(variants.len()),
            _ => Err("backend=sat-varisat cannot determine set width".to_string()),
        }
    }

    // --- Relation encoding ---

    fn encode_relation_expr(&mut self, step: usize, expr: &ExprIr) -> Result<EncodedValue, String> {
        match expr {
            ExprIr::Literal(Value::UInt(bits)) => {
                let ty = relation_type_for_expr(self.model, expr)?;
                if let FieldType::EnumRelation {
                    left_variants,
                    right_variants,
                } = ty
                {
                    let slot_count = left_variants.len() * right_variants.len();
                    Ok(EncodedValue::Relation(self.uint_to_bit_lits(*bits, slot_count)))
                } else {
                    unreachable!()
                }
            }
            ExprIr::FieldRef(field_id) => {
                let index = self.field_index(field_id)?;
                match &self.state_vars[step][index] {
                    EncodedValue::Relation(slots) => Ok(EncodedValue::Relation(slots.clone())),
                    _ => Err(format!(
                        "backend=sat-varisat expected a relation field, got `{field_id}`"
                    )),
                }
            }
            ExprIr::Binary {
                op: BinaryOp::RelationInsert,
                left,
                right,
            } => {
                let ty = relation_type_for_expr(self.model, expr)?;
                let right_len = match ty {
                    FieldType::EnumRelation { right_variants, .. } => right_variants.len(),
                    _ => unreachable!(),
                };
                let (target_left, target_right) = extract_pair_indexes(right, expr)?;
                let base = self.encode_relation_expr(step, left)?;
                let EncodedValue::Relation(mut slots) = base else {
                    return Err("expected relation".to_string());
                };
                let idx = relation_slot_index(target_left as usize, target_right as usize, right_len);
                if idx < slots.len() {
                    slots[idx] = self.bool_const(true);
                }
                Ok(EncodedValue::Relation(slots))
            }
            ExprIr::Binary {
                op: BinaryOp::RelationRemove,
                left,
                right,
            } => {
                let ty = relation_type_for_expr(self.model, expr)?;
                let right_len = match ty {
                    FieldType::EnumRelation { right_variants, .. } => right_variants.len(),
                    _ => unreachable!(),
                };
                let (target_left, target_right) = extract_pair_indexes(right, expr)?;
                let base = self.encode_relation_expr(step, left)?;
                let EncodedValue::Relation(mut slots) = base else {
                    return Err("expected relation".to_string());
                };
                let idx = relation_slot_index(target_left as usize, target_right as usize, right_len);
                if idx < slots.len() {
                    slots[idx] = self.bool_const(false);
                }
                Ok(EncodedValue::Relation(slots))
            }
            other => Err(format!(
                "backend=sat-varisat does not support relation expression `{other:?}`"
            )),
        }
    }

    fn encode_relation_slot_check(
        &mut self,
        step: usize,
        expr: &ExprIr,
        left_index: usize,
        right_index: usize,
    ) -> Result<Lit, String> {
        match expr {
            ExprIr::Literal(Value::UInt(bits)) => {
                let ty = relation_type_for_expr(self.model, expr)?;
                let right_len = match ty {
                    FieldType::EnumRelation { right_variants, .. } => right_variants.len(),
                    _ => unreachable!(),
                };
                Ok(self.bool_const(relation_literal_contains(
                    *bits, right_len, left_index, right_index,
                )))
            }
            ExprIr::FieldRef(field_id) => {
                let field_index = self.field_index(field_id)?;
                self.relation_slot_lit(step, field_index, left_index, right_index)
            }
            ExprIr::Binary {
                op: BinaryOp::RelationInsert,
                left,
                right,
            } => {
                let (target_left, target_right) = extract_pair_indexes(right, expr)?;
                if left_index == target_left as usize && right_index == target_right as usize {
                    Ok(self.bool_const(true))
                } else {
                    self.encode_relation_slot_check(step, left, left_index, right_index)
                }
            }
            ExprIr::Binary {
                op: BinaryOp::RelationRemove,
                left,
                right,
            } => {
                let (target_left, target_right) = extract_pair_indexes(right, expr)?;
                if left_index == target_left as usize && right_index == target_right as usize {
                    Ok(self.bool_const(false))
                } else {
                    self.encode_relation_slot_check(step, left, left_index, right_index)
                }
            }
            other => Err(format!(
                "backend=sat-varisat does not support relation expression `{other:?}`"
            )),
        }
    }

    fn encode_relation_intersects(
        &mut self,
        step: usize,
        left: &ExprIr,
        right: &ExprIr,
    ) -> Result<Lit, String> {
        let ty = relation_type_for_expr(self.model, left)?;
        match ty {
            FieldType::EnumRelation {
                left_variants,
                right_variants,
            } => {
                let mut overlaps = Vec::new();
                for left_index in 0..left_variants.len() {
                    for right_index in 0..right_variants.len() {
                        let a = self.encode_relation_slot_check(
                            step,
                            left,
                            left_index,
                            right_index,
                        )?;
                        let b = self.encode_relation_slot_check(
                            step,
                            right,
                            left_index,
                            right_index,
                        )?;
                        overlaps.push(self.bool_and(a, b));
                    }
                }
                Ok(self.bool_or_many(overlaps))
            }
            _ => unreachable!(),
        }
    }

    // --- Map encoding ---

    fn encode_map_expr(&mut self, step: usize, expr: &ExprIr) -> Result<EncodedValue, String> {
        match expr {
            ExprIr::Literal(Value::UInt(bits)) => {
                let ty = map_type_for_expr(self.model, expr)?;
                if let FieldType::EnumMap {
                    key_variants,
                    value_variants,
                } = ty
                {
                    let slot_count = key_variants.len() * value_variants.len();
                    Ok(EncodedValue::Map(self.uint_to_bit_lits(*bits, slot_count)))
                } else {
                    unreachable!()
                }
            }
            ExprIr::FieldRef(field_id) => {
                let index = self.field_index(field_id)?;
                match &self.state_vars[step][index] {
                    EncodedValue::Map(slots) => Ok(EncodedValue::Map(slots.clone())),
                    _ => Err(format!(
                        "backend=sat-varisat expected a map field, got `{field_id}`"
                    )),
                }
            }
            ExprIr::Binary {
                op: BinaryOp::MapPut,
                left,
                right,
            } => {
                let ty = map_type_for_expr(self.model, expr)?;
                let value_len = match ty {
                    FieldType::EnumMap { value_variants, .. } => value_variants.len(),
                    _ => unreachable!(),
                };
                let (target_key, target_value) = extract_pair_indexes(right, expr)?;
                let base = self.encode_map_expr(step, left)?;
                let EncodedValue::Map(mut slots) = base else {
                    return Err("expected map".to_string());
                };
                for value_index in 0..value_len {
                    let idx = map_slot_index(target_key as usize, value_index, value_len);
                    if idx < slots.len() {
                        slots[idx] = self.bool_const(value_index == target_value as usize);
                    }
                }
                Ok(EncodedValue::Map(slots))
            }
            ExprIr::Binary {
                op: BinaryOp::MapRemoveKey,
                left,
                right,
            } => {
                let ty = map_type_for_expr(self.model, expr)?;
                let value_len = match ty {
                    FieldType::EnumMap { value_variants, .. } => value_variants.len(),
                    _ => unreachable!(),
                };
                let target_key = extract_enum_index_from_expr(right, expr)?;
                let base = self.encode_map_expr(step, left)?;
                let EncodedValue::Map(mut slots) = base else {
                    return Err("expected map".to_string());
                };
                for value_index in 0..value_len {
                    let idx = map_slot_index(target_key as usize, value_index, value_len);
                    if idx < slots.len() {
                        slots[idx] = self.bool_const(false);
                    }
                }
                Ok(EncodedValue::Map(slots))
            }
            other => Err(format!(
                "backend=sat-varisat does not support map expression `{other:?}`"
            )),
        }
    }

    fn encode_map_slot_check(
        &mut self,
        step: usize,
        expr: &ExprIr,
        key_index: usize,
        value_index: usize,
    ) -> Result<Lit, String> {
        match expr {
            ExprIr::Literal(Value::UInt(bits)) => {
                let ty = map_type_for_expr(self.model, expr)?;
                let value_len = match ty {
                    FieldType::EnumMap { value_variants, .. } => value_variants.len(),
                    _ => unreachable!(),
                };
                Ok(self.bool_const(relation_literal_contains(
                    *bits, value_len, key_index, value_index,
                )))
            }
            ExprIr::FieldRef(field_id) => {
                let field_index = self.field_index(field_id)?;
                self.map_slot_lit(step, field_index, key_index, value_index)
            }
            ExprIr::Binary {
                op: BinaryOp::MapPut,
                left,
                right,
            } => {
                let (target_key, target_value) = extract_pair_indexes(right, expr)?;
                if key_index == target_key as usize {
                    Ok(self.bool_const(value_index == target_value as usize))
                } else {
                    self.encode_map_slot_check(step, left, key_index, value_index)
                }
            }
            ExprIr::Binary {
                op: BinaryOp::MapRemoveKey,
                left,
                right,
            } => {
                let target_key = extract_enum_index_from_expr(right, expr)?;
                if key_index == target_key as usize {
                    Ok(self.bool_const(false))
                } else {
                    self.encode_map_slot_check(step, left, key_index, value_index)
                }
            }
            other => Err(format!(
                "backend=sat-varisat does not support map expression `{other:?}`"
            )),
        }
    }

    fn encode_map_contains_key(
        &mut self,
        step: usize,
        expr: &ExprIr,
        key_index: usize,
    ) -> Result<Lit, String> {
        let ty = map_type_for_expr(self.model, expr)?;
        let value_len = match ty {
            FieldType::EnumMap { value_variants, .. } => value_variants.len(),
            _ => unreachable!(),
        };
        let mut slots = Vec::new();
        for value_index in 0..value_len {
            slots.push(self.encode_map_slot_check(step, expr, key_index, value_index)?);
        }
        Ok(self.bool_or_many(slots))
    }

    // --- Slot accessors ---

    fn relation_slot_lit(
        &self,
        step: usize,
        field_index: usize,
        left_index: usize,
        right_index: usize,
    ) -> Result<Lit, String> {
        match &self.state_vars[step][field_index] {
            EncodedValue::Relation(slots) => match &self.model.state_fields[field_index].ty {
                FieldType::EnumRelation { right_variants, .. } => {
                    Ok(slots[relation_slot_index(left_index, right_index, right_variants.len())])
                }
                _ => unreachable!(),
            },
            _ => Err(format!(
                "expected relation state for `{}`",
                self.model.state_fields[field_index].name
            )),
        }
    }

    fn map_slot_lit(
        &self,
        step: usize,
        field_index: usize,
        key_index: usize,
        value_index: usize,
    ) -> Result<Lit, String> {
        match &self.state_vars[step][field_index] {
            EncodedValue::Map(slots) => match &self.model.state_fields[field_index].ty {
                FieldType::EnumMap { value_variants, .. } => {
                    Ok(slots[map_slot_index(key_index, value_index, value_variants.len())])
                }
                _ => unreachable!(),
            },
            _ => Err(format!(
                "expected map state for `{}`",
                self.model.state_fields[field_index].name
            )),
        }
    }

    // --- Equivalence helpers ---

    fn add_equivalence_under_expr(
        &mut self,
        condition: Lit,
        target: &EncodedValue,
        value: EncodedValue,
    ) -> Result<(), String> {
        match (target, value) {
            (EncodedValue::Bool(target), EncodedValue::Bool(value)) => {
                self.add_equivalence_under(condition, *target, value);
                Ok(())
            }
            (EncodedValue::UInt(target_bits), EncodedValue::UInt(value_bits)) => {
                self.add_uint_equivalence_under(condition, target_bits, &value_bits);
                Ok(())
            }
            (EncodedValue::Relation(target_slots), EncodedValue::Relation(value_slots))
            | (EncodedValue::Map(target_slots), EncodedValue::Map(value_slots))
            | (EncodedValue::EnumSet(target_slots), EncodedValue::EnumSet(value_slots)) => {
                for (target, value) in target_slots.iter().zip(value_slots.iter()) {
                    self.add_equivalence_under(condition, *target, *value);
                }
                Ok(())
            }
            (target, value) => Err(format!(
                "backend=sat-varisat cannot assign `{}` to `{}`",
                encoded_kind_label(&value),
                encoded_kind_label(target)
            )),
        }
    }

    fn add_uint_equivalence_under(&mut self, condition: Lit, target: &[Lit], value: &[Lit]) {
        let width = target.len().max(value.len());
        for index in 0..width {
            let target = self.bit_at(target, index);
            let value = self.bit_at(value, index);
            self.add_equivalence_under(condition, target, value);
        }
    }

    fn add_equivalence_under(&mut self, condition: Lit, target: Lit, value: Lit) {
        self.solver.add_clause(&[!condition, !target, value]);
        self.solver.add_clause(&[!condition, target, !value]);
    }

    fn add_bits_equal_value(&mut self, bits: &[Lit], value: u64) {
        for (index, bit) in bits.iter().enumerate() {
            let mask = 1u64.checked_shl(index as u32).unwrap_or(0);
            let clause = if value & mask == 0 { !*bit } else { *bit };
            self.solver.add_clause(&[clause]);
        }
    }

    // --- Boolean SAT primitives ---

    fn bool_const(&self, value: bool) -> Lit {
        if value {
            self.true_lit
        } else {
            self.false_lit
        }
    }

    fn bool_and(&mut self, a: Lit, b: Lit) -> Lit {
        let z = self.fresh_lit();
        self.solver.add_clause(&[!z, a]);
        self.solver.add_clause(&[!z, b]);
        self.solver.add_clause(&[z, !a, !b]);
        z
    }

    fn bool_or(&mut self, a: Lit, b: Lit) -> Lit {
        let z = self.fresh_lit();
        self.solver.add_clause(&[z, !a]);
        self.solver.add_clause(&[z, !b]);
        self.solver.add_clause(&[!z, a, b]);
        z
    }

    fn bool_equal(&mut self, a: Lit, b: Lit) -> Lit {
        let z = self.fresh_lit();
        self.solver.add_clause(&[!z, !a, b]);
        self.solver.add_clause(&[!z, a, !b]);
        self.solver.add_clause(&[z, !a, !b]);
        self.solver.add_clause(&[z, a, b]);
        z
    }

    fn bool_xor(&mut self, a: Lit, b: Lit) -> Lit {
        !self.bool_equal(a, b)
    }

    fn bool_mux(&mut self, select: Lit, when_true: Lit, when_false: Lit) -> Lit {
        let true_branch = self.bool_and(select, when_true);
        let false_branch = self.bool_and(!select, when_false);
        self.bool_or(true_branch, false_branch)
    }

    fn bool_and_many(&mut self, lits: Vec<Lit>) -> Lit {
        let mut iter = lits.into_iter();
        let Some(first) = iter.next() else {
            return self.bool_const(true);
        };
        iter.fold(first, |acc, lit| self.bool_and(acc, lit))
    }

    fn bool_or_many(&mut self, lits: Vec<Lit>) -> Lit {
        let mut iter = lits.into_iter();
        let Some(first) = iter.next() else {
            return self.bool_const(false);
        };
        iter.fold(first, |acc, lit| self.bool_or(acc, lit))
    }

    fn slot_vec_equal(&mut self, a: &[Lit], b: &[Lit]) -> Lit {
        let equalities: Vec<Lit> = a
            .iter()
            .zip(b.iter())
            .map(|(a, b)| self.bool_equal(*a, *b))
            .collect();
        self.bool_and_many(equalities)
    }

    // --- Unsigned integer arithmetic ---

    fn uint_add(&mut self, left: &[Lit], right: &[Lit]) -> Vec<Lit> {
        let width = left.len().max(right.len());
        let mut carry = self.false_lit;
        let mut result = Vec::with_capacity(width + 1);
        for index in 0..width {
            let a = self.bit_at(left, index);
            let b = self.bit_at(right, index);
            let ab_xor = self.bool_xor(a, b);
            let sum = self.bool_xor(ab_xor, carry);
            let carry_ab = self.bool_and(a, b);
            let carry_ac = self.bool_and(a, carry);
            let carry_bc = self.bool_and(b, carry);
            let carry_tail = self.bool_or(carry_ac, carry_bc);
            carry = self.bool_or(carry_ab, carry_tail);
            result.push(sum);
        }
        result.push(carry);
        result
    }

    fn uint_saturating_sub(&mut self, left: &[Lit], right: &[Lit]) -> Vec<Lit> {
        let (difference, borrow) = self.subtract_bits(left, right);
        let zeros = vec![self.false_lit; difference.len()];
        self.select_bits(borrow, &zeros, &difference)
    }

    fn uint_mod(&mut self, dividend: &[Lit], divisor: &[Lit]) -> Vec<Lit> {
        let width = dividend.len().max(divisor.len());
        let divisor = self.extend_bits(divisor, width);
        let divisor_nonzero = self.uint_nonzero(&divisor);
        self.solver.add_clause(&[divisor_nonzero]);

        let mut remainder = vec![self.false_lit; width];
        for index in (0..width).rev() {
            let incoming = self.bit_at(dividend, index);
            let shifted = self.shift_left_insert(&remainder, incoming);
            let less_than = self.uint_less_than(&shifted, &divisor);
            let candidate = self.uint_saturating_sub(&shifted, &divisor);
            remainder = self.select_bits(!less_than, &candidate, &shifted);
        }
        remainder
    }

    fn shift_left_insert(&self, bits: &[Lit], incoming: Lit) -> Vec<Lit> {
        if bits.is_empty() {
            return Vec::new();
        }
        let mut shifted = Vec::with_capacity(bits.len());
        shifted.push(incoming);
        shifted.extend(bits.iter().take(bits.len().saturating_sub(1)).copied());
        shifted
    }

    fn uint_equal(&mut self, left: &[Lit], right: &[Lit]) -> Lit {
        let width = left.len().max(right.len());
        let mut equal = self.true_lit;
        for index in 0..width {
            let bit_equal = self.bool_equal(self.bit_at(left, index), self.bit_at(right, index));
            equal = self.bool_and(equal, bit_equal);
        }
        equal
    }

    fn uint_less_than(&mut self, left: &[Lit], right: &[Lit]) -> Lit {
        let (_, borrow) = self.subtract_bits(left, right);
        borrow
    }

    fn uint_nonzero(&mut self, bits: &[Lit]) -> Lit {
        let mut any = self.false_lit;
        for bit in bits {
            any = self.bool_or(any, *bit);
        }
        any
    }

    fn subtract_bits(&mut self, left: &[Lit], right: &[Lit]) -> (Vec<Lit>, Lit) {
        let width = left.len().max(right.len());
        let mut borrow = self.false_lit;
        let mut difference = Vec::with_capacity(width);
        for index in 0..width {
            let a = self.bit_at(left, index);
            let b = self.bit_at(right, index);
            let ab_xor = self.bool_xor(a, b);
            difference.push(self.bool_xor(ab_xor, borrow));
            let b_or_borrow = self.bool_or(b, borrow);
            let borrow_from_a = self.bool_and(!a, b_or_borrow);
            let borrow_from_b = self.bool_and(b, borrow);
            borrow = self.bool_or(borrow_from_a, borrow_from_b);
        }
        (difference, borrow)
    }

    fn select_bits(&mut self, select: Lit, when_true: &[Lit], when_false: &[Lit]) -> Vec<Lit> {
        let width = when_true.len().max(when_false.len());
        (0..width)
            .map(|index| {
                self.bool_mux(
                    select,
                    self.bit_at(when_true, index),
                    self.bit_at(when_false, index),
                )
            })
            .collect()
    }

    fn extend_bits(&self, bits: &[Lit], width: usize) -> Vec<Lit> {
        (0..width).map(|index| self.bit_at(bits, index)).collect()
    }

    fn bit_at(&self, bits: &[Lit], index: usize) -> Lit {
        bits.get(index).copied().unwrap_or(self.false_lit)
    }

    fn min_uint_const(&self, value: u64) -> Vec<Lit> {
        self.uint_const(value, bit_width_for_value(value))
    }

    fn uint_const(&self, value: u64, width: usize) -> Vec<Lit> {
        (0..width)
            .map(|index| {
                let mask = 1u64.checked_shl(index as u32).unwrap_or(0);
                if value & mask == 0 {
                    self.false_lit
                } else {
                    self.true_lit
                }
            })
            .collect()
    }

    fn uint_to_set_lits(&self, bits: u64, width: usize) -> Vec<Lit> {
        (0..width)
            .map(|index| {
                let present = bits & enum_variant_mask(index as u64) != 0;
                self.bool_const(present)
            })
            .collect()
    }

    fn uint_to_bit_lits(&self, bits: u64, count: usize) -> Vec<Lit> {
        (0..count)
            .map(|index| {
                let mask = 1u64.checked_shl(index as u32).unwrap_or(0);
                self.bool_const(bits & mask != 0)
            })
            .collect()
    }

    fn uint_expr_bounds(&self, expr: &ExprIr) -> Result<(u64, u64), String> {
        match expr {
            ExprIr::Literal(Value::UInt(value)) => Ok((*value, *value)),
            ExprIr::FieldRef(field_id) => {
                let index = self.field_index(field_id)?;
                match self.field_encodings[index] {
                    FieldEncoding::UInt { min, max, .. } => Ok((min, max)),
                    _ => Err(format!(
                        "backend=sat-varisat expected a bounded integer, but `{field_id}` is not"
                    )),
                }
            }
            ExprIr::Binary { op, left, right } => {
                let (left_min, left_max) = self.uint_expr_bounds(left)?;
                let (right_min, right_max) = self.uint_expr_bounds(right)?;
                match op {
                    BinaryOp::Add => Ok((
                        left_min.saturating_add(right_min),
                        left_max.saturating_add(right_max),
                    )),
                    BinaryOp::Sub => Ok((
                        left_min.saturating_sub(right_max),
                        left_max.saturating_sub(right_min),
                    )),
                    BinaryOp::Mod => {
                        if right_max == 0 {
                            Ok((0, 0))
                        } else {
                            Ok((0, left_max.min(right_max.saturating_sub(1))))
                        }
                    }
                    other => Err(format!(
                        "backend=sat-varisat does not support `{other:?}` for bounds computation"
                    )),
                }
            }
            _ => Err(format!(
                "backend=sat-varisat expected a bounded integer expression, got `{expr:?}`"
            )),
        }
    }

    // --- Misc helpers ---

    fn field_index(&self, field_id: &str) -> Result<usize, String> {
        self.model
            .state_fields
            .iter()
            .position(|field| field.id == field_id)
            .ok_or_else(|| format!("unknown field `{field_id}`"))
    }

    fn field_index_for_context(&self, expr: &ExprIr) -> Option<usize> {
        match expr {
            ExprIr::FieldRef(field_id) => self.field_index(field_id).ok(),
            _ => None,
        }
    }

    fn fresh_lit(&mut self) -> Lit {
        let lit = Lit::from_var(Var::from_index(self.next_var_index), true);
        self.next_var_index += 1;
        lit
    }
}

// --- Free functions ---

#[cfg(feature = "varisat-backend")]
fn encoded_kind_label(value: &EncodedValue) -> &'static str {
    match value {
        EncodedValue::Bool(_) => "bool",
        EncodedValue::UInt(_) => "bounded-int",
        EncodedValue::Relation(_) => "relation",
        EncodedValue::Map(_) => "map",
        EncodedValue::EnumSet(_) => "enum-set",
    }
}

#[cfg(feature = "varisat-backend")]
fn validate_varisat_model(model: &ModelIr, target_property_ids: &[String]) -> Result<(), String> {
    let property_id = target_property_ids
        .first()
        .ok_or_else(|| "missing target property for sat-varisat".to_string())?;
    let property = model
        .properties
        .iter()
        .find(|property| &property.property_id == property_id)
        .ok_or_else(|| format!("unknown property `{property_id}`"))?;
    if property.kind != PropertyKind::Invariant {
        return Err("backend=sat-varisat currently supports invariant properties only".to_string());
    }
    for field in &model.state_fields {
        if field_encoding(&field.ty).is_none() {
            return Err(format!(
                "backend=sat-varisat does not support state field `{}` of type `{}`",
                field.name,
                rust_type_label(&field.ty)
            ));
        }
    }
    Ok(())
}

#[cfg(feature = "varisat-backend")]
fn field_encoding(ty: &FieldType) -> Option<FieldEncoding> {
    match ty {
        FieldType::Bool => Some(FieldEncoding::Bool),
        FieldType::BoundedU8 { min, max } => Some(FieldEncoding::UInt {
            bit_width: 8,
            min: *min as u64,
            max: *max as u64,
        }),
        FieldType::BoundedU16 { min, max } => Some(FieldEncoding::UInt {
            bit_width: 16,
            min: *min as u64,
            max: *max as u64,
        }),
        FieldType::BoundedU32 { min, max } => Some(FieldEncoding::UInt {
            bit_width: 32,
            min: *min as u64,
            max: *max as u64,
        }),
        FieldType::EnumRelation {
            left_variants,
            right_variants,
        } => Some(FieldEncoding::Relation {
            slot_count: left_variants.len() * right_variants.len(),
        }),
        FieldType::EnumMap {
            key_variants,
            value_variants,
        } => Some(FieldEncoding::Map {
            slot_count: key_variants.len() * value_variants.len(),
        }),
        FieldType::EnumSet { variants } => Some(FieldEncoding::EnumSet {
            width: variants.len(),
        }),
        FieldType::String { .. } | FieldType::Enum { .. } => None,
    }
}

#[cfg(feature = "varisat-backend")]
fn allocate_field(encoding: &FieldEncoding, alloc: &mut impl FnMut() -> Lit) -> EncodedValue {
    match encoding {
        FieldEncoding::Bool => EncodedValue::Bool(alloc()),
        FieldEncoding::UInt { bit_width, .. } => {
            EncodedValue::UInt((0..*bit_width).map(|_| alloc()).collect())
        }
        FieldEncoding::Relation { slot_count } => {
            EncodedValue::Relation((0..*slot_count).map(|_| alloc()).collect())
        }
        FieldEncoding::Map { slot_count } => {
            EncodedValue::Map((0..*slot_count).map(|_| alloc()).collect())
        }
        FieldEncoding::EnumSet { width } => {
            EncodedValue::EnumSet((0..*width).map(|_| alloc()).collect())
        }
    }
}

#[cfg(feature = "varisat-backend")]
fn bit_width_for_value(value: u64) -> usize {
    let width = u64::BITS as usize - value.leading_zeros() as usize;
    width.max(1)
}

#[cfg(feature = "varisat-backend")]
fn rust_type_label(ty: &FieldType) -> &'static str {
    match ty {
        FieldType::Bool => "bool",
        FieldType::String { .. } => "String",
        FieldType::BoundedU8 { .. } => "u8",
        FieldType::BoundedU16 { .. } => "u16",
        FieldType::BoundedU32 { .. } => "u32",
        FieldType::Enum { .. } => "enum",
        FieldType::EnumSet { .. } => "FiniteEnumSet",
        FieldType::EnumRelation { .. } => "FiniteRelation",
        FieldType::EnumMap { .. } => "FiniteMap",
    }
}

#[cfg(feature = "varisat-backend")]
fn enum_variant_mask(index: u64) -> u64 {
    1u64.checked_shl(index as u32).unwrap_or(0)
}

#[cfg(feature = "varisat-backend")]
fn field_type_for_expr<'a>(model: &'a ModelIr, expr: &ExprIr) -> Option<&'a FieldType> {
    match expr {
        ExprIr::FieldRef(field) => model
            .state_fields
            .iter()
            .find(|state_field| state_field.id == *field)
            .map(|state_field| &state_field.ty),
        ExprIr::Binary { op, left, .. } => match op {
            BinaryOp::RelationInsert
            | BinaryOp::RelationRemove
            | BinaryOp::SetInsert
            | BinaryOp::SetRemove
            | BinaryOp::MapPut
            | BinaryOp::MapRemoveKey => field_type_for_expr(model, left),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(feature = "varisat-backend")]
fn relation_type_for_expr<'a>(model: &'a ModelIr, expr: &ExprIr) -> Result<&'a FieldType, String> {
    match field_type_for_expr(model, expr) {
        Some(ty @ FieldType::EnumRelation { .. }) => Ok(ty),
        other => Err(format!(
            "backend=sat-varisat expected relation expression, got `{other:?}`"
        )),
    }
}

#[cfg(feature = "varisat-backend")]
fn map_type_for_expr<'a>(model: &'a ModelIr, expr: &ExprIr) -> Result<&'a FieldType, String> {
    match field_type_for_expr(model, expr) {
        Some(ty @ FieldType::EnumMap { .. }) => Ok(ty),
        other => Err(format!(
            "backend=sat-varisat expected map expression, got `{other:?}`"
        )),
    }
}

#[cfg(feature = "varisat-backend")]
fn extract_enum_index_from_expr(expr: &ExprIr, parent: &ExprIr) -> Result<u64, String> {
    match expr {
        ExprIr::Literal(Value::EnumVariant { index, .. }) => Ok(*index),
        other => Err(format!(
            "backend=sat-varisat requires a finite enum literal, got `{other:?}` in `{parent:?}`"
        )),
    }
}

#[cfg(feature = "varisat-backend")]
fn extract_pair_indexes(expr: &ExprIr, parent: &ExprIr) -> Result<(u64, u64), String> {
    match expr {
        ExprIr::Literal(Value::PairVariant {
            left_index,
            right_index,
            ..
        }) => Ok((*left_index, *right_index)),
        other => Err(format!(
            "backend=sat-varisat requires a finite pair literal, got `{other:?}` in `{parent:?}`"
        )),
    }
}

#[cfg(feature = "varisat-backend")]
fn relation_slot_index(left_index: usize, right_index: usize, right_len: usize) -> usize {
    left_index * right_len + right_index
}

#[cfg(feature = "varisat-backend")]
fn map_slot_index(key_index: usize, value_index: usize, value_len: usize) -> usize {
    key_index * value_len + value_index
}

#[cfg(feature = "varisat-backend")]
fn relation_literal_contains(
    bits: u64,
    right_len: usize,
    left_index: usize,
    right_index: usize,
) -> bool {
    let bit_index = relation_slot_index(left_index, right_index, right_len);
    bits & (1u64.checked_shl(bit_index as u32).unwrap_or(0)) != 0
}

// --- Tests ---

#[cfg(all(test, feature = "varisat-backend"))]
mod tests {
    use super::{run_bounded_invariant_check_varisat, VarisatSolveStatus};
    use crate::{
        api::{check_source, CheckRequest},
        engine::{CheckOutcome, RunStatus},
        frontend::compile_model,
    };

    fn request(source: &str, request_id: &str, backend: Option<&str>) -> CheckRequest {
        CheckRequest {
            request_id: request_id.to_string(),
            source_name: format!("{request_id}.valid"),
            source: source.to_string(),
            property_id: None,
            backend: backend.map(str::to_string),
            solver_executable: None,
            solver_args: Vec::new(),
        }
    }

    fn completed_status(outcome: CheckOutcome) -> RunStatus {
        match outcome {
            CheckOutcome::Completed(result) => result.status,
            CheckOutcome::Errored(error) => panic!("unexpected error: {:?}", error.diagnostics),
        }
    }

    #[test]
    fn bounded_counter_finds_a_counterexample() {
        let model = compile_model(
            "model Counter\nstate:\n  x: u8[0..2]\ninit:\n  x = 0\naction Inc:\n  pre: x <= 1\n  post:\n    x = x + 1\naction Stay:\n  pre: x <= 2\n  post:\n    x = x\nproperty P_FAIL:\n  invariant: x <= 1\n",
        )
        .expect("model should compile");

        let status = run_bounded_invariant_check_varisat(&model, &["P_FAIL".to_string()], 2)
            .expect("varisat run should succeed");

        assert_eq!(
            status,
            VarisatSolveStatus::Sat(vec!["Inc".to_string(), "Inc".to_string()])
        );
    }

    #[test]
    fn subtraction_uses_saturating_semantics() {
        let model = compile_model(
            "model SaturatingSub\nstate:\n  x: u8[0..3]\ninit:\n  x = 0\naction Jump:\n  pre: x - 1 < 1\n  post:\n    x = x + 2\nproperty P_SAFE:\n  invariant: x <= 1\n",
        )
        .expect("model should compile");

        let status = run_bounded_invariant_check_varisat(&model, &["P_SAFE".to_string()], 1)
            .expect("varisat run should succeed");

        assert_eq!(status, VarisatSolveStatus::Sat(vec!["Jump".to_string()]));
    }

    #[test]
    fn modulo_with_positive_divisor_is_supported() {
        let model = compile_model(
            "model ModCounter\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Inc:\n  pre: x % 2 == 0\n  post:\n    x = x + 1\nproperty P_SAFE:\n  invariant: x % 2 == 0\n",
        )
        .expect("model should compile");

        let status = run_bounded_invariant_check_varisat(&model, &["P_SAFE".to_string()], 1)
            .expect("varisat run should succeed");

        assert_eq!(status, VarisatSolveStatus::Sat(vec!["Inc".to_string()]));
    }

    #[test]
    fn wide_bounded_integer_fields_are_supported() {
        let model = compile_model(
            "model WideCounter\nstate:\n  x: u32[0..10]\ninit:\n  x = 0\naction Jump:\n  pre: x < 1\n  post:\n    x = x + 2\nproperty P_SAFE:\n  invariant: x <= 1\n",
        )
        .expect("model should compile");

        let status = run_bounded_invariant_check_varisat(&model, &["P_SAFE".to_string()], 1)
            .expect("varisat run should succeed");

        assert_eq!(status, VarisatSolveStatus::Sat(vec!["Jump".to_string()]));
    }

    #[test]
    fn explicit_and_varisat_match_for_arithmetic_models() {
        let source = "model Counter\nstate:\n  x: u8[0..2]\ninit:\n  x = 0\naction Inc:\n  pre: x <= 1\n  post:\n    x = x + 1\naction Reset:\n  pre: x - 1 <= 1\n  post:\n    x = 0\nproperty P_SAFE:\n  invariant: x <= 2\n";

        let explicit = check_source(&request(source, "req-explicit", None));
        let varisat = check_source(&request(source, "req-varisat", Some("sat-varisat")));

        assert_eq!(completed_status(explicit), RunStatus::Pass);
        assert_eq!(completed_status(varisat), RunStatus::Pass);
    }
}
