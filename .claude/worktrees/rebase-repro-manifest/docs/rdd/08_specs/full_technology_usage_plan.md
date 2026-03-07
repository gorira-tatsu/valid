# 11. Full Technology Usage Plan

- ドキュメントID: `RDD-0001-11`
- バージョン: `v0.1`
- 目的: 本プロジェクトで使用する技術を一括整理し、それぞれを何のために使うか、どの層で使うか、どの段階で導入するかを定義する。

## 1. 本章の位置づけ

既存の章では、要件、設計、データモデル、研究背景、機能分解、MVP詳細まで整理した。本章はそれらを横断して、技術選択そのものを「実装計画」として定義する章である。

ここでいう技術には、言語、ランタイム、マクロ、IR、シリアライズ、可視化、テスト生成、外部ソルバ、CI、AI API、監査、将来のselfcheckまで含む。目的は、技術選択を暗黙知にせず、「どの技術をどこで使うか」「何には使わないか」を固定することにある。

## 2. 技術選択の原則

- 一次ソースはRust中心に保つ。
- 検証意味論は統一IRへ落とす。
- 外部ソルバはAdapter経由で使う。
- 証拠traceを最重要成果物として扱う。
- AI向けの機械可読性を優先する。
- 生成物はSSOT規約に従い派生物として扱う。

## 3. 言語と実装基盤

### 3.1 Rust

用途:

- コア実装言語
- kernel実装
- frontend実装
- engine実装
- testgen実装
- CLI/API実装

採用理由:

- 型システム
- 所有権/借用の安全性
- 既存ツールチェーン
- 開発対象と同一言語に寄せられる

使わないこと:

- Rust全意味論の完全検証を初期から背負うこと
- 任意Rust関数を無制約にsolverへ落とすこと

### 3.2 Rust Trait

用途:

- 実装境界の契約
- `VerifiedMachine` のようなモデル埋め込みI/F
- Adapter契約
- Repository契約

採用理由:

- 同一性問題の低減
- contract hashの計算対象

### 3.3 Rust Proc Macro / Declarative Macro

用途:

- モデル定義の埋め込み
- trait/IR/メタデータ生成
- 契約スナップショット生成補助

採用理由:

- Rust記法上で仕様を扱える
- 一次ソースを言語内に置ける

注意点:

- 全体探索をmacro内でやらない
- 長時間処理はbuild/CLIへ逃がす

## 4. フロントエンド技術

### 4.1 Parser

用途:

- モデル構文をAST化する

必要能力:

- span保持
- recoverable error
- comment/doc抽出

### 4.2 Name Resolver

用途:

- symbol table生成
- 参照整合チェック

### 4.3 Type Checker

用途:

- bounded int、bool、enum、structの型整合性を保証する

### 4.4 Lowering

用途:

- frontend ASTからbackend中立IRを生成する

## 5. 中間表現（IR）

### 5.1 Model IR

用途:

- frontendとengineの境界
- explicitとBMCの共通入力
- trace/reporter/testgenの共通参照元

構成:

- state schema
- init
- facts
- actions
- properties

### 5.2 Expr IR

用途:

- kernel評価
- guard評価
- solver制約生成

要件:

- backend中立
- span参照可能
- 型情報連携可能

### 5.3 Trace IR

用途:

- すべてのbackend結果の正規化先
- replay
- minimize
- testgen
- explain

## 6. カーネル技術

### 6.1 Pure Evaluation Engine

用途:

- 式評価
- guard評価
- 遷移適用

要求:

- 純粋関数
- `unsafe`なし
- I/Oなし

### 6.2 Replay Engine

用途:

- trace再生確認
- FAIL証拠の信頼強化

### 6.3 Minimal Property Checker

用途:

- invariant/deadlock/reachabilityの最小判定
- selfcheckの足場

## 7. Explicit Verification技術

### 7.1 BFS

用途:

- 最短反例探索

