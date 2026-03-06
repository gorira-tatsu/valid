# valid

Rust-first finite-state verification for business-rule models.

The intended primary path is to define models in Rust, export them through a
registry file, and run verification through `cargo-valid`.

## Status

Implemented now:

- explicit finite-state exploration
- evidence traces and explain output
- coverage and test-vector generation
- `cargo-valid` registry workflow
- bounded `smt-cvc5` backend for the current MVP subset

Still limited:

- the Rust DSL is still evolving
- `valid_model!` reduces boilerplate, but state/action derives are not built yet
- full SMT coverage beyond the bounded invariant subset is not implemented

## Rust-first flow

Define state and action types in Rust, then declare models with `valid_model!`
and export them with `valid_models!`.

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

Save that as `examples/valid_models.rs` or `src/bin/valid_models.rs`, then run:

```sh
cargo run --bin cargo-valid -- --file examples/valid_models.rs list --json
cargo run --bin cargo-valid -- --file examples/valid_models.rs inspect counter --json
cargo run --bin cargo-valid -- --file examples/valid_models.rs check counter --json
cargo run --bin cargo-valid -- --file examples/valid_models.rs testgen counter --strategy=witness --json
cargo run --bin cargo-valid -- --file examples/valid_models.rs all --json
```

Command meanings:

- `list`: show the model names exported by the registry file
- `inspect <model>`: show the model shape without verifying it
- `check <model>`: verify one model
- `all`: run `check` for every model exported by the registry file

## Repository examples

- [examples/valid_models.rs](/Users/tatsuhiko/code/valid/examples/valid_models.rs)
- [examples/README.md](/Users/tatsuhiko/code/valid/examples/README.md)

## Verification

```sh
cargo test -q
```
