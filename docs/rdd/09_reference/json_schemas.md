# JSON Schemas Index

本ファイルは schema の一覧と責務だけを持つ。完全なサンプルは各 specs 章を参照する。

## 1. RunResult

- ID: `schema.run_result`
- 定義箇所: `08_specs/explicit_engine_and_evidence_specs.md`
- 用途: `check` の top-level 結果

主要フィールド:

- `schema_version`
- `run_id`
- `backend`
- `status`
- `property_results`
- `stats`
- `diagnostics`
- `artifacts`

## 2. EvidenceTrace

- ID: `schema.evidence_trace`
- 定義箇所: `08_specs/explicit_engine_and_evidence_specs.md`
- 用途: replay 可能な証拠

主要フィールド:

- `evidence_id`
- `property_id`
- `kind`
- `trace_hash`
- `terminal_reason`
- `steps`

## 3. TestVector

- ID: `schema.test_vector`
- 定義箇所: `08_specs/testgen_contract_coverage_specs.md`

## 4. ContractSnapshot

- ID: `schema.contract_snapshot`
- 定義箇所: `08_specs/testgen_contract_coverage_specs.md`

## 5. ContractLock

- ID: `schema.contract_lock`
- 定義箇所: `08_specs/testgen_contract_coverage_specs.md`

## 6. ContractDrift

- ID: `schema.contract_drift`
- 定義箇所: `08_specs/testgen_contract_coverage_specs.md`

## 7. CoverageReport

- ID: `schema.coverage_report`
- 定義箇所: `08_specs/testgen_contract_coverage_specs.md`

## 8. AI Inspect Request/Response

- ID: `schema.ai.inspect`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`

## 9. AI Check Request/Response

- ID: `schema.ai.check`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`

## 10. AI Explain Request/Response

- ID: `schema.ai.explain`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`

## 11. AI Minimize Request/Response

- ID: `schema.ai.minimize`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`

## 12. AI Testgen Request/Response

- ID: `schema.ai.testgen`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`

## 13. CapabilityMatrix

- ID: `schema.capability_matrix`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`

## 14. SelfcheckReport

- ID: `schema.selfcheck_report`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`

## 15. バージョニング規約

- `schema_version` は `major.minor.patch`
- major 変更: 互換破壊
- minor 変更: 後方互換のあるフィールド追加
- patch 変更: typo や説明修正のみ