### 7.2 DFS

用途:

- メモリ節約探索

### 7.3 State Hashing

用途:

- 訪問済み管理

### 7.4 Predecessor Map

用途:

- trace復元

### 7.5 Limit Controller

用途:

- max states
- max depth
- time limit

## 8. BMC / Solver拡張技術

### 8.1 Bounded Unrolling

用途:

- k-step制約生成

### 8.2 SAT / SMT Backend Adapter

用途:

- 外部solver起動
- assignment収集
- diagnostics取得

### 8.3 Capability Matrix

用途:

- solverごとの対応機能管理

### 8.4 Assignment to Trace

用途:

- solver解を共通traceへ変換

## 9. 外部ソルバ候補

### 9.1 Kani

用途:

- Rust寄りBMC候補
- bounded bug finding

### 9.2 TLA+ ecosystem

用途:

- 並行遷移意味論の参照系
- 将来比較対象

### 9.3 Alloy系

用途:

- witness/counterexample生成思想
- relational/factsの参照

### 9.4 Loom

用途:

- 実装側並行テスト補助

### 9.5 Miri

用途:

- 低レベル動作確認、UB補助

## 10. 出力・可視化技術

### 10.1 JSON

用途:

- AI API
- artifact
- CI連携
- golden test

原則:

- schema_version必須
- 安定キー

### 10.2 Mermaid

用途:

- モデル可視化
- 反例トレース可視化

注意:

- 表示用副本であり正本ではない

### 10.3 Text Reporter

用途:

- 人間向け短い要約

## 11. テスト技術

### 11.1 Unit Test

用途:

- parser/typechecker/evaluator/transition単位検証

### 11.2 Golden Test

用途:

- JSON schema互換
- IR安定性

### 11.3 Regression Test Generation

用途:

- counterexample固定化

### 11.4 Property-style Test

用途:

- determinism
- simultaneous assignment invariants

## 12. 契約管理技術

### 12.1 Contract Hash

用途:

- trait/API変化の検知

### 12.2 Lock File

用途:

- 意図しない契約破壊の防止

### 12.3 Drift Detection

用途:

- generated doc drift
- contract drift

## 13. Coverage技術

### 13.1 Transition Coverage

用途:

- actionの実行率

### 13.2 Guard Coverage

用途:

- 条件分岐の偏り把握

### 13.3 State / Depth Metrics

用途:

- exploration depthの把握

### 13.4 Coverage Gate

用途:

- CIの条件付き品質ゲート

## 14. AIインターフェース技術

### 14.1 Inspect API

用途:

- モデル構造の把握

### 14.2 Check API

用途:

- 判定結果の構造化取得

### 14.3 Explain API

用途:

- 原因候補・修復ヒント提示

### 14.4 Minimize API

用途:

- 短い反例への縮約

### 14.5 Testgen API

用途:

- 修正前に回帰テストを確保

## 15. CI / 運用技術

### 15.1 Cargo Test

用途:

- unit/regression/selfcheckの実行基盤

### 15.2 CI Workflow

用途:

- check/contract/doc/coverageの自動実行

### 15.3 Artifact Store

用途:

- trace/vector/report保存

### 15.4 Nightly Jobs

用途:

- 深い探索
- BMC
- hotspot分析

## 16. SSOTを支える技術

### 16.1 Source Hash

用途:

- 一次ソースと派生物の対応付け

### 16.2 Generated Docs

用途:

- 仕様書の派生生成

### 16.3 Generated Tests

用途:

- Evidence派生物として回帰テストを管理

## 17. データ永続化技術

### 17.1 File-based MVP Storage

用途:

- run, trace, vector, coverage, contractの保存

理由:

- 初期導入容易
- artifactとの親和性

### 17.2 将来のStructured Storage

用途:

- 検索性向上
- 長期監査

ただしMVPでは先送り可能。

## 18. Selfcheck技術

