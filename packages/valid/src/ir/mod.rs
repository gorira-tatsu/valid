//! Canonical intermediate representations shared across backends.

pub mod action;
pub mod decision;
pub mod expr;
pub mod model;
pub mod path;
pub mod property;
pub mod value;

pub use action::{
    parse_action_identity, ActionIdentity, ActionIr, ActionParameterBinding, ActionRole, UpdateIr,
};
pub use decision::{Decision, DecisionKind, DecisionOutcome, DecisionPoint};
pub use expr::{BinaryOp, ExprIr, UnaryOp};
pub use model::{
    FieldId, FieldType, InitAssignment, ModelIr, PredicateIr, PropertyId, ScenarioIr, SourceSpan,
    StateField,
};
pub use path::{build_path_from_parts, decision_path_tags, infer_decision_path_tags, Path};
pub use property::{PropertyIr, PropertyKind, PropertyLayer};
pub use value::Value;
