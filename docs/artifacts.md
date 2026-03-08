# Artifact Index and Run History

`valid` now keeps two machine-readable summaries under the artifacts root:

- `artifacts/index.json`
  A flat index of emitted artifacts such as check results, evidence traces, generated test vectors, generated Rust replay tests, selfcheck reports, benchmark reports, and generated docs.
- `artifacts/run-history.json`
  A grouped view keyed by `run_id`, showing which artifact paths and artifact kinds belong to the same run.

## Why This Exists

These files make it easier to:

- list generated outputs without walking the filesystem ad hoc
- correlate traces, generated tests, and reports back to the same run identity
- automate retention and cleanup in CI
- hand review a run without guessing which files belong together

## Commands

- `valid artifacts --json`
- `cargo valid artifacts --json`

Both commands return the current artifact index and run history.

## Retention Guidance

- Keep `artifacts/index.json` and `artifacts/run-history.json` with the rest of the artifact tree if you want downstream automation to correlate runs.
- `valid clean artifacts` and `cargo valid clean artifacts` remove the artifact tree, including the index and run history files.
- `generated-tests/` can be treated as disposable generated output. The artifact index still records which generated test files belonged to which run before cleanup.

## Notes

- Generated docs use a synthetic `doc-...` run id because they are not tied to one verification execution.
- Benchmark reports use their report id as the run id for indexing.
