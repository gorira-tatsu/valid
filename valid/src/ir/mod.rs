//! Canonical intermediate representations shared across backends.

pub mod action;
pub mod expr;
pub mod model;
pub mod property;
pub mod value;

pub use action::{ActionIr, UpdateIr};
pub use expr::{BinaryOp, ExprIr, UnaryOp};
pub use model::{FieldId, FieldType, InitAssignment, ModelIr, PropertyId, SourceSpan, StateField};
pub use property::{PropertyIr, PropertyKind};
pub use value::Value;
