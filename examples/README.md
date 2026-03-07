# Example Models

This directory contains runnable examples that show the current implementation
boundary.

The intended primary modeling path is Rust-defined models, not a project-specific DSL.

## Registry example

`valid_models.rs` is the current minimal example of the Rust-first flow.
It keeps the model definitions in Rust and declares the exported registry with:

```rust
valid_state! {
    struct State {
        x: u8 [range = "0..=3"],
        locked: bool,
    }
}

valid_actions! {
    enum Action {
        Inc => "INC" [reads = ["x", "locked"], writes = ["x"]],
        Lock => "LOCK" [reads = ["locked"], writes = ["locked"]],
        Unlock => "UNLOCK" [reads = ["locked"], writes = ["locked"]],
    }
}

valid_model! {
    model CounterModel<State, Action>;
    init [State { x: 0, locked: false }];
    step |state, action| { /* transition logic */ }
    properties {
        invariant P_RANGE |state| state.x <= 3;
        invariant P_LOCKED_RANGE |state| !state.locked || state.x <= 3;
    }
}

run_registry_cli(valid_models![
    "counter" => CounterModel,
    "failing-counter" => FailingCounterModel,
]);
```

Use the explicit `model Name<State, Action>;` form. It keeps `rust-analyzer`
and macro diagnostics much saner than the old shorthand.

Project-first flow:

```sh
cargo install --path .
cargo valid models
cargo valid inspect refund-control
cargo valid graph refund-control
cargo valid readiness breakglass-access-regression
cargo valid migrate counter
cargo valid migrate counter --write
cargo valid migrate counter --check
cargo valid verify breakglass-access-regression
cargo valid generate-tests refund-control --strategy=path
cargo valid benchmark --json
cargo valid benchmark --baseline=record
cargo valid benchmark --baseline=compare --threshold-percent=25
cargo valid suite
cargo valid clean all
```

For a new project, `cargo valid init` now creates `valid.toml`, scaffolds
`examples/valid_models.rs`, and creates `tests/generated/.gitkeep`.

Registry override examples:

```sh
cargo valid --registry examples/iam_transition_registry.rs generate-tests iam-access --strategy=guard --json
cargo valid --registry examples/practical_use_cases_registry.rs verify breakglass-access-regression
```

Command meanings:

- `models`: show the model names exported by the registry file
- `inspect <model>`: show the model shape without verifying it
- `graph <model>`: render a Mermaid, DOT, or SVG diagram for the model shape
- `readiness <model>`: show capability-based migration hints and readiness findings
- `migrate <model>`: emit declarative transition snippets for step-based models, optionally with `--write` or `--check`
- `verify <model>`: verify one model
- `benchmark [model]`: run repeated verification timing for one model or the configured benchmark set, optionally with baseline record/compare
- `replay <model>`: replay an explicit action sequence and inspect the terminal state
- `suite`: run `verify` for every model exported by the registry file
- `clean`: remove generated tests and artifact output

`inspect --json` also reports a capability matrix. In practice:

- `counter` is explicit-ready but not solver-ready because it is written with a free-form `step`
- `iam-access` is solver-ready because it uses declarative `transitions { ... }`
- declarative graphs now show guard conditions, concrete field updates, and path tags directly
- `transition_details.path_tags` and `coverage.path_tags` expose the shared decision/path vocabulary
- `access-review-scale` also exercises finite enum state via `ReviewStage`

If you prefer ordinary Rust type declarations instead of `valid_state!` and
`valid_actions!`, the crate also supports `#[derive(ValidState)]` and
`#[derive(ValidAction)]` for the current common cases.

`iam_transition_registry.rs` shows the declarative transition mode, where
action/guard/effect structure is written as:

```rust
valid_model! {
    model IamAccessModel<AccessState, AccessAction>;
    init [/* ... */];
    transitions {
        transition AttachBoundary [tags = ["boundary_path"]] when |state| !state.boundary_attached => [/* next state */];
        transition AssumeSession [tags = ["session_path"]] when |state| state.boundary_attached && !state.session_active => [/* next state */];
    }
    properties {
        invariant P_BILLING_READ_REQUIRES_BOUNDARY |state| !state.billing_read_allowed || state.boundary_attached;
    }
}
```

`tags = [...]` is optional, but it is the preferred way to make
allow/deny/boundary/session paths explicit instead of relying on heuristics.

`iam_enterprise_registry.rs` is the heavier variant intended to pressure the
current lowering path. It uses explicit tags plus richer boolean expressions
such as `==`, `||`, and parenthesized guards / properties.

`enterprise_scale_registry.rs` pushes that further with larger practical state,
bounded `u16` and `u32` counters, and richer guard arithmetic such as `>=`,
`!=`, and subtraction. It is intended as the current “can this survive a heavier
business model?” suite.

`practical_use_cases_registry.rs` is the business-oriented suite for trying the
tool against more realistic workflows. It currently includes:

- `prod-deploy-safe`
  approvals + QA + freeze window + incident gating
- `breakglass-access-regression`
  a deliberate security exception regression that should fail
- `refund-control`
  fraud / risk / manager approval interaction
- `data-export-control`
  contract / DPA / region alignment gating

Use `cargo valid` directly:

