# PR-07 Contract / Drift Acceptance

関連ID:

- FR: `FR-060`〜`FR-063`
- Epic: `F-1`〜`F-4`
- Specs: [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md)
- 索引: [id_cross_reference.md](../09_reference/id_cross_reference.md)

## 1. 目的

- contract snapshot, lock, drift, doc drift を CI 契約にする。

## 2. 受け入れ条件

1. contract hash が決定的に計算される。
2. lock mismatch で失敗する。
3. drift JSON を出力できる。
4. doc drift を検知できる。

## 3. DoD

- snapshot / lock / drift の JSON golden を持つ。
