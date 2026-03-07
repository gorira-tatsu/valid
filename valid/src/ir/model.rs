use crate::ir::{value::Value, ActionIr, PropertyIr};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelIr {
    pub model_id: String,
    pub state_fields: Vec<StateField>,
    pub init: Vec<InitAssignment>,
    pub actions: Vec<ActionIr>,
    pub properties: Vec<PropertyIr>,
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
    BoundedU8 { min: u8, max: u8 },
    BoundedU16 { min: u16, max: u16 },
    BoundedU32 { min: u32, max: u32 },
    Enum { variants: Vec<String> },
    EnumSet { variants: Vec<String> },
}
