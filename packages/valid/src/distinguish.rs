use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::{
    ir::{ModelIr, Path, PropertyIr, Value},
    kernel::{
        eval::eval_expr,
        transition::{apply_action_transition, build_initial_state},
        MachineState,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistinguishCheckpoint {
    pub index: usize,
    pub action_id: Option<String>,
    pub action_label: Option<String>,
    pub left_state: BTreeMap<String, Value>,
    pub right_state: BTreeMap<String, Value>,
    pub left_guard_enabled: Option<bool>,
    pub right_guard_enabled: Option<bool>,
    pub left_property_holds: Option<bool>,
    pub right_property_holds: Option<bool>,
    pub left_path: Option<Path>,
    pub right_path: Option<Path>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistinguishingTrace {
    pub schema_version: String,
    pub left_model_id: String,
    pub right_model_id: String,
    pub left_property_id: Option<String>,
    pub right_property_id: Option<String>,
    pub divergence_kind: String,
    pub divergence_index: usize,
    pub summary: String,
    pub checkpoints: Vec<DistinguishCheckpoint>,
    pub review_hints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistinguishOptions {
    pub left_property_id: Option<String>,
    pub right_property_id: Option<String>,
    pub max_depth: usize,
}

#[derive(Debug, Clone)]
struct SearchNode {
    left: MachineState,
    right: MachineState,
    checkpoints: Vec<DistinguishCheckpoint>,
    depth: usize,
}

pub fn find_distinguishing_trace(
    left_model: &ModelIr,
    right_model: &ModelIr,
    options: &DistinguishOptions,
) -> Result<DistinguishingTrace, String> {
    let left_property = resolve_property(left_model, options.left_property_id.as_deref())?;
    let right_property = resolve_property(right_model, options.right_property_id.as_deref())?;

    let left_initial = build_initial_state(left_model).map_err(|diagnostic| diagnostic.message)?;
    let right_initial =
        build_initial_state(right_model).map_err(|diagnostic| diagnostic.message)?;

    let initial_checkpoint = DistinguishCheckpoint {
        index: 0,
        action_id: None,
        action_label: None,
        left_state: left_initial.as_named_map(left_model),
        right_state: right_initial.as_named_map(right_model),
        left_guard_enabled: None,
        right_guard_enabled: None,
        left_property_holds: eval_property(left_model, left_property, &left_initial)?,
        right_property_holds: eval_property(right_model, right_property, &right_initial)?,
        left_path: None,
        right_path: None,
        note: Some("initial state comparison".to_string()),
    };

    if state_key(&initial_checkpoint.left_state) != state_key(&initial_checkpoint.right_state) {
        return Ok(build_trace(
            left_model,
            right_model,
            options,
            "initial_state",
            0,
            "models diverge before any action is executed".to_string(),
            vec![initial_checkpoint],
        ));
    }
    if property_diverges(&initial_checkpoint) {
        return Ok(build_trace(
            left_model,
            right_model,
            options,
            "property_value",
            0,
            "properties disagree on the initial state".to_string(),
            vec![initial_checkpoint],
        ));
    }

    let action_ids = collect_action_ids(left_model, right_model);
    let mut queue = VecDeque::from([SearchNode {
        left: left_initial.clone(),
        right: right_initial.clone(),
        checkpoints: vec![initial_checkpoint],
        depth: 0,
    }]);
    let mut visited =
        BTreeSet::from([(state_pair_key(left_model, &left_initial, right_model, &right_initial))]);

    while let Some(node) = queue.pop_front() {
        if node.depth >= options.max_depth {
            continue;
        }

        for action_id in &action_ids {
            let left_variants = collect_variants(left_model, &node.left, action_id, left_property)?;
            let right_variants =
                collect_variants(right_model, &node.right, action_id, right_property)?;

            if left_variants.is_empty() && right_variants.is_empty() {
                continue;
            }

            let checkpoint_index = node.checkpoints.len();
            let action_label = left_variants
                .iter()
                .chain(right_variants.iter())
                .map(|variant| variant.action_label.clone())
                .next()
                .unwrap_or_else(|| action_id.clone());

            if left_variants.is_empty() != right_variants.is_empty() {
                let left_choice = left_variants.first();
                let right_choice = right_variants.first();
                let checkpoint = DistinguishCheckpoint {
                    index: checkpoint_index,
                    action_id: Some(action_id.clone()),
                    action_label: Some(action_label),
                    left_state: left_choice
                        .map(|variant| variant.state.clone())
                        .unwrap_or_else(|| node.left.as_named_map(left_model)),
                    right_state: right_choice
                        .map(|variant| variant.state.clone())
                        .unwrap_or_else(|| node.right.as_named_map(right_model)),
                    left_guard_enabled: Some(!left_variants.is_empty()),
                    right_guard_enabled: Some(!right_variants.is_empty()),
                    left_property_holds: left_choice
                        .map(|variant| variant.property_holds)
                        .unwrap_or(
                            node.checkpoints
                                .last()
                                .and_then(|item| item.left_property_holds),
                        ),
                    right_property_holds: right_choice
                        .map(|variant| variant.property_holds)
                        .unwrap_or(
                            node.checkpoints
                                .last()
                                .and_then(|item| item.right_property_holds),
                        ),
                    left_path: left_choice.and_then(|variant| variant.path.clone()),
                    right_path: right_choice.and_then(|variant| variant.path.clone()),
                    note: Some(format!(
                        "action `{action_id}` is enabled on one side and disabled on the other"
                    )),
                };
                let mut checkpoints = node.checkpoints.clone();
                checkpoints.push(checkpoint);
                return Ok(build_trace(
                    left_model,
                    right_model,
                    options,
                    "action_availability",
                    checkpoint_index,
                    format!("action `{action_id}` separates the two interpretations"),
                    checkpoints,
                ));
            }

            let left_map = keyed_variants(&left_variants);
            let right_map = keyed_variants(&right_variants);
            if left_map.keys().collect::<Vec<_>>() != right_map.keys().collect::<Vec<_>>() {
                let left_choice = left_map
                    .iter()
                    .find(|(key, _)| !right_map.contains_key(*key))
                    .or_else(|| left_map.iter().next())
                    .map(|(_, value)| value)
                    .expect("left variants should not be empty");
                let right_choice = right_map
                    .iter()
                    .find(|(key, _)| !left_map.contains_key(*key))
                    .or_else(|| right_map.iter().next())
                    .map(|(_, value)| value)
                    .expect("right variants should not be empty");
                let checkpoint = DistinguishCheckpoint {
                    index: checkpoint_index,
                    action_id: Some(action_id.clone()),
                    action_label: Some(action_label),
                    left_state: left_choice.state.clone(),
                    right_state: right_choice.state.clone(),
                    left_guard_enabled: Some(true),
                    right_guard_enabled: Some(true),
                    left_property_holds: left_choice.property_holds,
                    right_property_holds: right_choice.property_holds,
                    left_path: left_choice.path.clone(),
                    right_path: right_choice.path.clone(),
                    note: Some(format!(
                        "action `{action_id}` reaches different successor states"
                    )),
                };
                let mut checkpoints = node.checkpoints.clone();
                checkpoints.push(checkpoint);
                return Ok(build_trace(
                    left_model,
                    right_model,
                    options,
                    "state_transition",
                    checkpoint_index,
                    format!("action `{action_id}` produces different state transitions"),
                    checkpoints,
                ));
            }

            for key in left_map.keys() {
                let left_choice = left_map
                    .get(key)
                    .expect("key should exist in left map after comparison");
                let right_choice = right_map
                    .get(key)
                    .expect("key should exist in right map after comparison");
                let checkpoint = DistinguishCheckpoint {
                    index: checkpoint_index,
                    action_id: Some(action_id.clone()),
                    action_label: Some(action_label.clone()),
                    left_state: left_choice.state.clone(),
                    right_state: right_choice.state.clone(),
                    left_guard_enabled: Some(true),
                    right_guard_enabled: Some(true),
                    left_property_holds: left_choice.property_holds,
                    right_property_holds: right_choice.property_holds,
                    left_path: left_choice.path.clone(),
                    right_path: right_choice.path.clone(),
                    note: None,
                };
                if property_diverges(&checkpoint) {
                    let mut checkpoints = node.checkpoints.clone();
                    checkpoints.push(checkpoint);
                    return Ok(build_trace(
                        left_model,
                        right_model,
                        options,
                        "property_value",
                        checkpoint_index,
                        format!("properties disagree after action `{action_id}`"),
                        checkpoints,
                    ));
                }
                let pair_key = (key.clone(), key.clone());
                if visited.insert(pair_key) {
                    let mut checkpoints = node.checkpoints.clone();
                    checkpoints.push(checkpoint);
                    queue.push_back(SearchNode {
                        left: left_choice.machine_state.clone(),
                        right: right_choice.machine_state.clone(),
                        checkpoints,
                        depth: node.depth + 1,
                    });
                }
            }
        }
    }

    Err(format!(
        "no distinguishing trace found within depth {}; increase max_depth or provide narrower properties",
        options.max_depth
    ))
}

#[derive(Debug, Clone)]
struct VariantObservation {
    machine_state: MachineState,
    state: BTreeMap<String, Value>,
    action_label: String,
    path: Option<Path>,
    property_holds: Option<bool>,
}

fn collect_variants(
    model: &ModelIr,
    state: &MachineState,
    action_id: &str,
    property: Option<&PropertyIr>,
) -> Result<Vec<VariantObservation>, String> {
    let matching = model
        .actions
        .iter()
        .filter(|action| action.action_id == action_id)
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return Ok(Vec::new());
    }
    let mut variants = Vec::new();
    for action in matching {
        match apply_action_transition(model, state, action)
            .map_err(|diagnostic| diagnostic.message)?
        {
            Some(next_state) => {
                let property_holds = eval_property(model, property, &next_state)?;
                variants.push(VariantObservation {
                    state: next_state.as_named_map(model),
                    machine_state: next_state,
                    action_label: action.label.clone(),
                    path: Some(action.decision_path_for_guard(true)),
                    property_holds,
                })
            }
            None => {}
        }
    }
    Ok(variants)
}

fn build_trace(
    left_model: &ModelIr,
    right_model: &ModelIr,
    options: &DistinguishOptions,
    divergence_kind: &str,
    divergence_index: usize,
    summary: String,
    checkpoints: Vec<DistinguishCheckpoint>,
) -> DistinguishingTrace {
    DistinguishingTrace {
        schema_version: "1.0.0".to_string(),
        left_model_id: left_model.model_id.clone(),
        right_model_id: right_model.model_id.clone(),
        left_property_id: options.left_property_id.clone(),
        right_property_id: options.right_property_id.clone(),
        divergence_kind: divergence_kind.to_string(),
        divergence_index,
        summary,
        checkpoints,
        review_hints: review_hints(divergence_kind),
    }
}

fn review_hints(divergence_kind: &str) -> Vec<String> {
    match divergence_kind {
        "initial_state" => vec![
            "review init assignments first because the models already disagree before any action"
                .to_string(),
        ],
        "action_availability" => vec![
            "compare the guard expressions and scenario assumptions for the divergent action"
                .to_string(),
            "check whether the action sets are intentionally asymmetric".to_string(),
        ],
        "state_transition" => vec![
            "compare the post-state updates for the divergent action".to_string(),
            "review write sets and path tags to confirm the intended branch semantics".to_string(),
        ],
        "property_value" => vec![
            "inspect the property predicates on the shared prefix state".to_string(),
            "use the last checkpoint to review which field values changed the interpretation"
                .to_string(),
        ],
        _ => vec![
            "review the last checkpoint because it contains the first observable divergence"
                .to_string(),
        ],
    }
}

fn property_diverges(checkpoint: &DistinguishCheckpoint) -> bool {
    matches!(
        (checkpoint.left_property_holds, checkpoint.right_property_holds),
        (Some(left), Some(right)) if left != right
    )
}

fn resolve_property<'a>(
    model: &'a ModelIr,
    property_id: Option<&str>,
) -> Result<Option<&'a PropertyIr>, String> {
    let Some(property_id) = property_id else {
        return Ok(None);
    };
    model
        .properties
        .iter()
        .find(|property| property.property_id == property_id)
        .map(Some)
        .ok_or_else(|| {
            format!(
                "unknown property `{property_id}` for model `{}`",
                model.model_id
            )
        })
}

fn eval_property(
    model: &ModelIr,
    property: Option<&PropertyIr>,
    state: &MachineState,
) -> Result<Option<bool>, String> {
    let Some(property) = property else {
        return Ok(None);
    };
    match eval_expr(model, state, &property.expr).map_err(|diagnostic| diagnostic.message)? {
        Value::Bool(value) => Ok(Some(value)),
        _ => Err(format!(
            "property `{}` did not evaluate to bool during distinguishing search",
            property.property_id
        )),
    }
}

fn collect_action_ids(left_model: &ModelIr, right_model: &ModelIr) -> Vec<String> {
    left_model
        .actions
        .iter()
        .map(|action| action.action_id.clone())
        .chain(
            right_model
                .actions
                .iter()
                .map(|action| action.action_id.clone()),
        )
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn keyed_variants(variants: &[VariantObservation]) -> BTreeMap<String, &VariantObservation> {
    variants
        .iter()
        .map(|variant| (state_key(&variant.state), variant))
        .collect()
}

fn state_pair_key(
    left_model: &ModelIr,
    left: &MachineState,
    right_model: &ModelIr,
    right: &MachineState,
) -> (String, String) {
    (
        state_key(&left.as_named_map(left_model)),
        state_key(&right.as_named_map(right_model)),
    )
}

fn state_key(state: &BTreeMap<String, Value>) -> String {
    serde_json::to_string(state).expect("state map should serialize")
}

#[cfg(test)]
mod tests {
    use super::{find_distinguishing_trace, DistinguishOptions};
    use crate::frontend::compile_model;

    fn compare(
        left: &str,
        right: &str,
        left_property_id: Option<&str>,
        right_property_id: Option<&str>,
    ) -> super::DistinguishingTrace {
        let left_model = compile_model(left).expect("left model should compile");
        let right_model = compile_model(right).expect("right model should compile");
        find_distinguishing_trace(
            &left_model,
            &right_model,
            &DistinguishOptions {
                left_property_id: left_property_id.map(str::to_string),
                right_property_id: right_property_id.map(str::to_string),
                max_depth: 4,
            },
        )
        .expect("comparison should find a distinguishing trace")
    }

    #[test]
    fn finds_property_divergence_on_shared_prefix() {
        let source = "\
model MultiPropertyCounter
state:
  x: u8[0..2]
init:
  x = 0
action Inc:
  pre: x <= 1
  post:
    x = x + 1
action Reset:
  pre: x <= 2
  post:
    x = 0
property P_SAFE:
  invariant: x <= 2
property P_STRICT:
  invariant: x <= 1
";
        let trace = compare(source, source, Some("P_SAFE"), Some("P_STRICT"));
        assert_eq!(trace.divergence_kind, "property_value");
        assert_eq!(trace.divergence_index, 2);
        assert_eq!(
            trace
                .checkpoints
                .last()
                .and_then(|item| item.action_id.as_deref()),
            Some("Inc")
        );
        assert_eq!(
            trace
                .checkpoints
                .last()
                .and_then(|item| item.left_property_holds),
            Some(true)
        );
        assert_eq!(
            trace
                .checkpoints
                .last()
                .and_then(|item| item.right_property_holds),
            Some(false)
        );
    }

    #[test]
    fn finds_transition_divergence_between_models() {
        let left = "\
model ResetCounter
state:
  x: u8[0..2]
init:
  x = 0
action Inc:
  pre: x <= 1
  post:
    x = x + 1
action Reset:
  pre: x <= 2
  post:
    x = 0
";
        let right = "\
model StayCounter
state:
  x: u8[0..2]
init:
  x = 0
action Inc:
  pre: x <= 1
  post:
    x = x + 1
action Reset:
  pre: x <= 2
  post:
    x = x
";
        let trace = compare(left, right, None, None);
        assert_eq!(trace.divergence_kind, "state_transition");
        assert_eq!(
            trace
                .checkpoints
                .last()
                .and_then(|item| item.action_id.as_deref()),
            Some("Reset")
        );
    }
}
