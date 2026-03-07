//! Verification engines such as explicit BFS/DFS.

pub mod explicit;
pub use explicit::{
    check_explicit, CheckErrorEnvelope, CheckOutcome, ExplicitRunResult, PropertyResult,
};
use std::{
    env::consts::{ARCH, OS},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static RUN_SEED_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    SmtCvc5,
    SatVarisat,
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
    pub seed: u64,
    pub platform_metadata: PlatformMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformMetadata {
    pub os: String,
    pub arch: String,
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
            manifest: build_run_manifest(
                "req-local-0001".to_string(),
                "run-local-0001".to_string(),
                "sha256:unknown".to_string(),
                "sha256:unknown".to_string(),
                BackendKind::Explicit,
                env!("CARGO_PKG_VERSION").to_string(),
                None,
            ),
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

impl Default for PlatformMetadata {
    fn default() -> Self {
        current_platform_metadata()
    }
}

pub fn build_run_manifest(
    request_id: String,
    run_id: String,
    source_hash: String,
    contract_hash: String,
    backend_name: BackendKind,
    backend_version: String,
    seed: Option<u64>,
) -> RunManifest {
    RunManifest {
        request_id,
        run_id,
        schema_version: "1.0.0".to_string(),
        source_hash,
        contract_hash,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        backend_name,
        backend_version,
        seed: seed.unwrap_or_else(generate_run_seed),
        platform_metadata: current_platform_metadata(),
    }
}

pub fn current_platform_metadata() -> PlatformMetadata {
    PlatformMetadata {
        os: OS.to_string(),
        arch: ARCH.to_string(),
    }
}

pub fn generate_run_seed() -> u64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let nanos = elapsed.as_nanos();
    let counter = u128::from(RUN_SEED_COUNTER.fetch_add(1, Ordering::Relaxed));
    let pid = u128::from(std::process::id());
    mix_seed(nanos ^ (counter << 17) ^ pid)
}

fn mix_seed(value: u128) -> u64 {
    let mut seed = (value as u64) ^ ((value >> 64) as u64);
    seed ^= seed >> 33;
    seed = seed.wrapping_mul(0xff51afd7ed558ccd);
    seed ^= seed >> 33;
    seed = seed.wrapping_mul(0xc4ceb9fe1a85ec53);
    seed ^= seed >> 33;
    if seed == 0 {
        1
    } else {
        seed
    }
}
