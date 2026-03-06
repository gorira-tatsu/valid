# Governance and Operations

- ドキュメントID: `RDD-0001-18`
- バージョン: `v0.1`
- 目的: `RDD-0001-01` で定義した上位原則を、運用、導入、変更管理、例外規定へ落とし込む。
- 関連ID:
  - `REQ-002`, `REQ-006`, `REQ-007`, `REQ-010`
  - `FR-060`〜`FR-063`, `FR-070`〜`FR-073`
  - `NFR-020`, `NFR-022`, `NFR-032`
  - 親文書: [overview_and_scope.md](/Users/tatsuhiko/code/valid/docs/rdd/01_overview/overview_and_scope.md)

## 1. 位置づけ

この文書は上位方針そのものではない。`RDD-0001-01` の拘束条件を、導入、承認、運用、例外処理、成熟度評価へ適用するための補助文書である。

## 2. レビューと承認

### 2.1 変更クラス

- Class 1: 文書変更のみ
- Class 2: モデル変更だが property 意味論は不変
- Class 3: property 変更
- Class 4: contract 変更
- Class 5: kernel / adapter / CI 境界変更

### 2.2 承認方針

- Class 1-2 は通常レビューでよい。
- Class 3 はドメイン責任者承認を要する。
- Class 4 は contract drift レビューを要する。
- Class 5 はコードレビューと設計レビューの両方を要する。

## 3. 運用上の役割

- Domain Lead: `REQ-*` と property 対応の責任を持つ。
- Verification Engineer: backend 選定、assurance level、evidence 品質の責任を持つ。
- Platform Engineer: CI, schema, lock, artifact 運用の責任を持つ。
- AI Ops Owner: AI guardrail と API stability の責任を持つ。

## 4. 導入手順

1. 状態機械性の高い対象領域を1つ選ぶ。
2. `REQ-*` を明文化する。
3. 最小モデルを registry へ登録する。
4. `check`, `contract`, `doc` を CI に入れる。
5. FAIL から回帰テスト化までを定着させる。

## 5. 撤退条件

- UNKNOWN 比率が高止まりし、対策の見込みがない。
- モデル作成コストが継続的に回収できない。
- solver 差し替えごとに replay 契約が壊れる。

## 6. 品質ゲート優先順位

1. contract 整合
2. FAIL の有無
3. assurance level の妥当性
4. UNKNOWN の扱い
5. coverage 閾値
6. doc / artifact 整合

## 7. 例外規定

- 緊急障害対応では coverage 閾値を一時的に緩和してよい。
- ただし理由、期限、復旧計画を必ず残す。
- UNKNOWN の PASS 扱いは例外でも許可しない。

## 8. 成熟度モデル

- Level 0: 手動検証
- Level 1: 自動 check
- Level 2: FAIL の回帰テスト化
- Level 3: coverage 駆動運用
- Level 4: contract / drift / selfcheck 運用
- Level 5: self-hosted verification workflow

## 9. 文書運用

- 文字数は KPI にしない。
- 文書品質は完全性、参照性、規範文の明確さで評価する。
- `完`, `済`, `了` のような終端文は使わない。
- MUST / SHOULD / MAY を必要箇所で明示する。

## 10. 参照先

- 親方針: [overview_and_scope.md](/Users/tatsuhiko/code/valid/docs/rdd/01_overview/overview_and_scope.md)
- KPI / ロードマップ: [kpi_roadmap_risks.md](/Users/tatsuhiko/code/valid/docs/rdd/07_planning/kpi_roadmap_risks.md)
- PR 計画: [implementation_pr_plan.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/implementation_pr_plan.md)
