# 02. Functional Requirements

- ドキュメントID: `RDD-0001-02`
- バージョン: `v0.2`
- 目的: 機能要件を実装可能な粒度で定義し、DDD/クリーンアーキテクチャ/SSOT/ソルバ統合との対応を明示する。

## 1. 機能要件設計ポリシー

- 要件は実装単位（Use Case）へ対応づける。
- すべての要件は入力・処理・出力・失敗条件を定義する。
- `REQ-ID` との追跡可能性を必須化する。
- FAIL/UNKNOWN/ERRORを判定系として明確に分離する。

## 1.1 上流REQマッピング

この章は `FR-*` を定義する章だが、上流要求は [requirements_catalog.md](../11_requirements_catalog/requirements_catalog.md) にある `REQ-*` を一次ソースとする。以下を固定マッピングとし、以後の変更は `REQ-*` を先に更新してから `FR-*` へ反映する。

| REQ | 内容 | 対応FR |
| --- | --- | --- |
| `REQ-001` | モデルは有限状態として記述できること | `FR-001`〜`FR-005`, `FR-010`, `FR-012` |
| `REQ-002` | PASS / FAIL / UNKNOWN / ERROR を明確に区別すること | `FR-020`〜`FR-024`, `FR-072` |
| `REQ-003` | FAIL は replay 可能な証拠を持つこと | `FR-021`, `FR-030`, `FR-031`, `FR-032` |
| `REQ-004` | 反例は回帰テストへ変換できること | `FR-040`, `FR-041`, `FR-043` |
| `REQ-005` | coverage を測れること | `FR-042`, `FR-050`〜`FR-053` |
| `REQ-006` | Rust 実装境界の契約変更を検知できること | `FR-060`〜`FR-063` |
| `REQ-007` | AI が inspect / check / explain / minimize / testgen を機械可読に扱えること | `FR-070`〜`FR-073` |
| `REQ-008` | backend 追加時も上位契約が壊れないこと | `FR-023`, `FR-071` |
| `REQ-009` | kernel の重要性質を自己検証できること | `FR-011`, `FR-073` |
| `REQ-010` | 文書、schema、artifact の真実源が一貫していること | `FR-032`, `FR-062`, `FR-063` |

## 2. モデル記述要件

### FR-001 状態宣言
- 関連ID: `REQ-001`, `A-1`, `A-3`, `A-4`, [RDD-0001-10](../08_specs/mvp_frontend_and_kernel_specs.md), `PR-01`
- 入力: 状態変数定義（Bool, Enum, Bounded Int, Struct, 将来Set/Rel）。
- 処理: 型検査、有限性確認。
- 出力: 内部StateSchema。
- 失敗: 型不整合、無限型、重複名。

### FR-002 初期条件
- 関連ID: `REQ-001`, `A-1`, `A-4`, `C-1`, [RDD-0001-10](../08_specs/mvp_frontend_and_kernel_specs.md), [RDD-0001-12](../08_specs/explicit_engine_and_evidence_specs.md), `PR-01`, `PR-03`
- 複数初期状態を許容。
- 初期条件充足不能の場合はERRORではなく`UNSAT_INIT`として明示。

### FR-003 遷移定義
- 関連ID: `REQ-001`, `A-1`, `A-3`, `A-4`, `B-3`, [RDD-0001-10](../08_specs/mvp_frontend_and_kernel_specs.md), `PR-01`, `PR-02`
- guard + 同時代入を必須。
- 未更新変数はフレーム条件で保持。
- 同一遷移内の二重代入は禁止。

### FR-004 性質定義
- 関連ID: `REQ-001`, `REQ-003`, `A-1`, `A-4`, `C-2`, `D-1`, [RDD-0001-10](../08_specs/mvp_frontend_and_kernel_specs.md), [RDD-0001-12](../08_specs/explicit_engine_and_evidence_specs.md), `PR-01`, `PR-03`, `PR-04`
- invariant/reachability/deadlockはMVP必須。
- propertyはIDと説明文を持つ。

### FR-005 依存情報
- 関連ID: `REQ-001`, `A-4`, `E-3`, `G-2`, `H-3`, [RDD-0001-10](../08_specs/mvp_frontend_and_kernel_specs.md), [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md), [RDD-0001-14](../08_specs/ai_solver_selfcheck_specs.md)
- actionごとに`reads/writes`を持つ。
- 将来POR、影響分析、explain生成で利用する。

