# Glossary

- `ModelIr`: frontend を通過した後の統一モデル表現。
- `RunPlan`: 実行方針、limits、backend、artifact policy を持つ value object。
- `ExplicitRunResult`: explicit backend の正規化済み実行結果。
- `PropertyResult`: property 単位の結果。
- `EvidenceTrace`: 反例または witness を再生可能に表した証拠。
- `TraceStep`: state 遷移の1 step。
- `TestVector`: 回帰テストや witness テスト生成に使う中間形式。
- `ContractSnapshot`: trait/API 契約の正規化スナップショット。
- `CoverageReport`: transition/guard/state/depth の集計結果。
- `CapabilityMatrix`: backend が何を支援するかの対応表。
- `UnknownReasonCode`: `UNKNOWN` を返した理由。
- `ArtifactIndex`: run に紐づく artifact path の索引。
- `Selfcheck`: kernel や trace replay の性質を自分自身で検証する suite。
- `SSOT`: Single Source of Truth。本計画では spec source と contract metadata。
- `STO`: Source of Truth Operationally。運用上の真実源。JSON artifact や lock file の管理指針を含む。
