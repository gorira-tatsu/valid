# Old Document Migration Audit

目的:

- `old/` に退避した統合版にしか残っていない規範情報がないことを確認する。

## 1. 監査結果

- `FR-*` 系: 現行 `02_requirements` と `08_specs` へ再配置済み
- `NFR-*` 系: 現行 `02_requirements/non_functional_requirements.md` へ再配置済み
- `Phase / KPI / DoD` 系: 現行 `07_planning` と `10_delivery` へ再配置済み
- `Entity / schema / contract / coverage` 系: 現行 `04_domain`, `08_specs`, `09_reference` へ再配置済み

## 2. 運用

- 新しい規範情報は `old/` に追加しない。
- 旧版を参照して現行へ取り込んだ場合は、この監査ファイルを更新する。
