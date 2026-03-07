# Explicit Engine and Evidence Specs

- ドキュメントID: `RDD-0001-12`
- バージョン: `v0.3`
- 目的: `C-1`〜`C-7` と `D-1`〜`D-3` の詳細仕様を、実装可能な粒度まで固定する。
- 依存章:
  - [mvp_frontend_and_kernel_specs.md](mvp_frontend_and_kernel_specs.md)
  - [../09_reference/json_schemas.md](../09_reference/json_schemas.md)
  - [../09_reference/error_codes.md](../09_reference/error_codes.md)
  - [../09_reference/artifact_naming.md](../09_reference/artifact_naming.md)
- 関連ID:
  - FR: `FR-020`〜`FR-032`
  - NFR: `NFR-001`〜`NFR-003`, `NFR-010`〜`NFR-012`
  - Epic: `C-1`〜`C-7`, `D-1`〜`D-3`
  - PR: `PR-03`, `PR-04`
  - 参照索引: [../09_reference/id_cross_reference.md](../09_reference/id_cross_reference.md)

## 1. 対象範囲

本章で扱う機能:

- `C-1` 初期状態列挙
- `C-2` BFS探索
- `C-3` DFS探索
- `C-4` 訪問済み状態管理
- `C-5` predecessor記録
- `C-6` 上限制御
- `C-7` 実行統計
- `D-1` Evidence生成
- `D-2` Trace JSON出力
- `D-3` テキスト要約

非対象:

- LTLライブネス
- symbolic solver内部アルゴリズム
- Mermaid詳細仕様
- Explain APIの高度な原因分析

## 2. 設計原則

- explicit engine は MVP の基準実装である。
- `FAIL` の場合は必ず replay 可能な `EvidenceTrace` を返す。
- `PASS` の場合も property ごとの判定理由と統計を返す。
- `UNKNOWN` は曖昧な成功扱いを禁止し、理由コードを必ず保持する。
- `ERROR` は `PASS/FAIL/UNKNOWN` と独立した envelope で返す。
- BFS の最短反例性を壊す変更は互換破壊とみなす。
- 本章における規範入力は `ModelIr` と `RunPlan` である。ただしプロジェクト全体の SSOT は上位章で定義されたモデル一次ソースに従う。

## 3. 用語

- `RunManifest`: 実行の識別子、ハッシュ、version、limits、seed を保持するメタデータ。
- `RunPlan`: 実行方針、探索境界、資源制限、property選択、artifact方針をまとめた不変値。
- `ExplicitRunResult`: explicit backend の正規化済み結果。
- `PropertyResult`: property 単位の判定結果。
- `EvidenceTrace`: 反例または witness を再生可能な形に正規化した証拠。
- `TraceStep`: 1回の action 適用を表す単位。
- `UnknownReasonCode`: `UNKNOWN` の理由コード。
- `CheckErrorEnvelope`: 検証行為が成立しなかった場合の安定外形。

## 4. check API契約

### 4.1 Rust API

```rust
pub fn check_explicit(
    model: &ModelIr,
    run_plan: &RunPlan,
) -> CheckOutcome;

pub enum CheckOutcome {
    Completed(ExplicitRunResult),
    Errored(CheckErrorEnvelope),
}
```

### 4.2 CLI対応

```text
valid check --crate <crate-path> --model <model-id> \
  --backend explicit \
  --strategy bfs \
  --property <property-id> \
  --max-depth <n> \
  --max-states <n> \
  --time-limit <duration> \
  --json
```

### 4.3 契約

- 入力 `ModelIr` は型検査済みでなければならない。
- `RunPlan.backend` は `explicit` でなければならない。
- MVP では `RunPlan.property_selection` は `ExactlyOne(PropertyId)` のみ許可する。
- 全 property 実行は orchestrator が複数 run に分解する。
- 探索途中の limit 到達は `Completed(ExplicitRunResult { status = Unknown, ... })` で返す。
- 契約外失敗は `Errored(CheckErrorEnvelope)` で返す。

## 5. RunPlan 型

### 5.1 Rust構造

