# AI, Solver, and Selfcheck Specs

- ドキュメントID: `RDD-0001-14`
- バージョン: `v0.3`
- 目的: `H`, `I`, `J` エピックを AI 運用、solver adapter、selfcheck 実行まで落とし込む。
- 依存章:
  - [explicit_engine_and_evidence_specs.md](explicit_engine_and_evidence_specs.md)
  - [testgen_contract_coverage_specs.md](testgen_contract_coverage_specs.md)
  - [../09_reference/json_schemas.md](../09_reference/json_schemas.md)
  - [../09_reference/error_codes.md](../09_reference/error_codes.md)
- 関連ID:
  - FR: `FR-023`, `FR-070`〜`FR-073`
  - NFR: `NFR-040`〜`NFR-042`
  - Epic: `H-1`〜`H-5`, `I-1`〜`I-4`, `J-1`〜`J-3`
  - PR: `PR-09`, `PR-10`, `PR-11`
  - 参照索引: [../09_reference/id_cross_reference.md](../09_reference/id_cross_reference.md)
- 補助参照:
  - [../09_reference/repository_structure.md](../09_reference/repository_structure.md)
  - [../09_reference/implementation_pr_plan.md](../09_reference/implementation_pr_plan.md)
  - [../10_delivery/README.md](../10_delivery/README.md)

## 1. 対象範囲

- `H-1` Inspect API
- `H-2` Check API
- `H-3` Explain API
- `H-4` Minimize API
- `H-5` Testgen API
- `I-1` Solver Adapter Interface
- `I-2` BMC Run Plan
- `I-3` Assignment to Trace
- `I-4` Capability Matrix
- `J-1` Selfcheck Spec群
- `J-2` Selfcheck Runner
- `J-3` Selfcheck Report

## 2. 設計原則

- AI 向けI/Fは全文字列自由形式を避け、安定 JSON schema を返す。
- solver 結果は共通 result 形式へ正規化し、`FAIL` の replay 可能証拠は `Evidence::Trace` として返す。
- selfcheck は通常 CI と分離し、通常 run の信頼性に影響を与えない。
- explain は補助情報であり、意味論上の真実源ではない。
- backend 能力差は capability matrix で明示する。
- 失敗応答は `diagnostics` を持ち、segment と conflict を返す。
- backend 発見は registry-based discovery を前提とし、CLI/API は backend config を adapter 境界へ引き渡す。

## 3. H-1 Inspect API

### 3.1 request schema

```json
{
  "schema_version": "1.0.0",
  "request_id": "req-inspect-0001",
  "model_ref": {
    "kind": "path",
    "value": "specs/counterlock.valid"
  }
}
```

### 3.2 response schema

```json
{
  "schema_version": "1.0.0",
  "request_id": "req-inspect-0001",
  "status": "ok",
  "model": {
    "model_id": "counterlock",
    "state_fields": [
      { "name": "x", "type": "bounded_u8[0,7]" },
      { "name": "locked", "type": "bool" }
    ],
    "actions": [
      { "action_id": "A_INC", "label": "Inc" },
      { "action_id": "A_LOCK", "label": "Lock" }
    ],
    "properties": [
      { "property_id": "P_SAFE", "kind": "invariant" }
    ],
    "requirements": []
  }
}
```

### 3.3 受け入れ条件

- state/action/property/requirements を必ず返す。
- 順序は宣言順を維持する。
- path 由来の source 情報は返してよいが、機密データを含めない。

## 4. H-2 Check API

### 4.1 request schema

```json
{
  "schema_version": "1.0.0",
  "request_id": "req-check-0001",
  "source_name": "specs/counterlock.valid",
  "source": "model Counter ...",
  "property_id": "P_SAFE",
  "backend": "command",
  "solver_executable": "sh",
  "solver_args": [
    "-c",
    "printf 'STATUS=UNKNOWN\\nACTIONS=Jump\\nASSURANCE_LEVEL=BOUNDED\\nREASON_CODE=SOLVER_REPORTED_UNKNOWN\\nSUMMARY=command-backend\\nUNKNOWN_REASON=TIME_LIMIT_REACHED'"
  ]
}
```

### 4.2 response schema

- 成功時は `ExplicitRunResult` または solver 正規化結果を返す。
- 失敗時は `error_code` と `diagnostics` を返す。

```json
{
  "kind": "completed",
  "manifest": {
    "request_id": "req-check-0001",
    "run_id": "run-20260306-0001",
    "backend_name": "mock-bmc",
    "backend_version": "external"
  },
  "status": "UNKNOWN",
  "assurance_level": "incomplete"
}
```

### 4.3 diagnostics 仕様

