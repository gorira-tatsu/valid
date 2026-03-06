# valid

Rust-first finite-state verification for business-rule models.

`valid` is aimed at models such as authorization, pricing, entitlements, and
stateful workflow rules. The main path is:

1. Write a model in Rust
2. Export it through a small registry file
3. Run `cargo-valid` to inspect, check, explain, cover, and generate tests

`.valid` files still work, but they are now the compatibility path rather than
the primary one.

## What It Can Do

- Explore finite state spaces with the explicit backend
- Return replayable counterexample traces
- Explain failing transitions
- Report action and guard coverage
- Generate Rust test files from counterexamples and witnesses
- Run Rust-defined models through `cargo-valid`
- Run a bounded `smt-cvc5` path for the current MVP subset

## Current Limits

- The Rust DSL is still evolving
- `valid_state!` / `valid_actions!` / `valid_model!` reduce boilerplate, but
  derive macros are not implemented yet
- Full solver coverage beyond the current bounded invariant subset is not done
- `testgen` is useful, but still closer to regression asset generation than
  fully intelligent scenario design

## Quick Start

Run the full test suite:

```sh
cargo test -q
```

Try the Rust-first path:

```sh
cargo run --bin cargo-valid -- --file examples/valid_models.rs list --json
cargo run --bin cargo-valid -- --file examples/valid_models.rs inspect counter --json
cargo run --bin cargo-valid -- --file examples/valid_models.rs check failing-counter --json
```

Try the legacy `.valid` path:

```sh
cargo run --bin valid -- inspect examples/models/safe_counter.valid --json
cargo run --bin valid -- check examples/models/failing_counter.valid --json
cargo run --bin valid -- explain examples/models/failing_counter.valid --json
```

## Mental Model

There are two ways to use the repo today.

### 1. Rust-first path

Use this for new work.

- Put model code in `examples/*.rs`, `src/bin/*.rs`, or another Rust target
- Export models through `run_registry_cli(valid_models![...])`
- Run them with `cargo-valid --file <path>`

### 2. `.valid` path

Use this for compatibility fixtures and frontend/kernel tests.

- Write a `.valid` model file
- Run it with the `valid` binary

If you are deciding between the two, use the Rust-first path.

## Command Guide

These commands are the most important ones:

- `list`
  Show the model names exported by a registry file
- `inspect <model>`
  Show model structure without running verification
- `check <model>`
  Verify one model and return `PASS` / `FAIL` / `UNKNOWN`
- `explain <model>`
  Summarize why a failure likely happened
- `coverage <model>`
  Show action and guard coverage
- `testgen <model>`
  Generate Rust tests under `tests/generated/*.rs`
- `all`
  Run `check` for every model exported by a registry file

Examples:

```sh
cargo run --bin cargo-valid -- --file examples/valid_models.rs list --json
cargo run --bin cargo-valid -- --file examples/valid_models.rs inspect counter --json
cargo run --bin cargo-valid -- --file examples/valid_models.rs check counter --json
cargo run --bin cargo-valid -- --file examples/valid_models.rs all --json
```

## Rust DSL

The current Rust DSL is built from four macros:

- `valid_state!`
- `valid_actions!`
- `valid_model!`
- `valid_models!`

Minimal example:

```rust
use valid::{
    registry::run_registry_cli,
    valid_actions, valid_model, valid_models, valid_state,
};

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
    model CounterModel;
    init [State { x: 0, locked: false }];
    step |state, action| {
        match action {
            Action::Inc if !state.locked && state.x < 3 => vec![State {
                x: state.x + 1,
                locked: state.locked,
            }],
            Action::Lock => vec![State { x: state.x, locked: true }],
            Action::Unlock => vec![State { x: state.x, locked: false }],
            _ => Vec::new(),
        }
    }
    properties {
        invariant P_RANGE |state| state.x <= 3;
        invariant P_LOCKED_RANGE |state| !state.locked || state.x <= 3;
    }
}

fn main() {
    run_registry_cli(valid_models![
        "counter" => CounterModel,
    ]);
}
```

Save that as `examples/valid_models.rs` or another registry file, then run:

```sh
cargo run --bin cargo-valid -- --file examples/valid_models.rs list --json
```

