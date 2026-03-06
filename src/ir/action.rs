use crate::ir::expr::ExprIr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionIr {
    pub action_id: String,
    pub label: String,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub guard: ExprIr,
    pub updates: Vec<UpdateIr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateIr {
    pub field: String,
    pub value: ExprIr,
}
