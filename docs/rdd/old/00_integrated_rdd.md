# 統合RDD（Requirements / Design / Research）

- ドキュメントID: `RDD-0001`
- バージョン: `v0.1`
- 作成日: `2026-03-06`
- 対象プロジェクト仮称: `specforge`（最終名称は別途決定）
- 本書の目的: 「Rust記法で埋め込める形式検証エンジン」を、実装可能な要求仕様・設計仕様・運用仕様として一体定義する。

---

## 1. エグゼクティブサマリー

本計画の中核は、**Rust言語資産（型システム、所有権、ビルドエコシステム）を最大利用しつつ、形式検証を開発フローへ常設する**ことである。単なる検証器の実装ではなく、以下を同時に成立させる。

1. 仕様と実装の乖離を構造的に減らす（Rust trait/macroによる同一言語内運用）。
2. 反例・満たす例をトレースとして生成し、Rust unit testへ自動落とし込みする。
3. AIエージェントが主要ユーザーとして扱える、安定JSONベースの検証インターフェースを提供する。
4. 最終段階で、検証エンジン自身の重要部を自己検証可能なセルフホスト構造へ移行する。

この結果、レビュー対象の重心を「コードの局所レビュー」から「モデル/要件レビュー」へ移し、変更速度と安全性を同時に高める。

---

## 2. 背景・課題・機会

### 2.1 背景

- 形式手法は有効だが、現場導入では次の摩擦が大きい。
  - 仕様言語と実装言語の分断。
  - ツールごとの入出力不統一。
  - 反例が読みにくく、回帰テスト化が手作業。
  - CIと運用の接続が弱い。
- Rustでは検証関連ツールが増えているが、用途別に分散し、統合運用は未整備。

### 2.2 現状課題

- 「仕様が正しいか」と「コードが仕様に従うか」の二重管理が発生する。
- 並行遷移バグ・境界条件バグはコードレビューだけで取りこぼしやすい。
- AIに検証を回させるには、機械可読で安定した契約が不足している。

### 2.3 事業機会

- 形式検証を「専門作業」から「CIで継続実行する実務作業」に転換できる。
- AIエージェントの能力を、仕様反復・反例最小化・テスト生成へ活用できる。
- 仕様資産の再利用により、プロダクトライン横断の品質標準化が可能。

---

## 3. ビジョン・ゴール・非ゴール

### 3.1 ビジョン

- **Model-First Engineering**: 人間はモデルをレビューし、AIはモデルを実行・検証・回帰固定する。
- **Evidence-Driven CI**: 反例/証拠トレースを中心に品質ゲートを設計する。
- **Self-Hosting Verification**: 小さな検証カーネルを核に自己検証範囲を拡張する。

### 3.2 ゴール

- G1: Rust記法（trait/macro）でモデル記述と検証実行が可能。
- G2: 明示探索(MVP) + SAT/BMC(拡張)で反例/満たす例生成。
- G3: 反例をRustテストへ自動変換し、`cargo test`で再現可能。
- G4: 仕様・契約・生成物差分をCIゲート化。
- G5: AI操作前提の安定JSON APIを提供。

### 3.3 非ゴール（初期）

- Rust全言語機能の完全意味論を初期から内包しない。
- いきなり完全証明系（定理証明主体）へ全振りしない。
- すべてのバックエンドを自前実装しない（まずは統合と証拠検証を優先）。

---

## 4. ステークホルダーと責務

- プロダクトオーナー: 要件優先順位、KPI承認。
- モデルレビュアー（人間）: REQ-IDとプロパティ妥当性の審査。
- 実装者: trait準拠実装、アダプタ提供。
- AIエージェント: 検証実行、反例最小化、テスト生成、差分説明。
- CI運用者: ゲート設定、閾値管理、再現性担保。

---

## 5. ドメイン定義（ビジネスロジック）

### 5.1 ドメイン概念

- Requirement（要件）: `REQ-XXX`。自然言語要求。
- Model: 状態、遷移、制約、性質の形式表現。
- Property: 安全性/到達性/ライブネス等の判定対象。
- Evidence: 反例または満たす例のトレース。
- Test Vector: トレースを再生可能な中間表現。
- Contract: Rust trait/API境界の検証契約。

### 5.2 ビジネスルール

