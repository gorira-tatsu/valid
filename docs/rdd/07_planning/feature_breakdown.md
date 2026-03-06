# 09. Feature Breakdown

- ドキュメントID: `RDD-0001-09`
- バージョン: `v0.1`
- 目的: 実際に何の機能をどの順序で作るかを、実装可能な粒度まで分解する。

## 1. 本章の位置づけ

既存の章では、方針、要件、アーキテクチャ、データモデル、研究背景を整理した。本章はそれらを受けて、「次に何を実装するか」を具体的な機能単位へ落とす章である。対象は、エピック、機能、ユースケース、入出力、依存関係、完了条件である。

本章はタスク管理ツールの代替ではない。ただし、タスクを切る際の最小単位を固定する設計文書として扱う。ここに書かれた機能分解を無視して実装を進めると、境界の崩壊や順序の逆転が起こるため、本章を中間設計の基準文書とする。

## 2. 分解原則

- 機能は必ず1つの主要責務を持つ。
- 各機能は入出力と受け入れ条件を持つ。
- 実装順序は依存関係に従う。
- フェーズ後半の機能は、前段で得られる成果物を前提にする。
- 生成物や証拠のない機能は後回しにする。

## 3. エピック一覧

- Epic A: Modeling Frontend
- Epic B: Core Kernel
- Epic C: Explicit Verification Engine
- Epic D: Evidence and Reporting
- Epic E: Test Generation
- Epic F: Contract and Drift Management
- Epic G: Coverage
- Epic H: AI Interface
- Epic I: Solver Expansion
- Epic J: Selfcheck

## 4. Epic A: Modeling Frontend

### A-1 モデルソース読込

目的:

- モデル定義ファイルまたはRust macro入力を読み込む。

入力:

- spec source

出力:

- raw syntax tree

完了条件:

- 単純な状態宣言とaction宣言をパースできる。

### A-2 名前解決

目的:

- state/action/propertyの参照整合性を保証する。

入力:

- raw syntax tree

出力:

- resolved syntax tree

失敗条件:

- 未定義参照
- 重複定義

### A-3 型付け

目的:

- bounded int, enum, bool, structの型整合を保証する。

出力:

- typed model

### A-4 IR生成

目的:

- frontend表現から統一IRを生成する。

出力:

- Model IR

依存:

- A-1, A-2, A-3

### A-5 モデル検証

目的:

- 到達不能action、未使用property、unsat init候補などを静的検査する。

出力:

- model validation report

## 5. Epic B: Core Kernel

### B-1 式評価器

目的:

- bool/int/enum比較、論理演算、単純算術を純粋関数で評価する。

入力:

- expression
- state

出力:

- value

### B-2 ガード評価

目的:

- actionのguardを評価し、enabled/disabledを返す。

### B-3 遷移適用

目的:

- 同時代入規則に従って次状態を構築する。

### B-4 Property評価

目的:

- invariant/reachability/deadlockの最小評価ロジックを持つ。

### B-5 Trace Replay

目的:

- traceを再生し、FAIL/Witnessの整合性を確認する。

これはSelfcheck以前に必要な中核機能である。

## 6. Epic C: Explicit Verification Engine

### C-1 初期状態列挙

目的:

- initから探索開始点を生成する。

### C-2 BFS探索

目的:

- 最短反例復元に向く探索を提供する。

### C-3 DFS探索

目的:

- メモリ節約用途の探索を提供する。

### C-4 訪問済み状態管理

目的:

- 状態ハッシュにより再訪問を避ける。

### C-5 predecessor記録

目的:

- FAIL時にtrace復元可能にする。

### C-6 上限制御

目的:

- max states / depth / timeを執行する。

### C-7 実行統計

目的:

- 状態数、遷移数、深さなどをRun statsへ保存する。

## 7. Epic D: Evidence and Reporting

### D-1 Evidence生成

目的:

- FAILまたはwitnessからEvidenceTraceを生成する。

### D-2 Trace JSON出力

目的:

- 共通trace schemaへシリアライズする。

### D-3 テキスト要約

目的:

- 人間向けに最小情報を表示する。

### D-4 Mermaid生成

目的:

- stateDiagram/sequenceDiagramを生成する。

### D-5 Explain基礎

目的:

- involved vars、failure step、possible causesを返す。

## 8. Epic E: Test Generation

### E-1 Counterexample to Vector

目的:

- 反例traceをvectorへ変換する。

### E-2 Vector to Rust Test

目的:

- Rust unit testコードを生成する。

### E-3 Witness Test Generation

目的:

- 成功トレースやcoverage目的のvectorを生成する。

### E-4 Trace Minimization

目的:

- shorter reproducerを作る。

### E-5 Test Rendering Modes

目的:

- file generation
- include-based generation

の両方に対応する。

## 9. Epic F: Contract and Drift Management

### F-1 Contract Snapshot生成

目的:

- trait/API境界のハッシュを計算する。

### F-2 Lock照合

目的:

- 現在契約と保存済みlockを比較する。

### F-3 Drift出力

目的:

- 差分理由をJSONとtextで返す。

