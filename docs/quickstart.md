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

This gives you a working scaffold, confirms that the scaffold is healthy, shows
the starter model, renders the first overview graph, and produces an
implementation-facing handoff summary.

If this flow fails, run `valid doctor` first. If `doctor` reports missing
scaffold files, use `valid init --repair` and rerun onboarding.

## Manual Equivalent

`valid onboarding` is a guided wrapper around this sequence:

```sh
valid init
valid init --check
cargo valid models
cargo valid inspect approval-model
cargo valid graph approval-model --view=overview
cargo valid handoff approval-model
```

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
cargo valid inspect approval-model
```

Generate a handoff summary with recommended test vectors:

```sh
cargo valid handoff approval-model
```

Look at the generated graph:

```sh
cargo valid graph approval-model --view=overview
```

## When to Read More

- If install fails or you want distribution details, read
  [Install Guide](./install.md).
- If you want to understand the scaffolded layout, read
  [Project Organization Guide](./project-organization.md).
- If you want to understand generated test specs and handoff summaries, read
  [Testgen and Handoff Guide](./testgen-and-handoff.md).
- If you want to author or review models with AI, read
  [AI Authoring Guide](./ai/authoring-guide.md).
