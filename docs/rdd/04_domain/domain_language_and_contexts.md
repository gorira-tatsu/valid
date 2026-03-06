# Domain Language and Contexts

参照元:

- [business_logic_and_data_model.md](business_logic_and_data_model.md)
- [../09_reference/id_cross_reference.md](../09_reference/id_cross_reference.md)

関連ID:

- `FR-001`〜`FR-014`
- `FR-020`〜`FR-024`
- `FR-060`〜`FR-073`
- `A-*`, `C-*`, `D-*`, `F-*`, `H-*`

このファイルで読むもの:

- Ubiquitous Language
- Modeling / Verification / Evidence / Integration Context
- DDD観点での責務分離

主な参照箇所:

- Ubiquitous Language
- Bounded Contextごとの責務
- DDDから見たデータモデル判断

## Ubiquitous Language

- Requirement: 人間がレビューし、正しさの意図を表す要求。
- Property: engine が評価する検証対象。
- Model: 状態、遷移、property を持つ検証対象定義。
- Run: 1回の検証実行。
- Evidence: run の結果を再現可能にした証拠。
- Vector: evidence や coverage 目標から作るテスト入力列。
- Contract: Rust 実装境界の型と API 形状。
- Drift: contract hash と lock の不一致。
- Selfcheck: kernel の重要操作に対する継続検査。

この語彙は docs 全体で固定し、同義語の乱立を避ける。たとえば `counterexample` は Evidence Context では `failure trace` と言い換えず `EvidenceTrace` に統一する。

## Bounded Context

### Modeling Context

- 扱うもの: Requirement, ModelDefinition, PropertyDefinition, RequirementMap
- 入力: DSL, macro, markdown embedded spec
- 出力: 正規化済み `ModelIr`
- ここで決めること:
  - 有限性
  - 参照解決
  - property と requirement の対応

### Verification Context

- 扱うもの: RunPlan, VerificationRun, PropertyResult
- 入力: `ModelIr`, backend capability, run options
- 出力: PASS / FAIL / UNKNOWN / ERROR
- ここで決めること:
  - exploration strategy
  - limit behavior
  - UNKNOWN と ERROR の境界

### Evidence Context

- 扱うもの: EvidenceTrace, TraceStep, reporter output, minimized trace
- 入力: VerificationRun
- 出力: text, JSON, Mermaid, replay artifact
- ここで決めること:
  - trace canonical form
  - step diff format
  - replay 条件

### Integration Context

- 扱うもの: TestVector, CoverageReport, ContractSnapshot, ContractLock, AI API schema
- 入力: EvidenceTrace, contract source, generated artifacts
- 出力: generated tests, drift report, coverage report, AI responses
- ここで決めること:
  - schema versioning
  - artifact naming
  - CI gate

## Context 間の境界原則

- Modeling は backend を知らない。
- Verification は parser 実装詳細を知らない。
- Evidence は solver 実装詳細を知らない。
- Integration は kernel の内部アルゴリズムを知らない。

この分離により、DDD とクリーンアーキテクチャの両方で依存方向を内向きに保つ。