## 3. Rust埋め込み要件

### FR-010 `Finite` derive
- 関連ID: `REQ-001`, `A-3`, `A-4`, `Phase 2`, [RDD-0001-10](../08_specs/mvp_frontend_and_kernel_specs.md), [RDD-0001-14](../08_specs/ai_solver_selfcheck_specs.md)
- enum/struct/boundedに対応。
- 列挙順序は決定的。

### FR-011 `VerifiedMachine` trait
- 関連ID: `REQ-009`, `H-2`, `I-1`, `Phase 2`, [RDD-0001-14](../08_specs/ai_solver_selfcheck_specs.md), `PR-09`, `PR-10`
- `init_states()`
- `step(state,input)`
- `observe(state)`
- `invariants(state)`
- optional: `enabled_inputs(state)`

### FR-012 macro生成
- 関連ID: `REQ-001`, `A-1`〜`A-4`, `F-1`, `Phase 2`, [RDD-0001-10](../08_specs/mvp_frontend_and_kernel_specs.md), [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md)
- モデル定義から次を生成:
  - Contract trait
  - IR
  - 検証テスト雛形
  - メタデータ（action/property/relation）

### FR-013 debug連携
- 関連ID: `REQ-009`, `PR-02`, `Phase 2`, [RDD-0001-10](../08_specs/mvp_frontend_and_kernel_specs.md)
- `debug_assertions`時に軽量検証を実行可能。

### FR-014 release安全性
- 関連ID: `REQ-001`, `NFR-030`〜`NFR-032`, `Phase 2`, [RDD-0001-03](non_functional_requirements.md)
- releaseで検証依存を除去可能。

## 4. 検証実行要件

### FR-020 明示探索
- 関連ID: `REQ-002`, `C-2`, `C-3`, [RDD-0001-12](../08_specs/explicit_engine_and_evidence_specs.md), `PR-03`
- BFS/DFSを選択可能。
- 状態ハッシュで訪問済み管理。

### FR-021 反例復元
- 関連ID: `REQ-002`, `REQ-003`, `C-5`, `D-1`, [RDD-0001-12](../08_specs/explicit_engine_and_evidence_specs.md), `PR-03`, `PR-04`
- predecessor管理で最短トレース（BFS時）を復元。

### FR-022 上限制御
- 関連ID: `REQ-002`, `C-6`, `NFR-010`〜`NFR-012`, [RDD-0001-12](../08_specs/explicit_engine_and_evidence_specs.md), `PR-03`
- `max_states`, `max_depth`, `time_limit`。

### FR-023 BMC拡張
- 関連ID: `REQ-008`, `I-1`, `I-2`, `I-3`, `Phase 4`, [RDD-0001-14](../08_specs/ai_solver_selfcheck_specs.md), `PR-10`
- k-step展開。
- SAT/SMTへの制約変換。

### FR-024 UNKNOWN
- 関連ID: `REQ-002`, `C-6`, `H-2`, `NFR-001`〜`NFR-003`, [RDD-0001-12](../08_specs/explicit_engine_and_evidence_specs.md), [RDD-0001-14](../08_specs/ai_solver_selfcheck_specs.md)
- 制限超過・未対応をUNKNOWN返却。

## 5. 可視化/説明要件

### FR-030 Mermaid
- 関連ID: `REQ-003`, `REQ-010`, `D-3`, `D-4`, `FR-062`, [RDD-0001-12](../08_specs/explicit_engine_and_evidence_specs.md)
- stateDiagram/sequenceDiagram生成。

### FR-031 テキスト説明
- 関連ID: `REQ-003`, `REQ-007`, `D-3`, `H-3`, [RDD-0001-12](../08_specs/explicit_engine_and_evidence_specs.md), [RDD-0001-14](../08_specs/ai_solver_selfcheck_specs.md), `PR-04`
- 差分表示、失敗地点、有効遷移候補。

### FR-032 JSON
- 関連ID: `REQ-003`, `REQ-007`, `REQ-010`, `D-2`, `G-4`, `H-1`〜`H-5`, [json_schemas.md](../09_reference/json_schemas.md), `PR-04`, `PR-08`, `PR-09`
- schema_version必須。

