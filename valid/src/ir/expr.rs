use crate::ir::value::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    SetIsEmpty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mod,
    SetContains,
    SetInsert,
    SetRemove,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Equal,
    NotEqual,
    And,
    Or,
}
