# Explicit Engine and Evidence Specs

- ドキュメントID: `RDD-0001-12`
- バージョン: `v0.2`
- 目的: `C-1`〜`C-7` と `D-1`〜`D-3` の詳細仕様を、実装可能な粒度まで固定する。
- 依存章:
  - [mvp_frontend_and_kernel_specs.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/mvp_frontend_and_kernel_specs.md)
  - [json_schemas.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/json_schemas.md)
  - [error_codes.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/error_codes.md)
  - [artifact_naming.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/artifact_naming.md)

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

- explicit engineはMVPの基準実装である。
- `FAIL` の場合は必ず replay 可能な `EvidenceTrace` を返す。
- `PASS` の場合も property ごとの判定理由と統計を返す。
- `UNKNOWN` は曖昧な成功扱いを禁止し、理由コードを必ず保持する。
- backend固有形式は出さず、共通 schema へ正規化する。
- BFSの最短反例性を壊す変更は互換破壊とみなす。
- traceは一次ソースではないが、運用上の正本 artifact として扱う。

## 3. 用語

- `RunPlan`: 実行方針、limits、property選択、backend選択をまとめた不変値。
- `ExplicitRunResult`: explicit backend の正規化済み結果。
- `PropertyResult`: property 単位の判定結果。
- `EvidenceTrace`: 反例または witness を再生可能な形に正規化した証拠。
- `TraceStep`: 1回の action 適用を表す単位。
- `UnknownReasonCode`: `UNKNOWN` の理由コード。

## 4. check API契約

### 4.1 Rust API

```rust
pub fn check_explicit(
    model: &ModelIr,
    run_plan: &RunPlan,
) -> Result<ExplicitRunResult, CheckError>;
```

### 4.2 CLI対応

```text
valid check <spec-path> \
  --backend explicit \
  --strategy bfs \
  --property <property-id> \
  --max-states <n> \
  --max-depth <n> \
  --time-limit <duration> \
  --json
```

### 4.3 契約

- 入力 `ModelIr` は型検査済みでなければならない。
- `RunPlan.backend` は `explicit` でなければならない。
- property 選択が空の場合は、MVPでは全propertyを対象とする。
- `CheckError` は「実行前提が成立しない」場合にのみ返し、探索途中の limit 到達は `ExplicitRunResult.status = UNKNOWN` で返す。

### 4.4 前提違反

- backend mismatch
- property id not found
- unsupported property kind
- invalid limits
- model invariant broken before run

## 5. RunPlan 型

### 5.1 Rust構造

```rust
pub struct RunPlan {
    pub run_id: RunId,
    pub schema_version: SchemaVersion,
    pub backend: BackendKind,
    pub explicit_strategy: ExplicitStrategy,
    pub property_selection: PropertySelection,
    pub limits: RunLimits,
    pub artifact_policy: ArtifactPolicy,
    pub reporter_options: ReporterOptions,
}

pub enum ExplicitStrategy {
    Bfs,
    Dfs,
}

pub struct RunLimits {
    pub max_states: Option<u64>,
    pub max_depth: Option<u32>,
    pub time_limit_ms: Option<u64>,
}
```

### 5.2 意味論

- `run_id` は実行全体の識別子であり、artifact名の親キーとなる。
- `schema_version` は出力 artifact の schema を固定する。
- `property_selection` は `All` または `Only(Vec<PropertyId>)` を取る。
- `artifact_policy` は `EmitAll` `EmitOnFailure` `EmitNothing` を取る。
- `reporter_options` は text/JSON 出力の粒度制御を行うが、意味論を変えてはならない。

### 5.3 受け入れ条件

- `RunPlan` が同一なら artifact path と JSON 内容は決定的である。
- `RunPlan` は builder 経由でのみ構築し、不正値を持てない。

## 6. ExplicitRunResult schema

### 6.1 Rust構造

```rust
pub struct ExplicitRunResult {
    pub schema_version: SchemaVersion,
    pub run_id: RunId,
    pub backend: BackendKind,
    pub strategy: ExplicitStrategy,
    pub status: RunStatus,
    pub property_results: Vec<PropertyResult>,
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
    pub terminal_state_id: Option<StateId>,
    pub evidence_id: Option<EvidenceId>,
    pub unknown_reason: Option<UnknownReasonCode>,
    pub summary: String,
}
```

### 6.2 JSON schema

