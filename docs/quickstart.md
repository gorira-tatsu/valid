# Quickstart

Use this guide if you want to try the Rust DSL path quickly.

## 3-Minute Path

1. Install `valid` with Cargo.
2. Create or enter an empty project directory.
3. Run:

```sh
cargo install --git https://github.com/gorira-tatsu/valid --branch main valid --features varisat-backend
valid onboarding
```

This is the current Cargo-first install path while the crates.io release is
being finalized.

This gives you a working scaffold, confirms that the scaffold is healthy, shows
the starter model, renders the first overview graph, and produces an
implementation-facing handoff summary.
Before the first model review command, onboarding also warms the local Cargo
build so later steps stay focused on model output instead of compile logs.

What onboarding explains as it runs:

- detect the current project context by checking `Cargo.toml` and `valid.toml`
- create starter files such as `valid.toml`, `valid/registry.rs`,
  `valid/models/approval.rs`, `src/main.rs`, `.mcp/codex.toml`, and
  `docs/rdd/README.md` when bootstrap is needed
- validate that those scaffold files are still present before review commands
- warm the local build, which may create `Cargo.lock` and local `target/`
  artifacts
- show you which source file to read for the starter model and where the
  generated handoff artifact lands on disk

If this flow fails, run `valid doctor` first. If `doctor` reports missing
scaffold files, use `valid init --repair` and rerun onboarding.

## Manual Equivalent

`valid onboarding` is a guided wrapper around this sequence:

```sh
valid init
valid init --check
cargo build --quiet
valid models
valid inspect approval-model
valid graph approval-model --view=overview
valid handoff approval-model
```

Key files to look at after the walkthrough:

- `valid/models/approval.rs` for the starter model itself
- `valid/registry.rs` for the registry wiring used by project-mode `valid`
- `docs/rdd/README.md` for the scaffolded requirement notes location
- `.mcp/codex.toml` for local AI/MCP wiring
- `artifacts/handoff/ApprovalModel.md` for the generated starter handoff

For recovery:

```sh
valid doctor
valid init --repair
```

Use `valid doctor` for environment and PATH issues. Use `valid init --repair`
for safe scaffold recovery only after `doctor` points at missing bootstrap
files.

## What You Get

`valid init` creates a project-first layout:

- `valid.toml`
- `valid/registry.rs`
- `valid/models/`
- `docs/rdd/README.md`
- `.mcp/`
- `generated-tests/`
- `artifacts/`

The starter model is `approval-model`. It is intentionally small so you can
inspect it, graph it, and hand it off before writing your own models.

## First Commands to Try

Inspect the starter model:

```sh
valid inspect approval-model
```

Generate a handoff summary with recommended test vectors:

```sh
valid handoff approval-model
```

Look at the generated graph:

```sh
valid graph approval-model --view=overview
```

Once that starter project works, the next useful step is to compare the three
main review surfaces:

```sh
valid inspect approval-model --json
valid graph approval-model --format=json
valid handoff approval-model --json
```

Those three outputs now expose:

- bounded-domain summaries for state fields
- analysis profile metadata
- graph snapshots with reduction metadata
- ranked handoff/testgen summaries for implementation follow-up

Outside a scaffolded project, keep the command split simple:

- use `valid ...` for `.valid` files such as `examples/scenario_focus.valid`
- use `cargo valid --registry ...` for standalone Rust registry files under
  `examples/`

## When to Read More

- If install fails or you want distribution details, read
  [Install Guide](./install.md).
- If you want to understand the scaffolded layout, read
  [Project Organization Guide](./project-organization.md).
- If you want to understand generated test specs and handoff summaries, read
  [Testgen and Handoff Guide](./testgen-and-handoff.md).
- If you want to author or review models with AI, read
  [AI Authoring Guide](./ai/authoring-guide.md).