### 18.1 Selfcheck Specs

用途:

- kernel重要関数に対する自己検証仕様

### 18.2 Selfcheck Runner

用途:

- 通常CIと独立した検証系

### 18.3 Selfcheck Reports

用途:

- 通常runと区別された監査対象

## 19. 技術とエピックの対応

- Epic A -> parser, resolver, typechecker, lowering
- Epic B -> pure kernel, replay engine
- Epic C -> bfs/dfs/hash/predecessor
- Epic D -> json/text/mermaid/report
- Epic E -> vector/test rendering/minimizer
- Epic F -> contract hash/lock/drift
- Epic G -> coverage metrics
- Epic H -> inspect/check/explain API
- Epic I -> solver adapters/BMC
- Epic J -> selfcheck

## 20. 技術とフェーズの対応

### Phase 0

- Rust
- parser
- resolver
- typechecker
- IR
- kernel evaluator

### Phase 1

- explicit BFS/DFS
- trace JSON
- text reporter

### Phase 2

- contract hash
- lock file
- Rust macro integration

### Phase 3

- test vector
- Rust test renderer
- coverage

### Phase 4

- solver adapter
- BMC
- assignment -> trace

### Phase 5

- explain expansion
- AI API hardening
- concurrency reduction support

### Phase 6

- selfcheck specs
- selfcheck runner

## 21. 採用しないもの

- 別仕様言語を一次ソースにすること
- backend固有traceを正本にすること
- CIコメントや自然言語ログを監査正本にすること
- solverを直接CLIから叩く設計
- 手動更新前提の派生ドキュメント

## 22. DDD観点の整理

技術はContextに従属する。

- Modeling Context: parser, resolver, typechecker, IR
- Verification Context: kernel, explicit engine, BMC planner
- Evidence Context: trace, vector, minimizer, coverage
- Integration Context: CLI, API, CI, artifact handling

技術選択はこのContext境界を壊さないことが条件である。

## 23. クリーンアーキテクチャ観点の整理

技術は層ごとに配置する。

- Entity層: IR、State/Action/Trace値オブジェクト
- Use Case層: check, minimize, testgen, contract, coverage
- Adapter層: parser adapter, solver adapter, presenter, repository
- Driver層: filesystem, process launch, CI runtime

これにより、技術導入がそのまま責務導入になるのを防ぐ。

## 24. SSOT観点の整理

すべての技術は、一次ソースを増やさない方向で使う。

- Rust macroは一次ソースを増やさない
- JSON/Markdown/Mermaidは派生物
- trace/vectorは証拠派生物
- contract hashは境界の派生物

## 25. ソルバ観点の整理

solverは中核技術ではあるが、プロジェクトの中心ではない。中心は `Model IR -> normalized trace -> testable evidence` の流れである。solverはこの流れの一部を担う交換可能コンポーネントとして扱う。

## 26. 実装タスクへの落とし方

技術使用計画を実装へ落とす時は、次の順で切る。

1. 技術の導入場所を決める
2. その技術が作る成果物を決める
3. 依存する技術を確認する
4. 受け入れ条件を定義する

たとえば `Mermaid` は reporter層でのみ導入し、成果物は派生可視化、依存はtrace/schema、受け入れ条件は表示用文字列生成成功である。

## 27. 次に詳細化すべき対象

本章の次段として、以下を詳細化すると実装へ直結する。

1. `C-1`〜`C-6` と `D-1`〜`D-3`
2. `E-1`〜`E-4`
3. `F-1`〜`F-4`
4. `H-1`〜`H-5`

## 28. 結論

本章の目的は、技術選択を一括で見通せるようにすることである。Rust、IR、kernel、explicit engine、BMC adapter、JSON、Mermaid、testgen、contract、coverage、AI API、CI、selfcheckは、それぞれ独立に選ばれるのではなく、1つの検証運用パイプラインとして配置されるべきである。本章はその全体像を固定する。
