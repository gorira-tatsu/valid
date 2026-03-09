# Quickstart

Use this guide if you want to try `valid` quickly, especially if you are not
planning to author Rust models on day one.

## 3-Minute Path

1. Install a prebuilt `valid` binary for your platform.
2. Create or enter an empty project directory.
3. Run:

```sh
valid init
valid init --check
cargo valid models
cargo valid inspect approval-model
cargo valid handoff approval-model
```

This gives you a working scaffold, confirms that the scaffold is healthy, shows
the starter model, and produces an implementation-facing handoff summary.

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
cargo valid graph approval-model --view=failure
```

## If You Only Want Review and Handoff

You can still get value from `valid` without becoming a Rust model author on
day one.

Start with:

- `valid init`
- `valid init --check`
- `cargo valid inspect approval-model`
- `cargo valid handoff approval-model`

That path lets you review a model, inspect its properties, and hand a concrete
brief to an implementation team or AI workflow.

## When to Read More

- If install fails or you want distribution details, read
  [Install Guide](./install.md).
- If you want to understand the scaffolded layout, read
  [Project Organization Guide](./project-organization.md).
- If you want to understand generated test specs and handoff summaries, read
  [Testgen and Handoff Guide](./testgen-and-handoff.md).
- If you want to author or review models with AI, read
  [AI Authoring Guide](./ai/authoring-guide.md).
