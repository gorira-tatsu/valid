# Entity / Rust Type / JSON Schema Map

このファイルは、ドメインエンティティ、Rust実装型、JSON schema の対応を固定する。

| Domain Entity | Rust Type | JSON Schema | 主定義 |
|---|---|---|---|
| ModelDefinition | `ModelIr` | n/a | [RDD-0001-10](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/mvp_frontend_and_kernel_specs.md) |
| PropertyDefinition | `PropertyIr` | embedded in `RunResult` | [RDD-0001-10](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/mvp_frontend_and_kernel_specs.md), [json_schemas.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/json_schemas.md) |
| VerificationRun | `RunPlan` + `ExplicitRunResult` | `schema.run_result` | [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md) |
| PropertyResult | `PropertyResult` | embedded in `schema.run_result` | [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md) |
| EvidenceTrace | `EvidenceTrace` | `schema.evidence_trace` | [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md) |
| TraceStep | `TraceStep` | embedded in `schema.evidence_trace` | [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md) |
| TestVector | `TestVector` | `schema.test_vector` | [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md) |
| ContractSnapshot | `ContractSnapshot` | `schema.contract_snapshot` | [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md) |
| ContractLock | `ContractLockFile` | `schema.contract_lock` | [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md) |
| ContractDrift | `ContractDriftReport` | `schema.contract_drift` | [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md) |
| CoverageReport | `CoverageReport` | `schema.coverage_report` | [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md) |
| CapabilityMatrix | `CapabilityMatrix` | `schema.capability_matrix` | [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md) |
| SelfcheckReport | `SelfcheckReport` | `schema.selfcheck_report` | [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md) |

## 規約

- Domain 名と Rust 型名は 1 対 1 を基本とする。
- JSON schema は外部契約として別名を持ってよいが、対応表を必須とする。
- 1つの Domain Entity が複数 schema に分かれる場合は embedded と明記する。
