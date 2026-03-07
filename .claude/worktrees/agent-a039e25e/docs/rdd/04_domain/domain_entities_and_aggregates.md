# Domain Entities and Aggregates

参照元:

- [business_logic_and_data_model.md](business_logic_and_data_model.md)
- [../09_reference/id_cross_reference.md](../09_reference/id_cross_reference.md)

関連ID:

- `FR-001`〜`FR-005`
- `FR-020`〜`FR-024`
- `FR-040`〜`FR-043`
- `FR-050`〜`FR-053`
- `FR-060`〜`FR-063`
- `A-4`, `C-1`〜`C-7`, `D-1`〜`D-3`, `E-1`〜`E-5`, `F-1`〜`F-4`, `G-1`〜`G-5`

このファイルで読むもの:

- ModelDefinition
- RequirementMap
- PropertyDefinition
- VerificationRun
- PropertyResult
- EvidenceTrace
- TestVector
- CoverageReport
- ContractSnapshot
- 値オブジェクト
- Aggregate 不変条件

主な参照箇所:

- エンティティ設計
- 集約設計
- 値オブジェクト
- 不変条件の例

## エンティティ一覧

### Requirement

- 役割: 人間がレビューする要求の最上流単位。
- 識別子: `REQ-*`
- 主な属性:
  - `id`
  - `title`
  - `statement`
  - `status`
  - `mapped_property_ids`
- 不変条件:
  - 廃止済みでない `Requirement` は少なくとも1つの `PropertyDefinition` に対応づけられる。
  - `Requirement` の意味変更は禁止し、意味が変わる場合は新IDを発行する。

### ModelDefinition

- 役割: モデル定義の集約ルート。
- 主な属性:
  - `model_id`
  - `state_schema`
  - `action_definitions`
  - `property_definitions`
  - `metadata`
- 不変条件:
  - 状態変数名は一意。
  - action 名は一意。
  - property ID は一意。
  - 参照される state variable はすべて解決可能。

### VerificationRun

- 役割: 1回の検証実行を表す集約ルート。
- 主な属性:
  - `run_id`
  - `model_id`
  - `plan`
  - `status`
  - `property_results`
  - `artifacts`
- 不変条件:
  - `FAIL` のとき `EvidenceTrace` が必須。
  - `UNKNOWN` のとき `unknown_reason_code` が必須。
  - `PASS` のとき失敗証拠を持たない。

### EvidenceTrace

- 役割: replay 可能な証拠。
- 主な属性:
  - `trace_id`
  - `run_id`
  - `steps`
  - `property_id`
  - `result_kind`
- 不変条件:
  - `steps[0]` は init state。
  - 各 step は predecessor で連結される。
  - `FAIL` 証拠は最終 step で対象 property を破る。

### TestVector

- 役割: テスト生成の中間成果物。
- 主な属性:
  - `vector_id`
  - `source_trace_id`
  - `strategy`
  - `actions`
  - `oracle`
  - `seed`
- 不変条件:
  - `counterexample` 戦略なら `source_trace_id` 必須。
  - `oracle` は replay 可能であること。

## 値オブジェクト

- `ModelId`, `RequirementId`, `PropertyId`, `RunId`, `TraceId`, `VectorId`
- `SchemaVersion`
- `UnknownReasonCode`
- `CoverageRatio`
- `ContractHash`

これらは文字列や数値の薄い wrapper ではなく、文脈を区別する型として扱う。実装側では [entity_type_schema_map.md](../09_reference/entity_type_schema_map.md) にある Rust 型へ対応づける。

## 集約境界

### ModelAggregate

- 含むもの: `ModelDefinition`, `RequirementMap`, `PropertyDefinition`
- 境界理由: モデル整合性はロード時にまとめて成立させる必要がある。
- ここで守る不変条件:
  - property は必ず model 内の action / state を参照する。
  - requirement map は model 外の property を参照しない。

### RunAggregate

- 含むもの: `VerificationRun`, `PropertyResult`
- 境界理由: 1回の実行に関する整合性と status 遷移をまとめて管理するため。
- ここで守る不変条件:
  - `running -> pass/fail/unknown/error` 以外の遷移は禁止。
  - artifact 参照は run 単位で閉じる。

### EvidenceAggregate

- 含むもの: `EvidenceTrace`, `TraceStep`
- 境界理由: trace replay と reporter が同一単位で扱うため。
- ここで守る不変条件:
  - step index は 0 始まり連番。
  - action 名、guard 結果、state diff は同一 trace 内で整合している。

## 実装判断

- DDD の意味で最も重要なのは `ModelAggregate` と `RunAggregate` を分けること。
- parser / typechecker は `ModelAggregate` を作る責務を持つ。
- explicit engine / solver adapter は `RunAggregate` を作る責務を持つ。
- reporter / testgen は `EvidenceAggregate` を読むが、直接書き換えない。
