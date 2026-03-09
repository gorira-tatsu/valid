# Install Guide

`valid` is a Rust DSL and `cargo valid`-first tool. The canonical install path
is Cargo-based.

The package is maintained so `cargo publish --dry-run` stays green. Until the
crates.io release is live, use the Git install commands below.

## Recommended Modes

### 1. Cargo install

For Rust users and model authors:

```sh
cargo install --git https://github.com/gorira-tatsu/valid --branch main valid --features varisat-backend
```

This installs:

- `valid`
- `cargo-valid`

If you are working from a local checkout instead of Git:

```sh
cargo install --path . --features varisat-backend
```

If you want the smallest CLI build and only need explicit exploration:

```sh
cargo install --git https://github.com/gorira-tatsu/valid --branch main valid --features verification-runtime
```

That smaller build does not compile in `sat-varisat`.
`valid capabilities --backend=sat-varisat` will report `available=false`, and
`valid mcp` will omit `sat-varisat` from advertised backend enums unless the
binary was built with `--features varisat-backend`.
`valid selfcheck --json` will also mark the preferred SAT backend as unavailable
instead of pretending parity is ready.

## Important Limitation

`cargo valid` compiles Rust registry targets such as `examples/*.rs`.

Release builds of the `valid` library exclude the verification runtime by
default. If you need the CLI binaries or registry/runtime APIs in a release
build, enable `verification-runtime` explicitly.

That means:

- authoring Rust DSL models requires a Rust toolchain
- running `cargo valid` on a Rust registry project also requires `cargo`
- `.valid` is a compatibility path, not the main authoring path

## Backends

Current practical backend choices:

- `explicit`
  default, no extra solver dependency, broadest surface coverage
- `sat-varisat`
  preferred embedded pure-Rust SAT backend for the current bounded declarative
  subset. Use this as the main SAT path when the model stays within the
  supported finite lowering surface.
- `smt-cvc5`
  external solver path for the bounded SMT subset and broader experiments
- `command`
  generic external adapter for experiments and integration

## First Successful Run

If you want the shortest path to a working project:

```sh
valid --version
valid onboarding
```

This path is intentionally review-first. It lets you confirm the scaffold,
inspect the starter model, review the first overview graph, and get a handoff summary
before you write your own models.

If that first run fails, use:

```sh
valid doctor
```

Use `valid doctor` for shell/PATH/Cargo/project diagnostics first. If it reports
safe scaffold drift, continue with `valid init --repair`.

After install, these two commands tell you whether the preferred SAT path is
really usable in this binary:

```sh
valid capabilities --backend=sat-varisat
valid selfcheck --json
```

`valid capabilities` reports `preferred`, `selfcheck_status`, and
`parity_status`. `valid selfcheck --json` reports backend-level readiness plus
the varisat parity cases when that backend is compiled in.

## Project Setup

Create a project skeleton:

```sh
valid init
```

Then use:

```sh
cargo valid models
cargo valid inspect approval-model
cargo valid handoff approval-model
valid init --check
```

If `valid doctor` reports missing scaffold files, restore the safe layout
without overwriting existing files:

```sh
valid init --repair
```

If you want embedded SAT, use it on a small boolean declarative model:

```sh
cargo valid --registry examples/iam_transition_registry.rs verify iam-access --backend=sat-varisat
```

For external registry targets, `cargo-valid` automatically adds the
`varisat-backend` feature when you choose `--backend=sat-varisat`.

If you want project policy to prefer the SAT path once installed, add:

```toml
preferred_backends = ["explicit", "sat-varisat"]
```
