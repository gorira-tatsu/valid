---
name: valid-testgen
description: Generate tests from a valid model and review their quality. Use when the user wants test vectors, generated test files, and coverage-gap analysis.
argument-hint: "[model-or-path] [--property=ID] [--strategy=STRATEGY]"
disable-model-invocation: true
---

Run the `valid` test-generation workflow for `$ARGUMENTS`.

## Target Resolution

1. Prefer direct MCP tools when a `valid`, `pri`, or similarly named verification server exposes `inspect`, `testgen`, `coverage`, or `check`.
2. If no matching MCP tool is available, fall back to CLI.
3. Resolve the target exactly as `valid-check` does:
   - `.valid` file or existing path -> file mode
   - non-file first argument -> Rust registry model name
   - no argument with `valid.toml` -> run `cargo valid models --json` or `cargo run -q --bin cargo-valid -- valid models --json`, then resolve the most obvious candidate, otherwise ask

## CLI Fallback

- Rust/project mode:
  - Preferred if installed: `cargo valid`
  - Repo-local fallback: `cargo run -q --bin cargo-valid -- valid`
- `.valid` file mode:
  - Preferred if installed: `valid`
  - Repo-local fallback: `cargo run -q --bin valid --`
- Prefer `--json` for `inspect`, `generate-tests` or `testgen`, `coverage`, and follow-up `verify`.

## Workflow

1. Run `inspect` first so you know the model shape, properties, and capabilities.
2. Generate tests.
   - Project mode: `... generate-tests <model> --json`
   - File mode: `... testgen <file> --json`
   - If the user supplies `--strategy` or `--property`, pass it through.
3. Open the generated test file or vector output and review what was created.
   - Project mode usually writes under `generated-tests/`.
   - File mode may return vectors without writing files; summarize them clearly.
4. Run `coverage` on the same target to identify missing transitions, guards, or path tags.
5. If coverage gaps are obvious, propose additional targeted tests. If the user asked for implementation, add or adjust tests and rerun verification.

## Required Summary

Always return:

- Resolved target and execution path (MCP or CLI)
- Generated files or returned vector count
- What behaviors are now covered
- Missing coverage or weak assertions
- Specific additional test cases worth adding

Prioritize trace-backed and witness-backed cases over vague synthetic suggestions.
