use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DecisionKind {
    Guard,
    StateUpdate,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DecisionOutcome {
    GuardTrue,
    GuardFalse,
    UpdateApplied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionPoint {
    pub decision_id: String,
    pub action_id: String,
    pub kind: DecisionKind,
    pub label: String,
    pub field: Option<String>,
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub path_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub point: DecisionPoint,
    pub outcome: DecisionOutcome,
}

impl DecisionPoint {
    pub fn legacy_path_tags(&self) -> Vec<String> {
        let tags = self
            .path_tags
            .iter()
            .filter(|tag| !tag.is_empty())
            .cloned()
            .collect::<BTreeSet<_>>();
        if tags.is_empty() {
            vec!["transition_path".to_string()]
        } else {
            tags.into_iter().collect()
        }
    }
}

impl Decision {
    pub fn decision_id(&self) -> String {
        match self.outcome {
            DecisionOutcome::GuardTrue => format!("{}:true", self.point.decision_id),
            DecisionOutcome::GuardFalse => format!("{}:false", self.point.decision_id),
            DecisionOutcome::UpdateApplied => self.point.decision_id.clone(),
        }
    }
}
