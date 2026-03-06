//! Selfcheck smoke suite for the current kernel and engine contracts.

use crate::{
    engine::{check_explicit, CheckOutcome, PropertySelection, RunPlan},
    frontend::compile_model,
    ir::{
        ActionIr, BinaryOp, ExprIr, FieldType, InitAssignment, ModelIr, PropertyIr, PropertyKind,
        SourceSpan, StateField, UpdateIr, Value,
    },
    kernel::{
        eval::eval_expr,
        guard::evaluate_guard,
        replay::replay_actions,
        transition::{apply_action, build_initial_state},
    },
    support::{artifact::selfcheck_report_path, io::write_text_file},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfcheckCase {
    pub case_id: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelfcheckReport {
    pub schema_version: String,
    pub suite_id: String,
    pub run_id: String,
    pub status: String,
    pub cases: Vec<SelfcheckCase>,
}

pub fn run_smoke_selfcheck() -> SelfcheckReport {
    let mut cases = Vec::new();
    cases.push(run_expr_case());
    cases.push(run_guard_case());
    cases.push(run_transition_case());
    cases.push(run_replay_case());
    cases.push(run_engine_case());

    let status = if cases.iter().all(|case| case.status == "ok") {
        "ok"
    } else {
        "failed"
    };

    SelfcheckReport {
        schema_version: "1.0.0".to_string(),
        suite_id: "selfcheck-smoke".to_string(),
        run_id: "selfcheck-local-0001".to_string(),
        status: status.to_string(),
        cases,
    }
}

pub fn render_selfcheck_json(report: &SelfcheckReport) -> String {
    let mut out = String::new();
    out.push('{');
    out.push_str(&format!("\"schema_version\":\"{}\"", report.schema_version));
    out.push_str(&format!(",\"suite_id\":\"{}\"", report.suite_id));
    out.push_str(&format!(",\"run_id\":\"{}\"", report.run_id));
    out.push_str(&format!(",\"status\":\"{}\"", report.status));
    out.push_str(",\"cases\":[");
    for (index, case) in report.cases.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"case_id\":\"{}\",\"status\":\"{}\"}}",
            case.case_id, case.status
        ));
    }
    out.push_str("]}");
    out
}

pub fn write_selfcheck_artifact(report: &SelfcheckReport) -> Result<String, String> {
    let path = selfcheck_report_path(&report.suite_id, &report.run_id);
    write_text_file(&path, &render_selfcheck_json(report))?;
    Ok(path)
}

fn run_expr_case() -> SelfcheckCase {
    let model = selfcheck_model();
    let state = build_initial_state(&model).expect("selfcheck init");
    let expr = ExprIr::Binary {
        op: BinaryOp::Add,
        left: Box::new(ExprIr::FieldRef("x".to_string())),
        right: Box::new(ExprIr::Literal(Value::UInt(1))),
    };
    let status = match eval_expr(&model, &state, &expr) {
        Ok(Value::UInt(1)) => "ok",
        _ => "failed",
    };
    SelfcheckCase {
        case_id: "expr-eval".to_string(),
        status: status.to_string(),
    }
}

fn run_guard_case() -> SelfcheckCase {
    let model = selfcheck_model();
    let state = build_initial_state(&model).expect("selfcheck init");
    let action = model
        .actions
        .iter()
        .find(|item| item.action_id == "Jump")
        .expect("action");
    let status = match evaluate_guard(&model, &state, action) {
        Ok(true) => "ok",
        _ => "failed",
    };
    SelfcheckCase {
        case_id: "guard-eval".to_string(),
        status: status.to_string(),
    }
}

fn run_transition_case() -> SelfcheckCase {
    let model = selfcheck_model();
    let state = build_initial_state(&model).expect("selfcheck init");
    let status = match apply_action(&model, &state, "Jump") {
        Ok(Some(next)) if next.values == vec![Value::UInt(2)] => "ok",
        _ => "failed",
    };
    SelfcheckCase {
        case_id: "transition-apply".to_string(),
        status: status.to_string(),
    }
}

fn run_replay_case() -> SelfcheckCase {
    let model = selfcheck_model();
    let status = match replay_actions(&model, &["Jump".to_string()]) {
        Ok(state) if state.values == vec![Value::UInt(2)] => "ok",
        _ => "failed",
    };
    SelfcheckCase {
        case_id: "trace-replay".to_string(),
        status: status.to_string(),
    }
}

fn run_engine_case() -> SelfcheckCase {
    let source = "model Selfcheck\nstate:\n  x: u8[0..7]\ninit:\n  x = 0\naction Jump:\n  pre: true\n  post:\n    x = 2\nproperty P_SAFE:\n  invariant: x <= 1\n";
    let model = compile_model(source).expect("selfcheck model should compile");
    let mut plan = RunPlan::default();
    plan.property_selection = PropertySelection::ExactlyOne("P_SAFE".to_string());
    let status = match check_explicit(&model, &plan) {
        CheckOutcome::Completed(result) if result.status == crate::engine::RunStatus::Fail => "ok",
        _ => "failed",
    };
    SelfcheckCase {
        case_id: "explicit-counterexample".to_string(),
        status: status.to_string(),
    }
}

fn selfcheck_model() -> ModelIr {
    ModelIr {
        model_id: "Selfcheck".to_string(),
        state_fields: vec![StateField {
            id: "x".to_string(),
            name: "x".to_string(),
            ty: FieldType::BoundedU8 { min: 0, max: 7 },
            span: SourceSpan { line: 1, column: 1 },
        }],
        init: vec![InitAssignment {
            field: "x".to_string(),
            value: Value::UInt(0),
            span: SourceSpan { line: 2, column: 1 },
        }],
        actions: vec![ActionIr {
            action_id: "Jump".to_string(),
            label: "Jump".to_string(),
            reads: vec!["x".to_string()],
            writes: vec!["x".to_string()],
            guard: ExprIr::Literal(Value::Bool(true)),
            updates: vec![UpdateIr {
                field: "x".to_string(),
                value: ExprIr::Literal(Value::UInt(2)),
            }],
        }],
        properties: vec![PropertyIr {
            property_id: "P_SAFE".to_string(),
            kind: PropertyKind::Invariant,
            expr: ExprIr::Binary {
                op: BinaryOp::LessThanOrEqual,
                left: Box::new(ExprIr::FieldRef("x".to_string())),
                right: Box::new(ExprIr::Literal(Value::UInt(1))),
            },
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::{render_selfcheck_json, run_smoke_selfcheck, write_selfcheck_artifact};

    #[test]
    fn smoke_selfcheck_passes() {
        let report = run_smoke_selfcheck();
        assert_eq!(report.status, "ok");
        assert!(report.cases.len() >= 5);
    }

    #[test]
    fn renders_and_writes_selfcheck_report() {
        let report = run_smoke_selfcheck();
        let json = render_selfcheck_json(&report);
        assert!(json.contains("\"suite_id\":\"selfcheck-smoke\""));
        let path = write_selfcheck_artifact(&report).unwrap();
        assert!(path.ends_with("/report.json"));
    }
}
