# PR-06 Testgen Acceptance

関連ID:

- FR: `FR-040`〜`FR-043`
- Epic: `E-1`〜`E-5`
- Specs: [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md)
- 索引: [id_cross_reference.md](../09_reference/id_cross_reference.md)

## 1. 目的

- counterexample / witness を `TestVector` と Rust test に変換する。

## 2. 受け入れ条件

1. evidence から `TestVector` が作れる。
2. `tests/generated/*.rs` を出力できる。
3. minimization が目的関数を守る。

## 3. DoD

- vector schema と generated test の golden を持つ。
