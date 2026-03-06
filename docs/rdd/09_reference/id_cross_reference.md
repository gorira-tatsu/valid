# ID Cross Reference

このファイルは、章リンクではなく ID で辿るための索引である。要件、機能分解、詳細仕様、PR 受け入れ条件の対応をここで固定する。

## 1. RDD 文書ID

| ID | 文書 | パス |
|---|---|---|
| `RDD-0001-01` | Overview and Scope | [01_overview/overview_and_scope.md](/Users/tatsuhiko/code/valid/docs/rdd/01_overview/overview_and_scope.md) |
| `RDD-0001-02` | Functional Requirements | [02_requirements/functional_requirements.md](/Users/tatsuhiko/code/valid/docs/rdd/02_requirements/functional_requirements.md) |
| `RDD-0001-03` | Non-Functional Requirements | [02_requirements/non_functional_requirements.md](/Users/tatsuhiko/code/valid/docs/rdd/02_requirements/non_functional_requirements.md) |
| `RDD-0001-04` | Architecture | [03_architecture/architecture.md](/Users/tatsuhiko/code/valid/docs/rdd/03_architecture/architecture.md) |
| `RDD-0001-05` | Business Logic and Data Model | [04_domain/business_logic_and_data_model.md](/Users/tatsuhiko/code/valid/docs/rdd/04_domain/business_logic_and_data_model.md) |
| `RDD-0001-06` | Interfaces CLI API CI | [05_interfaces/interfaces_cli_api_ci.md](/Users/tatsuhiko/code/valid/docs/rdd/05_interfaces/interfaces_cli_api_ci.md) |
| `RDD-0001-07` | Research Review | [06_research/research_review.md](/Users/tatsuhiko/code/valid/docs/rdd/06_research/research_review.md) |
| `RDD-0001-08` | KPI Roadmap Risks | [07_planning/kpi_roadmap_risks.md](/Users/tatsuhiko/code/valid/docs/rdd/07_planning/kpi_roadmap_risks.md) |
| `RDD-0001-09` | Feature Breakdown | [07_planning/feature_breakdown.md](/Users/tatsuhiko/code/valid/docs/rdd/07_planning/feature_breakdown.md) |
| `RDD-0001-10` | MVP Frontend and Kernel Specs | [08_specs/mvp_frontend_and_kernel_specs.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/mvp_frontend_and_kernel_specs.md) |
| `RDD-0001-11` | Full Technology Usage Plan | [08_specs/full_technology_usage_plan.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/full_technology_usage_plan.md) |
| `RDD-0001-12` | Explicit Engine and Evidence Specs | [08_specs/explicit_engine_and_evidence_specs.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md) |
| `RDD-0001-13` | Testgen Contract Coverage Specs | [08_specs/testgen_contract_coverage_specs.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md) |
| `RDD-0001-14` | AI Solver Selfcheck Specs | [08_specs/ai_solver_selfcheck_specs.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md) |
| `RDD-0001-15` | Requirements Catalog | [11_requirements_catalog/requirements_catalog.md](/Users/tatsuhiko/code/valid/docs/rdd/11_requirements_catalog/requirements_catalog.md) |
| `RDD-0001-18` | Governance and Operations | [01_overview/governance_and_operations.md](/Users/tatsuhiko/code/valid/docs/rdd/01_overview/governance_and_operations.md) |

## 1.1 REQ -> FR / Specs / PR

| REQ ID | 概要 | FR | Specs | PR |
|---|---|---|---|---|
| `REQ-001` | 有限状態モデル記述 | `FR-001`〜`FR-005` | `RDD-0001-10` | `PR-01` |
| `REQ-002` | PASS/FAIL/UNKNOWN/ERROR分離 | `FR-020`〜`FR-024`, `FR-072` | `RDD-0001-12`, `RDD-0001-14` | `PR-03`, `PR-09` |
| `REQ-003` | FAIL証拠必須 | `FR-021`, `FR-031`, `FR-032` | `RDD-0001-12` | `PR-02`, `PR-04` |
| `REQ-004` | 反例の回帰テスト化 | `FR-040`, `FR-041`, `FR-043` | `RDD-0001-13` | `PR-06` |
| `REQ-005` | coverage計測 | `FR-042`, `FR-050`〜`FR-053` | `RDD-0001-13` | `PR-08` |
| `REQ-006` | contract drift検知 | `FR-060`〜`FR-063` | `RDD-0001-13` | `PR-07` |
| `REQ-007` | AI機械可読API | `FR-070`〜`FR-073` | `RDD-0001-14` | `PR-09` |
| `REQ-008` | backend追加時の共通trace維持 | `FR-023`, `FR-071` | `RDD-0001-14` | `PR-10` |
| `REQ-009` | selfcheck | `FR-011`, `FR-073` | `RDD-0001-14` | `PR-11` |
| `REQ-010` | source/schema/artifact整合 | `FR-032`, `FR-062`, `FR-063` | `RDD-0001-12`, `RDD-0001-13` | `PR-04`, `PR-07` |

## 2. FR -> Epic / Specs / PR

