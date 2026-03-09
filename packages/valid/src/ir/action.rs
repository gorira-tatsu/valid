use crate::ir::{expr::ExprIr, Path};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ActionRole {
    Business,
    Setup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionParameterBinding {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionIdentity {
    pub conceptual_action_id: String,
    pub concrete_action_id: String,
    pub parameter_bindings: Vec<ActionParameterBinding>,
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

pub fn parse_action_identity(action_id: &str) -> ActionIdentity {
    let Some((conceptual, remainder)) = action_id.split_once('[') else {
        return ActionIdentity {
            conceptual_action_id: action_id.to_string(),
            concrete_action_id: action_id.to_string(),
            parameter_bindings: Vec::new(),
        };
    };
    let Some(parameters) = remainder.strip_suffix(']') else {
        return ActionIdentity {
            conceptual_action_id: action_id.to_string(),
            concrete_action_id: action_id.to_string(),
            parameter_bindings: Vec::new(),
        };
    };
    let parameter_bindings = parameters
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let (name, value) = entry.split_once('=')?;
            Some(ActionParameterBinding {
                name: name.trim().to_string(),
                value: value.trim().to_string(),
            })
        })
        .collect::<Vec<_>>();
    if parameter_bindings.is_empty() {
        return ActionIdentity {
            conceptual_action_id: action_id.to_string(),
            concrete_action_id: action_id.to_string(),
            parameter_bindings,
        };
    }
    ActionIdentity {
        conceptual_action_id: conceptual.to_string(),
        concrete_action_id: action_id.to_string(),
        parameter_bindings,
    }
}
