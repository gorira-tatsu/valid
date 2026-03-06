//! Verification engines such as explicit BFS/DFS.

pub mod explicit;
pub use explicit::ExplicitRunResult;

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
    UnsatInit,
    StateLimitReached,
    DepthLimitReached,
    TimeLimitReached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    Pass,
    Fail,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPlan {
    pub strategy: SearchStrategy,
    pub property_id: Option<String>,
    pub max_states: Option<usize>,
    pub max_depth: Option<usize>,
    pub time_limit_ms: Option<u64>,
    pub detect_deadlocks: bool,
}

impl Default for RunPlan {
    fn default() -> Self {
        Self {
            strategy: SearchStrategy::Bfs,
            property_id: None,
            max_states: None,
            max_depth: None,
            time_limit_ms: None,
            detect_deadlocks: true,
        }
    }
}
