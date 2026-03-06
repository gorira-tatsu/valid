# Testgen, Contract, and Coverage Specs

- ドキュメントID: `RDD-0001-13`
- バージョン: `v0.2`
- 目的: `E`, `F`, `G` エピックを、API例、JSON例、擬似コード、テストケース一覧まで落とし込む。
- 依存章:
  - [explicit_engine_and_evidence_specs.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md)
  - [json_schemas.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/json_schemas.md)
  - [error_codes.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/error_codes.md)
  - [artifact_naming.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/artifact_naming.md)
- 関連ID:
  - FR: `FR-040`〜`FR-043`, `FR-050`〜`FR-053`, `FR-060`〜`FR-063`
  - Epic: `E-1`〜`E-5`, `F-1`〜`F-4`, `G-1`〜`G-5`
  - PR: `PR-06`, `PR-07`, `PR-08`
  - 参照索引: [id_cross_reference.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/id_cross_reference.md)
- 次に読む:
  - [ai_solver_selfcheck_specs.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md)
  - [../09_reference/implementation_pr_plan.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/implementation_pr_plan.md)

## 1. 対象範囲

- `E-1` Counterexample to Vector
- `E-2` Vector to Rust Test
- `E-3` Witness Test Generation
- `E-4` Trace Minimization
- `E-5` Test Rendering Modes
- `F-1` Contract Snapshot生成
- `F-2` Lock照合
- `F-3` Drift出力
- `F-4` Document Drift検知
- `G-1` Transition Coverage
- `G-2` Guard Coverage
- `G-3` State/Depth Summary
- `G-4` Coverage Report
- `G-5` Coverage Gate Evaluation

## 2. 設計原則

- 生成物はすべて trace か contract metadata から導出される派生物である。
- 回帰テストは「人間が再現方法を覚える」ためではなく、「CIが同じ失敗を再生する」ために作る。
- contract drift は hash だけではなく、diff 可能な構造も保存する。
- coverage は backend 非依存の指標として集約する。
- 生成方式は複数あってよいが、`TestVector` を中間形式として共通化する。

## 3. TestVector schema

### 3.1 Rust構造

```rust
pub struct TestVector {
    pub schema_version: SchemaVersion,
    pub vector_id: VectorId,
    pub source_kind: TestVectorSourceKind,
    pub evidence_id: Option<EvidenceId>,
    pub strategy: TestGenerationStrategy,
    pub generator_version: String,
    pub seed: Option<u64>,
    pub initial_state: Option<StateValue>,
    pub actions: Vec<VectorActionStep>,
    pub expected_observations: Vec<ExpectedObservation>,
    pub oracle_kind: OracleKind,
    pub metadata: VectorMetadata,
}
```

### 3.2 JSON例

```json
{
  "schema_version": "1.0.0",
  "vector_id": "vec-000001",
  "source_kind": "counterexample",
  "evidence_id": "ev-fail-0001",
  "strategy": "counterexample",
  "generator_version": "0.1.0",
  "seed": null,
  "initial_state": { "x": 0, "bad": false },
  "actions": [
    { "index": 0, "action_id": "A_INC", "action_label": "Inc" },
    { "index": 1, "action_id": "A_MARK_BAD", "action_label": "MarkBad" }
  ],
  "expected_observations": [
    { "index": 0, "observation": { "x": 1, "bad": false } },
    { "index": 1, "observation": { "x": 1, "bad": true } }
  ],
  "oracle_kind": "trace_replay",
  "metadata": {
    "property_id": "P_NO_BAD",
    "coverage_targets": [],
    "minimized": false
  }
}
```

### 3.3 スキーマ要件

- `actions.len()` と `expected_observations.len()` は一致してよいが、`oracle_kind` に応じて observation は省略可能である。
- `generator_version` は必須。
- `source_kind` と `strategy` は分ける。前者は由来、後者は生成戦略。

## 4. E-1 Counterexample to Vector

### 4.1 API例

