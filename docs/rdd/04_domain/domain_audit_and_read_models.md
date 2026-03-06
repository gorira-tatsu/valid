# Domain Audit and Read Models

参照元:

- [business_logic_and_data_model.md](/Users/tatsuhiko/code/valid/docs/rdd/04_domain/business_logic_and_data_model.md)
- [../09_reference/id_cross_reference.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/id_cross_reference.md)

関連ID:

- `FR-031`, `FR-032`, `FR-050`〜`FR-053`, `FR-063`, `FR-073`
- `D-3`, `G-1`〜`G-5`, `H-3`
- `KPI-01`〜`KPI-05`

このファイルで読むもの:

- 監査クエリ
- Audit Read Model
- Review Read Model
- AI Read Model
- 品質負債のモデル化

主な参照箇所:

- 読みモデル
- 品質負債のモデル化
- 典型クエリ
- 監査クエリを前提にした設計

## Read Model 一覧

### AuditRunReadModel

- 目的: 特定 run の判定理由を監査する。
- 主な項目:
  - `run_id`
  - `model_id`
  - `status`
  - `property_id`
  - `artifact_paths`
  - `unknown_reason_code`

### ReviewQueueReadModel

- 目的: 人間レビューが必要な項目を抽出する。
- 主な項目:
  - `requirement_id`
  - `property_ids`
  - `last_reviewed_at`
  - `open_drift`
  - `open_unknown`

### AIAssistReadModel

- 目的: AI API が必要な情報だけを安定 JSON で返す。
- 主な項目:
  - `action_catalog`
  - `property_catalog`
  - `capability_matrix`
  - `recent_failures`

## 典型クエリ

- どの `REQ-*` がまだ property に落ちていないか。
- どの property が直近 30 日で最も多く `UNKNOWN` になったか。
- contract drift が open のままの trait は何か。
- どの action が coverage 下限を割っているか。
- selfcheck で最後に失敗した kernel 関数は何か。

## 品質負債の表現

品質負債は Issue tracker のみに置かず、read model にも出す。最低限、以下の派生指標を計算対象とする。

- `unknown_hotspots`
- `coverage_gaps`
- `drift_without_review`
- `stale_requirements`
- `schema_version_fragmentation`

これにより、AI と人間が同じ欠陥一覧を参照できる。