`check` は `ERROR` と `UNKNOWN` の両方で `diagnostics` を返す。特に `UNKNOWN` では「不明だった」だけで終わらせず、停止セグメントと次の選択肢を返す。

```json
{
  "error_code": "UNKNOWN_TIME_LIMIT_REACHED",
  "segment": "engine.search",
  "severity": "warning",
  "message": "time limit reached before property set was exhausted",
  "conflicts": [
    "time_limit_ms=1000",
    "frontier_size=2819"
  ],
  "help": [
    "increase time_limit_ms",
    "narrow property_selection",
    "switch to a symbolic backend if available"
  ],
  "best_practices": [
    "treat UNKNOWN as non-passing in CI",
    "record limits used for every run"
  ]
}
```

## 5. H-3 Explain API

### 5.1 返却フィールド

`Explain` は次のフィールドを固定する。

- `schema_version`
- `request_id`
- `status`
- `evidence_id`
- `property_id`
- `failure_step_index`
- `involved_fields`
- `candidate_causes`
- `repair_hints`
- `confidence`
- `best_practices`

### 5.2 response 例

```json
{
  "schema_version": "1.0.0",
  "request_id": "req-explain-0001",
  "status": "ok",
  "evidence_id": "ev-fail-0001",
  "property_id": "P_NO_BAD",
  "failure_step_index": 1,
  "involved_fields": ["bad"],
  "candidate_causes": [
    {
      "kind": "write_set_overlap",
      "message": "action A_MARK_BAD writes bad and overlaps with failing fields bad"
    },
    {
      "kind": "field_flip",
      "message": "bad changed at step 1"
    },
    {
      "kind": "action_write_set",
      "message": "review writes [bad] and reads [bad] of action A_MARK_BAD at failing step 1"
    }
  ],
  "repair_hints": [
    "review guard of action A_MARK_BAD",
    "verify invariant P_NO_BAD is intended",
    "inspect the postcondition or implementation of action A_MARK_BAD",
    "check whether writes [bad] should be narrowed or guarded"
  ],
  "best_practices": [
    "keep write sets explicit so involved fields stay explainable",
    "add witness vectors for critical actions so explain results stay reproducible"
  ],
  "confidence": 0.95
}
```

### 5.3 非目標

- 自動修正の確定
- 論理的完全性の主張
- solver 内部証明の説明

### 5.4 help と best practice の区別

- `repair_hints`: 直近の修正候補
- `best_practices`: 今後同種の問題を減らす設計規約
- `candidate_causes`: 破綻理由の仮説

この3つを混ぜないことで、AI が「今やること」と「今後守ること」を分離できる。

`candidate_causes` は MVP でも複数候補を返してよい。少なくとも field 差分由来の候補と action write set 由来の候補を分離する。

## 6. H-4 Minimize API

### 6.1 request schema

```json
{
  "schema_version": "1.0.0",
  "request_id": "req-min-0001",
  "vector_ref": {
    "kind": "artifact",
    "value": "artifacts/run-fail-0001/vectors/vec-000001.json"
  },
  "goal": {
    "kind": "reproduce_failure",
    "property_id": "P_NO_BAD"
  }
}
```

### 6.2 response 例

```json
{
  "schema_version": "1.0.0",
  "request_id": "req-min-0001",
  "status": "ok",
  "original_steps": 5,
  "minimized_steps": 2,
  "vector_id": "vec-000001-min"
}
```

## 7. H-5 Testgen API

### 7.1 request schema

```json
{
  "schema_version": "1.0.0",
  "request_id": "req-testgen-0001",
  "source_name": "counterlock.valid",
  "source": "model CounterLock\n...",
  "strategy": "transition",
  "backend": "mock-bmc",
  "solver_executable": null,
  "solver_args": []
}
```

### 7.2 response 例

```json
{
  "schema_version": "1.0.0",
  "request_id": "req-testgen-0001",
  "status": "ok",
  "vector_ids": ["vec-000001"],
  "generated_files": [
    "tests/generated/vec-000001.rs"
  ]
}
```

`strategy` は MVP では `counterexample | transition | witness` を受け付ける。

## 7.3 H-4 Orchestrate API

### 7.3.1 request 例

```json
{
  "request_id": "req-orch-0001",
  "source_name": "counterlock.valid",
  "source": "model CounterLock\n...",
  "backend": "mock-bmc",
  "solver_executable": null,
  "solver_args": []
}
```

### 7.3.2 response 例

```json
{
  "schema_version": "1.0.0",
  "request_id": "req-orch-0001",
  "runs": [
    {
      "property_id": "P_SAFE",
      "status": "Fail",
      "assurance_level": "Bounded",
      "run_id": "run-local-0001-P_SAFE-bmc"
    }
  ]
}
```

