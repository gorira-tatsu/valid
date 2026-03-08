//! Selfcheck smoke suite for the current kernel and engine contracts.

use std::collections::BTreeMap;

use crate::{
    contract::snapshot_model,
    coverage::{collect_coverage, validate_coverage_report},
    engine::{check_explicit, AssuranceLevel, CheckOutcome, PropertySelection, RunPlan, RunStatus},
    evidence::{validate_trace, EvidenceKind, EvidenceTrace, TraceStep},
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
    support::{
        artifact::selfcheck_report_path,
        artifact_index::{record_artifact, ArtifactRecord},
        io::write_text_file,
        json::{parse_json, require_array_field, require_object, require_string_field},
        schema::{require_non_empty, require_schema_version},
    },
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
    cases.push(run_predecessor_case());
    cases.push(run_coverage_case());
    cases.push(run_contract_hash_case());

    let status = if cases.iter().all(|case| case.status == "PASS") {
        "PASS"
    } else {
        "FAIL"
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
    record_artifact(ArtifactRecord {
        artifact_kind: "selfcheck_report".to_string(),
        path: path.clone(),
        run_id: report.run_id.clone(),
        model_id: None,
        property_id: None,
        evidence_id: None,
        vector_id: None,
        suite_id: Some(report.suite_id.clone()),
    })?;
    Ok(path)
}

pub fn validate_selfcheck_report(report: &SelfcheckReport) -> Result<(), String> {
    require_schema_version(&report.schema_version)?;
    require_non_empty(&report.suite_id, "suite_id")?;
    require_non_empty(&report.run_id, "run_id")?;
    if !matches!(report.status.as_str(), "PASS" | "FAIL" | "UNKNOWN") {
        return Err("status must be PASS, FAIL, or UNKNOWN".to_string());
    }
    if report.cases.is_empty() {
        return Err("selfcheck report must contain at least one case".to_string());
    }
    for case in &report.cases {
        require_non_empty(&case.case_id, "cases[].case_id")?;
        if !matches!(case.status.as_str(), "PASS" | "FAIL" | "UNKNOWN") {
            return Err("case status must be PASS, FAIL, or UNKNOWN".to_string());
        }
    }
    Ok(())
}

pub fn validate_rendered_selfcheck_json(body: &str) -> Result<(), String> {
    let root = parse_json(body)?;
    let object = require_object(&root, "selfcheck")?;
    require_string_field(object, "schema_version")?;
    require_string_field(object, "suite_id")?;
    require_string_field(object, "run_id")?;
    require_string_field(object, "status")?;
    for case in require_array_field(object, "cases")? {
        let case_object = require_object(case, "cases[]")?;
        require_string_field(case_object, "case_id")?;
        require_string_field(case_object, "status")?;
    }
    Ok(())
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
        Ok(Value::UInt(1)) => "PASS",
        _ => "FAIL",
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
        Ok(true) => "PASS",
        _ => "FAIL",
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
        Ok(Some(next)) if next.values == vec![Value::UInt(2)] => "PASS",
        _ => "FAIL",
    };
    SelfcheckCase {
        case_id: "transition-apply".to_string(),
        status: status.to_string(),
    }
}

fn run_replay_case() -> SelfcheckCase {
    let model = selfcheck_model();
    let status = match replay_actions(&model, &["Jump".to_string()]) {
        Ok(state) if state.values == vec![Value::UInt(2)] => "PASS",
        _ => "FAIL",
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
        CheckOutcome::Completed(result) if result.status == crate::engine::RunStatus::Fail => {
            "PASS"
        }
        _ => "FAIL",
    };
    SelfcheckCase {
        case_id: "explicit-counterexample".to_string(),
        status: status.to_string(),
    }
}

fn run_predecessor_case() -> SelfcheckCase {
    let model = predecessor_selfcheck_model();
    let mut plan = RunPlan::default();
    plan.property_selection = PropertySelection::ExactlyOne("P_SAFE".to_string());
    let status = match check_explicit(&model, &plan) {
        CheckOutcome::Completed(result) if result.status == RunStatus::Fail => {
            let terminal_matches =
                result.property_result.terminal_state_id.as_deref() == Some("s-000003");
            match result.trace {
                Some(trace)
                    if terminal_matches
                        && validate_trace(&trace).is_ok()
                        && trace_matches_predecessor_chain(&trace) =>
                {
                    "PASS"
                }
                _ => "FAIL",
            }
        }
        _ => "FAIL",
    };
    SelfcheckCase {
        case_id: "predecessor-trace".to_string(),
        status: status.to_string(),
    }
}

