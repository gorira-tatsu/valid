use crate::ir::{expr::ExprIr, Path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionIr {
    pub action_id: String,
    pub label: String,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub path_tags: Vec<String>,
    pub guard: ExprIr,
    pub updates: Vec<UpdateIr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateIr {
    pub field: String,
    pub value: ExprIr,
}

impl ActionIr {
    pub fn decision_path(&self) -> Path {
        Path::from_action(self, true)
    }

    pub fn decision_path_for_guard(&self, guard_enabled: bool) -> Path {
        Path::from_action(self, guard_enabled)
    }
}