- BR-001: すべての `REQ-ID` は少なくとも1つの `Property` にマッピングされる。
- BR-002: `Property` は少なくとも1つの検証実行結果を持つ。
- BR-003: `FAIL` の場合、反例トレース保存は必須。
- BR-004: 反例が修正された場合、回帰テスト生成を必須とする。
- BR-005: 契約ハッシュ変更時、`lock`更新なしのマージは禁止。
- BR-006: `UNKNOWN` は `PASS` と同等扱いにしない。
- BR-007: カバレッジ閾値未達は品質警告または失敗（環境別ポリシー）。

### 5.3 判定状態

- `PASS`: 与えた探索条件下で性質成立。
- `FAIL`: 反例あり。
- `UNKNOWN`: 制限到達（時間/状態数/深さ）または未サポート構文。
- `ERROR`: パース/型/内部例外等の実行失敗。

---

## 6. 機能要件（Functional Requirements）

### 6.1 モデル記述

- FR-001: 状態変数（Bool, Enum, Bounded Int, 構造体）宣言。
- FR-002: 初期条件（複数初期状態含む）記述。
- FR-003: 遷移（guard + 同時代入）記述。
- FR-004: 不変条件、到達条件、デッドロック性記述。
- FR-005: アクションに `reads/writes`（または`modifies`）指定。

### 6.2 Rust埋め込み

- FR-010: `Finite` deriveにより状態/入力列挙。
- FR-011: `VerifiedMachine` trait経由で検証可能。
- FR-012: macroでモデル定義からtrait/IR/テストを自動生成。
- FR-013: `debug_assertions` 下で軽量不変条件チェックを実行。
- FR-014: releaseビルドで検証ランタイムを除外可能。

### 6.3 検証実行

- FR-020: 明示探索（BFS/DFS）を提供。
- FR-021: 反例トレース復元（最短優先はBFS時）。
- FR-022: 実行上限制御（`max_states`, `max_depth`, `time_limit`）。
- FR-023: SAT/BMCバックエンド（拡張）を提供。
- FR-024: `UNKNOWN` を明示返却。

### 6.4 可視化と説明

- FR-030: Mermaid（モデル図/反例トレース）出力。
- FR-031: テキスト説明（差分、ステップ、有効遷移）出力。
- FR-032: JSON出力（機械連携）。

### 6.5 テスト生成

- FR-040: 反例 -> Rust回帰テスト生成。
- FR-041: test vector JSON生成。
- FR-042: カバレッジ指向テスト生成（遷移・ガード・境界）。
- FR-043: トレース最小化（削減後も再現条件維持）。

### 6.6 カバレッジ

- FR-050: Transition coverage。
- FR-051: Guard coverage。
- FR-052: State/Depth統計。
- FR-053: レポートをJSON/テキストで出力。

### 6.7 CI/契約管理

- FR-060: 契約ハッシュ生成/照合。
- FR-061: lockファイル更新なし変更を検知。
- FR-062: 生成ドキュメント差分チェック。
- FR-063: 検証結果・証拠のアーティファクト保存。

### 6.8 AIインターフェース

- FR-070: `inspect` API（モデル自己記述情報）。
- FR-071: 安定JSONスキーマ（バージョニング付き）。
- FR-072: エラー分類コード返却。
- FR-073: `explain` API（原因候補・差分・修復候補）。

---

## 7. 非機能要件（Non-Functional Requirements）

### 7.1 正確性

- NFR-001: 誤PASSの最小化を最優先。
- NFR-002: 証拠再生でFAILを検証カーネルが確認可能。
- NFR-003: UNKNOWN/ERRORの偽装禁止。

### 7.2 性能

- NFR-010: MVPで `10^5~10^6` 状態規模を現実的に処理。
- NFR-011: メモリ・探索速度・枝刈り率を計測。
- NFR-012: カバレッジ計算は検証時間の20%以内を目標。

### 7.3 可用性・運用性

- NFR-020: CLI終了コードを安定化。
- NFR-021: Linux/macOS/Windows対応。
- NFR-022: CI再現性（seed固定、決定的順序）。

### 7.4 セキュリティ

- NFR-030: 不正入力でpanicしない（制御された失敗）。
- NFR-031: Mermaid生成時の文字列エスケープ徹底。
- NFR-032: 外部バックエンド実行をサンドボックス化可能。

### 7.5 保守性