fn run_coverage_case() -> SelfcheckCase {
    let model = coverage_selfcheck_model();
    let trace = coverage_selfcheck_trace();
    let report = collect_coverage(&model, &[trace]);
    let status = if validate_coverage_report(&report).is_ok()
        && report.model_id == "CoverageSelfcheck"
        && report.transition_coverage_percent == 100
        && report.guard_full_coverage_percent == 100
        && report.covered_actions == report.total_actions
        && report.action_execution_counts.get("Inc") == Some(&2)
        && report.action_execution_counts.get("Reset") == Some(&1)
        && report.guard_true_counts.get("Inc") == Some(&2)
        && report.guard_false_counts.get("Inc") == Some(&1)
        && report.guard_true_counts.get("Reset") == Some(&1)
        && report.guard_false_counts.get("Reset") == Some(&2)
        && report.path_tag_counts.get("inc_path") == Some(&2)
        && report.path_tag_counts.get("reset_path") == Some(&1)
        && report.visited_state_count == 4
        && report.repeated_state_count == 2
        && report.step_count == 3
        && report.max_depth_observed == 3
        && report.depth_histogram.get(&0) == Some(&1)
        && report.depth_histogram.get(&1) == Some(&1)
        && report.depth_histogram.get(&2) == Some(&1)
        && report.depth_histogram.get(&3) == Some(&1)
        && report.uncovered_guards.is_empty()
    {
        "PASS"
    } else {
        "FAIL"
    };
    SelfcheckCase {
        case_id: "coverage-aggregate".to_string(),
        status: status.to_string(),
    }
}

fn run_contract_hash_case() -> SelfcheckCase {
    let first = snapshot_model(&selfcheck_model());
    let second = snapshot_model(&selfcheck_model());
    let status = if first == second
        && first.contract_hash == second.contract_hash
        && first.contract_hash != "sha256:unknown"
    {
        "PASS"
    } else {
        "FAIL"
    };
    SelfcheckCase {
        case_id: "contract-hash-deterministic".to_string(),
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
            role: crate::ir::action::ActionRole::Business,
            reads: vec!["x".to_string()],
            writes: vec!["x".to_string()],
            path_tags: vec!["write_path".to_string()],
            guard: ExprIr::Literal(Value::Bool(true)),
            updates: vec![UpdateIr {
                field: "x".to_string(),
                value: ExprIr::Literal(Value::UInt(2)),
            }],
        }],
        predicates: vec![],
        scenarios: vec![],
        properties: vec![PropertyIr {
            property_id: "P_SAFE".to_string(),
            kind: PropertyKind::Invariant,
            layer: crate::ir::PropertyLayer::Assert,
            expr: ExprIr::Binary {
                op: BinaryOp::LessThanOrEqual,
                left: Box::new(ExprIr::FieldRef("x".to_string())),
                right: Box::new(ExprIr::Literal(Value::UInt(1))),
            },
            scope: None,
            action_filter: None,
        }],
    }
}

