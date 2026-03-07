use crate::ir::expr::ExprIr;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyIr {
    pub property_id: String,
    pub kind: PropertyKind,
    pub expr: ExprIr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyKind {
    Invariant,
    Reachability,
    DeadlockFreedom,
}

impl PropertyKind {
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "invariant" => Some(Self::Invariant),
            "reachability" => Some(Self::Reachability),
            "deadlock_freedom" => Some(Self::DeadlockFreedom),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invariant => "invariant",
            Self::Reachability => "reachability",
            Self::DeadlockFreedom => "deadlock_freedom",
        }
    }
}

impl fmt::Display for PropertyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
