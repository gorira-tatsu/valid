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

User-facing DSL documentation lives in [docs/README.md](./docs/README.md),
especially [docs/dsl/README.md](./docs/dsl/README.md),
[docs/dsl/language-spec.md](./docs/dsl/language-spec.md), and
[docs/dsl/language-evolution.md](./docs/dsl/language-evolution.md). The action
evolution plan lives in
[docs/dsl/parameterized-action-roadmap.md](./docs/dsl/parameterized-action-roadmap.md).
AI-assisted authoring should start with [docs/ai/authoring-guide.md](./docs/ai/authoring-guide.md),
[docs/ai/model-authoring-best-practices.md](./docs/ai/model-authoring-best-practices.md),
and [docs/ai/curriculum.md](./docs/ai/curriculum.md). Project layout and
file-splitting guidance lives in
[docs/project-organization.md](./docs/project-organization.md).
Installation and packaging guidance lives in
[docs/install.md](./docs/install.md), and the clean-architecture overview lives
in [docs/architecture.md](./docs/architecture.md).
Artifact inventory and run-history guidance lives in
[docs/artifacts.md](./docs/artifacts.md).

The product story is now:

- declarative `transitions { ... }` models are the canonical analysis path
- free-form `step` models are still supported, but may remain explicit-only
- `inspect` reports a capability matrix so you can see which path a model can use
- `lint` / `readiness` now reports both capability blockers and maintainability guidance

## What It Can Do

- Explore finite state spaces with the explicit backend
- Return replayable counterexample traces
- Explain failing transitions
- Report action and guard coverage
- Generate Rust test files from counterexamples and witnesses
- Run Rust-defined models through `cargo-valid`
- Run a pure-Rust embedded SAT path through `sat-varisat`
- Run a bounded `smt-cvc5` path for the current MVP subset
- Lower modulo-based declarative guards and properties such as FizzBuzz-style `%`

## Current Limits

- The Rust DSL is still evolving
- `#[derive(ValidState)]` / `#[derive(ValidAction)]` work for the current
  common cases, but the derive surface is still intentionally small
- Full solver coverage beyond the current bounded invariant subset is not done
- `testgen` is useful, but still closer to regression asset generation than
  fully intelligent scenario design

## Quick Start

There are two user stories:

- `binary user`
  install a prebuilt binary and use `valid` for `.valid` compatibility flows
- `Rust model author`
  install with Cargo and use `cargo valid` against Rust registries

If you want details, read [docs/install.md](./docs/install.md).

Run the full test suite:

```sh
cargo test -q --features verification-runtime
```

Initialize a project once:

```sh
cargo install --path . --features varisat-backend
cargo valid init
```

`cargo valid init` creates a minimal `valid.toml`, scaffolds a starter
registry file under `examples/valid_models.rs`, creates the default
`generated-tests/`, `artifacts/`, and `benchmarks/baselines/` directories, and
writes project-local AI/MCP bootstrap snippets under `.mcp/` plus
`docs/ai/bootstrap.md`:

```toml
registry = "examples/valid_models.rs"
default_backend = "explicit"
default_property = ""
default_solver_executable = ""
default_solver_args = []
suite_models = []
preferred_backends = ["explicit"]
default_suite = "smoke"
minimum_overall_coverage_percent = 80
minimum_business_coverage_percent = 75
minimum_setup_coverage_percent = 100
minimum_requirement_coverage_percent = 70

[critical_properties]
# approval-model = ["P_APPROVAL_IS_BOOLEAN"]

[property_suites.smoke]
entries = []

benchmark_models = []
benchmark_repeats = 3
generated_tests_dir = "generated-tests"
artifacts_dir = "artifacts"
benchmarks_dir = "artifacts/benchmarks"
benchmark_baseline_dir = "benchmarks/baselines"
benchmark_regression_threshold_percent = 25
default_graph_format = "mermaid"
```

Treat `valid.toml` as the single source of truth for project verification
policy. In addition to `critical_properties` and `property_suites`, you can
declare:

