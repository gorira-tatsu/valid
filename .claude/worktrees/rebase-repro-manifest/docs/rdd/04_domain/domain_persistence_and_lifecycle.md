# Domain Persistence and Lifecycle

参照元:

- [business_logic_and_data_model.md](business_logic_and_data_model.md)
- [../09_reference/id_cross_reference.md](../09_reference/id_cross_reference.md)

関連ID:

- `FR-021`, `FR-032`, `FR-041`, `FR-053`, `FR-063`
- `D-1`, `D-2`, `E-1`, `G-4`
- `NFR-002`, `NFR-022`, `NFR-042`

このファイルで読むもの:

- 永続化戦略
- Repository
- 履歴モデル
- 保持期間
- 参照整合性
- 将来移行への備え

主な参照箇所:

- 永続化戦略
- リポジトリ設計
- 履歴モデル
- データ保持期間
- 将来移行への備え

## 永続化対象

MVP で永続化対象とするのは次に限定する。

- `EvidenceTrace`
- `ExplicitRunResult`
- `TestVector`
- `CoverageReport`
- `ContractSnapshot`
- `ContractLock`
- `ContractDriftReport`
- `SelfcheckReport`

`ModelIr` 自体は一次ソースから再構成可能なので、永続化の主目的は監査と差分比較である。

## Repository 方針

### ModelRepository

- 責務: モデル一次ソースから `ModelAggregate` を再構成する。
- 実装: filesystem reader。DB は不要。

### ArtifactRepository

- 責務: run 単位、trace 単位、vector 単位で artifact を保存する。
- 実装: ローカルファイルシステムを正とし、将来 object storage へ抽象化可能にする。

### ContractRepository

- 責務: contract snapshot と lock を保存、比較する。
- 実装: リポジトリ直下ファイルと CI artifact の併用。

## ライフサイクル

### VerificationRun

1. `planned`
2. `running`
3. `pass | fail | unknown | error`
4. `reported`

`reported` はドメイン上の status というより artifact 生成完了を示す派生状態として扱う。

### EvidenceTrace

1. `raw`
2. `normalized`
3. `minimized`
4. `replayed`

最初から minimized を唯一成果物にすると情報を失うため、raw と normalized は保持する。

## 保持期間

- `contract.lock`, `schemas`, `requirements catalog`: 常設管理
- `coverage report`: PR / release 単位で保持
- `EvidenceTrace`: fail と unknown は長期保持、pass は summary のみでよい
- `SelfcheckReport`: 主要ブランチで継続保持

## 移行原則

- schema 変更時は `schema_version` を更新する。
- backward compatible でない変更は old artifact を自動変換しない。
- migration は明示的な tool で行い、読み込み側で暗黙変換しない。
