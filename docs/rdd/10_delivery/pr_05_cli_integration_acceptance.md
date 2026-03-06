# PR-05 CLI Integration Acceptance

関連ID:

- FR: `FR-032`, `FR-063`, `FR-070`〜`FR-072`
- Epic: `H-2`
- Specs: [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md), [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md)
- 索引: [id_cross_reference.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/id_cross_reference.md)

## 1. 目的

- `valid check <spec> --json` と artifact emission を成立させる。

## 2. 受け入れ条件

1. CLI から `check` を起動できる。
2. `--json` で `schema.run_result` に従う。
3. 終了コードが `PASS/FAIL/UNKNOWN/ERROR` で安定する。
4. artifact path が naming rule に従う。

## 3. DoD

- CLI adapter が Use Case を呼ぶだけの薄い層になっている。