```json
{
  "schema_version": "1.0.0",
  "run_id": "run-20260306-0001",
  "backend": "explicit",
  "strategy": "bfs",
  "status": "FAIL",
  "property_results": [
    {
      "property_id": "P_NO_BAD",
      "property_kind": "invariant",
      "status": "FAIL",
      "terminal_state_id": "s-000004",
      "evidence_id": "ev-000001",
      "unknown_reason": null,
      "summary": "bad state reached at depth 2"
    }
  ],
  "stats": {
    "states_seen": 5,
    "states_enqueued": 5,
    "transitions_tried": 8,
    "transitions_taken": 4,
    "max_depth_seen": 2,
    "runtime_ms": 3,
    "limit_hit": null
  },
  "diagnostics": [],
  "artifacts": {
    "result_json": "artifacts/run-20260306-0001/check-result.json",
    "evidence": [
      "artifacts/run-20260306-0001/evidence/ev-000001.trace.json"
    ]
  }
}
```

### 6.3 スキーマ要件

- すべての top-level key は必須。
- `property_results` は空にしない。
- `status` は `property_results` の最悪値を反映する。
- `artifacts` は path を相対パスで保持する。
- `diagnostics` は warning 含め順序安定で出す。

## 7. TraceStep と EvidenceTrace schema

### 7.1 Rust構造

```rust
pub struct EvidenceTrace {
    pub schema_version: SchemaVersion,
    pub evidence_id: EvidenceId,
    pub run_id: RunId,
    pub property_id: PropertyId,
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
    pub action_id: ActionId,
    pub action_label: String,
    pub to_state_id: StateId,
    pub depth: u32,
    pub state_before: StateValue,
    pub state_after: StateValue,
    pub diff: Vec<FieldDiff>,
}
```

### 7.2 FieldDiff

```rust
pub struct FieldDiff {
    pub field_path: String,
    pub before: Value,
    pub after: Value,
}
```

### 7.3 JSON schema

```json
{
  "schema_version": "1.0.0",
  "evidence_id": "ev-000001",
  "run_id": "run-20260306-0001",
  "property_id": "P_NO_BAD",
  "kind": "counterexample",
  "trace_hash": "sha256:1d7d66b0",
  "terminal_reason": {
    "kind": "property_failed",
    "detail": "invariant violated"
  },
  "initial_state_id": "s-000001",
  "terminal_state_id": "s-000004",
  "steps": [
    {
      "index": 0,
      "from_state_id": "s-000001",
      "action_id": "A_INC",
      "action_label": "Inc",
      "to_state_id": "s-000002",
      "depth": 1,
      "state_before": { "x": 0, "locked": false, "bad": false },
      "state_after": { "x": 1, "locked": false, "bad": false },
      "diff": [
        { "field_path": "x", "before": 0, "after": 1 }
      ]
    }
  ]
}
```

### 7.4 受け入れ条件

- `steps.len() == terminal depth`
- `state_before` / `state_after` が replay に十分
- `diff` は派生情報だが、必ず `state_before` と `state_after` から再計算可能
- `trace_hash` は step内容からのみ計算し、pathに依存しない

## 8. UNKNOWN 理由コード一覧

MVPで固定する理由コード:

- `UNKNOWN_STATE_LIMIT_REACHED`
- `UNKNOWN_DEPTH_LIMIT_REACHED`
- `UNKNOWN_TIME_LIMIT_REACHED`
- `UNKNOWN_INIT_ENUMERATION_LIMIT_REACHED`
- `UNKNOWN_UNSUPPORTED_PROPERTY_KIND`
- `UNKNOWN_ENGINE_ABORTED`

理由コードの原則:

- 1回の run で top-level は単一理由を返す。
- property 単位ではより細かい理由を返せる。
- `UNKNOWN` と `ERROR` は分離する。前者は契約内停止、後者は契約外失敗。

## 9. C-1 初期状態列挙

### 9.1 責務

- `InitSpec` から探索開始状態集合を得る。
- 複数初期状態を安定順序で列挙する。
- init が 0件の場合は `UNSAT_INIT` を返す。

### 9.2 擬似コード

```text
function enumerate_initial_states(model, limits):
  candidates = solve_init_assignments(model.init, model.state_schema)
  if candidates.is_empty():
    return Err(UNSAT_INIT)

  ordered = sort_by_canonical_state(candidates)
  if limits.init_enumeration_limit exists and ordered.len > limit:
    return UnknownInitLimit(ordered.prefix(limit))

  return InitialStateSet(ordered)
```