```sh
cargo valid --registry examples/practical_use_cases_registry.rs models
cargo valid --registry examples/practical_use_cases_registry.rs readiness prod-deploy-safe
cargo valid --registry examples/practical_use_cases_registry.rs verify breakglass-access-regression
cargo valid --registry examples/practical_use_cases_registry.rs coverage refund-control
cargo valid --registry examples/practical_use_cases_registry.rs graph refund-control
cargo valid --registry examples/practical_use_cases_registry.rs graph refund-control --format=svg
cargo valid --registry examples/practical_use_cases_registry.rs generate-tests refund-control --strategy=path
cargo valid --registry examples/practical_use_cases_registry.rs benchmark refund-control --repeat=5 --json
cargo valid --registry examples/practical_use_cases_registry.rs benchmark refund-control --repeat=5 --baseline=record
cargo valid --registry examples/practical_use_cases_registry.rs suite
cargo valid --registry examples/enterprise_scale_registry.rs inspect access-review-scale
cargo valid --registry examples/enterprise_scale_registry.rs verify quota-guardrail-regression
cargo valid --registry examples/enterprise_scale_registry.rs benchmark quota-guardrail-regression --property=P_EXPORT_REQUIRES_BUDGET_DISCIPLINE --repeat=5 --json
```

Tracked benchmark baselines live under `benchmarks/baselines/` so CI can compare
the practical and enterprise suites against committed reference numbers.

## Rust model examples

The underlying business semantics for these examples live in
`examples/use_cases/`, so the practical scenarios stay outside the engine
package while examples and integration tests still share the same model logic.

- `iam_like_authz.rs`
  - IAM-like `deny overrides`, `boundary`, `SCP`, and request-context oriented
    authorization reasoning
  - decision trace, explanation, and authorization coverage
- `iam_policy_diff.rs`
  - policy change verification
  - finds concrete requests that became newly allowed after a policy edit
- `train_fare.rs`
  - train fare calculation with realistic business rules
  - explanation, rule coverage, and invariant checks around child fare, day
    pass behavior, and monotonic distance pricing
- `saas_entitlements.rs`
  - SaaS plan/role/feature entitlement verification
  - checks enterprise-only features, admin-only APIs, and coverage of allow/deny paths

Run them with:

```sh
cargo run --example iam_like_authz
cargo run --example iam_policy_diff
cargo run --example train_fare
cargo run --example saas_entitlements
```

## Legacy compatibility fixtures

These `.valid` files remain as a temporary compatibility harness while the
Rust model definitions become the primary route.

## Models

- `models/safe_counter.valid`
  - complete `PASS`
  - usable with `inspect`, `check`, `coverage`, `testgen --strategy=witness`
- `models/failing_counter.valid`
  - explicit `FAIL` with replayable trace
  - usable with `check`, `trace`, `explain`, `minimize`, `testgen`
- `models/multi_property.valid`
  - multiple invariants for `orchestrate`
  - one `PASS` and one `FAIL` so aggregate coverage is visible
  - usable with `orchestrate --json`
- `models/type_error.valid`
  - frontend `TYPECHECK_ERROR`
- `models/parse_error.valid`
  - frontend `PARSE_ERROR`

## Quick commands

```sh
cargo run -- check examples/models/safe_counter.valid --json
cargo run -- check examples/models/failing_counter.valid --json
cargo run -- explain examples/models/failing_counter.valid --json
cargo run -- coverage examples/models/failing_counter.valid --json
cargo run -- orchestrate examples/models/multi_property.valid --json
cargo run -- inspect examples/models/safe_counter.valid --json
cargo run --example iam_like_authz
cargo run --example iam_policy_diff
cargo run --example train_fare
```

Bundled Rust-native models exposed through the main CLI path:

```sh
cargo run -- inspect rust:counter --json
cargo run -- check rust:failing-counter --json
cargo run -- coverage rust:counter --json
```

If `valid.toml` points at `examples/valid_models.rs`, you can use:

```sh
cargo valid models
cargo valid inspect counter
cargo valid verify failing-counter
cargo valid suite
cargo valid clean all
```

Without `valid.toml`, `cargo valid` still auto-discovers
`examples/valid_models.rs` or `src/bin/valid_models.rs` when present, so
`cargo valid inspect <model>` continues to work in the common case.

## Current capability boundary

Implemented now:

- parser / resolver / typechecker / lowering
- explicit backend
- `command` backend normalization
- bounded `smt-cvc5` backend for the current MVP IR subset
- evidence trace rendering
- explain / minimize
- witness and counterexample vector generation
- coverage and aggregate coverage
- contract snapshot / lock / drift
- selfcheck

Not fully implemented yet:

- full SMT coverage beyond the current bounded invariant subset
- complete JSON Schema validation against full schemas
- richer witness synthesis beyond short synthetic traces

Compatibility-only:

- `mock-bmc` remains as a legacy compatibility backend alias for older tests and protocol assumptions

## Command backend demo

The repository also includes a minimal solver-protocol script:

- `solvers/mock_command_solver.sh`

You can use it to exercise the generic command backend without installing a
real SMT solver.

```sh
cargo run -- check examples/models/failing_counter.valid \
  --backend=command \
  --solver-exec sh \
  --solver-arg examples/solvers/mock_command_solver.sh \
  --json
```