## 6. テスト生成要件

### FR-040 反例->回帰
- 関連ID: `REQ-004`, `E-1`, `E-2`, `PR-06`, [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md)
- 反例トレースからRust testを生成。

### FR-041 vector出力
- 関連ID: `REQ-004`, `E-1`, `E-3`, [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md), `PR-06`
- actions/expected/oracle/seedを保存。

### FR-042 カバレッジ指向
- 関連ID: `REQ-004`, `REQ-005`, `E-3`, `G-1`, `G-2`, `G-5`, `Phase 3`, [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md), `PR-06`, `PR-08`
- transition/guard/boundary/random戦略。

### FR-043 最小化
- 関連ID: `REQ-004`, `REQ-007`, `E-4`, `H-4`, [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md), [RDD-0001-14](../08_specs/ai_solver_selfcheck_specs.md), `PR-06`, `PR-09`
- 目的保持のままtrace縮約。

## 7. カバレッジ要件

### FR-050 transition
- 関連ID: `REQ-005`, `G-1`, `G-4`, `KPI-03`, [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md), [RDD-0001-08](../07_planning/kpi_roadmap_risks.md), `PR-08`
- 実行されたaction数/総action数。

### FR-051 guard
- 関連ID: `REQ-005`, `G-2`, `G-4`, [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md), `PR-08`
- true/false分岐実行率。

### FR-052 state/depth
- 関連ID: `REQ-005`, `C-7`, `G-3`, [RDD-0001-12](../08_specs/explicit_engine_and_evidence_specs.md), [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md), `PR-03`, `PR-08`
- 訪問状態数、深さ分布。

### FR-053 report
- 関連ID: `REQ-005`, `REQ-010`, `G-4`, `G-5`, `FR-032`, [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md), `PR-08`
- JSONと人間可読テキスト。

## 8. CI/契約管理要件

### FR-060 contract hash
- 関連ID: `REQ-006`, `F-1`, `F-2`, `KPI-05`, [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md), [RDD-0001-08](../07_planning/kpi_roadmap_risks.md), `PR-07`
- trait/API変化をハッシュ化。

### FR-061 lock check
- 関連ID: `REQ-006`, `F-2`, `F-3`, [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md), `PR-07`
- `--check` で不一致を失敗扱い。

### FR-062 doc check
- 関連ID: `REQ-010`, `F-4`, `FR-030`, [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md), `PR-07`
- 生成仕様書差分を検知。

### FR-063 artifact
- 関連ID: `REQ-010`, `D-2`, `F-4`, [artifact_naming.md](../09_reference/artifact_naming.md), `PR-04`, `PR-07`
- trace/vector/reportを保存。

## 9. AI API要件

### FR-070 inspect
- 関連ID: `REQ-007`, `H-1`, [RDD-0001-14](../08_specs/ai_solver_selfcheck_specs.md), `PR-09`
- モデル構造、型、property一覧を返す。

### FR-071 stable schema
- 関連ID: `REQ-007`, `REQ-008`, `H-2`, `H-5`, `json_schemas`, [json_schemas.md](../09_reference/json_schemas.md), `PR-09`
- 破壊変更はメジャーバージョン。

### FR-072 error code
- 関連ID: `REQ-002`, `REQ-007`, `H-2`, `H-3`, `error_codes`, [error_codes.md](../09_reference/error_codes.md), `PR-09`
- 失敗分類コードを返す。

### FR-073 explain
- 関連ID: `REQ-007`, `REQ-009`, `H-3`, `D-5`, `FR-031`, [RDD-0001-14](../08_specs/ai_solver_selfcheck_specs.md), `PR-09`
- 変数寄与、最小再現、修復ヒント。

## 10. DDD対応

### 10.1 Use Caseマッピング
- DefineModel
- RunVerification
- MinimizeEvidence
- GenerateRegressionTest
- ComputeCoverage
- ValidateContract

### 10.2 Aggregate責務
- ModelAggregate: 仕様整合。
- RunAggregate: 実行整合。
- EvidenceAggregate: 証拠整合。

