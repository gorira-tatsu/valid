//! Pure evaluation kernel.

pub mod eval;
pub mod guard;
pub mod replay;
pub mod transition;

use std::collections::BTreeMap;

use crate::ir::{ModelIr, Value};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MachineState {
    pub values: Vec<Value>,
}

impl MachineState {
    pub fn new(values: Vec<Value>) -> Self {
        Self { values }
    }

    pub fn get<'a>(&'a self, model: &'a ModelIr, field: &str) -> Option<&'a Value> {
        let index = model
            .state_fields
            .iter()
            .position(|item| item.id == field)?;
        self.values.get(index)
    }

    pub fn as_named_map(&self, model: &ModelIr) -> BTreeMap<String, Value> {
        model
            .state_fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                (
                    field.name.clone(),
                    self.values.get(index).cloned().unwrap_or(Value::UInt(0)),
                )
            })
            .collect()
    }
}
