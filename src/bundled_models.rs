use crate::{
    api::{
        ExplainResponse, InspectResponse, OrchestrateResponse, OrchestratedRunSummary,
        TestgenResponse,
    },
    coverage::CoverageReport,
    engine::CheckOutcome,
    modeling::{
        build_machine_test_vectors_for_strategy, check_machine_outcome, check_machine_outcomes,
        collect_machine_coverage, explain_machine, property_ids, Finite, ModelingAction,
        ModelingState,
    },
    testgen::{write_generated_test_files, TestVector},
};

crate::valid_state! {
    struct State {
        x: u8,
        locked: bool,
    }
}

crate::valid_actions! {
    enum Action {
        Inc => "INC",
        Lock => "LOCK",
        Unlock => "UNLOCK",
    }
}

crate::valid_model! {
    model CounterModel<State, Action>;
    init [State {
        x: 0,
        locked: false,
    }];
    step |state, action| {
        match action {
            Action::Inc if !state.locked && state.x < 3 => vec![State {
                x: state.x + 1,
                locked: state.locked,
            }],
            Action::Lock => vec![State {
                x: state.x,
                locked: true,
            }],
            Action::Unlock => vec![State {
                x: state.x,
                locked: false,
            }],
            _ => Vec::new(),
        }
    }
    properties {
        invariant P_RANGE |state| state.x <= 3;
        invariant P_LOCKED_RANGE |state| !state.locked || state.x <= 3;
    }
}

crate::valid_model! {
    model FailingCounterModel<State, Action>;
    init [State {
        x: 0,
        locked: false,
    }];
    step |state, action| {
        match action {
            Action::Inc if !state.locked && state.x < 3 => vec![State {
                x: state.x + 1,
                locked: state.locked,
            }],
            Action::Lock => vec![State {
                x: state.x,
                locked: true,
            }],
            Action::Unlock => vec![State {
                x: state.x,
                locked: false,
            }],
            _ => Vec::new(),
        }
    }
    properties {
        invariant P_FAIL |state| state.x <= 1;
    }
}

pub fn is_bundled_model_ref(name: &str) -> bool {
    parse_model_ref(name).is_some()
}

pub fn list_bundled_models() -> Vec<&'static str> {
    vec!["counter", "failing-counter"]
}

pub fn inspect_bundled_model(request_id: &str, model_ref: &str) -> Result<InspectResponse, String> {
    match parse_model_ref(model_ref) {
        Some(BundledModel::Counter) => Ok(build_inspect_response::<CounterModel>(request_id)),
        Some(BundledModel::FailingCounter) => {
            Ok(build_inspect_response::<FailingCounterModel>(request_id))
        }
        None => Err(format!("unknown bundled rust model `{model_ref}`")),
    }
}

pub fn check_bundled_model(request_id: &str, model_ref: &str) -> Result<CheckOutcome, String> {
    match parse_model_ref(model_ref) {
        Some(BundledModel::Counter) => Ok(check_machine_outcome::<CounterModel>(request_id)),
        Some(BundledModel::FailingCounter) => {
            Ok(check_machine_outcome::<FailingCounterModel>(request_id))
        }
        None => Err(format!("unknown bundled rust model `{model_ref}`")),
    }
}

pub fn explain_bundled_model(
    request_id: &str,
    model_ref: &str,
) -> Result<ExplainResponse, String> {
    match parse_model_ref(model_ref) {
        Some(BundledModel::Counter) => explain_machine::<CounterModel>(request_id),
        Some(BundledModel::FailingCounter) => explain_machine::<FailingCounterModel>(request_id),
        None => Err(format!("unknown bundled rust model `{model_ref}`")),
    }
}

pub fn coverage_bundled_model(model_ref: &str) -> Result<CoverageReport, String> {
    match parse_model_ref(model_ref) {
        Some(BundledModel::Counter) => Ok(collect_machine_coverage::<CounterModel>()),
        Some(BundledModel::FailingCounter) => Ok(collect_machine_coverage::<FailingCounterModel>()),
        None => Err(format!("unknown bundled rust model `{model_ref}`")),
    }
}

pub fn testgen_bundled_vectors(model_ref: &str, strategy: &str) -> Result<Vec<TestVector>, String> {
    let vectors = match parse_model_ref(model_ref) {
        Some(BundledModel::Counter) => build_machine_test_vectors_for_strategy::<CounterModel>(strategy),
        Some(BundledModel::FailingCounter) => build_machine_test_vectors_for_strategy::<FailingCounterModel>(strategy),
        None => return Err(format!("unknown bundled rust model `{model_ref}`")),
    };
    Ok(vectors)
}

pub fn testgen_bundled_model(
    request_id: &str,
    model_ref: &str,
    strategy: &str,
) -> Result<TestgenResponse, String> {
    let vectors = testgen_bundled_vectors(model_ref, strategy)?;
    let generated_files = write_generated_test_files(&vectors)?;
    Ok(TestgenResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        status: "ok".to_string(),
        vector_ids: vectors.iter().map(|vector| vector.vector_id.clone()).collect(),
        generated_files,
    })
}

pub fn orchestrate_bundled_model(
    request_id: &str,
    model_ref: &str,
) -> Result<OrchestrateResponse, String> {
    let outcomes = match parse_model_ref(model_ref) {
        Some(BundledModel::Counter) => check_machine_outcomes::<CounterModel>(request_id),
        Some(BundledModel::FailingCounter) => check_machine_outcomes::<FailingCounterModel>(request_id),
        None => return Err(format!("unknown bundled rust model `{model_ref}`")),
    };
    let coverage = coverage_bundled_model(model_ref)?;
    let runs = outcomes
        .into_iter()
        .map(|result| OrchestratedRunSummary {
            property_id: result.property_result.property_id,
            status: format!("{:?}", result.status),
            assurance_level: format!("{:?}", result.assurance_level),
            run_id: result.manifest.run_id,
        })
        .collect();
    Ok(OrchestrateResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        runs,
        aggregate_coverage: Some(coverage),
    })
}

fn build_inspect_response<M: crate::modeling::VerifiedMachine>(request_id: &str) -> InspectResponse {
    let state_fields = M::init_states()
        .first()
        .map(|state| state.snapshot().keys().cloned().collect())
        .unwrap_or_default();
    let actions = M::Action::all()
        .into_iter()
        .map(|action| action.action_id())
        .collect();
    InspectResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        status: "ok".to_string(),
        model_id: M::model_id().to_string(),
        state_fields,
        actions,
        properties: property_ids::<M>()
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BundledModel {
    Counter,
    FailingCounter,
}

fn parse_model_ref(model_ref: &str) -> Option<BundledModel> {
    match model_ref {
        "counter" | "rust:counter" => Some(BundledModel::Counter),
        "failing-counter" | "rust:failing-counter" => Some(BundledModel::FailingCounter),
        _ => None,
    }
}
