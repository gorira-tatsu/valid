use crate::ir::expr::ExprIr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyIr {
    pub property_id: String,
    pub kind: PropertyKind,
    pub expr: ExprIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertyKind {
    Invariant,
}