- `preferred_backends`
- `default_suite`
- coverage gates such as
  `minimum_overall_coverage_percent`,
  `minimum_business_coverage_percent`,
  `minimum_setup_coverage_percent`, and
  `minimum_requirement_coverage_percent`

`cargo valid list --json`, registry `list --json`, and MCP `valid_list_models`
now expose the same `verification_policy` object, and `cargo valid suite
--json` / `valid_suite_run` honor `default_suite` when no explicit suite is
selected.
After init, the shortest AI-assisted setup path is:

```sh
cargo valid models
cargo valid inspect approval-model
cat .mcp/codex.toml
```

The generated `.mcp/` snippets use `valid mcp --project .`, so they avoid
hard-coded local build paths and keep the project root as the source of truth.
Reusable CI workflow templates for `inspect`, `check`, `generate-tests`,
`conformance`, and `doc --check` now live in
[`.github/workflows/`](.github/workflows/) with usage notes under
[docs/ci/README.md](docs/ci/README.md). The repository validates them against a
fixture project in
[`tests/fixtures/projects/ci_template_project/`](tests/fixtures/projects/ci_template_project/).
For Rust implementations that live in the same process, the library also
exposes `valid::conformance::RustConformanceHarness` and
`run_rust_conformance(...)`, so model-derived vectors can be checked without an
external stdin/stdout runner. The external `valid conformance --runner ...`
path remains the compatibility path for non-Rust or process-boundary SUTs.
Keep the registry file thin and move real model logic into `src/models/` or
another explicit module tree. The recommended project layout is documented in
[docs/project-organization.md](./docs/project-organization.md).
That guide now also defines the pre-compose integration-model pattern:
standalone models for local rules, dedicated integration models for shared-state
cross-domain checks, and contract-only checks when the uncertainty is in the
implementation boundary instead of model composition.
This repository already includes a `valid.toml`, so from the repo root the
default `cargo valid` workflow points at the smallest step-first example:

```sh
cargo valid models
cargo valid inspect counter
cargo valid graph counter
cargo valid readiness counter
cargo valid lint counter --json
cargo valid migrate counter
cargo valid migrate counter --write
cargo valid migrate counter --check
cargo valid handoff counter --write
cargo valid handoff counter --check
cargo valid verify failing-counter
cargo valid benchmark
cargo valid benchmark --baseline=compare
cargo valid suite
```

Arithmetic-heavy declarative registries also work:

```sh
cargo valid --registry examples/fizzbuzz.rs inspect fizzbuzz
cargo valid --registry examples/fizzbuzz.rs verify fizzbuzz --property=P_FIZZBUZZ_DIVISIBLE_BY_BOTH
cargo valid --registry examples/fizzbuzz.rs graph fizzbuzz

cargo valid --registry examples/tenant_relation_registry.rs inspect tenant-relation-safe
cargo valid --registry examples/tenant_relation_registry.rs verify tenant-relation-regression --property=P_NO_CROSS_TENANT_ACCESS

cargo valid --registry examples/password_policy.rs inspect password-policy-safe
cargo valid --registry examples/password_policy.rs verify password-policy-regression --property=P_PASSWORD_POLICY_MATCHES_FLAG
```

Service-oriented grouped transition registries also work:

```sh
cargo valid --registry examples/saas_multi_tenant_registry.rs inspect tenant-isolation-safe
cargo valid --registry examples/saas_multi_tenant_registry.rs verify tenant-isolation-regression --property=P_NO_CROSS_TENANT_ACCESS
```

Use `--json` for CI, scripts, or AI integrations:

```sh
cargo valid verify failing-counter --json
```

Target critical properties or named suites from `valid.toml`:

```sh
cargo valid suite --critical
cargo valid suite --suite=smoke
cargo valid list --json
```

Try the legacy `.valid` path:

```sh
cargo run --features verification-runtime --bin valid -- inspect tests/fixtures/models/safe_counter.valid
cargo run --features verification-runtime --bin valid -- verify tests/fixtures/models/failing_counter.valid
cargo run --features verification-runtime --bin valid -- explain tests/fixtures/models/failing_counter.valid
```

