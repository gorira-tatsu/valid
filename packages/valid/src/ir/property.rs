use crate::ir::expr::ExprIr;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyIr {
    pub property_id: String,
    pub kind: PropertyKind,
    pub layer: PropertyLayer,
    pub expr: ExprIr,
    pub scope: Option<ExprIr>,
    pub action_filter: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyLayer {
    Assume,
    Assert,
}

impl PropertyLayer {
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "assume" => Some(Self::Assume),
            "assert" => Some(Self::Assert),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Assume => "assume",
            Self::Assert => "assert",
        }
    }
}

impl fmt::Display for PropertyLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    Invariant,
    Reachability,
    Cover,
    Transition,
    DeadlockFreedom,
    Temporal,
}

impl PropertyKind {
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "invariant" => Some(Self::Invariant),
            "reachability" => Some(Self::Reachability),
            "cover" => Some(Self::Cover),
            "transition" => Some(Self::Transition),
            "deadlock_freedom" => Some(Self::DeadlockFreedom),
            "temporal" => Some(Self::Temporal),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invariant => "invariant",
            Self::Reachability => "reachability",
            Self::Cover => "cover",
            Self::Transition => "transition",
            Self::DeadlockFreedom => "deadlock_freedom",
            Self::Temporal => "temporal",
        }
    }
}

impl fmt::Display for PropertyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