```rust
pub struct RunPlan {
    pub manifest: RunManifest,
    pub backend: BackendKind,
    pub explicit_strategy: ExplicitStrategy,
    pub property_selection: PropertySelection,
    pub search_bounds: SearchBounds,
    pub resource_limits: ResourceLimits,
    pub artifact_policy: ArtifactPolicy,
    pub reporter_options: ReporterOptions,
}

pub struct RunManifest {
    pub request_id: RequestId,
    pub run_id: RunId,
    pub schema_version: SchemaVersion,
    pub source_hash: SourceHash,
    pub contract_hash: ContractHash,
    pub engine_version: String,
    pub backend_name: BackendKind,
    pub backend_version: String,
    pub seed: Option<u64>,
}

pub struct SearchBounds {
    pub max_depth: Option<u32>,
}

pub struct ResourceLimits {
    pub max_states: Option<u64>,
    pub time_limit_ms: Option<u64>,
    pub memory_limit_mb: Option<u64>,
}

pub enum PropertySelection {
    ExactlyOne(PropertyId),
}
```

### 5.2 意味論

- `request_id` は外部要求の相関 ID である。
- `run_id` は個別実行と artifact の識別子である。
- `search_bounds.max_depth` は意味上の探索境界であり、そこまで探索を完了すれば `PASS + bounded` を返しうる。
- `resource_limits` は運用上の停止条件であり、途中停止時は `UNKNOWN + incomplete` を返す。
- `ArtifactPolicy` は任意 artifact にのみ効く。`FAIL` 時の `check-result.json` と `EvidenceTrace` は常に必須である。
- artifact path の決定性は `RunPlan + ArtifactNamingPolicy` によって保証する。

## 6. Completed / Error 結果外形

### 6.1 Rust構造

```rust
pub struct ExplicitRunResult {
    pub manifest: RunManifest,
    pub status: RunStatus,
    pub assurance_level: AssuranceLevel,
    pub property_result: PropertyResult,
    pub stats: ExplicitStats,
    pub diagnostics: Vec<RunDiagnostic>,
    pub artifacts: ArtifactIndex,
}

pub enum RunStatus {
    Pass,
    Fail,
    Unknown,
}

pub struct PropertyResult {
    pub property_id: PropertyId,
    pub property_kind: PropertyKind,
    pub status: RunStatus,
    pub assurance_level: AssuranceLevel,
    pub reason_code: Option<String>,
    pub unknown_reason: Option<UnknownReasonCode>,
    pub terminal_state_id: Option<StateId>,
    pub evidence_id: Option<EvidenceId>,
    pub summary: String,
}

pub struct CheckErrorEnvelope {
    pub manifest: RunManifest,
    pub status: ErrorStatus,
    pub assurance_level: AssuranceLevel,
    pub diagnostics: Vec<RunDiagnostic>,
}
```

### 6.2 JSON schema 例

```json
{
  "kind": "completed",
  "manifest": {
    "request_id": "req-20260306-0001",
    "run_id": "run-20260306-0001",
    "schema_version": "1.0.0",
    "source_hash": "sha256:abc123",
    "contract_hash": "sha256:def456",
    "engine_version": "0.1.0",
    "backend_name": "explicit",
    "backend_version": "0.1.0",
    "seed": null
  },
  "status": "PASS",
  "assurance_level": "bounded",
  "property_result": {
    "property_id": "P_SAFE",
    "property_kind": "invariant",
    "status": "PASS",
    "assurance_level": "bounded",
    "reason_code": "BOUNDED_SPACE_EXHAUSTED",
    "unknown_reason": null,
    "terminal_state_id": null,
    "evidence_id": null,
    "summary": "no violating state found within the configured depth bound"
  },
  "stats": {
    "states_seen": 42,
    "states_enqueued": 42,
    "transitions_tried": 80,
    "transitions_taken": 41,
    "max_depth_seen": 10,
    "runtime_ms": 4,
    "limit_hit": null
  },
  "diagnostics": [],
  "artifacts": {
    "result_json": "artifacts/run-20260306-0001/check-result.json",
    "evidence": []
  }
}
```

### 6.3 エラー外形 例