## MCP Server

`valid mcp` exposes `valid` over MCP stdio so Codex, Claude Code, Claude
Desktop, and other MCP clients can call it as tools.

Recommended AI flow:

1. call `valid_docs_index`
2. read `ai-authoring-guide` and `ai-curriculum` through `valid_docs_get`
3. if the brief is still moving, read `ai-requirement-refinement-workflow`
4. read one curated example through `valid_example_get`
5. move to `valid_inspect`, `valid_lint`, and `valid_check`

Available prompts:

- `refine_requirement`
- `clarify_requirement`
- `refine_requirement_from_evidence`
- `author_model`
- `review_model`
- `migrate_step_to_transitions`
- `explain_readiness_failure`
- `triage_conformance_failure`

Prompt-driven flow:

1. start with `refine_requirement` when the requirement is still ambiguous
2. use `refine_requirement_from_evidence` when counterexamples, dead actions, vacuity clues, or mismatches show requirement drift
3. `clarify_requirement` remains available for compatibility-oriented clients
4. move to `author_model` or `review_model`
5. use `migrate_step_to_transitions` for step-heavy models
6. use `explain_readiness_failure` or `triage_conformance_failure` when a run
   already failed

Available tools:

- `valid_docs_index`
- `valid_docs_get`
- `valid_examples_list`
- `valid_example_get`
- `valid_inspect`
- `valid_check`
- `valid_explain`
- `valid_handoff`
- `valid_coverage`
- `valid_testgen`
- `valid_replay`
- `valid_contract_snapshot`
- `valid_contract_check`
- `valid_suite_run`
- `valid_list_models`
- `valid_graph`
- `valid_lint`

Install it:

```sh
cargo install --path . --features varisat-backend
```

### DSL Mode

Use this when the source of truth is a `.valid` file.

```sh
claude mcp add valid-dsl -- valid mcp --model-file /absolute/path/to/model.valid
```

If you do not pin `--model-file` at startup, pass `model_file` or `source` in
each tool call.

### Registry Mode

Use this when the source of truth is a Rust registry project. `valid mcp`
reuses the same project-first discovery rules as `cargo valid`, so MCP clients
can point at a project root instead of a `target/debug/...` executable.

```sh
claude mcp add valid-registry -- valid mcp --manifest-path /absolute/path/to/project/Cargo.toml
```

If the project uses `valid.toml`, `valid mcp` will honor its `registry`
setting. Without `valid.toml`, it falls back to `examples/valid_models.rs` or
`src/bin/valid_models.rs`. You can also pin a target explicitly:

```sh
valid mcp --manifest-path /absolute/path/to/project/Cargo.toml --example valid_models
valid mcp --manifest-path /absolute/path/to/project/Cargo.toml --bin my_registry
valid mcp --manifest-path /absolute/path/to/project/Cargo.toml --registry examples/custom_registry.rs
```

If you prefer a project root over a manifest path, use:

```sh
valid mcp --project /absolute/path/to/project
```

To print a ready-to-paste client snippet instead of assembling config by hand:

```sh
valid mcp --project /absolute/path/to/project --print-config codex --name valid-registry
valid mcp --model-file /absolute/path/to/model.valid --print-config claude-desktop --name valid-dsl
```

For reproducible registry startup, you can pass build policy through to Cargo:

```sh
valid mcp --project /absolute/path/to/project --locked --offline
valid mcp --project /absolute/path/to/project --feature varisat-backend
```

Fresh `cargo valid init` projects now also include local snippets at:

- `.mcp/codex.toml`
- `.mcp/claude-code.json`
- `.mcp/claude-desktop.json`
- `docs/ai/bootstrap.md`

When registry mode is configured at startup, tool calls only need `model_name`.
Without startup configuration, pass `registry_binary` and `model_name` per
call.

`valid_contract_snapshot` and `valid_contract_check` can operate on one
registry model when `model_name` is provided, or on the full registry when it
is omitted.

