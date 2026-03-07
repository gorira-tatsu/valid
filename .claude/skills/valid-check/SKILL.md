---
name: valid-check
description: Verify a valid model or the current project model. Use when the user wants PASS/FAIL/UNKNOWN, automatic explain on failure, and concrete repair suggestions.
argument-hint: "[model-or-path] [--property=ID] [--backend=BACKEND]"
disable-model-invocation: true
---

Run the `valid` verification workflow for `$ARGUMENTS`.

## Target Resolution

1. Prefer direct MCP tools when a `valid`, `pri`, or similarly named verification server is available and exposes operations such as `inspect`, `check`, `explain`, `testgen`, `coverage`, `contract`, or `capabilities`.
2. If no matching MCP tool is available, fall back to CLI.
3. Resolve the target in this order:
   - If the first argument ends with `.valid` or is an existing model file path, use file mode.
   - If the first argument is present and is not a file path, treat it as a Rust registry model name.
   - If no argument is present and `valid.toml` exists, run `cargo valid models --json` or the repo-local fallback `cargo run -q --bin cargo-valid -- valid models --json`. If exactly one model is available, use it. If multiple models exist, prefer the one that matches the current file or directory context; otherwise ask the user to choose.
   - If no model can be resolved, stop and ask for a model name or file path.

## CLI Fallback

- Rust/project mode:
  - Preferred if installed: `cargo valid`
  - Repo-local fallback: `cargo run -q --bin cargo-valid -- valid`
- `.valid` file mode:
  - Preferred if installed: `valid`
  - Repo-local fallback: `cargo run -q --bin valid --`
- Prefer `--json` in CLI mode so you can summarize stable structured output.
- Treat exit codes as `0=PASS`, `1=FAIL`, `2=UNKNOWN`, `3=ERROR`.

## Workflow

1. Collect structure and capability context first.
   - Project mode: `... inspect <model> --json`
   - File mode: `... inspect <file> --json`
2. Run verification with the same target and pass through user-supplied `--property`, `--backend`, `--solver-exec`, and `--solver-arg` options when present.
   - Project mode: `... verify <model> --json`
   - File mode: `... verify <file> --json`
3. If verification returns `FAIL`, immediately run `explain` with the same target and options.
4. If verification returns `UNKNOWN`, call that out explicitly and report what backend or solver limitation caused it.
5. If the failure looks mechanical and the user is asking for implementation help, edit the affected model or code, rerun `inspect` and `verify`, and summarize the before/after result.

## Required Summary

Always return a human-readable report with:

- Resolved target and whether MCP or CLI was used
- Final status and backend
- Property checked or the default property set used
- Capability highlights from `inspect`
- For `FAIL`: failing property, trace/counterexample shape, candidate causes, and repair hints
- Concrete next commands or code changes

Quote exact commands only when they help the user reproduce the workflow.
