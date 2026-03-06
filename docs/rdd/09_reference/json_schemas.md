# JSON Schemas Index

本ファイルは schema の一覧と責務だけを持つ。完全なサンプルは各 specs 章を参照し、機械可読な schema 本体は `schemas/` を参照する。

## 1. RunResult

- ID: `schema.run_result`
- 定義箇所: `08_specs/explicit_engine_and_evidence_specs.md`
- 本体: [schemas/run_result.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/run_result.schema.json)
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
- 本体: [schemas/evidence_trace.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/evidence_trace.schema.json)
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
- 本体: [schemas/test_vector.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/test_vector.schema.json)

## 3.1 DiagnosticBundle

- ID: `schema.diagnostic_bundle`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`, `05_interfaces/interfaces_cli_api_ci.md`
- 本体: [schemas/diagnostic_bundle.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/diagnostic_bundle.schema.json)

## 4. ContractSnapshot

- ID: `schema.contract_snapshot`
- 定義箇所: `08_specs/testgen_contract_coverage_specs.md`
- 本体: [schemas/contract_snapshot.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/contract_snapshot.schema.json)

## 5. ContractLock

- ID: `schema.contract_lock`
- 定義箇所: `08_specs/testgen_contract_coverage_specs.md`
- 本体: [schemas/contract_lock.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/contract_lock.schema.json)

## 6. ContractDrift

- ID: `schema.contract_drift`
- 定義箇所: `08_specs/testgen_contract_coverage_specs.md`
- 本体: [schemas/contract_drift.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/contract_drift.schema.json)

## 7. CoverageReport

- ID: `schema.coverage_report`
- 定義箇所: `08_specs/testgen_contract_coverage_specs.md`
- 本体: [schemas/coverage_report.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/coverage_report.schema.json)

## 8. AI Inspect Request/Response

- ID: `schema.ai.inspect`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`
- 本体: [schemas/ai_inspect_request.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/ai_inspect_request.schema.json), [schemas/ai_inspect_response.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/ai_inspect_response.schema.json)

## 9. AI Check Request/Response

- ID: `schema.ai.check`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`
- 本体: [schemas/ai_check_request.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/ai_check_request.schema.json), [schemas/ai_check_response.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/ai_check_response.schema.json)

## 10. AI Explain Request/Response

- ID: `schema.ai.explain`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`
- 本体: [schemas/ai_explain_request.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/ai_explain_request.schema.json), [schemas/ai_explain_response.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/ai_explain_response.schema.json)

## 11. AI Minimize Request/Response

- ID: `schema.ai.minimize`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`
- 本体: [schemas/ai_minimize_request.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/ai_minimize_request.schema.json), [schemas/ai_minimize_response.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/ai_minimize_response.schema.json)

## 12. AI Testgen Request/Response

- ID: `schema.ai.testgen`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`
- 本体: [schemas/ai_testgen_request.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/ai_testgen_request.schema.json), [schemas/ai_testgen_response.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/ai_testgen_response.schema.json)

## 13. CapabilityMatrix

- ID: `schema.capability_matrix`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`
- 本体: [schemas/capability_matrix.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/capability_matrix.schema.json)

## 14. SelfcheckReport

- ID: `schema.selfcheck_report`
- 定義箇所: `08_specs/ai_solver_selfcheck_specs.md`
- 本体: [schemas/selfcheck_report.schema.json](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/schemas/selfcheck_report.schema.json)

## 15. バージョニング規約

- `schema_version` は `major.minor.patch`
- major 変更: 互換破壊
- minor 変更: 後方互換のあるフィールド追加
- patch 変更: typo や説明修正のみ
