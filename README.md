# valid

Rust-first finite-state verification for business-rule models.

`valid` is aimed at models such as authorization, pricing, entitlements, and
stateful workflow rules. The main path is:

1. Write a model in Rust
2. Export it through a small registry file
3. Run `cargo valid init` once for a new project
4. Use `cargo valid` from the project root

`.valid` files still work, but they are now the compatibility path rather than
the primary one.

The product story is now:

- declarative `transitions { ... }` models are the canonical analysis path
- free-form `step` models are still supported, but may remain explicit-only
- `inspect` reports a capability matrix so you can see which path a model can use

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
- `#[derive(ValidState)]` / `#[derive(ValidAction)]` work for the current
  common cases, but the derive surface is still intentionally small
- Full solver coverage beyond the current bounded invariant subset is not done
- `testgen` is useful, but still closer to regression asset generation than
  fully intelligent scenario design

## Quick Start

Run the full test suite:

```sh
cargo test -q
```

Initialize a project once:

```sh
cargo install --path .
cargo valid init
```

`cargo valid init` creates a minimal `valid.toml` and scaffolds a starter
registry file under `examples/valid_models.rs`:

```toml
registry = "examples/valid_models.rs"
default_backend = "explicit"
suite_models = []
generated_tests_dir = "tests/generated"
artifacts_dir = "artifacts"
default_graph_format = "mermaid"
```

This repository already includes a `valid.toml`, so from the repo root the
default `cargo valid` workflow points at the practical business suite:

```sh
cargo valid models
cargo valid inspect refund-control
cargo valid graph refund-control
cargo valid readiness breakglass-access-regression
cargo valid verify breakglass-access-regression
cargo valid suite
```

Use `--json` for CI, scripts, or AI integrations:

```sh
cargo valid verify breakglass-access-regression --json
```

Try the legacy `.valid` path:

```sh
cargo run --bin valid -- inspect examples/models/safe_counter.valid
cargo run --bin valid -- verify examples/models/failing_counter.valid
cargo run --bin valid -- explain examples/models/failing_counter.valid
```

## Mental Model

There are two ways to use the repo today.

### 1. Rust-first path

Use this for new work.

- Put model code in `examples/*.rs`, `src/bin/*.rs`, or another Rust target
- Export models through `run_registry_cli(valid_models![...])`
- Add `valid.toml`
- Run them with `cargo valid`

### 2. `.valid` path

Use this for compatibility fixtures and frontend/kernel tests.

- Write a `.valid` model file
- Run it with the `valid` binary

If you are deciding between the two, use the Rust-first path.

## Command Guide

Primary commands:

- `init`
  Write `valid.toml`, scaffold a registry file, and create `tests/generated/`
- `models`
  Show the model names exported by the configured registry
- `inspect <model>`
  Show model structure without running verification
- `graph <model>`
  Render a model diagram in Mermaid, DOT, SVG, text, or JSON
- `readiness <model>`
  Report capability-based migration findings and analysis-readiness gaps
- `verify <model>`
  Verify one model and return `PASS` / `FAIL` / `UNKNOWN`
- `explain <model>`
  Summarize why a failure likely happened
- `coverage <model>`
  Show action and guard coverage
- `generate-tests <model>`
  Generate Rust tests under `tests/generated/*.rs`
- `replay <model>`
  Replay an action sequence and return the terminal state
- `suite`
  Run `verify` for the configured suite or every model in the registry
- `clean`
  Remove generated tests and artifact output

Legacy aliases still work:

- `list`, `lint`, `check`, `testgen`, `all`, `--file`

Examples:

```sh
cargo valid init
cargo valid models
cargo valid inspect refund-control
cargo valid graph refund-control
cargo valid graph refund-control --format=dot
cargo valid graph refund-control --format=svg
cargo valid readiness breakglass-access-regression
cargo valid verify breakglass-access-regression
cargo valid verify breakglass-access-regression --json
cargo valid suite
cargo valid clean all
```

Override the configured registry only when needed:

```sh
cargo valid --registry examples/valid_models.rs inspect counter
cargo valid --registry examples/practical_use_cases_registry.rs models
cargo valid --registry examples/practical_use_cases_registry.rs verify breakglass-access-regression
cargo valid --registry examples/practical_use_cases_registry.rs graph refund-control
cargo valid --registry examples/iam_transition_registry.rs graph iam-access
cargo valid --registry examples/enterprise_scale_registry.rs verify quota-guardrail-regression
```

`inspect --json` now includes:

- `machine_ir_ready`
- `capabilities.parse_ready`
- `capabilities.explicit_ready`
- `capabilities.ir_ready`
- `capabilities.solver_ready`
- `capabilities.coverage_ready`
- `capabilities.explain_ready`
- `capabilities.testgen_ready`
- `capabilities.reasons`

For example, a `step`-only model can be explicit-ready but solver-not-ready,
while a declarative transition model can be solver-ready.

`transition_details` and coverage reports also expose inferred `path_tags`
such as `allow_path`, `deny_path`, `boundary_path`, `guard_path`, and
`write_path`. These are the shared decision/path vocabulary used by inspect,
coverage, explain, and test generation.

`graph` is built from the same inspect data. Declarative models render guard
conditions, concrete field updates, and path tags in Mermaid, DOT, and SVG.
Step-only models are explicitly marked `explicit-only / opaque-step`.

`verify --json` now includes CI-oriented summary fields such as `ci.exit_code`
and `review_summary`, while `explain` includes failing action metadata,
write-overlap hints, and reviewer-oriented next steps.

`readiness --json` now also includes migration snippets for opaque step models,
so you can lift a critical action into `transitions { ... }` from the report
instead of writing the first skeleton by hand.

## Rust DSL

The current Rust DSL is built from four macros:

- `valid_state!`
- `valid_actions!`
- `valid_model!`
- `valid_models!`

If you already have ordinary Rust types and do not want the macros to define
the types for you, there is also an attach-spec path:

- `valid_state_spec!`
- `valid_action_spec!`

For ordinary Rust type declarations, you can also derive directly:

```rust
use valid::{ValidAction, ValidState};

#[derive(Clone, Debug, PartialEq, Eq, ValidState)]
struct State {
    #[valid(range = "0..=3")]
    x: u8,
    locked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValidAction)]
enum Action {
    #[valid(action_id = "INC", reads = ["x", "locked"], writes = ["x"])]
    Inc,
    #[valid(action_id = "LOCK", reads = ["locked"], writes = ["locked"])]
    Lock,
    #[valid(action_id = "UNLOCK", reads = ["locked"], writes = ["locked"])]
    Unlock,
}
```

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
    model CounterModel<State, Action>;
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

Prefer the explicit `model Name<State, Action>;` form. It produces better
macro diagnostics than the old shorthand and is now the supported path.

Save that as `examples/valid_models.rs` or another registry file, then run:

```sh
cargo valid --registry examples/valid_models.rs models
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
        transition AttachBoundary [tags = ["boundary_path"]] when |state| !state.boundary_attached => [AccessState {
            boundary_attached: true,
            session_active: state.session_active,
            billing_read_allowed: state.billing_read_allowed,
        }];
        transition AssumeSession [tags = ["session_path"]] when |state| state.boundary_attached && !state.session_active => [AccessState {
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

Use explicit `tags = [...]` when a transition represents a domain-specific
decision path such as `allow_path`, `deny_path`, `boundary_path`, or
`session_path`. When tags are omitted, `valid` falls back to heuristics.

If a model uses only `step`, `inspect` will usually report capability reasons
such as `opaque_step_closure` or `missing_declarative_transitions`. That is the
intended migration signal toward declarative transitions when the model grows.

`graph` uses the same inspect metadata and emits Mermaid `flowchart` output, so
you can paste it directly into Mermaid Live, GitHub Markdown, or docs.
For `step`-only models, the graph now explicitly marks the model as
`explicit-only / opaque-step` instead of pretending it has declarative
transition structure.

## Test Generation

`testgen` writes generated Rust tests to `tests/generated/*.rs`.
Generated tests are replay-backed: they call `valid replay` or
`cargo-valid replay` and assert on the terminal state captured in the vector.

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
- `path`
  Generate vectors keyed to shared decision/path tags such as `allow_path`
  and `boundary_path`
- `random`
  Generate deterministic sampled paths

Examples:

```sh
cargo valid --registry examples/valid_models.rs generate-tests counter --strategy=witness
cargo valid --registry examples/iam_transition_registry.rs generate-tests iam-access --strategy=guard
cargo valid generate-tests refund-control --strategy=path
cargo run --bin valid -- generate-tests examples/models/safe_counter.valid --strategy=boundary
cargo run --bin valid -- generate-tests examples/models/multi_property.valid --property=P_STRICT --strategy=counterexample
cargo valid --registry examples/valid_models.rs replay failing-counter --property=P_FAIL --actions=INC,INC
```

## Examples In This Repo

Rust-first examples:

- [valid_models.rs](/Users/tatsuhiko/code/valid/examples/valid_models.rs)
- [iam_transition_registry.rs](/Users/tatsuhiko/code/valid/examples/iam_transition_registry.rs)
- [iam_enterprise_registry.rs](/Users/tatsuhiko/code/valid/examples/iam_enterprise_registry.rs)
- [practical_use_cases_registry.rs](/Users/tatsuhiko/code/valid/examples/practical_use_cases_registry.rs)
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

## Practical Use-Case Suite

The repo now includes a business-oriented registry suite under
[practical_use_cases_registry.rs](/Users/tatsuhiko/code/valid/examples/practical_use_cases_registry.rs).
It is intended to answer the question "would this survive real policy and
workflow modeling?" with concrete cases instead of toy counters.

Current suite:

- `prod-deploy-safe`
  Production deploy gating with approvals, QA, freeze windows, and incidents
- `breakglass-access-regression`
  Intentional security regression showing how exception paths can bypass
  incident/approval controls
- `refund-control`
  Finance workflow with fraud clearance, risk escalation, and manager approval
- `data-export-control`
  Contract/DPA/region gating for compliance-sensitive exports

Quick trial:

```sh
cargo valid --registry examples/practical_use_cases_registry.rs models
cargo valid --registry examples/practical_use_cases_registry.rs verify prod-deploy-safe
cargo valid --registry examples/practical_use_cases_registry.rs verify breakglass-access-regression
cargo valid --registry examples/practical_use_cases_registry.rs coverage refund-control
cargo valid --registry examples/practical_use_cases_registry.rs generate-tests refund-control --strategy=path
cargo valid --registry examples/practical_use_cases_registry.rs suite
```

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

Declarative Rust models can use the same adapter path:

```sh
cargo valid verify iam-access \
  --backend=command \
  --solver-exec sh \
  --solver-arg examples/solvers/mock_command_solver.sh \
  --json
```

From another crate root, `cargo valid` auto-discovers `valid.toml` first, then
falls back to `examples/valid_models.rs` or `src/bin/valid_models.rs` when
present, so the common case can be as short as:

```sh
cargo valid inspect my-model --json
```

To remove generated test files and artifact output:

```sh
cargo valid clean all --json
valid clean all --json
```

## Repository Layout

- `valid/src/`
  engine, DSL, lowering, solver adapters, and CLI implementation
- `examples/`
  registry files, practical scenarios, and domain-heavy example models
- `examples/use_cases/`
  shared business semantics for practical examples and domain E2E tests
- `tests/`
  engine-facing integration tests and CLI/E2E verification

`valid` is treated as the language/toolchain package. Domain-heavy examples
stay outside the engine source tree and are exercised through registries,
example binaries, and integration tests.

## Recommended Workflow

For new models:

1. Start with a Rust registry file under `examples/` or your own crate
2. Use `inspect` to confirm state fields, actions, and properties
3. Use `verify` to get the first proof or counterexample
4. Use `coverage` to see missing action and guard behavior
5. Use `generate-tests` to turn interesting traces into regression assets
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
