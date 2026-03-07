# PR-10 Solver Adapter Acceptance

関連ID:

- FR: `FR-023`, `FR-071`
- Epic: `I-1`〜`I-4`
- Specs: [RDD-0001-14](../08_specs/ai_solver_selfcheck_specs.md)
- 索引: [id_cross_reference.md](../09_reference/id_cross_reference.md)

## 1. 目的

- backend adapter を追加しても `EvidenceTrace` を共通化できるようにする。

## 2. 受け入れ条件

1. `SolverAdapter` trait が固定される。
2. capability matrix を返せる。
3. assignment -> trace 変換が動く。
4. normalized result が共通 schema に従う。

## 3. DoD

- mock backend で normalize の受け入れテストが通る。
