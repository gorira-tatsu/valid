use crate::ir::{value::Value, ActionIr, PropertyIr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelIr {
    pub model_id: String,
    pub state_fields: Vec<StateField>,
    pub init: Vec<InitAssignment>,
    pub actions: Vec<ActionIr>,
    pub predicates: Vec<PredicateIr>,
    pub scenarios: Vec<ScenarioIr>,
    pub properties: Vec<PropertyIr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredicateIr {
    pub predicate_id: String,
    pub expr: crate::ir::ExprIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioIr {
    pub scenario_id: String,
    pub expr: crate::ir::ExprIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateField {
    pub id: FieldId,
    pub name: String,
    pub ty: FieldType,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitAssignment {
    pub field: FieldId,
    pub value: Value,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpan {
    pub line: usize,
    pub column: usize,
}

pub type FieldId = String;
pub type PropertyId = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    Bool,
    String {
        min_len: Option<u32>,
        max_len: Option<u32>,
    },
    BoundedU8 {
        min: u8,
        max: u8,
    },
    BoundedU16 {
        min: u16,
        max: u16,
    },
    BoundedU32 {
        min: u32,
        max: u32,
    },
    Enum {
        variants: Vec<String>,
    },
    EnumSet {
        variants: Vec<String>,
    },
    EnumRelation {
        left_variants: Vec<String>,
        right_variants: Vec<String>,
    },
    EnumMap {
        key_variants: Vec<String>,
        value_variants: Vec<String>,
    },
}