## 8. I-1 Solver Adapter Interface

### 8.1 Rust trait案

```rust
pub trait SolverAdapter {
    fn backend_kind(&self) -> BackendKind;
    fn capabilities(&self) -> CapabilityMatrix;
    fn build_plan(&self, model: &ModelIr, run_plan: &RunPlan) -> Result<SolverRunPlan, SolverPlanError>;
    fn run(&self, model: &ModelIr, plan: &SolverRunPlan) -> Result<RawSolverResult, SolverExecutionError>;
    fn normalize(
        &self,
        model: &ModelIr,
        run_plan: &RunPlan,
        raw: RawSolverResult,
    ) -> Result<NormalizedRunResult, TraceNormalizationError>;
}
```

### 8.2 設計意図

- `build_plan` で backend 固有変換を閉じ込める。
- `normalize` で top-level schema の共通化を担保する。
- adapter は `ModelIr` を受け取るが、上位層に solver 固有 AST を漏らさない。
- `run` は model を受け取り、command backend を含む外部実行でも property selection と manifest を維持する。

### 8.3 backend config

MVP で受け付ける backend config は次の3種類とする。

- `explicit`
- `mock-bmc`
- `command { solver_executable, solver_args[] }`

`command` は最小の外部プロセス adapter であり、protocol は次を受け付ける。

- `STATUS=<PASS|FAIL|UNKNOWN>`
- `ACTIONS=a,b,c`
- `ASSURANCE_LEVEL=<COMPLETE|BOUNDED|INCOMPLETE>`
- `REASON_CODE=<machine_reason>`
- `SUMMARY=<human_summary>`
- `UNKNOWN_REASON=<TIME_LIMIT_REACHED|STATE_LIMIT_REACHED|ENGINE_ABORTED>`

## 9. I-2 BMC / command Run Plan

### 9.1 Rust構造

```rust
pub struct SolverRunPlan {
    pub run_id: RunId,
    pub backend: BackendKind,
    pub target_property_ids: Vec<PropertyId>,
    pub horizon: Option<u32>,
    pub limits: RunLimits,
    pub encoded_model_hash: String,
}
```

### 9.2 ルール

- BMC では `horizon` 必須。
- explicit backend では `horizon` 不要。
- `encoded_model_hash` は solver 入力の再現性確認に使う。
- command backend では `VALID_RUN_ID` を環境変数として外部プロセスへ渡す。

## 10. I-3 assignment -> trace 変換ルール

### 10.1 変換手順

1. solver assignment から `state_0 ... state_k` を復元する。
2. action selector から各 step の `action_id` を求める。
3. `state_i -> state_{i+1}` の差分を作る。
4. `TraceStep` へ正規化する。
5. explicit trace と同じ `EvidenceTrace` schema に落とす。

補足:

- command backend は `ACTIONS` を replay して `EvidenceTrace` を再構築する。
- `FAIL` で action 列が無い場合は Completed ではなく Error envelope とする。

### 10.2 擬似コード

```text
function assignment_to_trace(assignment):
  states = decode_states(assignment)
  actions = decode_selected_actions(assignment)
  steps = []
  for i in 0 .. len(actions)-1:
    steps.push(
      TraceStep(
        index = i,
        from = states[i],
        action = actions[i],
        to = states[i + 1],
        diff = diff(states[i], states[i + 1])
      )
    )
  return steps
```

### 10.3 テストケース

| ID | 条件 | 期待結果 |
|---|---|---|
| I3-01 | 単純2 step assignment | 2 step trace |
| I3-02 | action selector複数true | normalize error |
| I3-03 | state欠損 | normalize error |

## 11. I-4 Capability Matrix

### 11.1 項目

- `backend_name`
- `supports_explicit`
- `supports_bmc`
- `supports_certificate`
- `supports_trace`
- `supports_witness`
- `selfcheck_compatible`

### 11.2 JSON例

```json
{
  "schema_version": "1.0.0",
  "request_id": "req-cap-0001",
  "backend": "explicit",
  "capabilities": {
    "backend_name": "explicit",
    "supports_explicit": true,
    "supports_bmc": false,
    "supports_certificate": false,
    "supports_trace": true,
    "supports_witness": false,
    "selfcheck_compatible": true
  }
}
```

`supports_trace = false` の backend は `FAIL` の本番ゲートに使わない。`command` backend は `ACTIONS` を返す限り `supports_trace = true` とみなす。

### 11.3 request 例

