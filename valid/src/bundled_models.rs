use crate::{
    api::{
        ExplainResponse, InspectAction, InspectCapabilities, InspectProperty, InspectResponse,
        InspectStateField, InspectTransition, InspectTransitionUpdate, OrchestrateResponse,
        OrchestratedRunSummary, TestgenResponse,
    },
    coverage::collect_coverage,
    coverage::CoverageReport,
    engine::CheckOutcome,
    modeling::{
        build_machine_test_vectors_for_strategy, check_machine_outcome,
        check_machine_outcome_for_property, check_machine_outcomes, check_machine_with_adapter,
        collect_machine_coverage, explain_machine, lower_machine_model, machine_capability_report,
        property_ids, replay_machine_actions, ActionSpec, StateSpec,
    },
    orchestrator::run_all_properties_with_backend,
    solver::AdapterConfig,
    testgen::{
        build_counterexample_vector, render_replay_json, write_generated_test_files, ReplayTarget,
        TestVector,
    },
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

valid_derive::valid_model! {
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

valid_derive::valid_model! {
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

crate::valid_state! {
    struct AccessState {
        boundary_attached: bool,
        session_active: bool,
        billing_read_allowed: bool,
    }
}

crate::valid_actions! {
    enum AccessAction {
        AttachBoundary => "ATTACH_BOUNDARY" [reads = ["boundary_attached"], writes = ["boundary_attached"]],
        AssumeSession => "ASSUME_SESSION" [reads = ["boundary_attached", "session_active"], writes = ["session_active"]],
        EvaluateBillingRead => "EVAL_BILLING_READ" [reads = ["boundary_attached", "session_active"], writes = ["billing_read_allowed"]],
    }
}

valid_derive::valid_model! {
    model IamAccessModel<AccessState, AccessAction>;
    init [AccessState {
        boundary_attached: false,
        session_active: false,
        billing_read_allowed: false,
    }];
    transitions {
        transition AttachBoundary [tags = ["boundary_path"]] when |state| !state.boundary_attached => [AccessState {
            boundary_attached: true,
            session_active: state.session_active,
            billing_read_allowed: state.billing_read_allowed,
        }];
        transition AssumeSession [tags = ["session_path"]] when |state| state.boundary_attached && !state.session_active => [AccessState {
            boundary_attached: state.boundary_attached,
            session_active: true,
            billing_read_allowed: state.billing_read_allowed,
        }];
        transition EvaluateBillingRead [tags = ["allow_path", "boundary_path", "session_path"]] when |state| state.boundary_attached && state.session_active => [AccessState {
            boundary_attached: state.boundary_attached,
            session_active: state.session_active,
            billing_read_allowed: true,
        }];
    }
    properties {
        invariant P_BILLING_READ_REQUIRES_BOUNDARY |state| !state.billing_read_allowed || state.boundary_attached;
        invariant P_BILLING_READ_REQUIRES_SESSION |state| !state.billing_read_allowed || state.session_active;
    }
}

pub fn is_bundled_model_ref(name: &str) -> bool {
    parse_model_ref(name).is_some()
}

pub fn list_bundled_models() -> Vec<&'static str> {
    vec!["counter", "failing-counter", "iam-access"]
}

pub fn inspect_bundled_model(request_id: &str, model_ref: &str) -> Result<InspectResponse, String> {
    match parse_model_ref(model_ref) {
        Some(BundledModel::Counter) => Ok(build_inspect_response::<CounterModel>(request_id)),
        Some(BundledModel::FailingCounter) => {
            Ok(build_inspect_response::<FailingCounterModel>(request_id))
        }
        Some(BundledModel::IamAccess) => Ok(build_inspect_response::<IamAccessModel>(request_id)),
        None => Err(format!("unknown bundled rust model `{model_ref}`")),
    }
}

pub fn check_bundled_model(
    request_id: &str,
    model_ref: &str,
    property_id: Option<&str>,
    adapter: Option<&AdapterConfig>,
) -> Result<CheckOutcome, String> {
    match parse_model_ref(model_ref) {
        Some(BundledModel::Counter) => match adapter {
            Some(adapter) => {
                check_machine_with_adapter::<CounterModel>(request_id, property_id, adapter)
            }
            None => Ok(match property_id {
                Some(property_id) => {
                    check_machine_outcome_for_property::<CounterModel>(request_id, property_id)
                }
                None => check_machine_outcome::<CounterModel>(request_id),
            }),
        },
        Some(BundledModel::FailingCounter) => match adapter {
            Some(adapter) => {
                check_machine_with_adapter::<FailingCounterModel>(request_id, property_id, adapter)
            }
            None => Ok(match property_id {
                Some(property_id) => check_machine_outcome_for_property::<FailingCounterModel>(
                    request_id,
                    property_id,
                ),
                None => check_machine_outcome::<FailingCounterModel>(request_id),
            }),
        },
        Some(BundledModel::IamAccess) => match adapter {
            Some(adapter) => {
                check_machine_with_adapter::<IamAccessModel>(request_id, property_id, adapter)
            }
            None => Ok(match property_id {
                Some(property_id) => {
                    check_machine_outcome_for_property::<IamAccessModel>(request_id, property_id)
                }
                None => check_machine_outcome::<IamAccessModel>(request_id),
            }),
        },
        None => Err(format!("unknown bundled rust model `{model_ref}`")),
    }
}

pub fn explain_bundled_model(request_id: &str, model_ref: &str) -> Result<ExplainResponse, String> {
    match parse_model_ref(model_ref) {
        Some(BundledModel::Counter) => explain_machine::<CounterModel>(request_id),
        Some(BundledModel::FailingCounter) => explain_machine::<FailingCounterModel>(request_id),
        Some(BundledModel::IamAccess) => explain_machine::<IamAccessModel>(request_id),
        None => Err(format!("unknown bundled rust model `{model_ref}`")),
    }
}

pub fn coverage_bundled_model(model_ref: &str) -> Result<CoverageReport, String> {
    match parse_model_ref(model_ref) {
        Some(BundledModel::Counter) => Ok(collect_machine_coverage::<CounterModel>()),
        Some(BundledModel::FailingCounter) => Ok(collect_machine_coverage::<FailingCounterModel>()),
        Some(BundledModel::IamAccess) => Ok(collect_machine_coverage::<IamAccessModel>()),
        None => Err(format!("unknown bundled rust model `{model_ref}`")),
    }
}

pub fn testgen_bundled_vectors(model_ref: &str, strategy: &str) -> Result<Vec<TestVector>, String> {
    testgen_bundled_vectors_for_property(model_ref, None, strategy)
}

pub fn testgen_bundled_vectors_for_property(
    model_ref: &str,
    property_id: Option<&str>,
    strategy: &str,
) -> Result<Vec<TestVector>, String> {
    let vectors = match parse_model_ref(model_ref) {
        Some(BundledModel::Counter) => {
            build_machine_test_vectors_for_strategy::<CounterModel>(property_id, strategy)
        }
        Some(BundledModel::FailingCounter) => {
            build_machine_test_vectors_for_strategy::<FailingCounterModel>(property_id, strategy)
        }
        Some(BundledModel::IamAccess) => {
            build_machine_test_vectors_for_strategy::<IamAccessModel>(property_id, strategy)
        }
        None => return Err(format!("unknown bundled rust model `{model_ref}`")),
    };
    Ok(vectors)
}

pub fn testgen_bundled_model(
    request_id: &str,
    model_ref: &str,
    property_id: Option<&str>,
    strategy: &str,
    adapter: Option<&AdapterConfig>,
) -> Result<TestgenResponse, String> {
    let mut vectors = if let Some(adapter) = adapter {
        if !matches!(adapter, AdapterConfig::Explicit) && strategy == "counterexample" {
            bundled_counterexample_vectors_from_adapter(
                request_id,
                model_ref,
                property_id,
                adapter,
            )?
        } else {
            testgen_bundled_vectors_for_property(model_ref, property_id, strategy)?
        }
    } else {
        testgen_bundled_vectors_for_property(model_ref, property_id, strategy)?
    };
    annotate_bundled_replay_targets(model_ref, property_id, &mut vectors);
    let generated_files = write_generated_test_files(&vectors)?;
    Ok(TestgenResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        status: "ok".to_string(),
        vector_ids: vectors
            .iter()
            .map(|vector| vector.vector_id.clone())
            .collect(),
        vectors: vectors
            .iter()
            .map(|vector| crate::api::TestgenVectorSummary {
                vector_id: vector.vector_id.clone(),
                strictness: vector.strictness.clone(),
                derivation: vector.derivation.clone(),
                source_kind: vector.source_kind.clone(),
                strategy: vector.strategy.clone(),
            })
            .collect(),
        generated_files,
    })
}

pub fn replay_bundled_model(
    model_ref: &str,
    property_id: Option<&str>,
    action_ids: &[String],
    focus_action_id: Option<&str>,
) -> Result<String, String> {
    let (terminal_state, property_id, focus_action_enabled) = match parse_model_ref(model_ref) {
        Some(BundledModel::Counter) => {
            replay_machine_actions::<CounterModel>(property_id, action_ids, focus_action_id)?
        }
        Some(BundledModel::FailingCounter) => {
            replay_machine_actions::<FailingCounterModel>(property_id, action_ids, focus_action_id)?
        }
        Some(BundledModel::IamAccess) => {
            replay_machine_actions::<IamAccessModel>(property_id, action_ids, focus_action_id)?
        }
        None => return Err(format!("unknown bundled rust model `{model_ref}`")),
    };
    Ok(render_replay_json(
        property_id,
        action_ids,
        &terminal_state,
        focus_action_id,
        focus_action_enabled,
    ))
}

fn bundled_counterexample_vectors_from_adapter(
    request_id: &str,
    model_ref: &str,
    property_id: Option<&str>,
    adapter: &AdapterConfig,
) -> Result<Vec<TestVector>, String> {
    let outcome = check_bundled_model(request_id, model_ref, property_id, Some(adapter))?;
    match outcome {
        CheckOutcome::Completed(result) => {
            let Some(trace) = result.trace else {
                return Ok(Vec::new());
            };
            build_counterexample_vector(&trace).map(|vector| vec![vector])
        }
        CheckOutcome::Errored(error) => Err(error
            .diagnostics
            .first()
            .map(|diagnostic| diagnostic.message.clone())
            .unwrap_or_else(|| "backend testgen failed".to_string())),
    }
}

fn annotate_bundled_replay_targets(
    model_ref: &str,
    property_id: Option<&str>,
    vectors: &mut [TestVector],
) {
    for vector in vectors {
        let mut args = vec![
            "replay".to_string(),
            model_ref.trim_start_matches("rust:").to_string(),
        ];
        let property_id = property_id.unwrap_or(vector.property_id.as_str());
        args.push(format!("--property={property_id}"));
        if let Some(action_id) = &vector.focus_action_id {
            args.push(format!("--focus-action={action_id}"));
        }
        if !vector.actions.is_empty() {
            args.push(format!(
                "--actions={}",
                vector
                    .actions
                    .iter()
                    .map(|step| step.action_id.clone())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        args.push("--json".to_string());
        vector.replay_target = Some(ReplayTarget {
            runner: "cargo-valid".to_string(),
            args,
        });
    }
}

pub fn orchestrate_bundled_model(
    request_id: &str,
    model_ref: &str,
    adapter: Option<&AdapterConfig>,
) -> Result<OrchestrateResponse, String> {
    if let Some(adapter) = adapter {
        if !matches!(adapter, AdapterConfig::Explicit) {
            let model = match parse_model_ref(model_ref) {
                Some(BundledModel::Counter) => lower_machine_model::<CounterModel>()?,
                Some(BundledModel::FailingCounter) => lower_machine_model::<FailingCounterModel>()?,
                Some(BundledModel::IamAccess) => lower_machine_model::<IamAccessModel>()?,
                None => return Err(format!("unknown bundled rust model `{model_ref}`")),
            };
            let base_plan = crate::engine::RunPlan::default();
            let mut traces = Vec::new();
            let runs = run_all_properties_with_backend(&model, &base_plan, adapter)
                .into_iter()
                .map(|run| match run.outcome {
                    CheckOutcome::Completed(result) => {
                        if let Some(trace) = result.trace.clone() {
                            traces.push(trace);
                        }
                        OrchestratedRunSummary {
                            property_id: run.property_id,
                            status: format!("{:?}", result.status),
                            assurance_level: format!("{:?}", result.assurance_level),
                            run_id: result.manifest.run_id,
                        }
                    }
                    CheckOutcome::Errored(error) => OrchestratedRunSummary {
                        property_id: run.property_id,
                        status: "ERROR".to_string(),
                        assurance_level: format!("{:?}", error.assurance_level),
                        run_id: error.manifest.run_id,
                    },
                })
                .collect();
            let aggregate_coverage = if traces.is_empty() {
                None
            } else {
                Some(collect_coverage(&model, &traces))
            };
            return Ok(OrchestrateResponse {
                schema_version: "1.0.0".to_string(),
                request_id: request_id.to_string(),
                runs,
                aggregate_coverage,
            });
        }
    }
    let outcomes = match parse_model_ref(model_ref) {
        Some(BundledModel::Counter) => check_machine_outcomes::<CounterModel>(request_id),
        Some(BundledModel::FailingCounter) => {
            check_machine_outcomes::<FailingCounterModel>(request_id)
        }
        Some(BundledModel::IamAccess) => check_machine_outcomes::<IamAccessModel>(request_id),
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

fn build_inspect_response<M: crate::modeling::VerifiedMachine>(
    request_id: &str,
) -> InspectResponse {
    let state_field_details = M::State::state_fields()
        .into_iter()
        .map(|field| InspectStateField {
            name: field.name.to_string(),
            rust_type: field.rust_type.to_string(),
            range: field.range.map(str::to_string),
            variants: field
                .variants
                .unwrap_or_default()
                .into_iter()
                .map(str::to_string)
                .collect(),
            is_set: field.is_set,
        })
        .collect::<Vec<_>>();
    let action_details = M::Action::action_descriptors()
        .into_iter()
        .map(|action| InspectAction {
            action_id: action.action_id.to_string(),
            reads: action.reads.iter().map(|item| item.to_string()).collect(),
            writes: action.writes.iter().map(|item| item.to_string()).collect(),
        })
        .collect::<Vec<_>>();
    let transition_details = crate::modeling::machine_transition_ir::<M>()
        .into_iter()
        .map(|transition| InspectTransition {
            action_id: transition.action_id.to_string(),
            guard: transition.guard.map(str::to_string),
            effect: transition.effect.map(str::to_string),
            reads: transition
                .reads
                .iter()
                .map(|item| item.to_string())
                .collect(),
            writes: transition
                .writes
                .iter()
                .map(|item| item.to_string())
                .collect(),
            path_tags: crate::modeling::decision_path_tags(
                &transition.path_tags,
                transition.action_id,
                transition.reads.iter().copied(),
                transition.writes.iter().copied(),
                transition.guard,
                transition.effect,
            ),
            updates: transition
                .updates
                .iter()
                .filter_map(|update| {
                    update.expr.map(|expr| InspectTransitionUpdate {
                        field: update.field.to_string(),
                        expr: expr.to_string(),
                    })
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let property_details = M::properties()
        .into_iter()
        .map(|property| InspectProperty {
            property_id: property.property_id.to_string(),
            kind: format!("{:?}", property.property_kind),
        })
        .collect::<Vec<_>>();
    let capabilities = machine_capability_report::<M>();
    InspectResponse {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        status: "ok".to_string(),
        model_id: M::model_id().to_string(),
        machine_ir_ready: capabilities.ir_ready,
        machine_ir_error: capabilities.machine_ir_error.clone(),
        capabilities: InspectCapabilities {
            parse_ready: capabilities.parse_ready,
            explicit_ready: capabilities.explicit_ready,
            ir_ready: capabilities.ir_ready,
            solver_ready: capabilities.solver_ready,
            coverage_ready: capabilities.coverage_ready,
            explain_ready: capabilities.explain_ready,
            testgen_ready: capabilities.testgen_ready,
            reasons: capabilities.reasons.clone(),
        },
        state_fields: state_field_details
            .iter()
            .map(|field| field.name.clone())
            .collect(),
        actions: action_details
            .iter()
            .map(|action| action.action_id.clone())
            .collect(),
        properties: property_ids::<M>()
            .into_iter()
            .map(str::to_string)
            .collect(),
        state_field_details,
        action_details,
        transition_details,
        property_details,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BundledModel {
    Counter,
    FailingCounter,
    IamAccess,
}

fn parse_model_ref(model_ref: &str) -> Option<BundledModel> {
    match model_ref {
        "counter" | "rust:counter" => Some(BundledModel::Counter),
        "failing-counter" | "rust:failing-counter" => Some(BundledModel::FailingCounter),
        "iam-access" | "rust:iam-access" => Some(BundledModel::IamAccess),
        _ => None,
    }
}