### 9.3 テストケース

| ID | 条件 | 期待結果 |
|---|---|---|
| C1-01 | initが単一状態 | 1件列挙 |
| C1-02 | initが複数状態 | 決定的順序で列挙 |
| C1-03 | init充足不能 | `UNSAT_INIT` |
| C1-04 | init列挙数が上限超過 | `UNKNOWN_INIT_ENUMERATION_LIMIT_REACHED` |

## 10. C-2 BFS探索

### 10.1 責務

- 幅優先探索を行う。
- 最短反例トレースを保証する。
- deadlock と property failure を検出する。

### 10.2 擬似コード

```text
function run_bfs(model, initial_states, run_plan):
  queue = new FIFOQueue()
  visited = new VisitedSet()
  predecessor = new PredecessorMap()
  stats = empty_stats()
  start_time = now()

  for each init_state in initial_states:
    id = state_id(init_state)
    queue.push((init_state, 0))
    visited.insert(init_state)
    predecessor.mark_root(id)
    stats.states_enqueued += 1

  while queue is not empty:
    check_limits_or_return_unknown(run_plan.limits, stats, start_time)
    (state, depth) = queue.pop()
    stats.states_seen += 1
    stats.max_depth_seen = max(stats.max_depth_seen, depth)

    property_outcome = evaluate_properties(model, state)
    if property_outcome is FAIL:
      return fail_result(property_outcome, state, predecessor, stats)

    enabled = []
    for each action in model.actions:
      stats.transitions_tried += 1
      if guard_is_true(action, state):
        enabled.append(action)

    if enabled is empty and deadlock_property_selected(model):
      return fail_deadlock_result(state, predecessor, stats)

    for each action in enabled:
      if depth + 1 exceeds max_depth:
        return unknown_depth_limit(stats)
      next_state = apply_transition(action, state)
      stats.transitions_taken += 1
      if visited.insert_if_absent(next_state):
        next_id = state_id(next_state)
        predecessor.record(next_id, state_id(state), action.id, depth + 1)
        queue.push((next_state, depth + 1))
        stats.states_enqueued += 1

  return pass_result(stats)
```

### 10.3 最短反例性

- root depth を 0 とする。
- queue への push は action 定義順と init 順を維持する。
- visited 判定は enqueue 時に行う。
- これにより、最初に見つかった `FAIL` は最短 step 数を持つ。

### 10.4 テストケース

| ID | 条件 | 期待結果 |
|---|---|---|
| C2-01 | 2手で失敗するモデル | 2 step の反例 |
| C2-02 | 3手でも失敗可能 | 最短2 stepのみ返す |
| C2-03 | deadlockあり | deadlock evidence生成 |
| C2-04 | failureなし | PASS |

## 11. C-3 DFS探索

### 11.1 責務

- stack ベース探索を提供する。
- 最短反例は保証しない。
- 深めの探索を少メモリで回す。

### 11.2 擬似コード

```text
function run_dfs(model, initial_states, run_plan):
  stack = new Stack()
  visited = new VisitedSet()
  predecessor = new PredecessorMap()
  stats = empty_stats()
  start_time = now()

  for each init_state in reverse(initial_states):
    stack.push((init_state, 0))

  while stack is not empty:
    check_limits_or_return_unknown(run_plan.limits, stats, start_time)
    (state, depth) = stack.pop()
    if not visited.insert_if_absent(state):
      continue
    stats.states_seen += 1

    property_outcome = evaluate_properties(model, state)
    if property_outcome is FAIL:
      return fail_result(property_outcome, state, predecessor, stats)

    enabled = collect_enabled_actions(model, state)
    if enabled is empty and deadlock_property_selected(model):
      return fail_deadlock_result(state, predecessor, stats)

    for each action in reverse(enabled):
      if depth + 1 exceeds max_depth:
        return unknown_depth_limit(stats)
      next_state = apply_transition(action, state)
      predecessor.record_if_absent(state_id(next_state), state_id(state), action.id, depth + 1)
      stack.push((next_state, depth + 1))

  return pass_result(stats)
```

### 11.3 テストケース