```rust
pub fn build_counterexample_vector(
    evidence: &EvidenceTrace,
    oracle_kind: OracleKind,
) -> Result<TestVector, VectorBuildError>;
```

### 4.2 変換ルール

- `EvidenceTrace.steps[*].action_id` を `TestVector.actions` へ写す。
- `EvidenceTrace.steps[*].state_after` を `expected_observations` の基底とする。
- `property_id` は metadata へ保持する。
- `terminal_reason` は vector 本体ではなく、付随説明に保持する。

### 4.3 擬似コード

```text
function build_counterexample_vector(evidence):
  actions = []
  observations = []
  for step in evidence.steps:
    actions.push(action_step(step.action_id, step.action_label))
    observations.push(observation_step(step.index, step.state_after))

  return TestVector(
    vector_id = new_vector_id(),
    source_kind = counterexample,
    evidence_id = evidence.evidence_id,
    strategy = counterexample,
    initial_state = evidence.steps[0].state_before if any else null,
    actions = actions,
    expected_observations = observations,
    oracle_kind = trace_replay
  )
```

### 4.4 テストケース

| ID | 条件 | 期待結果 |
|---|---|---|
| E1-01 | 2 step evidence | 2 action vector |
| E1-02 | 1 step deadlock evidence | 0 actionまたは1 terminal marker扱いを固定 |
| E1-03 | empty steps | build error |

## 5. E-2 Rust testコード雛形

### 5.1 方針

- 生成コードは trait ベース adapter を前提にする。
- 生成コード自身にビジネスロジックを持たせない。
- test failure 時に `vector_id` と `evidence_id` を表示する。

### 5.2 雛形

```rust
#[test]
fn generated_vec_000001() {
    let vector = load_vector!("vec-000001");
    let mut sut = TestHarness::new();

    if let Some(init) = &vector.initial_state {
        sut.reset_to(init.clone());
    }

    for (i, step) in vector.actions.iter().enumerate() {
        let obs = sut.apply(&step.action_id);
        let expected = &vector.expected_observations[i].observation;
        assert_eq!(
            obs, *expected,
            "vector_id={} evidence_id={:?} step={}",
            vector.vector_id, vector.evidence_id, i
        );
    }
}
```

### 5.3 保存方式の判断

- MVP採用: `tests/generated/*.rs`
- Phase 2追加: `OUT_DIR + include!`

### 5.4 判断理由

`tests/generated/*.rs` を MVP採用する理由:

- 差分レビューしやすい。
- 失敗時の参照が容易。
- build.rs を必須にせずに始められる。

`OUT_DIR + include!` を Phase 2 に送る理由:

- repo を汚しにくいが、build 流れが複雑になる。
- 生成条件とキャッシュ制御が増える。
- 最初の価値は test の内容可視化にあるため、MVPでは読みやすさを優先する。

## 6. E-3 Witness Test Generation

### 6.1 transition coverage 戦略

- 各 `action_id` を少なくとも1回含む witness を生成する。
- 同じ action のみを繰り返す vector は優先しない。

擬似コード:

```text
function build_transition_coverage_vectors(traces, all_actions):
  uncovered = set(all_actions)
  vectors = []
  for trace in traces ordered by shortest-first:
    if trace covers any uncovered action:
      vectors.push(trace_to_vector(trace))
      uncovered -= actions_in(trace)
  return vectors
```

### 6.2 guard coverage 戦略

- guard ごとに true/false の両方を観測した trace を選ぶ。
- false 側は action 不成立を証明する trace fragment で良い。

JSON metadata 例:

```json
{
  "coverage_targets": [
    { "kind": "guard", "id": "G_LOCKED_FALSE", "polarity": "true" },
    { "kind": "guard", "id": "G_LOCKED_FALSE", "polarity": "false" }
  ]
}
```

### 6.3 boundary 戦略

- bounded int に対して `min`, `min+1`, `max-1`, `max` を優先する。
- range が小さい場合は全値列挙でもよい。

## 7. E-4 Trace Minimization