### F-4 Document Drift検知

目的:

- generated docと一次ソースの不整合を見つける。

## 10. Epic G: Coverage

### G-1 Transition Coverage

目的:

- action単位の実行率を測る。

### G-2 Guard Coverage

目的:

- 真偽両分岐の実行状況を測る。

### G-3 State/Depth Summary

目的:

- 探索深さ分布と状態観測量を保存する。

### G-4 Coverage Report

目的:

- JSON/textでレポート化する。

### G-5 Coverage Gate Evaluation

目的:

- 閾値と比較してpass/warn/failを返す。

## 11. Epic H: AI Interface

### H-1 Inspect API

目的:

- AIがモデル構造を読む。

### H-2 Check API

目的:

- AIがcheckを構造化結果で受ける。

### H-3 Explain API

目的:

- AIが原因候補と修復ヒントを受ける。

### H-4 Minimize API

目的:

- AIが反例を短くできる。

### H-5 Testgen API

目的:

- AIが修正前に回帰テストを確保する。

## 12. Epic I: Solver Expansion

### I-1 Solver Adapter Interface

目的:

- backend能力と結果形式を共通化する。

### I-2 BMC Run Plan

目的:

- bounded check用の計画を作る。

### I-3 Assignment to Trace

目的:

- solver結果をEvidenceTraceへ変換する。

### I-4 Capability Matrix

目的:

- backendごとの対応機能を保持する。

## 13. Epic J: Selfcheck

### J-1 Selfcheck Spec群

目的:

- kernelの重要関数を検証するspecを管理する。

### J-2 Selfcheck Runner

目的:

- CIで自己検証を走らせる。

### J-3 Selfcheck Report

目的:

- 通常runと区別できる形で結果を保存する。

## 14. フェーズ別の機能作成順

### Phase 0

- A-1
- A-2
- A-3
- A-4
- B-1
- B-2
- B-3

### Phase 1

- B-4
- C-1
- C-2
- C-4
- C-5
- C-6
- D-1
- D-2
- D-3

### Phase 2

- F-1
- F-2
- F-4
- H-1
- H-2

### Phase 3

- E-1
- E-2
- E-4
- G-1
- G-2
- G-4

### Phase 4

- I-1
- I-2
- I-3
- I-4
- E-3

### Phase 5

- G-3
- G-5
- H-3
- H-4
- H-5

### Phase 6

- J-1
- J-2
- J-3

## 15. 依存関係

- A系が終わらないとB/Cに進めない。
- B-5がないとD-1以降の証拠運用が不完全。
- D-1/D-2がないとE系へ進めない。
- F系がないとAI自動運用とCI厳格化が危険。
- I系はC/D/Eの共通traceが安定してから入れる。
- J系はB/F/Dが十分安定してから入れる。

## 16. 機能単位の受け入れ基準テンプレート

各機能は以下を満たした時に完了とする。

- 入力が定義されている
- 出力が定義されている
- 失敗条件が定義されている
- 主要パスに少なくとも1つの自動テストがある
- 関連するIDまたはartifactが追跡可能

## 17. DDD対応

この機能分解はContext境界を跨いでいないことが前提である。

- A系はModeling Context中心
- B/C/I系はVerification Context中心
- D/E/G/J系はEvidence Context中心
- F/H/CI連携はIntegration Context中心

## 18. クリーンアーキテクチャ対応

各機能は次のどこに属するかを明示する。

- Entity強化
- Use Case追加
- Adapter追加
- Driver追加

たとえば `I-1 Solver Adapter Interface` はAdapter層、`B-3 遷移適用` はEntity/Kernel層、`E-2 Vector to Rust Test` はAdapter寄りのUse Caseである。

## 19. STO/SSOT対応

機能追加時に守るべき原則:

- 一次ソースを増やさない
- 派生物は常にsource hashへ紐づける
- lock/doc/vectorは一次ソースから生成する

## 20. ソルバ対応

機能分解上、ソルバ依存は `I` 系へ閉じ込める。`C` 系はあくまで自前のexplicit、`I` 系は外部solver拡張として整理する。これにより、solverが増えても全体計画が壊れない。

## 21. 今すぐ切るべき実装単位

最初のPR群は以下に分けるのが妥当。

1. `A-1`〜`A-4`: frontend skeleton
2. `B-1`〜`B-3`: kernel evaluator
3. `C-1`〜`C-6`: explicit engine MVP
4. `D-1`〜`D-3`: evidence/report MVP
5. `F-1`〜`F-2`: contract baseline

## 22. この章の使い方

新しい実装を始める時は、まずこの章で該当機能を探し、なければエピックと依存関係を明示して追加する。いきなりタスク管理ツールへだけ機能を書き足さない。本章を更新することで、設計・実装・運用が同じ分解を参照できるようにする。

## 23. 結論

本章でやりたいのは、抽象的な要件を「実際に何を作るか」へ落とすことである。今後はこの章をベースに、各機能をさらに `API契約`, `入出力例`, `エラーパターン`, `テスト戦略` まで掘り下げていくのが自然な次の段階になる。
