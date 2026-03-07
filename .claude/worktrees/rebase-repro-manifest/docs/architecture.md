# Architecture

`valid` is moving toward a clean-architecture, solver-neutral layout.

The central design rule is:

- domain and application code must not depend on a specific SAT/SMT solver
- solver integrations are adapters behind a stable port
- CLI and file/project concerns stay at the outside

## Package Layout

- `packages/valid`
  main library and binaries
- `packages/valid_derive`
  procedural macros for the Rust DSL

## Layers

### Domain

Inside `packages/valid/src`, the domain-oriented core is:

- `ir`
  canonical finite-state model representation
- `engine`
  explicit exploration result model
- `evidence`
  traces, explain inputs, replay-oriented results
- `coverage`
  action/guard/path coverage
- `testgen`
  generated vector and replay asset creation

These modules should not depend on a concrete external solver process.

### Application / Use Cases

Application-facing orchestration lives in:

- `api`
- `benchmark`
- `orchestrator`
- `project`
- `registry`

These are use-case and DTO boundaries:

- requests/responses
- benchmark summaries
- migration audits
- project config loading

### Infrastructure / Adapters

Infrastructure concerns live in:

- `frontend`
  `.valid` parsing / lowering
- `solver`
  explicit adapter selection, `command`, `smt-cvc5`, `sat-varisat`
- `reporter`
  Mermaid / DOT / SVG / text rendering
- `support`
  file and artifact helpers

## Solver Port

The important abstraction is:

- `AdapterConfig`
- `SolverAdapter`
- shared `ModelIr`

Each backend adapts the same finite model:

- `explicit`
- `mock-bmc`
- `sat-varisat`
- `smt-cvc5`
- `command`

The long-term goal is:

- keep the domain model solver-neutral
- keep encoding typed and explicit
- add backends without changing the application layer

## DTO Boundary

`api/mod.rs` is the current DTO-heavy boundary.

It converts between:

- user/project requests
- internal IR / engine results
- JSON/text reports

This is the right place for request validation and backward-compatible response
shape management.

## DDD View

The main bounded contexts are:

- modeling
- verification
- evidence
- coverage
- generated regression assets
- project/runtime configuration

This is not a full enterprise DDD implementation, but the boundaries are meant
to keep:

- domain semantics stable
- infrastructure replaceable
- CLI and transport concerns outside the core

## Current Direction

Near-term priorities:

- keep `transitions { ... }` as the canonical analysis path
- treat `step` as explicit-first / migration-oriented
- prefer embedded or optional backends over hard runtime dependencies
- improve proc-macro diagnostics and linter/readiness output
- keep examples small and readable, move heavy scenarios to `benchmarks/`