| FR ID | 概要 | Epic/機能ID | 詳細仕様 | PR |
|---|---|---|---|---|
| `FR-001`〜`FR-005` | モデル記述 | `A-1`〜`A-5` | [RDD-0001-10](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/mvp_frontend_and_kernel_specs.md) | [PR-01](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/pr_01_frontend_acceptance.md) |
| `FR-010`〜`FR-014` | Rust埋め込み | `A-*`, `B-*`, Phase 2 | [RDD-0001-10](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/mvp_frontend_and_kernel_specs.md), [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md) | `PR-01`, `PR-02`, `PR-09` |
| `FR-020`〜`FR-024` | explicit/BMC/UNKNOWN | `C-1`〜`C-7` | [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md) | [PR-03](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/pr_03_explicit_engine_acceptance.md) |
| `FR-030`〜`FR-032` | 可視化・説明・JSON | `D-1`〜`D-3` | [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md) | [PR-04](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/pr_04_evidence_reporter_acceptance.md) |
| `FR-040`〜`FR-043` | テスト生成・最小化 | `E-1`〜`E-5` | [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md) | `PR-06` |
| `FR-050`〜`FR-053` | coverage | `G-1`〜`G-5` | [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md) | `PR-08` |
| `FR-060`〜`FR-063` | contract/doc/artifact | `F-1`〜`F-4` | [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md) | `PR-07` |
| `FR-070`〜`FR-073` | AI API | `H-1`〜`H-5` | [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md) | `PR-09` |

## 3. NFR -> 対象仕様

| NFR ID | 概要 | 主対象 |
|---|---|---|
| `NFR-001`〜`NFR-003` | 誤PASS防止 / 証拠再生 / UNKNOWN分離 | [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md), [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md) |
| `NFR-010`〜`NFR-012` | 性能 / 計測 / coverage cost | [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md), [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md) |
| `NFR-020`〜`NFR-022` | CLI / 可搬性 / 再現性 | [RDD-0001-06](/Users/tatsuhiko/code/valid/docs/rdd/05_interfaces/interfaces_cli_api_ci.md), [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md) |
| `NFR-030`〜`NFR-032` | 入力安全性 / Mermaid / sandbox | [RDD-0001-06](/Users/tatsuhiko/code/valid/docs/rdd/05_interfaces/interfaces_cli_api_ci.md), [RDD-0001-11](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/full_technology_usage_plan.md) |
| `NFR-040`〜`NFR-042` | kernel最小化 / backend拡張 / schema互換 | [RDD-0001-10](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/mvp_frontend_and_kernel_specs.md), [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md) |

## 4. Epic / 機能ID -> 詳細仕様

| 機能ID | 詳細仕様 |
|---|---|
| `A-1`〜`A-4` | [RDD-0001-10](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/mvp_frontend_and_kernel_specs.md) |
| `B-1`〜`B-3` | [RDD-0001-10](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/mvp_frontend_and_kernel_specs.md) |
| `C-1`〜`C-7`, `D-1`〜`D-3` | [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md) |
| `E-1`〜`E-5`, `F-1`〜`F-4`, `G-1`〜`G-5` | [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md) |
| `H-1`〜`H-5`, `I-1`〜`I-4`, `J-1`〜`J-3` | [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md) |

## 5. PR -> 受け入れ条件

| PR ID | 受け入れ条件 |
|---|---|
| `PR-01` | [pr_01_frontend_acceptance.md](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/pr_01_frontend_acceptance.md) |
| `PR-02` | [pr_02_kernel_acceptance.md](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/pr_02_kernel_acceptance.md) |
| `PR-03` | [pr_03_explicit_engine_acceptance.md](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/pr_03_explicit_engine_acceptance.md) |
| `PR-04` | [pr_04_evidence_reporter_acceptance.md](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/pr_04_evidence_reporter_acceptance.md) |
| `PR-05` | [pr_05_cli_integration_acceptance.md](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/pr_05_cli_integration_acceptance.md) |
| `PR-06` | [pr_06_testgen_acceptance.md](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/pr_06_testgen_acceptance.md) |
| `PR-07` | [pr_07_contract_drift_acceptance.md](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/pr_07_contract_drift_acceptance.md) |
| `PR-08` | [pr_08_coverage_acceptance.md](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/pr_08_coverage_acceptance.md) |
| `PR-09` | [pr_09_ai_api_acceptance.md](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/pr_09_ai_api_acceptance.md) |
| `PR-10` | [pr_10_solver_adapter_acceptance.md](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/pr_10_solver_adapter_acceptance.md) |
| `PR-11` | [pr_11_selfcheck_acceptance.md](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/pr_11_selfcheck_acceptance.md) |

## 6. Phase -> 中心成果物

| Phase | 主対象 | 参照 |
|---|---|---|
| `Phase 0` | frontend / kernel 土台 | [RDD-0001-09](/Users/tatsuhiko/code/valid/docs/rdd/07_planning/feature_breakdown.md), [RDD-0001-08](/Users/tatsuhiko/code/valid/docs/rdd/07_planning/kpi_roadmap_risks.md) |
| `Phase 1` | explicit MVP | [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md) |
| `Phase 2` | Rust integration / contract / basic AI | [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md), [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md) |
| `Phase 3` | test automation | [RDD-0001-13](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md) |
| `Phase 4` | solver expansion | [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md) |
| `Phase 5` | concurrency / reduction | [RDD-0001-07](/Users/tatsuhiko/code/valid/docs/rdd/06_research/research_review.md), [RDD-0001-08](/Users/tatsuhiko/code/valid/docs/rdd/07_planning/kpi_roadmap_risks.md) |
| `Phase 6` | selfcheck / self-host step 1 | [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md) |

## 7. 運用ルール

- 新しい FR/NFR/Epic/PR を追加したら、この索引を更新する。
- 章本文には `関連ID` を書き、ここへの逆参照を置く。
- ID の意味変更は許容しない。意味変更は新ID発行で扱う。
