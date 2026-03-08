use crate::{
    ir::ModelIr,
    support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode},
};

use super::{
    transition::{apply_action, build_initial_state},
    MachineState,
};

pub fn replay_actions(model: &ModelIr, action_ids: &[String]) -> Result<MachineState, Diagnostic> {
    let mut state = build_initial_state(model)?;
    for action_id in action_ids {
        state = apply_action(model, &state, action_id)?.ok_or_else(|| {
            Diagnostic::new(
                ErrorCode::InvalidTransitionUpdate,
                DiagnosticSegment::KernelTransition,
                format!("action `{action_id}` was not enabled during replay"),
            )
            .with_help("replay only evidence traces emitted from an executed run")
        })?;
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use crate::ir::{
        ActionIr, ExprIr, FieldType, InitAssignment, ModelIr, SourceSpan, StateField, UpdateIr,
        Value,
    };

    use super::replay_actions;

    #[test]
    fn replays_trace_to_terminal_state() {
        let model = ModelIr {
            model_id: "Replay".to_string(),
            state_fields: vec![StateField {
                id: "x".to_string(),
                name: "x".to_string(),
                ty: FieldType::BoundedU8 { min: 0, max: 2 },
                span: SourceSpan { line: 1, column: 1 },
            }],
            init: vec![InitAssignment {
                field: "x".to_string(),
                value: Value::UInt(0),
                span: SourceSpan { line: 2, column: 1 },
            }],
            actions: vec![ActionIr {
                action_id: "Inc".to_string(),
                label: "Inc".to_string(),
                role: crate::ir::action::ActionRole::Business,
                reads: vec!["x".to_string()],
                writes: vec!["x".to_string()],
                path_tags: vec!["write_path".to_string()],
                guard: ExprIr::Literal(Value::Bool(true)),
                updates: vec![UpdateIr {
                    field: "x".to_string(),
                    value: ExprIr::Binary {
                        op: crate::ir::BinaryOp::Add,
                        left: Box::new(ExprIr::FieldRef("x".to_string())),
                        right: Box::new(ExprIr::Literal(Value::UInt(1))),
                    },
                }],
            }],
            predicates: vec![],
            scenarios: vec![],
            properties: vec![],
        };

        let terminal = replay_actions(&model, &["Inc".to_string(), "Inc".to_string()]).unwrap();
        assert_eq!(terminal.values, vec![Value::UInt(2)]);
    }
}