- NFR-040: カーネルを依存最小化（`unsafe`禁止）。
- NFR-041: プラグインバックエンドの追加容易性。
- NFR-042: スキーマ後方互換方針を明文化。

---

## 8. アーキテクチャ設計

### 8.1 論理アーキテクチャ

1. `spec-frontend`
- Rust macro/trait定義を解析しモデルIRを生成。

2. `core-kernel`
- 式評価、遷移適用、証拠トレース再生。

3. `engine-explicit`
- BFS/DFS/枝刈り/反例復元。

4. `engine-bmc`
- 有界展開、SAT/SMT連携、解復元。

5. `orchestrator`
- 実行計画、制限管理、バックエンド選択。

6. `testgen`
- トレースからRustテストおよびvector生成。

7. `coverage`
- モデルカバレッジ集計。

8. `reporter`
- JSON/テキスト/Mermaid出力。

9. `ai-api`
- inspect/check/minimize/testgen/explain API。

### 8.2 依存方向

- `core-kernel` <- `engine-*` <- `orchestrator` <- `cli/api`
- `testgen/coverage/reporter` は `orchestrator` 結果を消費。
- `spec-frontend` は `core` 型のみ参照。

### 8.3 デプロイ形態

- ローカルCLI実行。
- CIジョブ（検証専用ステージ）。
- 将来: サービス化（APIサーバ）対応。

---

## 9. 並行遷移モデル仕様

### 9.1 MVP意味論

- インターリーブ意味論（1ステップ1アクション）。
- `Next = A1 ∨ A2 ∨ ...`。
- 非更新変数はフレーム条件（不変）を暗黙適用。

### 9.2 拡張意味論

- 独立アクション同時実行（オプション）。
- `reads/writes` に基づく衝突検査。
- 部分順序削減(POR)を導入。

### 9.3 ライブネス

- 初期はSafety中心。
- 将来、`G/F/X/U`と公平性制約を段階導入。

---

## 10. データモデル設計

### 10.1 永続化対象

- ModelDefinition
- RequirementMap
- VerificationRun
- PropertyResult
- EvidenceTrace
- TestVector
- CoverageReport
- ContractSnapshot

### 10.2 エンティティ定義（論理）

#### ModelDefinition
- `model_id` (PK)
- `version`
- `source_hash`
- `created_at`
- `schema_version`

#### RequirementMap
- `req_id` (PK)
- `model_id` (FK)
- `property_ids` (array)
- `owner`

#### VerificationRun
- `run_id` (PK)
- `model_id` (FK)
- `backend` (`explicit|bmc|...`)
- `config_json`
- `status` (`PASS|FAIL|UNKNOWN|ERROR`)
- `started_at`, `finished_at`

#### PropertyResult
- `run_id` (FK)
- `property_id`
- `status`
- `message`
- `evidence_id` (nullable)

#### EvidenceTrace
- `evidence_id` (PK)
- `run_id` (FK)
- `kind` (`counterexample|witness`)
- `steps_json`
- `minimized` (bool)

#### TestVector
- `vector_id` (PK)
- `evidence_id` (FK nullable)
- `strategy` (`counterexample|transition|guard|boundary|random`)
- `seed`
- `vector_json`

#### CoverageReport
- `coverage_id` (PK)
- `run_id` (FK)
- `transition_coverage`
- `guard_coverage`
- `state_observed`
- `depth_stats_json`

#### ContractSnapshot
- `contract_id` (PK)
- `hash`
- `lock_version`
- `generated_at`

### 10.3 JSONスキーマ（要点）

#### trace.json
- `trace_id`, `model_id`, `kind`, `steps[]`
- `steps[i]`: `state_before`, `action`, `state_after`, `observed`, `diff`

#### test_vector.json
- `vector_id`, `seed`, `actions[]`, `expected[]`, `oracle`

#### check_result.json
- `run_id`, `status`, `property_results[]`, `limits`, `stats`

#### explain.json
- `primary_cause_candidates[]`, `involved_vars[]`, `repair_hints[]`

---

## 11. API/CLI詳細仕様

### 11.1 CLI

- `specforge inspect <spec>`
- `specforge check <spec> [--backend explicit|bmc]`
- `specforge trace <run_or_evidence> [--format text|json|mermaid]`
- `specforge minimize --trace <trace.json>`
- `specforge testgen <spec> --strategy <...>`
- `specforge coverage <spec> --tests <vectors.json>`
- `specforge contract --check|--update`
- `specforge doc --check|--emit`

