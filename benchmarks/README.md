## Benchmark Baselines

This directory stores committed benchmark baselines for `cargo valid benchmark`.

- `baselines/`
  JSON summaries recorded with `--baseline=record`
- These files are compared in CI with `--baseline=compare`
- The comparison gates on status counts and explored state-space metrics, and
  only considers elapsed time regressions when the baseline is large enough to
  avoid millisecond-level noise

Update the committed baselines intentionally when model semantics or expected
search complexity changes.