fn predecessor_selfcheck_model() -> ModelIr {
    ModelIr {
        model_id: "PredecessorSelfcheck".to_string(),
        state_fields: vec![StateField {
            id: "x".to_string(),
            name: "x".to_string(),
            ty: FieldType::BoundedU8 { min: 0, max: 7 },
            span: selfcheck_span(),
        }],
        init: vec![InitAssignment {
            field: "x".to_string(),
            value: Value::UInt(0),
            span: selfcheck_span(),
        }],
        actions: vec![
            ActionIr {
                action_id: "Step".to_string(),
                label: "Step".to_string(),
                role: crate::ir::action::ActionRole::Business,
                reads: vec!["x".to_string()],
                writes: vec!["x".to_string()],
                path_tags: vec!["step_path".to_string()],
                guard: ExprIr::Binary {
                    op: BinaryOp::LessThan,
                    left: Box::new(ExprIr::FieldRef("x".to_string())),
                    right: Box::new(ExprIr::Literal(Value::UInt(2))),
                },
                updates: vec![UpdateIr {
                    field: "x".to_string(),
                    value: ExprIr::Binary {
                        op: BinaryOp::Add,
                        left: Box::new(ExprIr::FieldRef("x".to_string())),
                        right: Box::new(ExprIr::Literal(Value::UInt(1))),
                    },
                }],
            },
            ActionIr {
                action_id: "Jump".to_string(),
                label: "Jump".to_string(),
                role: crate::ir::action::ActionRole::Business,
                reads: vec!["x".to_string()],
                writes: vec!["x".to_string()],
                path_tags: vec!["jump_path".to_string()],
                guard: ExprIr::Binary {
                    op: BinaryOp::Equal,
                    left: Box::new(ExprIr::FieldRef("x".to_string())),
                    right: Box::new(ExprIr::Literal(Value::UInt(2))),
                },
                updates: vec![UpdateIr {
                    field: "x".to_string(),
                    value: ExprIr::Literal(Value::UInt(3)),
                }],
            },
        ],
        predicates: vec![],
        scenarios: vec![],
        properties: vec![PropertyIr {
            property_id: "P_SAFE".to_string(),
            kind: PropertyKind::Invariant,
            layer: crate::ir::PropertyLayer::Assert,
            expr: ExprIr::Binary {
                op: BinaryOp::LessThanOrEqual,
                left: Box::new(ExprIr::FieldRef("x".to_string())),
                right: Box::new(ExprIr::Literal(Value::UInt(2))),
            },
            scope: None,
            action_filter: None,
        }],
    }
}

fn coverage_selfcheck_model() -> ModelIr {
    ModelIr {
        model_id: "CoverageSelfcheck".to_string(),
        state_fields: vec![StateField {
            id: "x".to_string(),
            name: "x".to_string(),
            ty: FieldType::BoundedU8 { min: 0, max: 2 },
            span: selfcheck_span(),
        }],
        init: vec![InitAssignment {
            field: "x".to_string(),
            value: Value::UInt(0),
            span: selfcheck_span(),
        }],
        actions: vec![
            ActionIr {
                action_id: "Inc".to_string(),
                label: "Inc".to_string(),
                role: crate::ir::action::ActionRole::Business,
                reads: vec!["x".to_string()],
                writes: vec!["x".to_string()],
                path_tags: vec!["inc_path".to_string()],
                guard: ExprIr::Binary {
                    op: BinaryOp::LessThan,
                    left: Box::new(ExprIr::FieldRef("x".to_string())),
                    right: Box::new(ExprIr::Literal(Value::UInt(2))),
                },
                updates: vec![UpdateIr {
                    field: "x".to_string(),
                    value: ExprIr::Binary {
                        op: BinaryOp::Add,
                        left: Box::new(ExprIr::FieldRef("x".to_string())),
                        right: Box::new(ExprIr::Literal(Value::UInt(1))),
                    },
                }],
            },
            ActionIr {
                action_id: "Reset".to_string(),
                label: "Reset".to_string(),
                role: crate::ir::action::ActionRole::Business,
                reads: vec!["x".to_string()],
                writes: vec!["x".to_string()],
                path_tags: vec!["reset_path".to_string()],
                guard: ExprIr::Binary {
                    op: BinaryOp::Equal,
                    left: Box::new(ExprIr::FieldRef("x".to_string())),
                    right: Box::new(ExprIr::Literal(Value::UInt(2))),
                },
                updates: vec![UpdateIr {
                    field: "x".to_string(),
                    value: ExprIr::Literal(Value::UInt(0)),
                }],
            },
        ],
        predicates: vec![],
        scenarios: vec![],
        properties: vec![PropertyIr {
            property_id: "P_SAFE".to_string(),
            kind: PropertyKind::Invariant,
            layer: crate::ir::PropertyLayer::Assert,
            expr: ExprIr::Binary {
                op: BinaryOp::LessThanOrEqual,
                left: Box::new(ExprIr::FieldRef("x".to_string())),
                right: Box::new(ExprIr::Literal(Value::UInt(2))),
            },
            scope: None,
            action_filter: None,
        }],
    }
}

