# Repository Structure

`src/` 前提での MVP 構成案をここで固定する。

## 1. MVP 構成

```text
src/
  lib.rs
  bin/
    valid.rs
  frontend/
    mod.rs
    parser.rs
    resolver.rs
    typecheck.rs
    ir_lowering.rs
  ir/
    mod.rs
    model.rs
    expr.rs
    action.rs
    property.rs
    value.rs
  kernel/
    mod.rs
    eval.rs
    guard.rs
    transition.rs
    replay.rs
  engine/
    mod.rs
    explicit.rs
    bfs.rs
    dfs.rs
    visited.rs
    predecessor.rs
    limits.rs
  evidence/
    mod.rs
    trace.rs
    reporter_json.rs
    reporter_text.rs
  testgen/
    mod.rs
    vector.rs
    render_rust.rs
    minimize.rs
  contract/
    mod.rs
    snapshot.rs
    lock.rs
    drift.rs
  coverage/
    mod.rs
    collect.rs
    report.rs
    gate.rs
  api/
    mod.rs
    inspect.rs
    check.rs
    explain.rs
    minimize.rs
    testgen.rs
  solver/
    mod.rs
    traits.rs
    explicit_adapter.rs
    normalize.rs
  selfcheck/
    mod.rs
    suite.rs
    runner.rs
    report.rs
  support/
    mod.rs
    ids.rs
    hashing.rs
    time.rs
    artifacts.rs
```

## 2. MVPで本当に作る型

- `StateValue`
- `Value`
- `ExprIr`
- `ActionIr`
- `PropertyIr`
- `ModelIr`
- `RunPlan`
- `TraceStep`
- `EvidenceTrace`
- `TestVector`
- `ContractSnapshot`
- `CoverageReport`

## 3. Phase 2 で追加

- `SolverRunPlan`
- `CapabilityMatrix`
- `SelfcheckReport`
- BMC adapter 固有型

## 4. 原則

- domain core は `ir/`, `kernel/`, `engine/`
- interface は `api/`, `evidence/`, `testgen/`
- infra は `solver/`, `support/artifacts.rs`