### 7.1 目的関数

MVPの目的関数優先順位:

1. 同じ property failure を再現
2. action 数最小
3. state diff の複雑さ最小
4. 先頭からの安定性維持

### 7.2 擬似コード

```text
function minimize_trace(vector, predicate):
  current = vector
  changed = true
  while changed:
    changed = false
    for each removable_slice in candidate_slices(current):
      candidate = current without removable_slice
      if predicate(candidate) == true:
        current = candidate
        changed = true
        break
  return current
```

### 7.3 predicate

- `reproduces_failure`
- `covers_targets`
- `reaches_terminal_state`

### 7.4 テストケース

| ID | 条件 | 期待結果 |
|---|---|---|
| E4-01 | 冗長stepあり | 短縮される |
| E4-02 | 最小trace | 変更なし |
| E4-03 | coverage目的 | target喪失しない |

## 8. E-5 Test Rendering Modes

### 8.1 file generation

出力先:

- `tests/generated/<vector-id>.rs`

利点:

- PRでレビューしやすい
- 失敗箇所の探索が容易

欠点:

- 生成物管理が必要

### 8.2 include mode

出力先:

- `OUT_DIR/generated_vectors.rs`

利点:

- repoに生成物を残さない

欠点:

- build 依存が増える
- 再現性の理解が少し難しい

## 9. F-1 Contract Snapshot生成

### 9.1 入力対象

contract hash の入力対象:

- generated trait 名
- associated type 名と順序
- method 名
- method 引数型
- return type
- public spec metadata version

入力対象外:

- docstring
- whitespace
- private helper function

### 9.2 JSON例

```json
{
  "schema_version": "1.0.0",
  "contract_id": "counterlock-contract",
  "contract_hash": "sha256:ab9911",
  "traits": [
    {
      "name": "CounterLockContract",
      "associated_types": ["State", "Input", "Observation"],
      "methods": [
        "init() -> State",
        "apply(input: Input) -> Observation"
      ]
    }
  ]
}
```

## 10. F-2 Lock fileフォーマット

### 10.1 JSON例

```json
{
  "schema_version": "1.0.0",
  "generated_at": "2026-03-06T12:00:00Z",
  "entries": [
    {
      "contract_id": "counterlock-contract",
      "contract_hash": "sha256:ab9911",
      "source_hash": "sha256:cc5500"
    }
  ]
}
```

### 10.2 契約

- ファイル名: `valid.lock.json`
- 追記ではなく全再生成
- entries は `contract_id` 昇順

## 11. F-3 Drift出力

### 11.1 JSON例

```json
{
  "schema_version": "1.0.0",
  "status": "drift_detected",
  "contract_id": "counterlock-contract",
  "old_hash": "sha256:ab9911",
  "new_hash": "sha256:dde022",
  "changes": {
    "added_methods": ["snapshot() -> State"],
    "removed_methods": [],
    "changed_signatures": []
  }
}
```

### 11.2 テストケース

| ID | 条件 | 期待結果 |
|---|---|---|
| F3-01 | method追加 | drift_detected |
| F3-02 | signature変更 | drift_detected |
| F3-03 | 非契約変更のみ | no_drift |

## 12. F-4 Document Drift検知

### 12.1 対象

- generated markdown summary
- generated mermaid
- generated schema index

### 12.2 判定

- source hash と generated hash が不一致なら drift
- source 未変更なら generated 未変更が前提

## 13. G-1/G-2/G-3 coverage 指標

### 13.1 Transition Coverage

```text
executed_unique_actions / total_actions
```

### 13.2 Guard Coverage

```text
for each guard:
  true_seen: bool
  false_seen: bool
```

### 13.3 State/Depth Summary

- visited_unique_states
- max_depth
- depth_histogram

## 14. G-4 Coverage Report

### 14.1 JSON例

