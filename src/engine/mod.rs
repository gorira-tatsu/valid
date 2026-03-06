//! Verification engines such as explicit BFS/DFS.

pub mod explicit;
pub use explicit::{
    check_explicit, CheckErrorEnvelope, CheckOutcome, ExplicitRunResult, PropertyResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStrategy {
    Bfs,
    Dfs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssuranceLevel {
    Complete,
    Bounded,
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownReason {
    StateLimitReached,
    TimeLimitReached,
    EngineAborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorStatus {
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Pass,
    Fail,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Explicit,
    MockBmc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunManifest {
    pub request_id: String,
    pub run_id: String,
    pub schema_version: String,
    pub source_hash: String,
    pub contract_hash: String,
    pub engine_version: String,
    pub backend_name: BackendKind,
    pub backend_version: String,
    pub seed: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchBounds {
    pub max_depth: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceLimits {
    pub max_states: Option<usize>,
    pub time_limit_ms: Option<u64>,
    pub memory_limit_mb: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertySelection {
    ExactlyOne(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactPolicy {
    EmitAll,
    EmitOnFailure,
    EmitNothing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReporterOptions {
    pub json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPlan {
    pub manifest: RunManifest,
    pub strategy: SearchStrategy,
    pub property_selection: PropertySelection,
    pub search_bounds: SearchBounds,
    pub resource_limits: ResourceLimits,
    pub artifact_policy: ArtifactPolicy,
    pub reporter_options: ReporterOptions,
    pub detect_deadlocks: bool,
}

impl Default for RunPlan {
    fn default() -> Self {
        Self {
            manifest: RunManifest {
                request_id: "req-local-0001".to_string(),
                run_id: "run-local-0001".to_string(),
                schema_version: "1.0.0".to_string(),
                source_hash: "sha256:unknown".to_string(),
                contract_hash: "sha256:unknown".to_string(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                backend_name: BackendKind::Explicit,
                backend_version: env!("CARGO_PKG_VERSION").to_string(),
                seed: None,
            },
            strategy: SearchStrategy::Bfs,
            property_selection: PropertySelection::ExactlyOne("P_SAFE".to_string()),
            search_bounds: SearchBounds { max_depth: None },
            resource_limits: ResourceLimits {
                max_states: None,
                time_limit_ms: None,
                memory_limit_mb: None,
            },
            artifact_policy: ArtifactPolicy::EmitOnFailure,
            reporter_options: ReporterOptions { json: false },
            detect_deadlocks: true,
        }
    }
}
