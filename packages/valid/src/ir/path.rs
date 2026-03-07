use std::collections::BTreeSet;

use crate::ir::{
    ActionIr, Decision, DecisionKind, DecisionOutcome, DecisionPoint, ExprIr, UpdateIr,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Path {
    pub decisions: Vec<Decision>,
}

impl Path {
    pub fn new(decisions: Vec<Decision>) -> Self {
        Self { decisions }
    }

    pub fn from_action(action: &ActionIr, guard_enabled: bool) -> Self {
        build_path_from_parts(
            &action.action_id,
            &action.reads,
            &action.writes,
            action.path_tags.clone(),
            Some(render_expr_label(&action.guard)),
            action
                .updates
                .iter()
                .map(|update| (update.field.clone(), render_update_label(update)))
                .collect(),
            guard_enabled,
        )
    }

    pub fn from_legacy_tags(tags: Vec<String>) -> Self {
        let tags = normalize_path_tags(tags);
        Self {
            decisions: vec![Decision {
                point: DecisionPoint {
                    decision_id: "legacy#path".to_string(),
                    action_id: "legacy".to_string(),
                    kind: DecisionKind::Guard,
                    label: "legacy path".to_string(),
                    field: None,
                    reads: Vec::new(),
                    writes: Vec::new(),
                    path_tags: tags,
                },
                outcome: DecisionOutcome::GuardTrue,
            }],
        }
    }

    pub fn extend(&mut self, other: Path) {
        self.decisions.extend(other.decisions);
    }

    pub fn legacy_path_tags(&self) -> Vec<String> {
        let tags = self
            .decisions
            .iter()
            .flat_map(|decision| decision.point.legacy_path_tags())
            .collect::<BTreeSet<_>>();
        if tags.is_empty() {
            vec!["transition_path".to_string()]
        } else {
            tags.into_iter().collect()
        }
    }

    pub fn decision_ids(&self) -> Vec<String> {
        self.decisions.iter().map(Decision::decision_id).collect()
    }
}

pub fn build_path_from_parts(
    action_id: &str,
    reads: &[String],
    writes: &[String],
    path_tags: Vec<String>,
    guard_label: Option<String>,
    updates: Vec<(String, String)>,
    guard_enabled: bool,
) -> Path {
    let path_tags = normalize_path_tags(path_tags);
    let mut decisions = Vec::new();
    decisions.push(Decision {
        point: DecisionPoint {
            decision_id: format!("{action_id}#guard"),
            action_id: action_id.to_string(),
            kind: DecisionKind::Guard,
            label: guard_label.unwrap_or_else(|| format!("{action_id} transition")),
            field: None,
            reads: reads.to_vec(),
            writes: writes.to_vec(),
            path_tags: path_tags.clone(),
        },
        outcome: if guard_enabled {
            DecisionOutcome::GuardTrue
        } else {
            DecisionOutcome::GuardFalse
        },
    });
    if guard_enabled {
        for (field, label) in updates {
            decisions.push(Decision {
                point: DecisionPoint {
                    decision_id: format!("{action_id}#update:{field}"),
                    action_id: action_id.to_string(),
                    kind: DecisionKind::StateUpdate,
                    label,
                    field: Some(field),
                    reads: reads.to_vec(),
                    writes: writes.to_vec(),
                    path_tags: path_tags.clone(),
                },
                outcome: DecisionOutcome::UpdateApplied,
            });
        }
    }
    if decisions.is_empty() {
        return Path {
            decisions: vec![Decision {
                point: DecisionPoint {
                    decision_id: format!("{action_id}#path"),
                    action_id: action_id.to_string(),
                    kind: DecisionKind::Guard,
                    label: "legacy path".to_string(),
                    field: None,
                    reads: reads.to_vec(),
                    writes: writes.to_vec(),
                    path_tags,
                },
                outcome: DecisionOutcome::GuardTrue,
            }],
        };
    }
    Path { decisions }
}

pub fn infer_decision_path_tags<'a, RI, WI>(
    action_id: &str,
    reads: RI,
    writes: WI,
    guard: Option<&str>,
    effect: Option<&str>,
) -> Vec<String>
where
    RI: IntoIterator<Item = &'a str>,
    WI: IntoIterator<Item = &'a str>,
{
    let reads = reads.into_iter().collect::<Vec<_>>();
    let writes = writes.into_iter().collect::<Vec<_>>();
    let mut tags = BTreeSet::new();
    if guard.is_some() {
        tags.insert("guard_path".to_string());
    }
    if !reads.is_empty() {
        tags.insert("read_path".to_string());
    }
    if !writes.is_empty() {
        tags.insert("write_path".to_string());
    }
    let mut text = action_id.to_ascii_lowercase();
    for part in &reads {
        text.push(' ');
        text.push_str(&part.to_ascii_lowercase());
    }
    for part in &writes {
        text.push(' ');
        text.push_str(&part.to_ascii_lowercase());
    }
    if let Some(guard) = guard {
        text.push(' ');
        text.push_str(&guard.to_ascii_lowercase());
    }
    if let Some(effect) = effect {
        text.push(' ');
        text.push_str(&effect.to_ascii_lowercase());
    }
    for (needle, tag) in [
        ("allow", "allow_path"),
        ("deny", "deny_path"),
        ("boundary", "boundary_path"),
        ("exception", "exception_path"),
        ("session", "session_path"),
        ("lock", "state_gate_path"),
    ] {
        if text.contains(needle) {
            tags.insert(tag.to_string());
        }
    }
    if tags.is_empty() {
        tags.insert("transition_path".to_string());
    }
    tags.into_iter().collect()
}

pub fn decision_path_tags<'a, RI, WI>(
    explicit_tags: &[&str],
    action_id: &str,
    reads: RI,
    writes: WI,
    guard: Option<&str>,
    effect: Option<&str>,
) -> Vec<String>
where
    RI: IntoIterator<Item = &'a str>,
    WI: IntoIterator<Item = &'a str>,
{
    let mut tags = explicit_tags
        .iter()
        .map(|tag| tag.to_string())
        .collect::<BTreeSet<_>>();
    tags.extend(infer_decision_path_tags(
        action_id, reads, writes, guard, effect,
    ));
    tags.into_iter().collect()
}

fn normalize_path_tags(tags: Vec<String>) -> Vec<String> {
    let tags = tags
        .into_iter()
        .filter(|tag| !tag.is_empty())
        .collect::<BTreeSet<_>>();
    if tags.is_empty() {
        vec!["transition_path".to_string()]
    } else {
        tags.into_iter().collect()
    }
}

fn render_expr_label(expr: &ExprIr) -> String {
    format!("{expr:?}")
}

fn render_update_label(update: &UpdateIr) -> String {
    format!("{} = {:?}", update.field, update.value)
}