```json
{
  "kind": "error",
  "manifest": {
    "request_id": "req-20260306-0001",
    "run_id": "run-20260306-0001",
    "schema_version": "1.0.0",
    "source_hash": "sha256:abc123",
    "contract_hash": "sha256:def456",
    "engine_version": "0.1.0",
    "backend_name": "explicit",
    "backend_version": "0.1.0",
    "seed": null
  },
  "status": "ERROR",
  "assurance_level": "incomplete",
  "diagnostics": [
    {
      "error_code": "ERROR_INVALID_INIT",
      "segment": "engine.search",
      "message": "init does not produce any well-typed initial state"
    }
  ]
}
```

## 7. EvidenceTrace / TraceStep schema

### 7.1 Rust構造

```rust
pub struct EvidenceTrace {
    pub schema_version: SchemaVersion,
    pub evidence_id: EvidenceId,
    pub run_id: RunId,
    pub property_id: PropertyId,
    pub assurance_level: AssuranceLevel,
    pub kind: EvidenceKind,
    pub trace_hash: TraceHash,
    pub terminal_reason: TerminalReason,
    pub initial_state_id: StateId,
    pub terminal_state_id: StateId,
    pub steps: Vec<TraceStep>,
}

pub struct TraceStep {
    pub index: u32,
    pub from_state_id: StateId,
    pub action_id: Option<ActionId>,
    pub action_label: Option<String>,
    pub to_state_id: StateId,
    pub depth: u32,
    pub state_before: StateValue,
    pub state_after: StateValue,
    pub diff: Vec<FieldDiff>,
}

pub struct FieldDiff {
    pub field_path: JsonPointer,
    pub before: Value,
    pub after: Value,
}
```

### 7.2 受け入れ条件

- `TraceStep.field_path` は JSON Pointer とする。例: `/x`, `/queue/0/state`
- 初期状態が即時に property failure なら `steps = []` を許容する。
- `trace_hash` は trace 内容のみから計算する。`property_id` や path は入力に含めない。
- `diff` は派生情報であり、`state_before` と `state_after` から再計算可能でなければならない。

## 8. Unknown / Error 理由コード

### 8.1 UnknownReasonCode

- `UNKNOWN_STATE_LIMIT_REACHED`
- `UNKNOWN_TIME_LIMIT_REACHED`
- `UNKNOWN_ENGINE_ABORTED`

### 8.2 ErrorReasonCode

- `ERROR_INVALID_INIT`
- `ERROR_PROPERTY_NOT_FOUND`
- `ERROR_UNSUPPORTED_PROPERTY_KIND`
- `ERROR_BACKEND_MISMATCH`
- `ERROR_PREDECESSOR_BROKEN`
- `ERROR_INTERNAL_INVARIANT_BROKEN`

### 8.3 方針

- `UNSAT_INIT` は `UNKNOWN` ではなく `ERROR_INVALID_INIT` として扱う。
- `UNKNOWN` は契約内停止、`ERROR` は契約外失敗である。

## 9. 初期状態列挙

### 9.1 責務

- `InitSpec` から探索開始状態集合を得る。
- 複数初期状態を安定順序で列挙する。
- 初期状態が1件も生成できない場合は `ERROR_INVALID_INIT` を返す。

### 9.2 擬似コード

```text
function enumerate_initial_states(model):
  candidates = solve_init_assignments(model.init, model.state_schema)
  if candidates.is_empty():
    raise Error(ERROR_INVALID_INIT)
  return sort_by_canonical_state(candidates)
```

## 10. BFS探索

### 10.1 責務

- 幅優先探索を行う。
- 最短反例トレースを保証する。
- bounded 探索と resource 制限を区別する。

### 10.2 擬似コード

