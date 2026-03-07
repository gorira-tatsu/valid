use crate::ir::ModelIr;
#[cfg(feature = "varisat-backend")]
use crate::ir::{BinaryOp, ExprIr, StateField, UnaryOp, Value};
#[cfg(feature = "varisat-backend")]
use crate::ir::{FieldType, PropertyKind};

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
#[derive(Debug, Clone)]
enum EncodedFieldState {
    Bool(Lit),
    Relation(Vec<Lit>),
    Map(Vec<Lit>),
}

#[cfg(feature = "varisat-backend")]
struct CnfEncoder<'a> {
    model: &'a ModelIr,
    property_id: &'a str,
    depth: usize,
    solver: Solver<'static>,
    next_var_index: usize,
    state_lits: Vec<Vec<EncodedFieldState>>,
    action_lits: Vec<Vec<Lit>>,
}

#[cfg(feature = "varisat-backend")]
impl<'a> CnfEncoder<'a> {
    fn new(model: &'a ModelIr, property_id: &'a str, depth: usize) -> Self {
        let mut next_var_index = 0usize;
        let mut alloc = || {
            let var = Var::from_index(next_var_index);
            next_var_index += 1;
            Lit::from_var(var, true)
        };

        let state_lits = (0..=depth)
            .map(|_| {
                model
                    .state_fields
                    .iter()
                    .map(|field| allocate_field_state(field, &mut alloc))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let action_lits = (0..depth)
            .map(|_| model.actions.iter().map(|_| alloc()).collect::<Vec<_>>())
            .collect::<Vec<_>>();

        Self {
            model,
            property_id,
            depth,
            solver: Solver::new(),
            next_var_index,
            state_lits,
            action_lits,
        }
    }

    fn encode(&mut self) -> Result<(), String> {
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

    fn encode_init(&mut self) -> Result<(), String> {
        for assignment in &self.model.init {
            let field_index = self.field_index(&assignment.field)?;
            let field = &self.model.state_fields[field_index];
            match (
                &field.ty,
                &assignment.value,
                &self.state_lits[0][field_index],
            ) {
                (FieldType::Bool, Value::Bool(value), EncodedFieldState::Bool(lit)) => {
                    self.solver.add_clause(&[if *value { *lit } else { !*lit }]);
                }
                (
                    FieldType::EnumRelation {
                        left_variants,
                        right_variants,
                    },
                    Value::UInt(bits),
                    EncodedFieldState::Relation(slots),
                ) => {
                    for left_index in 0..left_variants.len() {
                        for right_index in 0..right_variants.len() {
                            let slot = slots[relation_slot_index(
                                left_index,
                                right_index,
                                right_variants.len(),
                            )];
                            self.solver.add_clause(&[
                                if relation_literal_contains(
                                    *bits,
                                    right_variants.len(),
                                    left_index,
                                    right_index,
                                ) {
                                    slot
                                } else {
                                    !slot
                                },
                            ]);
                        }
                    }
                }
                (
                    FieldType::EnumMap {
                        key_variants,
                        value_variants,
                    },
                    Value::UInt(bits),
                    EncodedFieldState::Map(slots),
                ) => {
                    for key_index in 0..key_variants.len() {
                        for value_index in 0..value_variants.len() {
                            let slot =
                                slots[map_slot_index(key_index, value_index, value_variants.len())];
                            self.solver.add_clause(&[
                                if relation_literal_contains(
                                    *bits,
                                    value_variants.len(),
                                    key_index,
                                    value_index,
                                ) {
                                    slot
                                } else {
                                    !slot
                                },
                            ]);
                        }
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

    fn encode_transitions(&mut self) -> Result<(), String> {
        for step in 0..self.depth {
            for (action_index, action) in self.model.actions.iter().enumerate() {
                let selector = self.action_lits[step][action_index];
                let guard = self.encode_bool_expr(step, &action.guard)?;
                self.solver.add_clause(&[!selector, guard]);
                for field_index in 0..self.model.state_fields.len() {
                    let field_id = self.model.state_fields[field_index].id.clone();
                    let default_expr = ExprIr::FieldRef(field_id.clone());
                    let expr = action
                        .updates
                        .iter()
                        .find(|update| update.field == field_id)
                        .map(|update| &update.value)
                        .unwrap_or(&default_expr);
                    self.encode_field_assignment_under(selector, step, field_index, expr)?;
                }
            }
        }
        Ok(())
    }

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

    fn encode_bool_expr(&mut self, step: usize, expr: &ExprIr) -> Result<Lit, String> {
        match expr {
            ExprIr::Literal(Value::Bool(value)) => Ok(self.bool_const(*value)),
            ExprIr::FieldRef(field) => {
                let index = self.field_index(field)?;
                match &self.state_lits[step][index] {
                    EncodedFieldState::Bool(lit) => Ok(*lit),
                    _ => Err(format!(
                        "backend=sat-varisat cannot use non-boolean field `{field}` as a predicate"
                    )),
                }
            }
            ExprIr::Unary { op, expr } => match op {
                UnaryOp::Not => Ok(!self.encode_bool_expr(step, expr)?),
                UnaryOp::SetIsEmpty => self.encode_is_empty(step, expr),
                UnaryOp::StringLen => Err(
                    "backend=sat-varisat does not yet support string length expressions; use explicit backend"
                        .to_string(),
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
                BinaryOp::Equal => self.encode_equal(step, left, right),
                BinaryOp::NotEqual => Ok(!self.encode_equal(step, left, right)?),
                BinaryOp::StringContains => Err(
                    "backend=sat-varisat does not yet support string contains expressions; use explicit backend"
                        .to_string(),
                ),
                BinaryOp::RegexMatch => Err(
                    "backend=sat-varisat does not yet support regex_match expressions; use explicit backend"
                        .to_string(),
                ),
                BinaryOp::RelationContains => {
                    let (left_index, right_index) = extract_pair_indexes(right, expr)?;
                    self.encode_relation_slot_expr(
                        step,
                        left,
                        left_index as usize,
                        right_index as usize,
                        None,
                    )
                }
                BinaryOp::RelationIntersects => self.encode_relation_intersects(step, left, right),
                BinaryOp::MapContainsKey => {
                    let key_index = extract_enum_index_from_expr(right, expr)? as usize;
                    self.encode_map_contains_key(step, left, key_index, None)
                }
                BinaryOp::MapContainsEntry => {
                    let (key_index, value_index) = extract_pair_indexes(right, expr)?;
                    self.encode_map_slot_expr(
                        step,
                        left,
                        key_index as usize,
                        value_index as usize,
                        None,
                    )
                }
                BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mod
                | BinaryOp::SetContains
                | BinaryOp::SetInsert
                | BinaryOp::SetRemove
                | BinaryOp::RelationInsert
                | BinaryOp::RelationRemove
                | BinaryOp::MapPut
                | BinaryOp::MapRemoveKey
                | BinaryOp::LessThan
                | BinaryOp::LessThanOrEqual
                | BinaryOp::GreaterThan
                | BinaryOp::GreaterThanOrEqual => Err(format!(
                    "backend=sat-varisat currently supports only boolean declarative expressions; unsupported operator `{op:?}`"
                )),
            },
            ExprIr::Literal(other) => Err(format!(
                "backend=sat-varisat currently supports only boolean expressions, got `{other:?}`"
            )),
        }
    }

    fn encode_equal(&mut self, step: usize, left: &ExprIr, right: &ExprIr) -> Result<Lit, String> {
        if let Some(ty) = composite_type_for_exprs(self.model, left, right) {
            self.encode_composite_equal(step, left, right, ty)
        } else {
            let a = self.encode_bool_expr(step, left)?;
            let b = self.encode_bool_expr(step, right)?;
            Ok(self.bool_equal(a, b))
        }
    }

    fn encode_composite_equal(
        &mut self,
        step: usize,
        left: &ExprIr,
        right: &ExprIr,
        ty: &FieldType,
    ) -> Result<Lit, String> {
        let mut equalities = Vec::new();
        match ty {
            FieldType::EnumRelation {
                left_variants,
                right_variants,
            } => {
                for left_index in 0..left_variants.len() {
                    for right_index in 0..right_variants.len() {
                        let a = self.encode_relation_slot_expr(
                            step,
                            left,
                            left_index,
                            right_index,
                            Some(ty),
                        )?;
                        let b = self.encode_relation_slot_expr(
                            step,
                            right,
                            left_index,
                            right_index,
                            Some(ty),
                        )?;
                        equalities.push(self.bool_equal(a, b));
                    }
                }
            }
            FieldType::EnumMap {
                key_variants,
                value_variants,
            } => {
                for key_index in 0..key_variants.len() {
                    for value_index in 0..value_variants.len() {
                        let a = self.encode_map_slot_expr(
                            step,
                            left,
                            key_index,
                            value_index,
                            Some(ty),
                        )?;
                        let b = self.encode_map_slot_expr(
                            step,
                            right,
                            key_index,
                            value_index,
                            Some(ty),
                        )?;
                        equalities.push(self.bool_equal(a, b));
                    }
                }
            }
            _ => {
                return Err(format!(
                    "backend=sat-varisat cannot compare composite type `{ty:?}`"
                ));
            }
        }
        Ok(self.bool_and_many(equalities))
    }

    fn encode_is_empty(&mut self, step: usize, expr: &ExprIr) -> Result<Lit, String> {
        match field_type_for_expr(self.model, expr) {
            Some(FieldType::EnumRelation {
                left_variants,
                right_variants,
            }) => {
                let mut negated = Vec::new();
                for left_index in 0..left_variants.len() {
                    for right_index in 0..right_variants.len() {
                        negated.push(!self.encode_relation_slot_expr(
                            step,
                            expr,
                            left_index,
                            right_index,
                            None,
                        )?);
                    }
                }
                Ok(self.bool_and_many(negated))
            }
            Some(FieldType::EnumMap {
                key_variants,
                value_variants,
            }) => {
                let mut negated = Vec::new();
                for key_index in 0..key_variants.len() {
                    for value_index in 0..value_variants.len() {
                        negated.push(!self.encode_map_slot_expr(
                            step,
                            expr,
                            key_index,
                            value_index,
                            None,
                        )?);
                    }
                }
                Ok(self.bool_and_many(negated))
            }
            Some(FieldType::EnumSet { .. }) => Err(
                "backend=sat-varisat does not yet support finite-set operations; use explicit or smt-cvc5"
                    .to_string(),
            ),
            other => Err(format!(
                "backend=sat-varisat cannot evaluate is_empty over `{other:?}`"
            )),
        }
    }

    fn encode_relation_intersects(
        &mut self,
        step: usize,
        left: &ExprIr,
        right: &ExprIr,
    ) -> Result<Lit, String> {
        let relation_ty = relation_type_for_expr(self.model, left, None)?;
        match relation_ty {
            FieldType::EnumRelation {
                left_variants,
                right_variants,
            } => {
                let mut overlaps = Vec::new();
                for left_index in 0..left_variants.len() {
                    for right_index in 0..right_variants.len() {
                        let a = self.encode_relation_slot_expr(
                            step,
                            left,
                            left_index,
                            right_index,
                            None,
                        )?;
                        let b = self.encode_relation_slot_expr(
                            step,
                            right,
                            left_index,
                            right_index,
                            None,
                        )?;
                        overlaps.push(self.bool_and(a, b));
                    }
                }
                Ok(self.bool_or_many(overlaps))
            }
            _ => unreachable!(),
        }
    }

    fn encode_map_contains_key(
        &mut self,
        step: usize,
        expr: &ExprIr,
        key_index: usize,
        expected_ty: Option<&FieldType>,
    ) -> Result<Lit, String> {
        let map_ty = map_type_for_expr(self.model, expr, expected_ty)?;
        match map_ty {
            FieldType::EnumMap { value_variants, .. } => {
                let mut slots = Vec::new();
                for value_index in 0..value_variants.len() {
                    slots.push(self.encode_map_slot_expr(
                        step,
                        expr,
                        key_index,
                        value_index,
                        expected_ty,
                    )?);
                }
                Ok(self.bool_or_many(slots))
            }
            _ => unreachable!(),
        }
    }

    fn encode_field_assignment_under(
        &mut self,
        selector: Lit,
        step: usize,
        field_index: usize,
        expr: &ExprIr,
    ) -> Result<(), String> {
        let field = &self.model.state_fields[field_index];
        match &field.ty {
            FieldType::Bool => {
                let next = self.bool_lit(step + 1, field_index)?;
                let value = self.encode_bool_expr(step, expr)?;
                self.add_equivalence_under(selector, next, value);
            }
            FieldType::EnumRelation {
                left_variants,
                right_variants,
            } => {
                for left_index in 0..left_variants.len() {
                    for right_index in 0..right_variants.len() {
                        let next =
                            self.relation_slot_lit(step + 1, field_index, left_index, right_index)?;
                        let value = self.encode_relation_slot_expr(
                            step,
                            expr,
                            left_index,
                            right_index,
                            Some(&field.ty),
                        )?;
                        self.add_equivalence_under(selector, next, value);
                    }
                }
            }
            FieldType::EnumMap {
                key_variants,
                value_variants,
            } => {
                for key_index in 0..key_variants.len() {
                    for value_index in 0..value_variants.len() {
                        let next =
                            self.map_slot_lit(step + 1, field_index, key_index, value_index)?;
                        let value = self.encode_map_slot_expr(
                            step,
                            expr,
                            key_index,
                            value_index,
                            Some(&field.ty),
                        )?;
                        self.add_equivalence_under(selector, next, value);
                    }
                }
            }
            other => {
                return Err(format!(
                    "backend=sat-varisat does not support state field `{}` of type `{}`",
                    field.name,
                    rust_type_label(other)
                ));
            }
        }
        Ok(())
    }

    fn encode_relation_slot_expr(
        &mut self,
        step: usize,
        expr: &ExprIr,
        left_index: usize,
        right_index: usize,
        expected_ty: Option<&FieldType>,
    ) -> Result<Lit, String> {
        match expr {
            ExprIr::Literal(Value::UInt(bits)) => {
                match relation_type_for_expr(self.model, expr, expected_ty)? {
                    FieldType::EnumRelation { right_variants, .. } => {
                        Ok(self.bool_const(relation_literal_contains(
                            *bits,
                            right_variants.len(),
                            left_index,
                            right_index,
                        )))
                    }
                    _ => unreachable!(),
                }
            }
            ExprIr::FieldRef(field) => {
                let field_index = self.field_index(field)?;
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
                    self.encode_relation_slot_expr(step, left, left_index, right_index, expected_ty)
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
                    self.encode_relation_slot_expr(step, left, left_index, right_index, expected_ty)
                }
            }
            other => Err(format!(
                "backend=sat-varisat does not support relation expression `{other:?}`"
            )),
        }
    }

    fn encode_map_slot_expr(
        &mut self,
        step: usize,
        expr: &ExprIr,
        key_index: usize,
        value_index: usize,
        expected_ty: Option<&FieldType>,
    ) -> Result<Lit, String> {
        match expr {
            ExprIr::Literal(Value::UInt(bits)) => {
                match map_type_for_expr(self.model, expr, expected_ty)? {
                    FieldType::EnumMap { value_variants, .. } => {
                        Ok(self.bool_const(relation_literal_contains(
                            *bits,
                            value_variants.len(),
                            key_index,
                            value_index,
                        )))
                    }
                    _ => unreachable!(),
                }
            }
            ExprIr::FieldRef(field) => {
                let field_index = self.field_index(field)?;
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
                    self.encode_map_slot_expr(step, left, key_index, value_index, expected_ty)
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
                    self.encode_map_slot_expr(step, left, key_index, value_index, expected_ty)
                }
            }
            other => Err(format!(
                "backend=sat-varisat does not support map expression `{other:?}`"
            )),
        }
    }

    fn bool_lit(&self, step: usize, field_index: usize) -> Result<Lit, String> {
        match &self.state_lits[step][field_index] {
            EncodedFieldState::Bool(lit) => Ok(*lit),
            _ => Err(format!(
                "expected boolean state for `{}`",
                self.model.state_fields[field_index].name
            )),
        }
    }

    fn relation_slot_lit(
        &self,
        step: usize,
        field_index: usize,
        left_index: usize,
        right_index: usize,
    ) -> Result<Lit, String> {
        match &self.state_lits[step][field_index] {
            EncodedFieldState::Relation(slots) => match &self.model.state_fields[field_index].ty {
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
        match &self.state_lits[step][field_index] {
            EncodedFieldState::Map(slots) => match &self.model.state_fields[field_index].ty {
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

    fn bool_equal(&mut self, a: Lit, b: Lit) -> Lit {
        let z = self.fresh_lit();
        self.solver.add_clause(&[!z, !a, b]);
        self.solver.add_clause(&[!z, a, !b]);
        self.solver.add_clause(&[z, !a, !b]);
        self.solver.add_clause(&[z, a, b]);
        z
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

    fn add_equivalence_under(&mut self, condition: Lit, target: Lit, value: Lit) {
        self.solver.add_clause(&[!condition, !target, value]);
        self.solver.add_clause(&[!condition, target, !value]);
    }

    fn bool_const(&mut self, value: bool) -> Lit {
        let lit = self.fresh_lit();
        self.solver.add_clause(&[if value { lit } else { !lit }]);
        lit
    }

    fn field_index(&self, field_id: &str) -> Result<usize, String> {
        self.model
            .state_fields
            .iter()
            .position(|field| field.id == field_id)
            .ok_or_else(|| format!("unknown field `{field_id}`"))
    }

    fn fresh_lit(&mut self) -> Lit {
        let lit = Lit::from_var(Var::from_index(self.next_var_index), true);
        self.next_var_index += 1;
        lit
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
        if !matches!(
            field.ty,
            FieldType::Bool | FieldType::EnumRelation { .. } | FieldType::EnumMap { .. }
        ) {
            return Err(format!(
                "backend=sat-varisat currently supports bool, FiniteRelation, and FiniteMap state fields only; `{}` is `{}`",
                field.name,
                rust_type_label(&field.ty)
            ));
        }
    }
    Ok(())
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
fn allocate_field_state(field: &StateField, alloc: &mut impl FnMut() -> Lit) -> EncodedFieldState {
    match &field.ty {
        FieldType::Bool => EncodedFieldState::Bool(alloc()),
        FieldType::EnumRelation {
            left_variants,
            right_variants,
        } => EncodedFieldState::Relation(
            (0..left_variants.len().saturating_mul(right_variants.len()))
                .map(|_| alloc())
                .collect(),
        ),
        FieldType::EnumMap {
            key_variants,
            value_variants,
        } => EncodedFieldState::Map(
            (0..key_variants.len().saturating_mul(value_variants.len()))
                .map(|_| alloc())
                .collect(),
        ),
        _ => EncodedFieldState::Bool(alloc()),
    }
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
            | BinaryOp::MapPut
            | BinaryOp::MapRemoveKey => field_type_for_expr(model, left),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(feature = "varisat-backend")]
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

#[cfg(feature = "varisat-backend")]
fn relation_type_for_expr<'a>(
    model: &'a ModelIr,
    expr: &'a ExprIr,
    expected_ty: Option<&'a FieldType>,
) -> Result<&'a FieldType, String> {
    match expected_ty.or_else(|| field_type_for_expr(model, expr)) {
        Some(ty @ FieldType::EnumRelation { .. }) => Ok(ty),
        other => Err(format!(
            "backend=sat-varisat expected relation expression, got `{other:?}`"
        )),
    }
}

#[cfg(feature = "varisat-backend")]
fn map_type_for_expr<'a>(
    model: &'a ModelIr,
    expr: &'a ExprIr,
    expected_ty: Option<&'a FieldType>,
) -> Result<&'a FieldType, String> {
    match expected_ty.or_else(|| field_type_for_expr(model, expr)) {
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
