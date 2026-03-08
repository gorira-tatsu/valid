extern crate self as valid;

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod api;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod benchmark;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
#[doc(hidden)]
pub mod bundled_models;
pub mod cli;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod conformance;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod contract;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod coverage;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod doc;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod distinguish;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod engine;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod evidence;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod external_registry;
pub mod frontend;
pub mod ir;
pub mod kernel;
pub mod mcp;
pub mod modeling;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod orchestrator;
pub mod project;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod registry;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod reporter;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod selfcheck;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod solver;
pub mod support;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
pub mod testgen;

pub use modeling::{
    contains, iff, implies, insert, is_empty, len, map_contains_entry, map_contains_key, map_put,
    map_remove, regex_match, rel_contains, rel_insert, rel_intersects, rel_remove, remove,
    str_contains, xor, FiniteEnumSet, FiniteMap, FiniteRelation,
};
pub use valid_derive::{ValidAction, ValidEnum, ValidState};