```text
function run_bfs(model, initial_states, run_plan):
  queue = new FIFOQueue()
  visited = new VisitedSet()
  predecessor = new PredecessorMap()
  stats = empty_stats()
  start_time = now()
  bounded_frontier_cut = false

  for each init_state in initial_states:
    queue.push((init_state, 0))
    visited.insert(init_state)
    predecessor.mark_root(init_state)

  while queue is not empty:
    if resource_limits_hit(run_plan.resource_limits, stats, start_time):
      return completed_unknown(incomplete, matching_reason)

    (state, depth) = queue.pop()
    stats.states_seen += 1
    stats.max_depth_seen = max(stats.max_depth_seen, depth)

    property_outcome = evaluate_selected_property(model, state)
    if property_outcome is FAIL:
      return completed_fail(complete_if_unbounded_else_bounded)

    enabled = collect_enabled_actions(model, state)
    if enabled is empty and detect_deadlocks(run_plan):
      return completed_fail(complete_if_unbounded_else_bounded)

    if run_plan.search_bounds.max_depth exists and depth == max_depth:
      bounded_frontier_cut = bounded_frontier_cut or enabled.not_empty()
      continue

    for each action in enabled:
      next_state = apply_transition(action, state)
      stats.transitions_tried += 1
      stats.transitions_taken += 1
      if visited.insert_if_absent(next_state):
        predecessor.record(next_state, state, action.id, depth + 1)
        queue.push((next_state, depth + 1))
        stats.states_enqueued += 1

  if bounded_frontier_cut:
    return completed_pass(bounded)
  return completed_pass(complete)
```

### 10.3 BFS境界意味論

- `max_depth` は探索境界であり、そこまでの探索を完了したなら `PASS + bounded` を返す。
- 深さ境界を理由に途中で即 `UNKNOWN` を返してはならない。
- `time_limit` と `max_states` は resource 制限であり、途中停止時は `UNKNOWN + incomplete` を返す。

## 11. DFS探索

### 11.1 責務

- stack ベース探索を提供する。
- 最短反例は保証しない。
- bounded 探索と resource 制限を区別する。

### 11.2 擬似コード

```text
function run_dfs(model, initial_states, run_plan):
  stack = new Stack()
  visited = new VisitedSet()
  predecessor = new PredecessorMap()
  stats = empty_stats()
  start_time = now()
  bounded_frontier_cut = false

  for each init_state in reverse(initial_states):
    stack.push((init_state, 0))

  while stack is not empty:
    if resource_limits_hit(run_plan.resource_limits, stats, start_time):
      return completed_unknown(incomplete, matching_reason)

    (state, depth) = stack.pop()
    if not visited.insert_if_absent(state):
      continue

    property_outcome = evaluate_selected_property(model, state)
    if property_outcome is FAIL:
      return completed_fail(complete_if_unbounded_else_bounded)

    enabled = collect_enabled_actions(model, state)
    if enabled is empty and detect_deadlocks(run_plan):
      return completed_fail(complete_if_unbounded_else_bounded)

    if run_plan.search_bounds.max_depth exists and depth == max_depth:
      bounded_frontier_cut = bounded_frontier_cut or enabled.not_empty()
      continue

    for each action in reverse(enabled):
      next_state = apply_transition(action, state)
      predecessor.record_if_absent(next_state, state, action.id, depth + 1)
      stack.push((next_state, depth + 1))

  if bounded_frontier_cut:
    return completed_pass(bounded)
  return completed_pass(complete)
```

## 12. 訪問済み状態管理

- state は field 名の辞書順で canonicalize する。
- enum は宣言順の安定整数へ変換する。
- bounded int は数値のまま保持する。
- struct の field 順序差異は無視する。

## 13. predecessor 復元

```rust
pub struct PredecessorEntry {
    pub parent_state_id: Option<StateId>,
    pub via_action_id: Option<ActionId>,
    pub depth: u32,
}
```

- root には `parent_state_id = None`
- broken chain は `ERROR_PREDECESSOR_BROKEN`
- BFS では reconstructed trace が最短

## 14. limit到達時の振る舞い

### 14.1 優先順位

resource limit 同時到達時の優先順位:

1. time
2. state
3. memory

### 14.2 ポリシー

- search bound 到達は `PASS + bounded` または `FAIL + bounded` の範囲で解釈する。
- resource limit 到達は `UNKNOWN + incomplete` とする。
- limit 到達時点までの stats は捨てない。
- `FAIL` の最小 artifact は suppress できない。

## 15. 実行統計

```rust
pub struct ExplicitStats {
    pub states_seen: u64,
    pub states_enqueued: u64,
    pub transitions_tried: u64,
    pub transitions_taken: u64,
    pub max_depth_seen: u32,
    pub runtime_ms: u64,
    pub limit_hit: Option<LimitKind>,
}
```