```json
{
  "request_id": "req-cap-0001",
  "backend": "command",
  "solver_executable": "solver-wrapper",
  "solver_args": ["--profile", "ci"]
}
```

## 12. J-1 Selfcheck対象

MVP selfcheck 対象:

- 式評価
- guard評価
- 遷移適用
- trace replay

Phase 2 対象:

- predecessor 復元
- coverage 集計
- contract hash 正規化

### 12.1 spec 例

```text
selfcheck expr_addition_preserves_bounds
selfcheck guard_false_disables_action
selfcheck transition_updates_only_declared_fields
selfcheck replay_matches_terminal_state
```

## 13. J-2 Selfcheck Runner

### 13.1 実行モード

- `ci-standard`: 通常CI。selfcheckは走らない。
- `ci-selfcheck`: selfcheck専用job。通常checkと分離。
- `local-selfcheck`: 開発者が明示実行。

### 13.2 分離方針

- selfcheck失敗は `ci-selfcheck` job を fail にする。
- `ci-standard` の成功条件に selfcheck を混ぜない。
- 理由は、selfcheck は安定化に時間がかかり、通常開発フローを止めすぎるため。

### 13.3 CLI例

```text
valid selfcheck run --suite kernel-core --json
```

## 14. J-3 Selfcheck artifact形式

### 14.1 JSON例

```json
{
  "schema_version": "1.0.0",
  "suite_id": "kernel-core",
  "run_id": "selfcheck-20260306-0001",
  "status": "PASS",
  "cases": [
    {
      "case_id": "expr_addition_preserves_bounds",
      "status": "PASS",
      "evidence_id": null
    },
    {
      "case_id": "replay_matches_terminal_state",
      "status": "PASS",
      "evidence_id": null
    }
  ]
}
```

### 14.2 保持方針

- 通常 run artifact と別ディレクトリに保存する。
- `artifacts/selfcheck/<suite-id>/<run-id>/report.json`
- evidence がある場合も同一配下に置く。

## 15. API失敗例

```json
{
  "schema_version": "1.0.0",
  "request_id": "req-check-0002",
  "status": "error",
  "error_code": "UNSUPPORTED_BACKEND",
  "diagnostics": [
    {
      "level": "error",
      "message": "backend bmc requested but no adapter is installed"
    }
  ]
}
```

## 16. 総合テストケース一覧

| ID | 分類 | 目的 | 期待結果 |
|---|---|---|---|
| H-01 | inspect | 基本schema | state/action/property一覧 |
| H-02 | check | explicit result | normalized result |
| H-03 | explain | failure fields | involved_fields返却 |
| H-04 | minimize | counterexample短縮 | shorter vector |
| H-05 | testgen | generated file返却 | file path返却 |
| I-01 | adapter | capability matrix | expected flags |
| I-02 | adapter | build_plan | horizon設定 |
| I-03 | normalize | assignment to trace | evidence trace生成 |
| J-01 | selfcheck | expr evaluation | PASS |
| J-02 | selfcheck | trace replay | PASS |
| J-03 | selfcheck | broken replay spec | FAIL |

## 17. DDD対応

- AI API は `Integration Context`。
- solver adapter は `Verification Context` と `Integration Context` の anti-corruption layer。
- selfcheck は `Kernel Assurance Context` として扱う。

## 18. クリーンアーキテクチャ対応

- API handler は input adapter。
- solver adapter は infrastructure adapter。
- selfcheck runner は use case。
- normalized result / capability matrix / explain DTO は interface 向け DTO。

## 19. SSOT対応

- AI API は source を更新しない。
- solver assignment は一次ソースではなく、正規化前の中間 artifact にすぎない。
- selfcheck report は監査用派生物である。

## 20. ソルバ方針

- MVP backend は explicit。
- Phase 2 で BMC adapter を足す。
- solver 追加時の受け入れ条件は `normalize` で `EvidenceTrace` が生成できること。
- solver 固有の例外事情は `diagnostics.raw_backend` に隔離し、共通契約に漏らさない。

## 21. 完了条件

1. `inspect/check/explain/minimize/testgen` の request/response schema が固定されている。
2. `Explain` の返却フィールドが固定されている。
3. solver adapter trait が定義されている。
4. capability matrix 項目が確定している。
5. assignment -> trace 変換規則が疑似コード化されている。
6. selfcheck対象、artifact形式、CI分離方針が定義済みである。

## 22. 結論

本章が固まると、このプロジェクトは単なるCLIではなく、AIが操作でき、複数 backend を増やせて、kernel 自身の信頼性も段階的に上げられる基盤になる。ここを曖昧にすると、将来の solver 拡張や selfcheck 導入時に契約が崩れる。