| ID | 条件 | 期待結果 |
|---|---|---|
| C3-01 | BFSと同一FAILモデル | FAILカテゴリは一致 |
| C3-02 | BFSより長い反例 | 許容 |
| C3-03 | 深い線形モデル | BFSより少メモリを想定 |

## 12. C-4 訪問済み状態管理

### 12.1 正規化

- state は field 名の辞書順で canonicalize する。
- enum は宣言順の安定整数へ変換する。
- bounded int は数値のまま保持する。
- struct の field 順序差異は無視する。

### 12.2 擬似コード

```text
function canonicalize_state(state):
  pairs = []
  for field in sorted(state.fields):
    pairs.append((field.name, canonicalize_value(field.value)))
  return pairs

function hash_state(state):
  canonical = canonicalize_state(state)
  return stable_hash(canonical)
```

### 12.3 テストケース

| ID | 条件 | 期待結果 |
|---|---|---|
| C4-01 | 同値状態を2経路で到達 | 1回のみ訪問 |
| C4-02 | field順序が異なる表現 | 同一ハッシュ |
| C4-03 | ハッシュ衝突想定モック | 等価比較で保護 |

## 13. C-5 predecessor復元

### 13.1 データ構造

```rust
pub struct PredecessorEntry {
    pub parent_state_id: Option<StateId>,
    pub via_action_id: Option<ActionId>,
    pub depth: u32,
}
```

### 13.2 擬似コード

```text
function reconstruct_trace(predecessor, terminal_state_id):
  steps = []
  cursor = terminal_state_id

  while predecessor[cursor].parent_state_id exists:
    parent = predecessor[cursor].parent_state_id
    action = predecessor[cursor].via_action_id
    steps.prepend(build_trace_step(parent, action, cursor))
    cursor = parent

  return steps
```

### 13.3 受け入れ条件

- root には `parent_state_id = None`
- broken chain は engine bug とみなし `ERROR_PREDECESSOR_BROKEN`
- BFS では reconstructed trace が最短

### 13.4 テストケース

| ID | 条件 | 期待結果 |
|---|---|---|
| C5-01 | 1 step fail | 1 step trace |
| C5-02 | 3 step fail | 親子連鎖を逆順復元 |
| C5-03 | predecessor欠損 | internal error |

## 14. C-6 limit到達時の振る舞い

### 14.1 優先順位

limit 同時到達時の優先順位:

1. time
2. state
3. depth

### 14.2 擬似コード

```text
function check_limits_or_return_unknown(limits, stats, start_time):
  if limits.time_limit_ms exists and elapsed_ms(start_time) >= limits.time_limit_ms:
    raise Unknown(UNKNOWN_TIME_LIMIT_REACHED)
  if limits.max_states exists and stats.states_seen >= limits.max_states:
    raise Unknown(UNKNOWN_STATE_LIMIT_REACHED)
  if limits.max_depth exists and stats.max_depth_seen >= limits.max_depth and frontier_not_empty():
    raise Unknown(UNKNOWN_DEPTH_LIMIT_REACHED)
```

### 14.3 ポリシー

- limit 到達時点までの stats は捨てない。
- 中途半端な trace を evidence として保存しない。
- ただし frontier snapshot を debug artifact として残す余地は設ける。

### 14.4 テストケース

| ID | 条件 | 期待結果 |
|---|---|---|
| C6-01 | state上限超過 | `UNKNOWN_STATE_LIMIT_REACHED` |
| C6-02 | depth上限超過 | `UNKNOWN_DEPTH_LIMIT_REACHED` |
| C6-03 | time上限超過 | `UNKNOWN_TIME_LIMIT_REACHED` |

## 15. C-7 実行統計

### 15.1 Rust構造

```rust
pub struct ExplicitStats {
    pub states_seen: u64,
    pub states_enqueued: u64,
    pub transitions_tried: u64,
    pub transitions_taken: u64,
    pub max_depth_seen: u32,
    pub runtime_ms: u64,
    pub limit_hit: Option<UnknownReasonCode>,
}
```

### 15.2 テストケース

| ID | 条件 | 期待結果 |
|---|---|---|
| C7-01 | 反例1件 | counters整合 |
| C7-02 | PASS run | limit_hitなし |
| C7-03 | UNKNOWN run | limit_hitあり |

## 16. D-1 Evidence生成

### 16.1 生成条件