If `valid.toml` declares `critical_properties` or `property_suites`, MCP
clients can discover them through `valid_list_models`, run them through
`valid_suite_run`, and read rerun recommendations from `valid_contract_check`.

Configuration templates live at:

- [docs/mcp/claude_desktop_config.json](docs/mcp/claude_desktop_config.json)
- [docs/mcp/claude_code.mcp.json](docs/mcp/claude_code.mcp.json)
- [docs/mcp/codex_config.toml](docs/mcp/codex_config.toml)

`valid-mcp` remains available as a low-level compatibility binary for clients
that want to pass `--registry-binary` or `--model-file` directly, but `valid
mcp` is the recommended setup path.

## Mental Model

There are two ways to use the repo today.

### 1. Rust-first path

Use this for new work.

- Put model code in `examples/*.rs`, `src/bin/*.rs`, or another Rust target
- Export models through `run_registry_cli(valid_models![...])`
- Add `valid.toml`
- Run them with `cargo valid`
- This path requires a Rust toolchain because `cargo valid` compiles the
  registry target

### 2. `.valid` path

Use this for compatibility fixtures and frontend/kernel tests.

- Write a `.valid` model file
- Run it with the `valid` binary

If you are deciding between the two, use the Rust-first path.

## Install Modes

- Prebuilt binary
  easiest distribution path; recommended for operators and reviewers
  tagged releases publish `valid-linux-x86_64.tar.gz` and
  `valid-macos-aarch64.tar.gz` on GitHub Releases
- `cargo install --path . --features varisat-backend`
  recommended for authors working on Rust DSL registries
- Docker
  useful for CI and isolated local runs

The embedded SAT backend is optional at compile time. The release workflow
builds binaries with `varisat-backend` enabled, while source installs can
choose the feature set explicitly. `verification-runtime` is required for CLI
binaries and registry-driven embedding in release builds; plain library release
builds leave that runtime out by default.

## Command Guide

Primary commands:

- `init`
  Write `valid.toml`, scaffold a registry file, and create `generated-tests/`
- `models`
  Show the model names exported by the configured registry
- `inspect <model>`
  Show model structure without running verification
- `graph <model>`
  Render a model diagram in Mermaid, DOT, SVG, text, or JSON
- `readiness <model>`
  Report capability-based migration findings and analysis-readiness gaps
- `migrate <model>`
  Print declarative transition snippets for step-based models. Add `--write`
  to persist them under `artifacts/migrations/`, or add `--check` to run a
  migration audit that reports action coverage and whether manual review is
  still required.
- `verify <model>`
  Verify one model and return `PASS` / `FAIL` / `UNKNOWN`
- `benchmark [model]`
  Run repeated verification timing for one model or for `benchmark_models`.
  Use `--baseline=record` and `--baseline=compare` to gate regressions.
- `explain <model>`
  Summarize why a failure likely happened
- `coverage <model>`
  Show action and guard coverage