```json
{
  "schema_version": "1.0.0",
  "model_id": "counterlock",
  "summary": {
    "transition_coverage": 1.0,
    "guard_full_coverage": 0.5,
    "visited_unique_states": 12,
    "max_depth": 4
  },
  "actions": [
    { "action_id": "A_INC", "covered": true, "count": 5 },
    { "action_id": "A_LOCK", "covered": true, "count": 2 }
  ],
  "guards": [
    { "guard_id": "G_NOT_LOCKED", "true_seen": true, "false_seen": false }
  ],
  "depth_histogram": {
    "0": 1,
    "1": 2,
    "2": 4,
    "3": 3,
    "4": 2
  }
}
```

### 14.2 text summary 例

```text
COVERAGE model=counterlock
transition_coverage=100.00%
guard_full_coverage=50.00%
visited_unique_states=12
max_depth=4
uncovered_guards=G_NOT_LOCKED:false
```

## 15. G-5 Coverage Gate Evaluation

### 15.1 判定ルール

- `PASS`: transition coverage >= 1.0 かつ guard full coverage >= 0.8
- `WARN`: transition coverage >= 0.8
- `FAIL`: それ未満

### 15.2 JSON例

```json
{
  "schema_version": "1.0.0",
  "status": "warn",
  "policy_id": "default-mvp-policy",
  "reasons": [
    "guard_full_coverage below threshold"
  ]
}
```

## 16. API例

```rust
pub fn generate_tests(
    vectors: &[TestVector],
    mode: TestRenderingMode,
) -> Result<GeneratedTestBatch, TestRenderError>;

pub fn compute_contract_snapshot(
    metadata: &ContractMetadata,
) -> Result<ContractSnapshot, ContractHashError>;

pub fn compute_coverage(
    model: &ModelIr,
    traces: &[EvidenceTrace],
    vectors: &[TestVector],
) -> Result<CoverageReport, CoverageError>;
```

## 17. 総合テストケース一覧

| ID | 分類 | 目的 | 期待結果 |
|---|---|---|---|
| T-01 | vector | counterexample変換 | vector生成 |
| T-02 | vector | witness変換 | coverage metadata付与 |
| T-03 | render | generated file mode | `tests/generated/*.rs` 生成 |
| T-04 | render | include mode | `OUT_DIR` 生成 |
| T-05 | minimize | 冗長trace短縮 | 短縮成功 |
| T-06 | contract | hash生成 | 決定的hash |
| T-07 | contract | lock mismatch | fail |
| T-08 | contract | doc drift | drift検知 |
| T-09 | coverage | action全踏破 | 100% |
| T-10 | coverage | guard片側未踏破 | warn/fail |

## 18. DDD対応

- `TestVector` は `Evidence Context` の派生 aggregate。
- `ContractSnapshot` は `Integration Context` の契約資産。
- `CoverageReport` は `Verification Context` の read model。

## 19. クリーンアーキテクチャ対応

- vector build / minimize / coverage compute は use case。
- test renderer / lock storage / drift reporter は adapter。
- contract hash の正規化は domain service。

## 20. SSOT対応

- 一次ソースは spec と contract metadata。
- vector、generated tests、lock、coverage report はすべて派生物。
- 派生物には `source_hash` または `contract_hash` を保持する。

## 21. ソルバ対応

- explicit/BMC どちらの trace も `TestVector` に落とせること。
- coverage は trace の由来を問わない。
- solver固有フィールドは `metadata.backend` に隔離し、上位契約へ漏らさない。

## 22. 完了条件

1. `TestVector` schema が固定されている。
2. Rust test 雛形が固定されている。
3. counterexample / transition coverage / guard coverage の仕様が存在する。
4. minimization の目的関数が固定されている。
5. contract hash / lock / drift / coverage report / gate の JSON 例が存在する。
6. 保存方式の判断と理由が文書化されている。

## 23. 結論

本章が固まると、検証結果は「見て終わるログ」ではなく、回帰テスト、契約保護、品質ゲートとして再利用できる。これがないと検証は単発作業で終わるため、MVP以降の価値発現に直結する章である。
