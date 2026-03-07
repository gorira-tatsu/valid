use crate::ir::ModelIr;
#[cfg(feature = "varisat-backend")]
use crate::ir::{BinaryOp, ExprIr, UnaryOp, Value};
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
struct CnfEncoder<'a> {
    model: &'a ModelIr,
    property_id: &'a str,
    depth: usize,
    solver: Solver<'static>,
    next_var_index: usize,
    state_lits: Vec<Vec<Lit>>,
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
                    .map(|_| alloc())
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
            let lit = self.state_lits[0][field_index];
            match assignment.value {
                Value::Bool(value) => self.solver.add_clause(&[if value { lit } else { !lit }]),
                _ => {
                    return Err(format!(
                        "backend=sat-varisat only supports boolean init assignments, got `{}`",
                        assignment.field
                    ))
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
                for field in &self.model.state_fields {
                    let field_index = self.field_index(&field.id)?;
                    let next = self.state_lits[step + 1][field_index];
                    let expr = action
                        .updates
                        .iter()
                        .find(|update| update.field == field.id)
                        .ok_or_else(|| {
                            format!(
                                "missing update for field `{}` in action `{}`",
                                field.id, action.action_id
                            )
                        })?;
                    let value = self.encode_bool_expr(step, &expr.value)?;
                    self.add_equivalence_under(selector, next, value);
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
                Ok(self.state_lits[step][index])
            }
            ExprIr::Unary { op, expr } => match op {
                UnaryOp::Not => Ok(!self.encode_bool_expr(step, expr)?),
                UnaryOp::SetIsEmpty => Err(
                    "backend=sat-varisat does not yet support set operations; use explicit or smt-cvc5"
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
                BinaryOp::Equal => self.encode_bool_equal(step, left, right),
                BinaryOp::NotEqual => Ok(!self.encode_bool_equal(step, left, right)?),
                BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mod
                | BinaryOp::SetContains
                | BinaryOp::SetInsert
                | BinaryOp::SetRemove
                | BinaryOp::RelationContains
                | BinaryOp::RelationInsert
                | BinaryOp::RelationRemove
                | BinaryOp::RelationIntersects
                | BinaryOp::MapContainsKey
                | BinaryOp::MapContainsEntry
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

    fn encode_bool_equal(
        &mut self,
        step: usize,
        left: &ExprIr,
        right: &ExprIr,
    ) -> Result<Lit, String> {
        let a = self.encode_bool_expr(step, left)?;
        let b = self.encode_bool_expr(step, right)?;
        let z = self.fresh_lit();
        self.solver.add_clause(&[!z, !a, b]);
        self.solver.add_clause(&[!z, a, !b]);
        self.solver.add_clause(&[z, !a, !b]);
        self.solver.add_clause(&[z, a, b]);
        Ok(z)
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
        if field.ty != FieldType::Bool {
            return Err(format!(
                "backend=sat-varisat currently supports boolean state fields only; `{}` is `{}`",
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
        FieldType::BoundedU8 { .. } => "u8",
        FieldType::BoundedU16 { .. } => "u16",
        FieldType::BoundedU32 { .. } => "u32",
        FieldType::Enum { .. } => "enum",
        FieldType::EnumSet { .. } => "FiniteEnumSet",
    }
}