### 10.3 Domain Service
- PropertyEvaluator
- TraceMinimizer
- CoverageCalculator
- ContractHasher

## 11. クリーンアーキテクチャ対応

- Use Case層にFRを実装。
- Adapter層でCLI/API/Solverを吸収。
- Entity層にState/Action/Traceを定義。
- External層依存はInterface経由。

## 12. SSOT対応

- 一次ソースをモデル定義に固定。
- FRはすべて一次ソースから導出可能にする。
- 生成物の手修正はCIで検知。

## 13. ソルバ対応

- FR-020系はexplicit solverが担当。
- FR-023系はbmc solverが担当。
- いずれも共通trace形式へ正規化。

## 14. データモデル対応（要件観点）

- FR単位でResultエンティティを保持。
- `run_id + property_id` を最小追跡キーとする。
- テスト生成は`evidence_id`を必須参照。

## 15. 受け入れテストテンプレ

- Given: モデル定義。
- When: check実行。
- Then: statusと証拠が整合。

- Given: FAIL結果。
- When: testgen --strategy counterexample。
- Then: cargo testで再現。

## 16. 失敗モード定義

- ParseFailure
- TypeFailure
- InitUnsat
- Timeout
- LimitExceeded
- InternalInvariantViolation

## 17. 変更影響分析

- action追加: coverage分母増加。
- property追加: 実行時間増加。
- 型拡張: 状態空間増加。
- guard変更: 到達集合変化。

## 18. 非互換変更ルール

- traitシグネチャ変更。
- JSONキー変更。
- trace形式変更。

これらはメジャー変更として扱う。

## 19. 運用上の優先実装順

1. 明示探索と反例復元。
2. 反例テスト化。
3. contract check。
4. coverage。
5. bmc。

## 20. 補遺A: 要件詳細記述規約

各FRは以下項目を持つ。
- ID
- 目的
- 入力
- 処理
- 出力
- 失敗条件
- 観測可能性
- 追跡ID

## 21. 補遺B: trace->test変換規約

- 1 step = 1 action。
- 期待値はobserveベース。
- 内部状態比較はオプション。
- 不定値は許容しない。

## 22. 補遺C: explain規約

- 主要因候補を最大3件返す。
- 根拠step番号を必須付与。
- 修復ヒントは破壊的変更を推奨しない。

## 23. 補遺D: モデル品質規約

- 死にactionを放置しない。
- 到達不能propertyは警告。
- REQ紐づけのないpropertyは禁止。

## 24. 補遺E: AI安全運用規約

- 自動修正は別ブランチ。
- 署名付き実行ログ。
- 連続失敗時は停止。

## 25. 補遺F: 章内サマリ

本章は機能要件を仕様化した。設計実装時はFR IDをコードコメントやテスト名へ反映し、追跡可能性を保つ。特に反例->テスト化、契約チェック、共通trace正規化は中核機能として優先実装する。


## 26. 詳細仕様: Use Case別I/O定義

### UC-01 DefineModel
- Input: モデル構文木、メタ情報、REQマップ。
- Process: 構文検証、型検証、有限性判定、整合性チェック。
- Output: ModelDefinition、ValidationReport。
- Error: duplicate symbol, unresolved reference, unsupported type。

### UC-02 CheckProperties
- Input: model_id、backend、limits。
- Process: run plan構築、engine実行、property判定。
- Output: VerificationRun、PropertyResult[]。
- Error: backend unavailable, timeout, internal error。

### UC-03 BuildEvidence
- Input: failed property、predecessor map。
- Process: trace復元、正規化、ハッシュ付与。
- Output: EvidenceTrace。

### UC-04 MinimizeEvidence
- Input: evidence trace、goal。
- Process: step削減、再検証、最小化反復。
- Output: minimized trace。

### UC-05 GenerateTests
- Input: evidence/witness、strategy、max_cases。
- Process: vector生成、rust test code生成。
- Output: test files、test vectors。

### UC-06 ComputeCoverage
- Input: run results、vectors。
- Process: action/guard/state集計。
- Output: CoverageReport。

### UC-07 ValidateContract
- Input: generated contract snapshot、lock。
- Process: hash comparison。
- Output: pass/fail。

## 27. 詳細仕様: データ契約

