---
name: valid-contract
description: Manage valid contract snapshots and lock files. Use when the user wants contract lock/check workflows, drift explanations, or guidance on whether to refresh a lock file.
argument-hint: "[snapshot|lock|check|drift] [model-or-path] [lock-file]"
disable-model-invocation: true
---

Run the `valid` contract-management workflow for `$ARGUMENTS`.

## Action Resolution

1. If the first argument is `snapshot`, `lock`, `check`, or `drift`, use it as the contract action.
2. Otherwise:
   - If `valid.lock.json` already exists, default to `check`.
   - If no lock file exists, default to `lock`.
3. Use `drift` only for `.valid` file mode, because the standalone `valid` binary exposes `drift` rather than `check`.

## Target Resolution

1. Prefer direct MCP tools when a `valid`, `pri`, or similarly named verification server exposes contract snapshot, lock, or drift/check operations.
2. If no matching MCP tool is available, fall back to CLI.
3. Resolve the target like this:
   - `.valid` file or existing path -> file mode
   - non-file first argument after the action -> Rust registry model name
   - no target argument with `valid.toml` -> project mode against the configured registry

## CLI Fallback

- Rust/project mode:
  - Preferred if installed: `cargo valid`
  - Repo-local fallback: `cargo run -q --bin cargo-valid -- valid`
  - Use `contract snapshot`, `contract lock`, or `contract check`.
  - `contract` works even if some help text does not list it yet.
- `.valid` file mode:
  - Preferred if installed: `valid`
  - Repo-local fallback: `cargo run -q --bin valid --`
  - Use `contract snapshot`, `contract lock`, or `contract drift`.
- Prefer `--json` whenever the selected command supports it.

## Workflow

1. Run the requested contract action.
2. If the result reports drift or missing entries, explain:
   - What changed
   - Whether the change looks intentional or breaking
   - Whether the next step should be code review, lock refresh, or both
3. Do not overwrite the lock file silently when drift is detected unless the user explicitly asked to regenerate it.
4. If contract generation fails because the model is not on the supported path yet, surface that limitation clearly instead of masking it.

## Required Summary

Always return:

- Resolved action, target, and execution path
- Lock file used or written
- Drift status
- Human-readable explanation of changed fields or hashes
- Recommended follow-up action

If a lock file was updated, say so explicitly and include the file path.