- `FAIL` の場合は必須。
- `PASS` は原則 evidence不要。ただし将来 witness に対応。
- `UNKNOWN` は evidence不要。ただし debug snapshot は任意。

### 16.2 擬似コード

```text
function build_evidence_trace(model, predecessor, terminal_state, property_id):
  steps = reconstruct_trace(predecessor, state_id(terminal_state))
  trace_hash = hash_trace(steps, property_id)
  return EvidenceTrace(
    evidence_id = generate_evidence_id(),
    run_id = current_run_id(),
    property_id = property_id,
    kind = counterexample,
    trace_hash = trace_hash,
    terminal_reason = classify_terminal_reason(terminal_state, property_id),
    initial_state_id = root_state_id(steps, terminal_state),
    terminal_state_id = state_id(terminal_state),
    steps = steps
  )
```

### 16.3 テストケース

| ID | 条件 | 期待結果 |
|---|---|---|
| D1-01 | invariant failure | counterexample trace生成 |
| D1-02 | deadlock failure | terminal_reasonがdeadlock |
| D1-03 | trace hash安定性 | 同じtraceで同じhash |

## 17. D-2 Trace JSON サンプル

### 17.1 PASS 例

```json
{
  "schema_version": "1.0.0",
  "run_id": "run-pass-0001",
  "backend": "explicit",
  "strategy": "bfs",
  "status": "PASS",
  "property_results": [
    {
      "property_id": "P_SAFE",
      "property_kind": "invariant",
      "status": "PASS",
      "terminal_state_id": null,
      "evidence_id": null,
      "unknown_reason": null,
      "summary": "no violating state found in reachable space"
    }
  ],
  "stats": {
    "states_seen": 3,
    "states_enqueued": 3,
    "transitions_tried": 6,
    "transitions_taken": 2,
    "max_depth_seen": 2,
    "runtime_ms": 2,
    "limit_hit": null
  },
  "diagnostics": [],
  "artifacts": {
    "result_json": "artifacts/run-pass-0001/check-result.json",
    "evidence": []
  }
}
```

### 17.2 FAIL 例

```json
{
  "schema_version": "1.0.0",
  "evidence_id": "ev-fail-0001",
  "run_id": "run-fail-0001",
  "property_id": "P_NO_BAD",
  "kind": "counterexample",
  "trace_hash": "sha256:4af0102d",
  "terminal_reason": {
    "kind": "property_failed",
    "detail": "bad became true"
  },
  "initial_state_id": "s-000001",
  "terminal_state_id": "s-000003",
  "steps": [
    {
      "index": 0,
      "from_state_id": "s-000001",
      "action_id": "A_INC",
      "action_label": "Inc",
      "to_state_id": "s-000002",
      "depth": 1,
      "state_before": { "x": 0, "bad": false },
      "state_after": { "x": 1, "bad": false },
      "diff": [
        { "field_path": "x", "before": 0, "after": 1 }
      ]
    },
    {
      "index": 1,
      "from_state_id": "s-000002",
      "action_id": "A_MARK_BAD",
      "action_label": "MarkBad",
      "to_state_id": "s-000003",
      "depth": 2,
      "state_before": { "x": 1, "bad": false },
      "state_after": { "x": 1, "bad": true },
      "diff": [
        { "field_path": "bad", "before": false, "after": true }
      ]
    }
  ]
}
```

### 17.3 UNKNOWN 例

```json
{
  "schema_version": "1.0.0",
  "run_id": "run-unknown-0001",
  "backend": "explicit",
  "strategy": "dfs",
  "status": "UNKNOWN",
  "property_results": [
    {
      "property_id": "P_SAFE",
      "property_kind": "invariant",
      "status": "UNKNOWN",
      "terminal_state_id": null,
      "evidence_id": null,
      "unknown_reason": "UNKNOWN_STATE_LIMIT_REACHED",
      "summary": "state limit reached before fixed point"
    }
  ],
  "stats": {
    "states_seen": 1000,
    "states_enqueued": 1000,
    "transitions_tried": 1998,
    "transitions_taken": 999,
    "max_depth_seen": 18,
    "runtime_ms": 19,
    "limit_hit": "UNKNOWN_STATE_LIMIT_REACHED"
  },
  "diagnostics": [],
  "artifacts": {
    "result_json": "artifacts/run-unknown-0001/check-result.json",
    "evidence": []
  }
}
```

