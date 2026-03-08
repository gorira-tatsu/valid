use crate::ir::{expr::ExprIr, Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActionRole {
    Business,
    Setup,
}

impl ActionRole {
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "business" => Some(Self::Business),
            "setup" => Some(Self::Setup),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Business => "business",
            Self::Setup => "setup",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionIr {
    pub action_id: String,
    pub label: String,
    pub role: ActionRole,
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
