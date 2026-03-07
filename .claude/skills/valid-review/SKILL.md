---
name: valid-review
description: Review a valid model with inspect, lint/readiness, and coverage. Use when the user wants a capability summary, solver compatibility notes, and prioritized improvement advice.
argument-hint: "[model-or-path] [--backend=BACKEND]"
disable-model-invocation: true
---

Run a `valid` review workflow for `$ARGUMENTS`.

## Target Resolution

1. Prefer direct MCP tools when a `valid`, `pri`, or similarly named verification server exposes `inspect`, `lint` or `readiness`, `coverage`, `capabilities`, or `orchestrate`.
2. If no matching MCP tool is available, fall back to CLI.
3. Resolve the target the same way as `valid-check`:
   - `.valid` file or existing path -> file mode
   - non-file first argument -> Rust registry model name
   - no argument with `valid.toml` -> resolve from `cargo valid models --json` or `cargo run -q --bin cargo-valid -- valid models --json`, then ask only when still ambiguous

## CLI Fallback

- Rust/project mode:
  - Preferred if installed: `cargo valid`
  - Repo-local fallback: `cargo run -q --bin cargo-valid -- valid`
- `.valid` file mode:
  - Preferred if installed: `valid`
  - Repo-local fallback: `cargo run -q --bin valid --`
- Prefer `--json` for stable parsing.

## Workflow

1. Run `inspect` first and capture the capability matrix.
2. Run lint/readiness.
   - Project mode: use `readiness` if available; it maps to lint-style readiness findings.
   - File mode: use `lint`.
3. Run `coverage`.
4. If the model has multiple properties and the user wants a broader health check, optionally run `orchestrate` to gather aggregate coverage.
5. Summarize solver compatibility from the capability matrix:
   - Whether the model is explicit-ready
   - Whether it appears IR/solver ready
   - Whether coverage, explain, and testgen are trustworthy for this model
6. Rank findings by severity and point at the smallest high-value fixes first.

## Required Summary

Always include:

- Resolved target and execution path
- Capability matrix summary
- Solver/backend compatibility notes
- Readiness or lint findings
- Coverage strengths and gaps
- Prioritized recommendations

Prefer concrete examples such as missing guard coverage, explicit-only limitations, or properties that should be split or renamed.
