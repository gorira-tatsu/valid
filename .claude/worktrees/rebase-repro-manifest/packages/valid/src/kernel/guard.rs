use crate::{
    ir::{ActionIr, ModelIr, Value},
    support::diagnostics::{Diagnostic, DiagnosticSegment, ErrorCode},
};

use super::{eval::eval_expr, MachineState};

pub fn evaluate_guard(
    model: &ModelIr,
    state: &MachineState,
    action: &ActionIr,
) -> Result<bool, Diagnostic> {
    match eval_expr(model, state, &action.guard)? {
        Value::Bool(value) => Ok(value),
        _ => Err(Diagnostic::new(
            ErrorCode::EvalError,
            DiagnosticSegment::KernelEval,
            format!(
                "guard for action `{}` did not evaluate to bool",
                action.action_id
            ),
        )
        .with_help("ensure guards lower to boolean expressions")
        .with_best_practice("reserve value-producing expressions for update right-hand sides")),
    }
}
