# PR-11 Selfcheck Acceptance

関連ID:

- FR: `FR-011`, `FR-073`
- Epic: `J-1`〜`J-3`
- Specs: [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md)
- 索引: [id_cross_reference.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/id_cross_reference.md)

## 1. 目的

- kernel の重要性質を selfcheck suite として独立実行する。

## 2. 受け入れ条件

1. expr evaluation selfcheck がある。
2. guard evaluation selfcheck がある。
3. transition selfcheck がある。
4. trace replay selfcheck がある。
5. selfcheck CI job が通常 CI と分離される。

## 3. DoD

- `SelfcheckReport` JSON golden を持つ。
