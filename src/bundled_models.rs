use std::collections::BTreeMap;

use crate::{
    api::{
        ExplainResponse, InspectResponse, OrchestrateResponse, OrchestratedRunSummary,
        TestgenResponse,
    },
    coverage::CoverageReport,
    engine::CheckOutcome,
    ir::Value,
    modeling::{
        build_machine_test_vectors, check_machine_outcome, collect_machine_coverage,
        explain_machine, Finite, ModelingAction, ModelingState, VerifiedMachine,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct State {
    x: u8,
    locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Action {
    Inc,
    Lock,
    Unlock,
}

impl Finite for Action {
    fn all() -> Vec<Self> {
        vec![Self::Inc, Self::Lock, Self::Unlock]
    }
}

impl ModelingAction for Action {
    fn action_id(&self) -> String {
        match self {
            Action::Inc => "INC".to_string(),
            Action::Lock => "LOCK".to_string(),
            Action::Unlock => "UNLOCK".to_string(),
        }
    }
}

impl ModelingState for State {
    fn snapshot(&self) -> BTreeMap<String, Value> {
        BTreeMap::from([
            ("x".to_string(), Value::UInt(self.x as u64)),
            ("locked".to_string(), Value::Bool(self.locked)),
        ])
    }
}

struct CounterModel;
struct FailingCounterModel;

impl VerifiedMachine for CounterModel {
    type State = State;
    type Action = Action;

    fn model_id() -> &'static str {
        "CounterModel"
    }

    fn property_id() -> &'static str {
        "P_RANGE"
    }

    fn init_states() -> Vec<Self::State> {
        vec![State {
            x: 0,
            locked: false,
        }]
    }

    fn step(state: &Self::State, action: &Self::Action) -> Vec<Self::State> {
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

    fn holds(state: &Self::State) -> bool {
        state.x <= 3
    }
}

impl VerifiedMachine for FailingCounterModel {
    type State = State;
    type Action = Action;

    fn model_id() -> &'static str {
        "FailingCounterModel"
    }

    fn property_id() -> &'static str {
        "P_FAIL"
    }

    fn init_states() -> Vec<Self::State> {
        CounterModel::init_states()
    }

    fn step(state: &Self::State, action: &Self::Action) -> Vec<Self::State> {
        CounterModel::step(state, action)
    }

    fn holds(state: &Self::State) -> bool {
        state.x <= 1
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

pub fn testgen_bundled_model(
    request_id: &str,
    model_ref: &str,
) -> Result<TestgenResponse, String> {
    let vectors = match parse_model_ref(model_ref) {
        Some(BundledModel::Counter) => build_machine_test_vectors::<CounterModel>(),
        Some(BundledModel::FailingCounter) => build_machine_test_vectors::<FailingCounterModel>(),
        None => return Err(format!("unknown bundled rust model `{model_ref}`")),
    };
    Ok(TestgenResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        status: "ok".to_string(),
        vector_ids: vectors.iter().map(|vector| vector.vector_id.clone()).collect(),
        generated_files: vectors
            .iter()
            .map(crate::testgen::generated_test_output_path)
            .collect(),
    })
}

pub fn orchestrate_bundled_model(
    request_id: &str,
    model_ref: &str,
) -> Result<OrchestrateResponse, String> {
    let outcome = check_bundled_model(request_id, model_ref)?;
    let coverage = coverage_bundled_model(model_ref)?;
    let runs = match outcome {
        CheckOutcome::Completed(result) => vec![OrchestratedRunSummary {
            property_id: result.property_result.property_id,
            status: format!("{:?}", result.status),
            assurance_level: format!("{:?}", result.assurance_level),
            run_id: result.manifest.run_id,
        }],
        CheckOutcome::Errored(error) => vec![OrchestratedRunSummary {
            property_id: "unknown".to_string(),
            status: "ERROR".to_string(),
            assurance_level: format!("{:?}", error.assurance_level),
            run_id: error.manifest.run_id,
        }],
    };
    Ok(OrchestrateResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        runs,
        aggregate_coverage: Some(coverage),
    })
}

fn build_inspect_response<M: VerifiedMachine>(request_id: &str) -> InspectResponse {
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
        properties: vec![M::property_id().to_string()],
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