### 11.2 終了コード

- `0`: 成功（PASS）
- `1`: 検証失敗（FAIL）
- `2`: UNKNOWN
- `3`: 実行エラー（ERROR）

### 11.3 AI API（将来HTTP化可能）

- `POST /inspect`
- `POST /check`
- `POST /minimize`
- `POST /testgen`
- `POST /coverage`
- `POST /explain`

応答はすべて `schema_version` を含む。

---

## 12. 仕様書運用（Model/Doc一致）

- 仕様を単一ソースとして管理（Rust macroまたはspec block）。
- 生成ドキュメント（Markdown + Mermaid）をCIで整合確認。
- `REQ-ID`単位のトレーサビリティを必須化。
- 仕様差分レビュー時、対応Propertyと影響カバレッジ差分を同時提示。

---

## 13. CI/CD統合

### 13.1 必須パイプライン

1. `cargo test`（通常テスト）
2. `specforge check`（必須プロパティ）
3. `specforge contract --check`
4. `specforge doc --check`
5. 失敗時: 反例アーティファクト保存

### 13.2 推奨パイプライン

- `specforge testgen` + 再実行
- カバレッジ閾値判定
- 夜間ジョブでBMC深掘り（長時間設定）

### 13.3 ブランチポリシー

- `FAIL` はマージ不可。
- `UNKNOWN` は許可条件を明示（例: 実験ブランチのみ）。
- 契約ハッシュ差分はレビュー必須。

---

## 14. セキュリティ・コンプライアンス・監査

- 入力検証: パース段階で構文/型/範囲を厳密検証。
- 実行分離: 外部ソルバ呼び出しの権限制御。
- 監査証跡: run_id単位で設定・結果・証拠を保管。
- 改ざん耐性: 契約ハッシュと出力ハッシュを保存。

---

## 15. 先行研究・既存技術レビュー（一次情報中心）

### 15.1 Alloy / 時相拡張

- Alloy 6では temporal operator（`always`, `eventually`, `after` 等）が導入され、動的仕様記述が容易化。
- 参考:
  - Alloy time docs: https://alloy.readthedocs.io/en/latest/language/time.html
- 示唆:
  - 構造制約 + 時相制約の併用が有効。
  - 本計画では `Facts` と `Properties` を分離してIR化する。

### 15.2 TLA+ / TLC / TLAPS / Apalache

- TLA+は並行・反応系に強い状態遷移記述。
- TLCは有限状態モデルの明示探索に強い。
- TLAPSは機械化証明（モデル検査を超える性質）に対応。
- ApalacheはTLA+向けシンボリックモデルチェッカ。
- 参考:
  - Hyperbook: https://lamport.azurewebsites.net/tla/hyperbook.html
  - TLC tools: https://lamport.org/tla/tools.html
  - TLAPS: https://proofs.tlapl.us/doc/web/content/Home.html
  - Apalache: https://apalache-mc.org/
- 示唆:
  - MVPは明示探索、拡張でシンボリックに移行する二段戦略が妥当。

### 15.3 Rust向け検証ツール群

- Kani: Rust向けbit-precise model checker。
  - https://github.com/model-checking/kani
  - https://model-checking.github.io/kani/kani-tutorial.html
- Prusti: Rust+Viper基盤、仕様注釈を用いた自動検証。
  - https://viperproject.github.io/prusti-dev/user-guide/
  - https://viperproject.github.io/prusti-dev/dev-guide/pipeline/summary.html
- Creusot: Why3連携の演繹的検証。
  - https://github.com/creusot-rs/creusot
  - https://creusot-rs.github.io/creusot/guide/tutorial.html
- Verus: Rust向けSMTベース検証。
  - https://verus-lang.github.io/verus/guide/
- MIRAI: MIR抽象解釈。
  - https://github.com/facebookexperimental/MIRAI
- Miri: MIRインタプリタ（UB検出等）。
  - https://github.com/rust-lang/miri
- Loom: 並行実行順序テスト。
  - https://github.com/tokio-rs/loom

示唆:
- 「全部自前で解く」より、**統合オーケストレータ + 証拠検証 + テスト化**を中核にする方が実務価値が高い。

### 15.4 基礎研究（方向性の裏付け）