- `generate-tests <model>`
  Generate Rust tests under `generated-tests/*.rs`. JSON output includes
  `strictness` and `derivation` so CI can distinguish strict trace-backed
  vectors from heuristic or synthetic ones. Replay-backed vectors also carry
  expected property/path assertions so reviews can tell whether a generated
  case is checking a property failure, a witness, or a tagged policy path.
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
cargo valid inspect counter
cargo valid graph counter
cargo valid --registry examples/saas_multi_tenant_registry.rs graph tenant-isolation-safe --format=dot
cargo valid --registry examples/saas_multi_tenant_registry.rs graph tenant-isolation-safe --format=svg
cargo valid readiness counter
cargo valid migrate counter
cargo valid migrate counter --write
cargo valid migrate counter --check
cargo valid verify failing-counter
cargo valid verify failing-counter --json
cargo valid benchmark --json
cargo valid benchmark --baseline=record
cargo valid benchmark --baseline=compare --threshold-percent=25
cargo valid suite
cargo valid clean all
```

Override the configured registry only when needed:

```sh
cargo valid --registry examples/valid_models.rs inspect counter
cargo valid --registry examples/saas_multi_tenant_registry.rs verify tenant-isolation-regression
cargo valid --registry examples/saas_multi_tenant_registry.rs graph tenant-isolation-safe
cargo valid --registry examples/iam_transition_registry.rs graph iam-access
cargo valid --registry benchmarks/registries/enterprise_scale_registry.rs verify quota-guardrail-regression
```

## Packaging Notes

If you are distributing `valid` to non-Rust users:

- ship prebuilt `valid` and `cargo-valid` binaries
- document that `.valid` compatibility mode works without Rust DSL authoring
- document that Rust registry projects still require `cargo` because their
  models are compiled on demand

This is why the repository keeps solver integrations behind adapters and keeps
the install guide separate from the DSL guide.

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

Declarative models can now use either flat `transition Action ...` entries or
grouped `on Action { ... }` syntax. Both lower to the same transition IR.

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
use valid::{ValidAction, ValidEnum, ValidState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValidEnum)]
enum ReviewStage {
    Draft,
    Approved,
}

#[derive(Clone, Debug, PartialEq, Eq, ValidState)]
struct State {
    #[valid(range = "0..=3")]
    x: u8,
    #[valid(enum)]
    review_stage: Option<ReviewStage>,
    #[valid(set)]
    approvals: valid::FiniteEnumSet<ReviewStage>,
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
    FiniteEnumSet,
    registry::run_registry_cli,
    valid_actions, valid_model, valid_models, valid_state, ValidEnum,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidEnum)]
enum ReviewStage {
    Draft,
    Approved,
}

valid_state! {
    struct State {
        x: u8 [range = "0..=3"],
        review_stage: Option<ReviewStage> [enum],
        approvals: FiniteEnumSet<ReviewStage> [set],
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
    init [State { x: 0, review_stage: ReviewStage::Draft, locked: false }];
    step |state, action| {
        match action {
            Action::Inc if !state.locked && state.x < 3 => vec![State {
                x: state.x + 1,
                review_stage: state.review_stage,
                locked: state.locked,
            }],
            Action::Lock => vec![State { x: state.x, review_stage: state.review_stage, locked: true }],
            Action::Unlock => vec![State { x: state.x, review_stage: state.review_stage, locked: false }],
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

If you embed `valid` as a library and use `registry::run_registry_cli(...)` or
other verification/runtime APIs from a release build, enable the
`verification-runtime` feature on that dependency.

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

`testgen` writes generated Rust tests to `generated-tests/*.rs`.
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
cargo valid --registry examples/saas_multi_tenant_registry.rs generate-tests tenant-isolation-regression --strategy=counterexample
cargo run --features verification-runtime --bin valid -- generate-tests tests/fixtures/models/safe_counter.valid --strategy=boundary
cargo run --features verification-runtime --bin valid -- generate-tests tests/fixtures/models/multi_property.valid --property=P_STRICT --strategy=counterexample
cargo valid --registry examples/valid_models.rs replay failing-counter --property=P_FAIL --actions=INC,INC
```

## Examples In This Repo

Rust-first examples:

- [valid_models.rs](examples/valid_models.rs)
- [fizzbuzz.rs](examples/fizzbuzz.rs)
- [saas_multi_tenant_registry.rs](examples/saas_multi_tenant_registry.rs)
- [iam_transition_registry.rs](examples/iam_transition_registry.rs)
- [examples/README.md](examples/README.md)

Benchmark and stress registries:

- [practical_use_cases_registry.rs](benchmarks/registries/practical_use_cases_registry.rs)
- [enterprise_scale_registry.rs](benchmarks/registries/enterprise_scale_registry.rs)
- [iam_enterprise_registry.rs](benchmarks/registries/iam_enterprise_registry.rs)

Compatibility fixtures:

- [safe_counter.valid](tests/fixtures/models/safe_counter.valid)
- [failing_counter.valid](tests/fixtures/models/failing_counter.valid)
- [multi_property.valid](tests/fixtures/models/multi_property.valid)

## Core Examples

The default project registry is
[valid_models.rs](examples/valid_models.rs). It is
intentionally tiny and easy to debug.

Use [saas_multi_tenant_registry.rs](examples/saas_multi_tenant_registry.rs)
when you want a still-readable service-oriented example with:

- tenant onboarding growth limits
- enterprise-only entitlements
- isolation review gating
- cross-tenant access regression detection

Quick trial:

```sh
cargo valid models
cargo valid inspect counter
cargo valid verify failing-counter
cargo valid --registry examples/saas_multi_tenant_registry.rs graph tenant-isolation-safe
cargo valid --registry examples/saas_multi_tenant_registry.rs generate-tests tenant-isolation-regression --strategy=counterexample
```

Heavier workflow and scale suites still exist, but they now live under
`benchmarks/registries/` and are treated as stress inputs instead of the
default examples.

## Solver Use

The default and most reliable backend today is the explicit engine.

For the current bounded SMT subset, you can also run:

```sh
cargo run --features verification-runtime --bin valid -- check tests/fixtures/models/failing_counter.valid \
  --backend=smt-cvc5 \
  --solver-exec cvc5 \
  --solver-arg --lang \
  --solver-arg smt2 \
  --json
```

There is also a mock command-backend demo:

```sh
cargo run --features verification-runtime --bin valid -- check tests/fixtures/models/failing_counter.valid \
  --backend=command \
  --solver-exec sh \
  --solver-arg tests/fixtures/solvers/mock_command_solver.sh \
  --json
```

Declarative Rust models can use the same adapter path:

```sh
cargo valid verify iam-access \
  --backend=command \
  --solver-exec sh \
  --solver-arg tests/fixtures/solvers/mock_command_solver.sh \
  --json
```

From another crate root, `cargo valid` auto-discovers `valid.toml` first, then
falls back to `examples/valid_models.rs` or `src/bin/valid_models.rs` when
present. If neither exists, it now errors instead of silently exposing bundled
fixtures, so the common case can be as short as:

```sh
cargo valid inspect my-model --json
```

To remove generated test files and artifact output:

```sh
cargo valid clean all --json
valid clean all --json
```

To measure a benchmark registry repeatedly:

```sh
cargo valid benchmark --json
cargo valid benchmark --baseline=record
cargo valid benchmark --baseline=compare --threshold-percent=25
cargo valid --registry benchmarks/registries/enterprise_scale_registry.rs benchmark quota-guardrail-regression --property=P_EXPORT_REQUIRES_BUDGET_DISCIPLINE --repeat=5 --json
./scripts/benchmark-suite.sh compare
./scripts/benchmark-suite.sh record
```

Benchmark baselines are meant to live in-repo under `benchmarks/baselines/` so
CI can compare deterministic state-space metrics and elapsed time against a
tracked reference set. The standard CI job now runs the shared suite from
`.github/workflows/ci.yml` on pushes, pull requests, manual dispatch, and the
nightly schedule at `18:00 UTC`, emitting warnings when a comparison is
missing/invalid and failing the benchmark job when the default 25% regression
threshold is exceeded.

## Repository Layout

- `examples/`
  small user-facing registry examples only
- `benchmarks/registries/`
  larger practical and scale-oriented registries
- `tests/fixtures/`
  compatibility `.valid` models, mock solvers, and domain fixtures used by tests
- `tests/`
  engine-facing integration tests and CLI/E2E verification
- `packages/valid/src/`
  engine, DSL, lowering, solver adapters, and CLI implementation
- `packages/valid_derive/`
  proc-macro crate for DSL derives; `valid_model!` itself currently expands via
  `macro_rules!` in `packages/valid/src/modeling/mod.rs`

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

- [examples/README.md](examples/README.md)
- [rust_native_modeling_specs.md](docs/rdd/08_specs/rust_native_modeling_specs.md)
- [testgen_contract_coverage_specs.md](docs/rdd/08_specs/testgen_contract_coverage_specs.md)
