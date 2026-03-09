# Install Guide

`valid` has two practical installation modes:

1. `binary user`
   You want to run the CLI, inspect graphs, verify `.valid` files, or operate
   an already-prepared Rust registry project.
2. `model author`
   You want to write or edit Rust DSL models and run `cargo valid` against
   them.

## Recommended Modes

### 1. Prebuilt binary

This is the recommended distribution path for non-Rust users and reviewers.

- download the GitHub Release asset for your platform:
  `valid-linux-x86_64.tar.gz` or `valid-macos-aarch64.tar.gz`
- extract the archive, for example `tar -xzf valid-linux-x86_64.tar.gz`
- put `valid` and `cargo-valid` on your `PATH`
- run `valid --version`
- run `valid onboarding`

Tagged `v*` pushes publish these tarballs automatically on GitHub Releases.

Recommended release build:

```sh
cargo build --release --features varisat-backend
```

The release workflow in `.github/workflows/release.yml` builds binaries with
the embedded `sat-varisat` backend enabled.

### 2. Cargo install

For Rust users and model authors:

```sh
cargo install --path . --features varisat-backend
```

This installs:

- `valid`
- `cargo-valid`

If you want the smallest CLI build and only need explicit exploration:

```sh
cargo install --path . --features verification-runtime
```

That smaller build does not compile in `sat-varisat`.
`valid capabilities --backend=sat-varisat` will report `available=false`, and
`valid mcp` will omit `sat-varisat` from advertised backend enums unless the
binary was built with `--features varisat-backend`.

### 3. Docker

For CI or isolated execution:

```sh
docker build -t valid .
docker run --rm -it valid valid --help
```

The Docker image enables `varisat-backend`.

## Important Limitation

`cargo valid` compiles Rust registry targets such as `examples/*.rs`.

Release builds of the `valid` library exclude the verification runtime by
default. If you need the CLI binaries or registry/runtime APIs in a release
build, enable `verification-runtime` explicitly.

That means:

- authoring Rust DSL models requires a Rust toolchain
- running `cargo valid` on a Rust registry project also requires `cargo`
- running `valid` against `.valid` files does not require a Rust registry

So today:

- `.valid` compatibility mode is usable by binary-only users
- review, inspect, graph, handoff, and MCP setup are usable by binary-first users
- Rust DSL authoring is still a Rust-user workflow

## Backends

Current practical backend choices:

- `explicit`
  default, no extra solver dependency, broadest surface coverage
- `sat-varisat`
  embedded pure-Rust SAT backend for the current boolean declarative subset.
  Treat this as an embedded/portable backend, not yet the broadest solver path.
- `smt-cvc5`
  external solver path for the bounded SMT subset
- `command`
  generic external adapter for experiments and integration

## First Successful Run

If you want the shortest path to a working project:

```sh
valid onboarding
```

This path is intentionally review-first. It lets you confirm the scaffold,
inspect the starter model, review the first overview graph, and get a handoff summary
before you write your own models.

If that first run fails, use:

```sh
valid doctor
```

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

If the scaffold exists but some standard files disappeared, restore the safe
layout without overwriting existing files:

```sh
valid init --repair
```

If you want embedded SAT, use it on a small boolean declarative model:

```sh
cargo valid --registry examples/iam_transition_registry.rs verify iam-access --backend=sat-varisat
```

For external registry targets, `cargo-valid` automatically adds the
`varisat-backend` feature when you choose `--backend=sat-varisat`.