### 27.1 `check_result`契約
- `run_id`: string
- `status`: enum(PASS/FAIL/UNKNOWN/ERROR)
- `backend`: string
- `limits`: object
- `stats`: object
- `properties`: array

### 27.2 `property_result`契約
- `property_id`: string
- `status`: enum
- `message`: string
- `evidence_id`: string|null
- `steps_examined`: number

### 27.3 `trace`契約
- `trace_id`
- `kind`
- `steps[]`
- `hash`
- `schema_version`

### 27.4 `vector`契約
- `vector_id`
- `seed`
- `strategy`
- `actions[]`
- `expected[]`

## 28. 詳細仕様: 戦略別testgen

### 28.1 counterexample
- FAIL traceをそのまま回帰化。
- 最小化優先。

### 28.2 transition coverage
- 未到達action優先で探索。
- case間重複を低減。

### 28.3 guard coverage
- true/falseの不足側を優先。

### 28.4 boundary
- min/max/max-1中心。

### 28.5 random
- seed固定で再現可能。

## 29. 詳細仕様: Explainability

- explainは判定ロジックではなく説明ロジック。
- 原因候補はヒューリスティックでよいが根拠step必須。
- 将来、統計因果や依存グラフ解析へ拡張可能。

## 30. DDD拡張

### 30.1 Repository
- ModelRepository
- RunRepository
- EvidenceRepository
- CoverageRepository
- ContractRepository

### 30.2 Factory
- ModelFactory
- RunPlanFactory
- TestVectorFactory

### 30.3 Policy
- UnknownPolicy
- MergeGatePolicy
- CoverageGatePolicy

## 31. クリーンアーキテクチャ拡張

### 31.1 Input Port
- DefineModelInput
- CheckInput
- MinimizeInput
- TestGenInput

### 31.2 Output Port
- CheckPresenter
- CoveragePresenter
- ExplainPresenter

### 31.3 Adapter
- CLIAdapter
- JsonApiAdapter
- SolverProcessAdapter
- FileStorageAdapter

## 32. SSOT拡張要件

- モデル一次ソースは署名付きメタデータを持つ。
- 生成物は必ず`source_hash`を保持。
- lockは一次ソース由来のみ更新可能。

## 33. ソルバ機能要件詳細

### 33.1 explicit
- deterministic traversal option。
- predecessor capture。
- limit-aware termination。

### 33.2 bmc
- bounded unroll。
- sat assignment to trace conversion。
- unsat core optional。

### 33.3 solver-neutral
- normalized result contract。
- capability declaration（supports_liveness等）。

## 34. エラー処理要件

- エラーは機械可読コード + 人間向け説明。
- stacktrace依存禁止。
- recoverable/unrecoverableを区別。

## 35. ロギング要件

- `run_id`相関ログ。
- action単位のdebugログ（opt-in）。
- solver invocationログ。

## 36. セキュリティ要件（機能寄り）

- 外部ソルバ入力は一時ファイル隔離。
- コマンドインジェクション防止。
- 生成コードへの危険文字列埋め込み対策。

## 37. 監査要件

- run毎の完全再現情報を保存。
- 誰が何を更新したか履歴化。
- 契約変更理由を必須入力。

## 38. 運用要件

- 夜間重検証とPR軽検証を分離。
- UNKNOWN再実行キュー。
- 長時間ジョブの中断再開。

## 39. KPI関連機能要件

- KPI算出用メトリクスをAPIで取得可能。
- fail-to-test conversion timeを計測。
- unknown ratio dashboard用集計を提供。

## 40. テスト要件（製品テスト）

- unit: parser/typechecker/evaluator。
- integration: check->trace->testgen。
- e2e: CI gate simulation。
- golden: JSON schema compatibility。

## 41. 品質ゲート要件

- contract mismatchで即fail。
- fail without evidenceを禁止。
- unknown threshold超過を警告/失敗。

## 42. パフォーマンス機能要件

- 状態ハッシュ最適化。
- trace圧縮保存。
- coverage集計の増分計算。

## 43. 互換性要件

- CLIオプションの後方互換。
- JSONの互換宣言。
- trace readerの旧版対応。

## 44. リリース要件

- schema migration手順。
- deprecation policy。
- long-term support branch方針。

## 45. 章末統合サマリ

