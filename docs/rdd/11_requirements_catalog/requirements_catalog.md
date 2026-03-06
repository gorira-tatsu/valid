# Requirements Catalog

- ドキュメントID: `RDD-0001-15`
- 目的: `REQ-*` を一次ソースとして定義し、`Property`, `FR`, `Epic`, `PR` へ追跡可能にする。
- 参照:
  - [id_cross_reference.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/id_cross_reference.md)
  - [functional_requirements.md](/Users/tatsuhiko/code/valid/docs/rdd/02_requirements/functional_requirements.md)
  - [business_logic_and_data_model.md](/Users/tatsuhiko/code/valid/docs/rdd/04_domain/business_logic_and_data_model.md)

## 1. 要求記述ルール

- `REQ-*` は人間レビュー対象の要求である。
- 各要求は少なくとも1つの `Property` と1つ以上の `FR-*` に対応づく。
- `REQ-*` の意味変更は禁止し、必要なら新IDを発行する。

## 2. 要求一覧

### REQ-001 モデルは有限状態として記述できること

- 説明: 状態、初期条件、遷移、性質を有限型前提で定義できること。
- 対応FR: `FR-001`〜`FR-005`
- 対応Epic: `A-1`〜`A-5`
- 対応Spec: [RDD-0001-10](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/mvp_frontend_and_kernel_specs.md)
- 対応PR: `PR-01`

### REQ-002 検証実行は PASS / FAIL / UNKNOWN / ERROR を明確に区別すること

- 説明: 成立、不成立、未確定、内部失敗を混同しない。
- 対応FR: `FR-020`〜`FR-024`, `FR-072`
- 対応Epic: `C-1`〜`C-7`, `H-2`
- 対応Spec: [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md), [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md)
- 対応PR: `PR-03`, `PR-09`

### REQ-003 FAIL は必ず replay 可能な証拠を持つこと

- 説明: 失敗判定は trace と replay で裏付けられること。
- 対応FR: `FR-021`, `FR-031`, `FR-032`
- 対応Epic: `B-5`, `D-1`〜`D-3`
- 対応Spec: [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md)
- 対応PR: `PR-02`, `PR-04`

### REQ-004 反例は回帰テストへ変換できること

- 説明: 一度見つかった失敗を CI 上で固定化できること。
- 対応FR: `FR-040`, `FR-041`, `FR-043`
- 対応Epic: `E-1`, `E-2`, `E-4`
- 対応Spec: [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md)
- 対応PR: `PR-06`

### REQ-005 モデル実行とテスト実行の coverage を測れること

- 説明: 遷移、ガード、状態深さの観測を品質指標として扱えること。
- 対応FR: `FR-042`, `FR-050`〜`FR-053`
- 対応Epic: `G-1`〜`G-5`
- 対応Spec: [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md)
- 対応PR: `PR-08`

### REQ-006 Rust 実装境界の契約変更を検知できること

- 説明: trait/API 破壊を lock と hash で明示的に管理できること。
- 対応FR: `FR-060`〜`FR-063`
- 対応Epic: `F-1`〜`F-4`
- 対応Spec: [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md)
- 対応PR: `PR-07`

### REQ-007 AI が inspect / check / explain / minimize / testgen を機械可読に扱えること

- 説明: 人間向けテキストではなく安定 JSON 契約で操作できること。
- 対応FR: `FR-070`〜`FR-073`
- 対応Epic: `H-1`〜`H-5`
- 対応Spec: [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md)
- 対応PR: `PR-09`

### REQ-008 backend を追加しても上位契約が壊れないこと

- 説明: explicit/BMC/将来backend で共通 trace schema を維持できること。
- 対応FR: `FR-023`, `FR-071`
- 対応Epic: `I-1`〜`I-4`
- 対応Spec: [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md)
- 対応PR: `PR-10`

### REQ-009 kernel の重要性質を自己検証できること

- 説明: 式評価、guard、遷移適用、replay などの核を selfcheck で監査できること。
- 対応FR: `FR-011`, `FR-073`
- 対応Epic: `J-1`〜`J-3`
- 対応Spec: [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md)
- 対応PR: `PR-11`

### REQ-010 文書、schema、artifact の真実源が一貫していること

- 説明: 生成物の手修正を防ぎ、source/lock/schema の整合性を維持する。
- 対応FR: `FR-032`, `FR-062`, `FR-063`
- 対応Epic: `D-2`, `F-4`
- 対応Spec: [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md), [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md)
- 対応PR: `PR-04`, `PR-07`

## 3. 完了条件

- `REQ-*` から `FR-*` へ辿れる。
- `REQ-*` から `Spec` と `PR` へ辿れる。
- 新しい要求追加時にこのカタログを更新する運用が決まっている。
