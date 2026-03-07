# Domain Business Rules

参照元:

- [business_logic_and_data_model.md](business_logic_and_data_model.md)
- [../09_reference/id_cross_reference.md](../09_reference/id_cross_reference.md)

関連ID:

- `BR-001`〜`BR-008`
- `FR-004`, `FR-021`, `FR-024`, `FR-040`, `FR-060`〜`FR-063`
- `D-1`, `E-1`, `F-1`〜`F-4`

このファイルで読むもの:

- Requirement, Model, Property, Run, Evidence, Vector, Contract, Coverage の定義
- BR-001 〜 BR-008
- Contextごとの業務ルール

主な参照箇所:

- 中核概念
- ドメインルール
- Contextごとの責務

## 中核ルール

### BR-001 Requirement は Property に写像される

- `REQ-*` はレビュー単位であり、検証単位ではない。
- 検証は `PropertyDefinition` を通じて実行される。
- そのため、各 `REQ-*` は最低1つの property に写像される必要がある。

### BR-002 FAIL には replay 可能な証拠が必要

- `FAIL` は説明文だけでは成立しない。
- `EvidenceTrace` を replay し、対象 property が確かに破れることを確認できなければならない。

### BR-003 UNKNOWN は失敗ではない

- 上限制約到達、未対応 property、backend capability 不足は `UNKNOWN` として扱う。
- `ERROR` は内部失敗、入出力破損、schema 不整合に限定する。

### BR-004 生成物は一次ソースではない

- `trace.json`, `coverage.json`, `contract.lock`, Mermaid, generated Rust tests はすべて派生成果物である。
- これらはレビュー対象になり得るが、手修正は禁止する。

### BR-005 Contract 変更は明示的に承認される

- contract hash の変化は drift として報告する。
- lock 更新なしの contract 変更は CI 失敗とする。

### BR-006 Coverage は品質指標であり、真偽判定ではない

- coverage の高さは correctness を証明しない。
- ただし regression blind spot を減らす主要手段として扱う。

### BR-007 AI API は説明可能性を失ってはならない

- inspect/check/explain/minimize/testgen はすべて schema 固定 JSON を返す。
- LLM 向け convenience text は補助であり、真の契約は JSON にある。

### BR-008 Selfcheck は kernel 信頼境界を縮めるためにある

- selfcheck は engine 全体を証明するものではない。
- 目的は kernel の重要関数が壊れていないことを CI で継続確認すること。

## Context ごとの適用

### Modeling Context

- `BR-001`, `BR-004` を主に適用する。
- Requirement と Property の対応がここで閉じる。

### Verification Context

- `BR-002`, `BR-003` を主に適用する。
- 判定種別と trace 整合性はここで確定する。

### Integration Context

- `BR-005`, `BR-007` を主に適用する。
- CLI, AI API, generated tests, contract lock の整合を守る。

### Reliability Context

- `BR-006`, `BR-008` を主に適用する。
- selfcheck, coverage gate, artifact retention を扱う。