- RustBelt（Rust基盤の形式的土台）。
  - https://plv.mpi-sws.org/rustbelt/popl18/
- Flux（refinement types for Rust）。
  - https://arxiv.org/abs/2207.04034
- Foundational VeriFast（検証結果の証明支援出力）。
  - https://arxiv.org/abs/2601.13727
- 示唆:
  - TCB最小化と「証拠を別カーネルで確認」の戦略は研究潮流と整合。

### 15.5 エコシステム動向

- Rust標準ライブラリ検証コンテスト（複数ツール協調）。
  - https://github.com/model-checking/verify-rust-std
  - https://model-checking.github.io/verify-rust-std/tools.html
- 示唆:
  - ツール多様性を前提にした統合設計は将来互換性が高い。

---

## 16. リスクと対策

### 16.1 主リスク

- R-001: 状態爆発によりUNKNOWN多発。
- R-002: モデル自体の誤り（正しく検証しても意味がない）。
- R-003: 複数バックエンドで結果不整合。
- R-004: AI自動修正がモデルを壊す。

### 16.2 対策

- M-001: 上限制御 + 抽象化 + POR + 最小化導入。
- M-002: `REQ-ID`レビューを人間責務として明確化。
- M-003: 証拠トレースを共通形式で再生検証。
- M-004: AI操作にガードレール（スキーマ検証、契約ロック、CI必須）。

---

## 17. KPI（初期目標）

### 17.1 開発効率KPI

- KPI-001: 反例発見から回帰テスト化まで平均30分以内。
- KPI-002: 仕様変更PRのレビュー時間中央値を30%削減。

### 17.2 品質KPI

- KPI-010: 重要不具合再発率を四半期で50%削減。
- KPI-011: 並行/境界関連バグの本番流出を半減。

### 17.3 検証運用KPI

- KPI-020: 主要モデルのTransition coverage 85%以上。
- KPI-021: Guard coverage 70%以上。
- KPI-022: UNKNOWN率を3リリースで30%削減。

### 17.4 AI運用KPI

- KPI-030: AI実行の再現失敗率（同seedで不一致）1%未満。
- KPI-031: explain応答の修復採用率50%以上。

### 17.5 セルフホストKPI

- KPI-040: カーネル重要関数の自己検証対象率を段階的に拡大。
- KPI-041: 自己検証CIの定常成功率95%以上。

---

## 18. 実装ロードマップ

### Phase 0: 土台
- IR定義、スキーマ定義、最小CLI。

### Phase 1: MVP
- 明示探索、invariant/reachability/deadlock、反例復元、Mermaid。

### Phase 2: Rust統合強化
- trait/macro自動生成、contract lock、debug assertion統合。

### Phase 3: テスト自動化
- 反例テスト化、vector形式、coverage計測。

### Phase 4: BMC/SAT
- bounded展開、例/反例生成、目的駆動testgen。

### Phase 5: 並行最適化
- reads/writes活用、POR、同時実行拡張。

### Phase 6: Self-host Step 1
- カーネル証拠検証の自己適用、CI常設。

---

## 19. 受け入れ基準（Definition of Done）

- DoD-001: `check` が3種性質を判定し、FAILで反例を返す。
- DoD-002: 反例からRustテストを生成し `cargo test` で再現可能。
- DoD-003: `contract --check` がCIで機能し、破壊変更を検知。
- DoD-004: `doc --check` が仕様生成差分を検知。
- DoD-005: JSONスキーマ互換ポリシーが文書化済み。
- DoD-006: 主要モデルでcoverageレポートを出力可能。

---

## 20. 付録: 推奨リポジトリ構成（初期）

```text
/docs/rdd/
  00_integrated_rdd.md
  requirements.md
/src/
  core-kernel/
  engine-explicit/
  engine-bmc/
  spec-frontend/
  orchestrator/
  testgen/
  coverage/
  reporter/
  cli/
  ai-api/
/selfcheck/
  specs/
  ci/
```

---

## 21. 結論

本計画の価値は、形式検証アルゴリズム単体ではなく、**モデル・証拠・テスト・CI・AI操作を単一運用面に統合すること**にある。Rust資産を最大活用しながら、Alloy的な具体例生成能力とTLA+的な遷移系記述能力を同一IR上で運用することで、実務に定着する「検証可能開発基盤」を構築する。

