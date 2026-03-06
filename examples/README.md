# Example Models

This directory contains reproducible `.valid` models that show the current
implementation boundary.

These fixtures are a temporary compatibility harness. The intended primary
modeling path is Rust-native (`Finite` + `VerifiedMachine`), not a project-
specific DSL.

Rust-native authorization example:

- `iam_like_authz.rs`
  - IAM-like `deny overrides`, `boundary`, `SCP`, and request-context oriented
    authorization reasoning without `.valid`

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
