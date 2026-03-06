# PR-02 Kernel Acceptance

関連ID:

- FR: `FR-010`〜`FR-014`
- Epic: `B-1`〜`B-3`
- Specs: [RDD-0001-10](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/mvp_frontend_and_kernel_specs.md)
- 索引: [id_cross_reference.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/id_cross_reference.md)

## 1. 範囲

- `src/kernel/eval.rs`
- `src/kernel/guard.rs`
- `src/kernel/transition.rs`
- `src/kernel/replay.rs`

## 2. 目的

- typed IR を純粋関数で評価できるようにする。

## 3. 受け入れ条件

1. bool/int/enum の基本式を評価できる。
2. guard の true/false を判定できる。
3. declared updates のみで次状態を構築できる。
4. simultaneous update を守る。
5. trace replay で terminal state を再現できる。

## 4. 代表テスト

- `x + 1 <= 7`
- `!locked`
- `x = x + 1` と `locked = true`
- replay 2 step

## 5. エラー

- `INVALID_TRANSITION_UPDATE`
- `TRACE_REPLAY_ERROR`

## 6. DoD

- eval/guard/transition/replay が unit test で通る。
- replay は `EvidenceTrace` schema と整合する。
