//! Frontend: parsing, name resolution, typechecking, and IR lowering.

pub mod ir_lowering;
pub mod parser;
pub mod resolver;
pub mod typecheck;

use crate::ir::ModelIr;
use crate::support::diagnostics::Diagnostic;

pub fn compile_model(source: &str) -> Result<ModelIr, Vec<Diagnostic>> {
    let parsed = parser::parse_model(source)?;
    let resolved = resolver::resolve_model(parsed)?;
    let typed = typecheck::typecheck_model(resolved)?;
    ir_lowering::lower_model(typed)
}
