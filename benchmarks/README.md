## Benchmark Baselines

This directory stores committed benchmark baselines for `cargo valid benchmark`.

- `baselines/`
  JSON summaries recorded with `--baseline=record`
- `../scripts/benchmark-suite.sh`
  shared benchmark suite runner for CI `compare` and local `record`
- These files are compared in CI from `.github/workflows/ci.yml`
- The comparison gates on status counts and explored state-space metrics, and
  only considers elapsed time regressions when the baseline is large enough to
  avoid millisecond-level noise
- The default regression threshold is 25%, overridable via
  `VALID_BENCHMARK_THRESHOLD_PERCENT`

Update the committed baselines intentionally when model semantics or expected
search complexity changes.

To refresh the full committed suite after an intentional model change:

```sh
./scripts/benchmark-suite.sh record
```

To run the same regression gate locally before pushing:

```sh
./scripts/benchmark-suite.sh compare
```