## Declarative Transition Mode

If you want action/guard/effect metadata to stay visible, use declarative
transitions instead of a free-form `step` block.

```rust
valid_model! {
    model IamAccessModel<AccessState, AccessAction>;
    init [AccessState {
        boundary_attached: false,
        session_active: false,
        billing_read_allowed: false,
    }];
    transitions {
        transition AttachBoundary when |state| !state.boundary_attached => [AccessState {
            boundary_attached: true,
            session_active: state.session_active,
            billing_read_allowed: state.billing_read_allowed,
        }];
        transition AssumeSession when |state| state.boundary_attached && !state.session_active => [AccessState {
            boundary_attached: state.boundary_attached,
            session_active: true,
            billing_read_allowed: state.billing_read_allowed,
        }];
    }
    properties {
        invariant P_BILLING_READ_REQUIRES_BOUNDARY |state| !state.billing_read_allowed || state.boundary_attached;
    }
}
```

This mode is better aligned with future solver lowering, stronger explain, and
metadata-aware test generation.

## Test Generation

`testgen` writes generated Rust tests to `tests/generated/*.rs`.

Available strategies:

- `counterexample`
  Turn a failing trace into a regression test
- `witness`
  Generate small positive-path tests
- `transition`
  Cover observed transitions
- `guard`
  Generate vectors for enabled and disabled guard cases
- `boundary`
  Try to hit min/max bounded values
- `random`
  Generate deterministic sampled paths

Examples:

```sh
cargo run --bin cargo-valid -- --file examples/valid_models.rs testgen counter --strategy=witness --json
cargo run --bin cargo-valid -- --file examples/iam_transition_registry.rs testgen iam-access --strategy=guard --json
cargo run --bin valid -- testgen examples/models/safe_counter.valid --strategy=boundary --json
```

## Examples In This Repo

Rust-first examples:

- [valid_models.rs](/Users/tatsuhiko/code/valid/examples/valid_models.rs)
- [iam_transition_registry.rs](/Users/tatsuhiko/code/valid/examples/iam_transition_registry.rs)
- [examples/README.md](/Users/tatsuhiko/code/valid/examples/README.md)

Domain-oriented examples:

- `cargo run --example iam_like_authz`
- `cargo run --example iam_policy_diff`
- `cargo run --example train_fare`
- `cargo run --example saas_entitlements`

Compatibility fixtures:

- [safe_counter.valid](/Users/tatsuhiko/code/valid/examples/models/safe_counter.valid)
- [failing_counter.valid](/Users/tatsuhiko/code/valid/examples/models/failing_counter.valid)
- [multi_property.valid](/Users/tatsuhiko/code/valid/examples/models/multi_property.valid)

## Solver Use

The default and most reliable backend today is the explicit engine.

For the current bounded SMT subset, you can also run:

```sh
cargo run --bin valid -- check examples/models/failing_counter.valid \
  --backend=smt-cvc5 \
  --solver-exec cvc5 \
  --solver-arg --lang \
  --solver-arg smt2 \
  --json
```

There is also a mock command-backend demo:

```sh
cargo run --bin valid -- check examples/models/failing_counter.valid \
  --backend=command \
  --solver-exec sh \
  --solver-arg examples/solvers/mock_command_solver.sh \
  --json
```

## Recommended Workflow

For new models:

1. Start with a Rust registry file under `examples/` or your own crate
2. Use `inspect` to confirm state fields, actions, and properties
3. Use `check` to get the first proof or counterexample
4. Use `coverage` to see missing action and guard behavior
5. Use `testgen` to turn interesting traces into regression assets
6. Move to declarative `transitions` when the model gets large enough that
   reads/writes/guards matter

For large specifications such as IAM-like authorization:

- prefer explicit metadata on ranges, reads, and writes
- prefer declarative transitions over opaque `step` logic
- keep actions narrow and composable
- split giant policies into smaller modeled transitions and properties

## Where To Read Next

- [examples/README.md](/Users/tatsuhiko/code/valid/examples/README.md)
- [rust_native_modeling_specs.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/rust_native_modeling_specs.md)
- [testgen_contract_coverage_specs.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md)