## 18. D-3 text reporter 出力例

### 18.1 PASS

```text
RUN run-pass-0001 backend=explicit strategy=bfs
STATUS PASS
PROPERTY P_SAFE invariant PASS
STATS states_seen=3 transitions_tried=6 max_depth_seen=2 runtime_ms=2
```

### 18.2 FAIL

```text
RUN run-fail-0001 backend=explicit strategy=bfs
STATUS FAIL
PROPERTY P_NO_BAD invariant FAIL at state s-000003
TRACE ev-fail-0001 steps=2 hash=sha256:4af0102d
STEP 0 Inc x:0->1
STEP 1 MarkBad bad:false->true
STATS states_seen=5 transitions_tried=8 max_depth_seen=2 runtime_ms=3
```

### 18.3 UNKNOWN

```text
RUN run-unknown-0001 backend=explicit strategy=dfs
STATUS UNKNOWN
PROPERTY P_SAFE invariant UNKNOWN reason=UNKNOWN_STATE_LIMIT_REACHED
STATS states_seen=1000 transitions_tried=1998 max_depth_seen=18 runtime_ms=19
```

## 19. 総合テストケース一覧

### 19.1 必須ケース

| ID | 種別 | 目的 | 期待結果 |
|---|---|---|---|
| X-01 | init | unsat init | `UNSAT_INIT` |
| X-02 | property | deadlock | deadlock evidence |
| X-03 | bfs | shortest counterexample | 最短trace |
| X-04 | limits | state limit reached | UNKNOWN |
| X-05 | limits | depth limit reached | UNKNOWN |
| X-06 | limits | time limit reached | UNKNOWN |
| X-07 | serialization | result JSON | schema validation成功 |
| X-08 | serialization | evidence JSON | schema validation成功 |
| X-09 | reporter | text summary | golden一致 |
| X-10 | replay | evidence replay | terminal state一致 |

### 19.2 非機能寄りケース

| ID | 種別 | 目的 | 期待結果 |
|---|---|---|---|
| X-11 | determinism | 同一入力2回実行 | hash/stats以外完全一致 |
| X-12 | stability | action順固定 | trace順序安定 |
| X-13 | memory | DFS fallback | OOMしない |

## 20. DDD 対応

- Explicit engine は `Verification Context` の application service である。
- `EvidenceTrace` は `Evidence Context` の aggregate root として扱う。
- `RunPlan` は value object であり、永続化前提ではなく実行契約である。
- `PropertyResult` と `ExplicitStats` は run aggregate の内部構成要素である。

## 21. クリーンアーキテクチャ対応

- kernel は domain layer に置く。
- explicit engine は use case layer に置く。
- JSON reporter / text reporter は interface adapter に置く。
- artifact 出力は infrastructure layer に置く。

依存方向:

```text
CLI/API -> UseCase(check_explicit) -> Kernel
CLI/API -> Reporter(JSON/Text) -> ArtifactStore
ExplicitEngine -> Kernel
Reporter -> Result DTO
```

## 22. STO/SSOT 対応

- 一次ソースは `ModelIr` と `RunPlan` である。
- `ExplicitRunResult` は派生物だが、再現・監査のため保存対象とする。
- `EvidenceTrace` は run から派生するが、後続の testgen と explain の入力となるため保存必須とする。
- text summary は捨ててもよいが、JSON と trace は捨てない。

## 23. ソルバ・将来拡張との接続

- 本章の schema は explicit 専用にしない。
- BMC backend も `RunPlan -> NormalizedResult -> EvidenceTrace` の形へ合わせる。
- explicit/BMC 両者で `EvidenceTrace` が一致しなければ explain/testgen を共有できないため、この章の共通 schema を上位契約とする。

## 24. 完了条件

1. `check_explicit` の Rust API が固定されている。
2. `RunPlan`, `ExplicitRunResult`, `EvidenceTrace` の schema が固定されている。
3. BFS/DFS/predecessor/limit の疑似コードがあり、実装の正誤判定基準になる。
4. PASS/FAIL/UNKNOWN の JSON 例と text 例が存在する。
5. 最低10件のテストケース一覧が存在する。

## 25. 結論

本章が固定されると、`check` 実装の責務は曖昧ではなくなる。以後の実装論点はアルゴリズムの正誤と性能に限定され、入出力契約や証拠形式で揉めない状態を作れる。
