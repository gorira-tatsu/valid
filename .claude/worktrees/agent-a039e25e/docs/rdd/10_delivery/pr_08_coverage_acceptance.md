# PR-08 Coverage Acceptance

関連ID:

- FR: `FR-050`〜`FR-053`
- Epic: `G-1`〜`G-5`
- Specs: [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md)
- 索引: [id_cross_reference.md](../09_reference/id_cross_reference.md)

## 1. 目的

- transition / guard / state-depth coverage を集計し、gate 判定できるようにする。

## 2. 受け入れ条件

1. `CoverageReport` を JSON で出力できる。
2. transition coverage を計算できる。
3. guard coverage を計算できる。
4. state/depth summary を計算できる。
5. threshold policy で gate 判定できる。
6. report JSON に `gate` を埋め込める。

## 3. DoD

- report JSON golden と gate 判定テストを持つ。