## 16. Evidence生成

### 16.1 生成条件

- `FAIL` の場合は必須。
- `PASS` は witness 非対応の間は任意。
- `UNKNOWN` は evidence 不要。ただし debug snapshot は任意。

### 16.2 擬似コード

```text
function build_evidence_trace(model, predecessor, terminal_state, property_id):
  steps = reconstruct_trace(predecessor, state_id(terminal_state))
  trace_hash = hash_trace(steps)
  return EvidenceTrace(
    evidence_id = generate_evidence_id(),
    run_id = current_run_id(),
    property_id = property_id,
    assurance_level = current_assurance_level(),
    kind = counterexample,
    trace_hash = trace_hash,
    terminal_reason = classify_terminal_reason(terminal_state, property_id),
    initial_state_id = root_state_id(steps, terminal_state),
    terminal_state_id = state_id(terminal_state),
    steps = steps
  )
```

## 17. 代表 JSON 例

### 17.1 PASS + bounded

```json
{
  "kind": "completed",
  "status": "PASS",
  "assurance_level": "bounded",
  "property_result": {
    "property_id": "P_SAFE",
    "status": "PASS",
    "assurance_level": "bounded",
    "reason_code": "BOUNDED_SPACE_EXHAUSTED",
    "unknown_reason": null,
    "summary": "no violating state found within the configured depth bound"
  }
}
```

### 17.2 FAIL with 0-step trace

```json
{
  "schema_version": "1.0.0",
  "evidence_id": "ev-fail-0001",
  "run_id": "run-fail-0001",
  "property_id": "P_INIT_SAFE",
  "assurance_level": "complete",
  "kind": "counterexample",
  "trace_hash": "sha256:4af0102d",
  "terminal_reason": {
    "kind": "property_failed",
    "detail": "initial state violates invariant"
  },
  "initial_state_id": "s-000001",
  "terminal_state_id": "s-000001",
  "steps": []
}
```

### 17.3 UNKNOWN + incomplete

```json
{
  "kind": "completed",
  "status": "UNKNOWN",
  "assurance_level": "incomplete",
  "property_result": {
    "property_id": "P_SAFE",
    "status": "UNKNOWN",
    "assurance_level": "incomplete",
    "reason_code": null,
    "unknown_reason": "UNKNOWN_STATE_LIMIT_REACHED",
    "summary": "state limit reached before the configured search completed"
  }
}
```

### 17.4 ERROR_INVALID_INIT

```json
{
  "kind": "error",
  "status": "ERROR",
  "assurance_level": "incomplete",
  "diagnostics": [
    {
      "error_code": "ERROR_INVALID_INIT",
      "segment": "engine.search",
      "message": "init does not produce any well-typed initial state"
    }
  ]
}
```

## 18. テストケース一覧

| ID | 条件 | 期待結果 |
|---|---|---|
| C1-01 | initが単一状態 | 1件列挙 |
| C1-02 | initが複数状態 | 決定的順序で列挙 |
| C1-03 | init充足不能 | `ERROR_INVALID_INIT` |
| C2-01 | 2手で失敗するモデル | 2 step の反例 |
| C2-02 | 3手でも失敗可能 | BFS は最短反例 |
| C2-03 | deadlockあり | deadlock evidence生成 |
| C2-04 | 深さ境界まで探索完了 | `PASS + bounded` |
| C3-01 | BFSと同一FAILモデル | FAILカテゴリは一致 |
| C4-01 | 同値状態を2経路で到達 | 1回のみ訪問 |
| C5-01 | predecessor欠損 | `ERROR_PREDECESSOR_BROKEN` |
| C6-01 | state上限超過 | `UNKNOWN_STATE_LIMIT_REACHED` |
| C6-02 | time上限超過 | `UNKNOWN_TIME_LIMIT_REACHED` |
| D1-01 | invariant failure | counterexample trace生成 |
| D1-02 | 初期状態即FAIL | `steps = []` |
| D1-03 | 同じtrace | 同じ `trace_hash` |