本章は、要件を単なる箇条書きではなく、実装と運用へ落とし込むための詳細仕様として定義した。特に、DDD（境界と集約）、クリーンアーキテクチャ（依存逆転）、SSOT（一次ソース固定）、ソルバ中立（IR正規化）を機能要件に直接埋め込み、後工程で抜け漏れが発生しない構造にしている。

## 46. 追加補遺

- FRの追加は既存IDの再利用禁止。
- 章内で重複する要件は統合し、派生要件として管理。
- 実装時はFR-IDをモジュールdocに残す。

## 47. 追加補遺2

- すべてのFRは「観測可能な完了条件」を持つ。
- 観測不能要件は品質要件に移管する。
- 責務が曖昧なFRは採用しない。

## 48. 追加補遺3

- 将来の形式仕様拡張（LTL、公平性、抽象解釈）に備え、FR-023系列の拡張点を維持する。
- 既存FRとの整合確認を必須化する。


## 49. 追加補遺4: 要件とデータモデルの相互制約

- FR-040/041はEvidenceTraceとTestVectorの存在を前提とするため、該当エンティティが未永続化の場合は要件未達と判定する。
- FR-060/061はContractSnapshotがrunと独立して更新される設計を前提とする。
- FR-053はCoverageReportを過去runと比較可能な構造で保持することを前提とする。

## 50. 追加補遺5: 章内DoD

- FR一覧が実装項目と1対1で追跡可能。
- すべての要件に失敗時挙動が定義済み。
- DDD/CA/SSOT/Solverとの対応節が存在。

## 51. 追加補遺6: 要件レビュー手順

1. REQ-IDに紐づくFRを列挙。
2. 影響するエンティティを列挙。
3. Use Case境界で変更影響を確認。
4. Adapter層への波及有無を確認。
5. ソルバ出力契約互換を確認。

## 52. 追加補遺7: 実装前チェック

- パーサ仕様が要件を満たすか。
- 型系がboundedを扱えるか。
- traceフォーマットがtestgen要件を満たすか。
- CIでcontract/doc/checkが連結されているか。

## 53. 追加補遺8: 実装後チェック

- FAIL時に必ずtraceが出るか。
- traceから必ずtestを生成できるか。
- 生成testが再現するか。
- unknownの理由が可視化されるか。

## 54. 追加補遺9

本章は要件章であるため、実装詳細は下位章に委譲しつつ、実装が満たすべき契約を十分に固定する。設計変更時は本章FR-IDを基点に差分を管理する。


## 55. 追加補遺10: 具体実装における推奨分割

- `spec_frontend`: FR-001〜014
- `engine_explicit`: FR-020〜024
- `reporter`: FR-030〜032
- `testgen`: FR-040〜043
- `coverage`: FR-050〜053
- `contract`: FR-060〜063
- `ai_api`: FR-070〜073

この分割により責務境界を維持し、DDDのContext分割と一致させる。

## 56. 追加補遺11: 要件の優先度

- P0: FR-001〜024, FR-040, FR-060, FR-061
- P1: FR-030, FR-041, FR-050〜053, FR-070〜073
- P2: FR-042, FR-043, FR-062, FR-063, FR-023拡張

## 57. 追加補遺12

要件の拡張は、既存運用の再現性を壊さないことを条件とする。特にtrace schemaとcontract hashの互換性を維持できない変更は、事前に移行手順を定義しない限り受け入れない。


## 58. 追加補遺13

機能要件は、単に機能一覧を示すだけでなく、将来の技術選択を拘束する。例えばソルバ変更やAI実行方式変更が生じても、FRで定義した出力契約と判定規則を維持することで、上位運用を壊さない。この不変条件を守ることを本章の最終目的とする。


## 59. 追加補遺14

本章のFRは、実装タスク分解、受け入れ試験、CIゲート、監査証跡の基準として兼用できるよう設計している。これにより、仕様から運用までの断絶を抑制する。


## 60. 章末確認

- FR 追加時は `REQ-*` との対応を更新する。
- 互換性規則を変える場合は関連 schema と acceptance 条件を同時に更新する。
- 実装着手時は [implementation_pr_plan.md](../09_reference/implementation_pr_plan.md) と整合していることを確認する。
