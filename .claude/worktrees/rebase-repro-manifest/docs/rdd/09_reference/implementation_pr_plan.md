# Implementation PR Plan

関連文書:

- 詳細仕様: [../08_specs/README.md](../08_specs/README.md)
- リポジトリ構造: [repository_structure.md](repository_structure.md)
- 受け入れ基準: [../10_delivery/README.md](../10_delivery/README.md)

## 1. MVP / Phase 区分

### MVP

- frontend: parse / resolve / typecheck / IR
- kernel: expr / guard / transition / replay
- explicit engine: bfs / dfs / visited / predecessor / limits
- evidence: trace JSON / text summary

### Phase 2

- testgen
- contract lock / drift
- coverage report / gate
- AI API
- solver adapter

### Phase 3

- selfcheck
- BMC backend
- include mode

## 2. PR 分割案

### PR-01 Frontend Skeleton

範囲:

- `frontend/`
- `ir/`

DoD:

- simple model parse
- resolve/typecheck
- `ModelIr` 生成

### PR-02 Kernel Core

範囲:

- `kernel/eval.rs`
- `kernel/guard.rs`
- `kernel/transition.rs`
- `kernel/replay.rs`

DoD:

- unit test で基本式評価
- guard true/false
- simultaneous update
- trace replay

### PR-03 Explicit Engine

範囲:

- `engine/bfs.rs`
- `engine/dfs.rs`
- `engine/visited.rs`
- `engine/predecessor.rs`
- `engine/limits.rs`

DoD:

- PASS/FAIL/UNKNOWN
- shortest counterexample for BFS
- limit handling

### PR-04 Evidence / Reporter

範囲:

- `evidence/trace.rs`
- `evidence/reporter_json.rs`
- `evidence/reporter_text.rs`

DoD:

- `EvidenceTrace` JSON 出力
- text summary golden test

### PR-05 CLI Integration

範囲:

- `bin/valid.rs`
- `api/check.rs`

DoD:

- `valid check <spec> --json`
- artifact emission

### PR-06 Testgen MVP

DoD:

- counterexample -> vector
- vector -> rust test

### PR-07 Contract / Drift

DoD:

- snapshot hash
- lock compare
- drift JSON

### PR-08 Coverage

DoD:

- transition/guard coverage
- gate

### PR-09 AI API

DoD:

- inspect/check/explain/minimize/testgen JSON

### PR-10 Solver Adapter

DoD:

- trait
- explicit adapter
- normalization contract

### PR-11 Selfcheck

DoD:

- kernel suite
- separate CI job

## 3. 原則

- PR は機能横断より責務単位で切る。
- 1 PR で 1つの公開契約を固定する。
- schema を含む PR は必ず JSON golden を持つ。
- trace や lock を壊す変更は migration note 必須。

## 4. 受け入れ仕様

- [../10_delivery/pr_01_frontend_acceptance.md](../10_delivery/pr_01_frontend_acceptance.md)
- [../10_delivery/pr_02_kernel_acceptance.md](../10_delivery/pr_02_kernel_acceptance.md)
- [../10_delivery/pr_03_explicit_engine_acceptance.md](../10_delivery/pr_03_explicit_engine_acceptance.md)
- [../10_delivery/pr_04_evidence_reporter_acceptance.md](../10_delivery/pr_04_evidence_reporter_acceptance.md)
- [../10_delivery/pr_05_cli_integration_acceptance.md](../10_delivery/pr_05_cli_integration_acceptance.md)
- [../10_delivery/pr_06_testgen_acceptance.md](../10_delivery/pr_06_testgen_acceptance.md)
- [../10_delivery/pr_07_contract_drift_acceptance.md](../10_delivery/pr_07_contract_drift_acceptance.md)
- [../10_delivery/pr_08_coverage_acceptance.md](../10_delivery/pr_08_coverage_acceptance.md)
- [../10_delivery/pr_09_ai_api_acceptance.md](../10_delivery/pr_09_ai_api_acceptance.md)
- [../10_delivery/pr_10_solver_adapter_acceptance.md](../10_delivery/pr_10_solver_adapter_acceptance.md)
- [../10_delivery/pr_11_selfcheck_acceptance.md](../10_delivery/pr_11_selfcheck_acceptance.md)
