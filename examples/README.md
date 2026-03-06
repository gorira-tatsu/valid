# Example Models

This directory contains runnable examples that show the current implementation
boundary.

The intended primary modeling path is Rust-native, not a project-specific DSL.

## Rust-native examples

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
valid native-demo iam-authz --json
valid native-demo iam-policy-diff --json
valid native-demo train-fare --json
valid native-demo saas-entitlements --json
```

## Legacy compatibility fixtures

These `.valid` files remain as a temporary compatibility harness while the
Rust-native path becomes the primary route.

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

## Current capability boundary

Implemented now:

- parser / resolver / typechecker / lowering
- explicit backend
- `mock-bmc` and `command` backend normalization
- evidence trace rendering
- explain / minimize
- witness and counterexample vector generation
- coverage and aggregate coverage
- contract snapshot / lock / drift
- selfcheck

Not fully implemented yet:

- full external solver integrations beyond the generic command protocol
- complete JSON Schema validation against full schemas
- richer witness synthesis beyond short synthetic traces

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
