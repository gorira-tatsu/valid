# PR-03 Explicit Engine Acceptance

関連ID:

- FR: `FR-020`〜`FR-024`
- Epic: `C-1`〜`C-7`
- Specs: [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md)
- 索引: [id_cross_reference.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/id_cross_reference.md)

## 1. 範囲

- `src/engine/bfs.rs`
- `src/engine/dfs.rs`
- `src/engine/visited.rs`
- `src/engine/predecessor.rs`
- `src/engine/limits.rs`

## 2. 目的

- PASS/FAIL/UNKNOWN を返す explicit backend を成立させる。

## 3. 受け入れ条件

1. BFS で shortest counterexample を返す。
2. DFS で FAIL/PASS/UNKNOWN の結果カテゴリが一致する。
3. state/depth/time limit を処理できる。
4. predecessor から trace を復元できる。
5. `UNSAT_INIT` を返せる。
6. deadlock を検出できる。

## 4. テストケース

- unsat init
- shortest counterexample
- deadlock
- state limit reached
- depth limit reached
- time limit reached

## 5. DoD

- `check_explicit` 相当の内部関数が動く。
- stats が埋まる。
- UNKNOWN reason が固定値で返る。