fn coverage_selfcheck_trace() -> EvidenceTrace {
    EvidenceTrace {
        schema_version: "1.0.0".to_string(),
        evidence_id: "ev-selfcheck-coverage".to_string(),
        run_id: "run-selfcheck-coverage".to_string(),
        property_id: "P_SAFE".to_string(),
        evidence_kind: EvidenceKind::Trace,
        assurance_level: AssuranceLevel::Complete,
        trace_hash: "sha256:selfcheck-coverage".to_string(),
        steps: vec![
            TraceStep {
                index: 0,
                from_state_id: "s-000000".to_string(),
                action_id: Some("Inc".to_string()),
                action_label: Some("Inc".to_string()),
                to_state_id: "s-000001".to_string(),
                depth: 1,
                state_before: selfcheck_state_snapshot(0),
                state_after: selfcheck_state_snapshot(1),
                path: None,
                note: None,
            },
            TraceStep {
                index: 1,
                from_state_id: "s-000001".to_string(),
                action_id: Some("Inc".to_string()),
                action_label: Some("Inc".to_string()),
                to_state_id: "s-000002".to_string(),
                depth: 2,
                state_before: selfcheck_state_snapshot(1),
                state_after: selfcheck_state_snapshot(2),
                path: None,
                note: None,
            },
            TraceStep {
                index: 2,
                from_state_id: "s-000002".to_string(),
                action_id: Some("Reset".to_string()),
                action_label: Some("Reset".to_string()),
                to_state_id: "s-000003".to_string(),
                depth: 3,
                state_before: selfcheck_state_snapshot(2),
                state_after: selfcheck_state_snapshot(0),
                path: None,
                note: None,
            },
        ],
    }
}

fn trace_matches_predecessor_chain(trace: &EvidenceTrace) -> bool {
    let expected = [
        (
            "s-000000",
            Some("Step"),
            "s-000001",
            1u32,
            Value::UInt(0),
            Value::UInt(1),
        ),
        (
            "s-000001",
            Some("Step"),
            "s-000002",
            2u32,
            Value::UInt(1),
            Value::UInt(2),
        ),
        (
            "s-000002",
            Some("Jump"),
            "s-000003",
            3u32,
            Value::UInt(2),
            Value::UInt(3),
        ),
    ];
    trace.steps.len() == expected.len()
        && trace.steps.iter().enumerate().all(|(index, step)| {
            let (from_state_id, action_id, to_state_id, depth, before, after) = &expected[index];
            step.index == index
                && step.from_state_id == *from_state_id
                && step.action_id.as_deref() == *action_id
                && step.action_label.as_deref() == *action_id
                && step.to_state_id == *to_state_id
                && step.depth == *depth
                && step.state_before.get("x") == Some(before)
                && step.state_after.get("x") == Some(after)
        })
}

fn selfcheck_state_snapshot(x: u64) -> BTreeMap<String, Value> {
    BTreeMap::from([("x".to_string(), Value::UInt(x))])
}

fn selfcheck_span() -> SourceSpan {
    SourceSpan { line: 1, column: 1 }
}

#[cfg(test)]
mod tests {
    use super::{
        render_selfcheck_json, run_contract_hash_case, run_coverage_case, run_predecessor_case,
        run_smoke_selfcheck, validate_rendered_selfcheck_json, validate_selfcheck_report,
        write_selfcheck_artifact,
    };

    #[test]
    fn smoke_selfcheck_passes() {
        let report = run_smoke_selfcheck();
        assert_eq!(report.status, "PASS");
        assert!(report.cases.len() >= 8);
        let case_ids = report
            .cases
            .iter()
            .map(|case| case.case_id.as_str())
            .collect::<Vec<_>>();
        assert!(case_ids.contains(&"predecessor-trace"));
        assert!(case_ids.contains(&"coverage-aggregate"));
        assert!(case_ids.contains(&"contract-hash-deterministic"));
        validate_selfcheck_report(&report).unwrap();
    }

    #[test]
    fn phase_two_cases_pass() {
        assert_eq!(run_predecessor_case().status, "PASS");
        assert_eq!(run_coverage_case().status, "PASS");
        assert_eq!(run_contract_hash_case().status, "PASS");
    }

    #[test]
    fn renders_and_writes_selfcheck_report() {
        let report = run_smoke_selfcheck();
        let json = render_selfcheck_json(&report);
        assert!(json.contains("\"suite_id\":\"selfcheck-smoke\""));
        validate_rendered_selfcheck_json(&json).unwrap();
        let path = write_selfcheck_artifact(&report).unwrap();
        assert!(path.ends_with("/report.json"));
    }
}
