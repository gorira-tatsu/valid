//! # valid
//!
//! `valid` is a Rust-first finite-state verification library for business-rule
//! models.
//!
//! The main product path is:
//!
//! 1. define state and actions in Rust
//! 2. define the machine with [`valid_model!`] or the derive-based state/action
//!    surface
//! 3. export models through a small registry
//! 4. run verification and review flows with `cargo valid`
//!
//! The library surface is organized around a few main areas:
//!
//! - [`modeling`] for the Rust DSL building blocks and helper predicates
//! - [`api`] for inspect/check/explain/testgen-oriented request and response
//!   types
//! - [`conformance`] for feeding generated vectors into a real implementation
//! - [`project`] for project bootstrap, scaffold checking, and project policy
//! - [`reporter`] for graph and trace renderers
//!
//! For end-user workflows and command examples, see the repository
//! [`README.md`](https://github.com/gorira-tatsu/valid/blob/main/README.md).

extern crate self as valid;

#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Machine-readable inspect/check/explain/testgen request and response types.
pub mod api;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Benchmark and performance-reporting helpers for registry-backed workflows.
pub mod benchmark;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
#[doc(hidden)]
pub mod bundled_models;
/// CLI schema, completion, and structured command metadata helpers.
pub mod cli;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Helper-based model composition utilities for shared-state review flows.
pub mod compose;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Implementation-facing conformance requests, reports, and Rust harness traits.
pub mod conformance;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Contract snapshot, drift, and lock-file support.
pub mod contract;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Coverage collection and rendering for explicit exploration.
pub mod coverage;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Candidate-model comparison and distinguishing-trace support.
pub mod distinguish;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Documentation rendering and drift checks for generated model docs.
pub mod doc;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Verification engines and run-plan execution.
pub mod engine;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Evidence, traces, review summaries, and diagnostic renderers.
pub mod evidence;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Cargo-project target resolution for external registry workflows.
pub mod external_registry;
/// Frontend parsing, resolution, typechecking, and lowering for `.valid` input.
pub mod frontend;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Implementation handoff summaries and handoff drift checks.
pub mod handoff;
/// Shared intermediate representation used across the frontends and backends.
pub mod ir;
/// State evaluation, guard checking, replay, and transition application helpers.
pub mod kernel;
/// MCP server implementation and MCP-specific catalogs/prompts.
pub mod mcp;
/// Rust DSL macros, finite containers, and predicate helper functions.
pub mod modeling;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Multi-property orchestration helpers.
pub mod orchestrator;
/// Project bootstrap, project policy parsing, and scaffold lifecycle helpers.
pub mod project;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Registry-facing entrypoints used by `cargo valid` and generated projects.
pub mod registry;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Graph, SVG, Mermaid, and text renderers for review workflows.
pub mod reporter;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Environment and backend self-check helpers.
pub mod selfcheck;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Solver adapters and solver-specific bounded checking helpers.
pub mod solver;
/// Shared support helpers such as diagnostics, hashing, schema, and I/O.
pub mod support;
#[cfg(any(debug_assertions, feature = "verification-runtime"))]
/// Test-spec generation and replay vector helpers.
pub mod testgen;

/// Finite-container helpers and predicate functions re-exported from the Rust
/// DSL surface.
pub use modeling::{
    contains, iff, implies, insert, is_empty, len, map_contains_entry, map_contains_key, map_put,
    map_remove, regex_match, rel_contains, rel_insert, rel_intersects, rel_remove, remove,
    str_contains, xor, FiniteEnumSet, FiniteMap, FiniteRelation,
};
/// Derive macros for state, action, and enum modeling.
pub use valid_derive::{ValidAction, ValidEnum, ValidState};
