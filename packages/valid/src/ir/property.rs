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
}

impl PropertyKind {
    pub fn parse(input: &str) -> Option<Self> {
        match input {
            "invariant" => Some(Self::Invariant),
            "reachability" => Some(Self::Reachability),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Invariant => "invariant",
            Self::Reachability => "reachability",
        }
    }
}

impl fmt::Display for PropertyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
