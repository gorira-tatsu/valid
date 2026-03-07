use crate::ir::value::Value;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExprIr {
    Literal(Value),
    FieldRef(String),
    Unary {
        op: UnaryOp,
        expr: Box<ExprIr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<ExprIr>,
        right: Box<ExprIr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Not,
    SetIsEmpty,
    StringLen,
    TemporalAlways,
    TemporalEventually,
    TemporalNext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Add,
    Sub,
    Mod,
    StringContains,
    RegexMatch,
    SetContains,
    SetInsert,
    SetRemove,
    RelationContains,
    RelationInsert,
    RelationRemove,
    RelationIntersects,
    MapContainsKey,
    MapContainsEntry,
    MapPut,
    MapRemoveKey,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Equal,
    NotEqual,
    And,
    Or,
    TemporalUntil,
}
