extern crate self as valid;

pub mod api;
pub mod benchmark;
#[doc(hidden)]
pub mod bundled_models;
pub mod cli;
pub mod contract;
pub mod coverage;
pub mod engine;
pub mod evidence;
pub mod frontend;
pub mod ir;
pub mod kernel;
pub mod modeling;
pub mod orchestrator;
pub mod project;
pub mod registry;
pub mod reporter;
pub mod selfcheck;
pub mod solver;
pub mod support;
pub mod testgen;

pub use modeling::{
    contains, iff, implies, insert, is_empty, len, map_contains_entry, map_contains_key, map_put,
    map_remove, regex_match, rel_contains, rel_insert, rel_intersects, rel_remove, remove,
    str_contains, xor, FiniteEnumSet, FiniteMap, FiniteRelation,
};
pub use valid_derive::{ValidAction, ValidEnum, ValidState};
