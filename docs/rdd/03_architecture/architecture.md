# 04. Architecture

- ドキュメントID: `RDD-0001-04`
- バージョン: `v0.3`
- 目的: 本システムの構造、境界、依存方向、実行時フロー、拡張点を定義する。

## 1. 本章の方針

本システムのアーキテクチャは、検証アルゴリズムの巧妙さよりも、長期運用に耐える責務分離を優先する。形式検証基盤では、要件変更、ソルバ追加、AI連携、CI制約、証拠形式変更が継続的に発生する。したがって、最初から「後で差し替える層」と「絶対に壊したくない層」を明示し、依存方向を固定しなければならない。

本章では、DDDの境界とクリーンアーキテクチャの層を対応づける。目的は、業務意味論、検証意味論、外部技術詳細、運用配線を同じ場所に混ぜないことである。

## 2. アーキテクチャ原則

- 核心意味論は小さく保つ。
- 外部ソルバ、CLI、HTTP、CIはすべて外側である。
- 反例traceは最重要の中間資産である。
- 一次ソースはモデルであり、ドキュメントやテストは派生物である。
- すべてのバックエンド出力は共通IR/共通traceへ正規化する。

## 3. 論理レイヤ

### 3.1 Entities

最内層。純粋なドメイン概念を保持する。

- `StateSchema`
- `ActionSchema`
- `Property`
- `Trace`
- `RunPlan`
- `ContractSnapshot`
- `CoverageSummary`

この層はソルバ名、CLI引数、ファイルパス、環境変数を知らない。

### 3.2 Use Cases

業務手続きを表現する。

- `DefineModel`
- `ValidateModel`
- `BuildRunPlan`
- `ExecuteVerification`
- `ReplayEvidence`
- `MinimizeEvidence`
- `GenerateRegressionTests`
- `ComputeCoverage`
- `CheckContract`
- `BuildDocumentation`
- `ExplainFailure`

Use Caseは、Domain Policyを適用しつつ、複数のEntitiesを組み合わせる。

### 3.3 Interface Adapters

外部入出力とUse Caseの境界を埋める。

- CLI Adapter
- JSON API Adapter
- Solver Adapter
- File Repository Adapter
- Mermaid Renderer
- Rust Test Renderer

### 3.4 Frameworks / Drivers

最外層。環境依存要素。

- OS process
- filesystem
- CI runner
- external solver binary
- future HTTP runtime

## 4. DDDのBounded Context

### 4.1 Modeling Context

責務:

- REQ-IDとPropertyの対応づけ
- 状態/遷移/制約の定義
- モデルの型整合性

このContextの中心は「何を保証したいか」であり、「どう解くか」ではない。

### 4.2 Verification Context

責務:

- 実行計画作成
- backend選択
- 制限適用
- 判定結果生成

このContextはモデルを解く責務を持つが、証拠を人間やCIにどう見せるかまでは持たない。

### 4.3 Evidence Context

責務:

- trace保存
- trace再生
- trace最小化
- vector生成
- 監査追跡

Evidence Contextの中心概念は `EvidenceTrace` であり、これは不変資産として扱う。

### 4.4 Integration Context

責務:

- CLI/API/CI結線
- 外部ソルバ起動
- 生成物の出力
- 環境依存の吸収

## 5. モジュール構成

### 5.1 `spec-frontend`

役割:

- Rust macro/trait入力からモデルを抽出する。
- パース、名前解決、型付けを行う。
- IRを生成する。

責務外:

- 実際の探索
- ソルバ起動
- テスト生成

### 5.2 `core-kernel`

役割:

- 式評価
- ガード評価
- 遷移適用
- trace再生
- 最小限のproperty判定

設計条件:

- `unsafe` 禁止
- I/O禁止
- 時刻/乱数/環境依存禁止

### 5.3 `engine-explicit`

役割:

- BFS/DFS
- predecessor保持
- 訪問済み管理
- 限界到達判定

このモジュールはMVPの主戦力であり、初期価値の大半を担う。

### 5.4 `engine-bmc`

役割:

- bounded unroll
- backend solver制約生成
- assignmentからtraceへの復元

このモジュールは将来拡張だが、アーキテクチャ上は最初から席を用意しておく。

### 5.5 `orchestrator`

役割:

- RunPlan構築
- backend能力と要求の照合
- fallback選択
- 結果正規化

このモジュールを設けることで、上位のCLI/APIがsolver差異を知らずに済む。

### 5.6 `testgen`

役割:

- traceからvector生成
- vectorからRustテスト生成
- カバレッジ目的のテスト集合構成

### 5.7 `coverage`

役割:

- action、guard、state、depthの集計
- run比較
- 閾値判定

### 5.8 `reporter`

役割:

- JSON serialization
- text summary
- Mermaid rendering
- explain payload組立

### 5.9 `ai-api`

役割:

- inspect/check/minimize/testgen/explainの安定契約
- エラー分類の固定
- 将来のHTTP層に対するFacade

## 6. 依存方向

### 6.1 不変ルール

- `core-kernel` は他モジュールへ依存しない。
- `engine-*` は `core-kernel` に依存してよいが、その逆は禁止。
- `orchestrator` は `engine-*` と `reporter` を使ってよい。
- `reporter` は domain型を読めるが、domainの意味を変更してはならない。
- `cli/api` は `orchestrator` を通じてのみ実行する。

### 6.2 禁止依存

- CLIがsolver processを直接起動すること。
- testgenがparser内部型へ依存すること。
- reporterがfilesystemレイアウトを前提にすること。

## 7. 実行時シーケンス

### 7.1 `check`

1. CLI/APIが入力を受ける。
2. `spec-frontend` がモデルIRを構築する。
3. `orchestrator` がRunPlanを作る。
4. 適切な `engine-*` を選択する。
5. engineが結果を返す。
6. `core-kernel` が必要に応じてtrace再生確認を行う。
7. `reporter` がJSON/テキストを返す。

### 7.2 `testgen`

1. 既存Evidenceまたはcheck結果を入力する。
2. `testgen` がvectorを構築する。
3. Rust test rendererがコードを生成する。
4. `reporter` がメタ情報とともに出力する。

### 7.3 `contract --check`

1. モデル/trait由来のContractSnapshotを再計算する。
2. lockファイルと照合する。
3. 差分があれば構造化結果を返す。

## 8. ランタイム境界

### 8.1 同期境界

MVPではCLI実行を同期フローとする。非同期化は将来のAPI層でのみ考慮する。

### 8.2 並行実行

検証対象の並行性と、ツール自身の並行実行は分けて扱う。ツール内部の並行化は最適化であり、意味論ではない。

### 8.3 一時資産

外部ソルバ用一時ファイル、生成物、ログは `Integration Context` の責務で扱い、domain層へパスを漏らさない。

## 9. 状態管理アーキテクチャ

### 9.1 不変データ優先

State、Trace、Vectorは原則不変データとして扱う。理由は再現性と監査性のためである。

### 9.2 ID設計

- `model_id`
- `run_id`
- `evidence_id`
- `vector_id`
- `coverage_id`
- `contract_id`

IDはコンテキスト境界をまたぐ共通キーであり、疎結合のために重要である。

### 9.3 ハッシュ設計

ハッシュは完全性確認のために使う。

- `source_hash`
- `trace_hash`
- `vector_hash`
- `contract_hash`

## 10. エラーアーキテクチャ

### 10.1 Domain Error

- `ModelValidationError`
- `UnsupportedFeature`
- `TraceReplayError`
- `ContractMismatch`
- `UnknownResult`

### 10.2 Adapter Error

- `ProcessSpawnError`
- `SerializationError`
- `FilesystemError`
- `ProtocolMappingError`

Adapter ErrorはDomain Errorへ翻訳されて上位へ上がる。

### 10.3 ユーザー可視エラー

ユーザーへは、機械可読コード + 人間向け短文説明の組で返す。

## 11. STO/SSOTアーキテクチャ

### 11.1 一次ソース

モデル定義を一次ソースとする。仕様書、Mermaid、契約lock、テストベクタは派生物である。

### 11.2 派生物生成

- `doc` で仕様書派生
- `contract` で契約派生
- `testgen` でテスト派生

### 11.3 整合性保持

派生物は `source_hash` を持つ。一次ソースと派生物の差分はCIで検出する。

## 12. ソルバアーキテクチャ

### 12.1 Solver Adapter Contract

各ソルバadapterは以下を満たす。

- capability宣言
- RunPlan入力
- normalized result出力
- diagnostics出力
- trace変換またはtrace非対応宣言

### 12.2 Backend選択ロジック

`orchestrator` が以下を見て選択する。

- property種別
- limits
- backend availability
- policy

### 12.3 Fallback

たとえばBMC未対応環境ではexplicitへ自動フォールバックできる。ただし意味論差異がある場合はユーザーへ明示する。

## 13. DDDから見たアーキテクチャ判断

この設計では、`ModelDefinition` を中心としたモデルの意味と、`VerificationRun` を中心とした実行の意味を別集約として扱う。これは「仕様は長寿命、実行は短寿命」という性質の違いに基づく。Evidenceはさらに別集約で、実行の派生物だが独立に保全される。

この分離により、同じモデルに対する複数run、同じrunからの複数evidence派生、同じevidenceからの複数test vector生成を自然に表現できる。

## 14. クリーンアーキテクチャから見た設計判断

設計上もっとも重要なのは、外部ソルバやCLIの都合でdomainを汚染しないことである。たとえばsolverが返す独自diagnosticsをそのままEntityへ入れると、次のsolver統合で破綻する。したがって、Entityに入れてよいのは共通意味を持つ概念だけであり、solver固有の詳細はAdapterのdiagnostics payloadへ隔離する。

## 15. データモデルとの接続

本章は詳細スキーマを定義しないが、アーキテクチャ上の接続ルールを定める。

- Entity IDは永続化キーに対応する。
- RepositoryはContextごとに分ける。
- traceとvectorはappend-only運用を基本とする。
- contract snapshotはrun lifecycleから分離する。

## 16. AI運用との接続

AIは `ai-api` からしかシステム内部へ触れない前提とする。これにより、AIがsolverやfilesystemの詳細へ依存せずに済む。AI用I/Fで重要なのは、構造化エラー、inspect可能性、再現用パラメータの完全露出である。

## 17. セルフホストへの布石

### 17.1 Kernel最小化

セルフホストでは、最小カーネルが自分自身の一部を検証できる必要がある。そのため、今の時点からkernelへ余計な責務を持ち込まない。

### 17.2 証拠優先

成功証明よりも先に、失敗証拠の再生と検証を固める。これはTCBを縮めるための現実的順序である。

### 17.3 Selfcheck専用境界

自己検証用specやCIは通常利用経路と分離した `selfcheck` 境界に置く。

## 18. 拡張戦略

### 18.1 近接拡張

- coverage指標追加
- explainヒューリスティック改善
- 新しいreport format

### 18.2 中期拡張

- liveness backend
- POR強化
- richer set/relation support

### 18.3 長期拡張

- proof artifact support
- self-host coverage expansion
- distributed verification orchestration

## 19. 章固有の受け入れ基準

- 主要モジュールの責務が重複していない。
- 依存方向が文書として固定されている。
- DDD ContextとLayerが対応づけられている。
- ソルバ統合点がAdapterとして定義されている。
- STOと生成物の経路が明記されている。

## 20. 章固有の管理規約

- 新モジュール追加時はContext所属を明記する。
- 外部依存追加時はAdapter境界を明記する。
- kernel責務を増やす変更はADR必須。
- 依存方向を変更する変更はArchitecture Board承認必須。

## 21. 結論

本章の目的は、実装しやすい構造を示すことではなく、壊れにくい構造を先に固定することである。形式検証基盤は、アルゴリズムの改善よりも境界の崩壊で寿命を失う。したがって、モジュール分割、Context境界、Layer依存、trace中心設計、solver adapter化を最初から明示することが、本システムの中長期価値を決める。

## 22. シーケンス設計の詳細

### 22.1 モデル定義フロー

1. `spec-frontend` がソースを読み取る。
2. 構文木を生成する。
3. 名前解決と型解決を行う。
4. 中間IRへ変換する。
5. ModelAggregateとして保存可能な形へ正規化する。

このフローでは、solverやCIの概念はまだ登場しない。モデリングはモデリングとして閉じる。

### 22.2 検証フロー

1. `orchestrator` がPropertyとPolicyを読み取る。
2. backend能力と要求を照合する。
3. 実行計画を生成する。
4. engineが結果を返す。
5. `core-kernel` が必要に応じてtrace再生確認を行う。
6. `reporter` が整形する。

### 22.3 証拠派生フロー

1. EvidenceTraceを読み取る。
2. 最小化ポリシーを適用する。
3. Vectorを生成する。
4. Rust testへ変換する。
5. CoverageやExplainに利用する。

## 23. 配置アーキテクチャ

### 23.1 開発時

- ローカルCLI
- ローカルartifactディレクトリ
- optional solver binaries

### 23.2 CI時

- ephemeral workspace
- artifact upload
- required backend matrix

### 23.3 将来のサービス時

- API server
- job queue
- artifact store
- optional cache layer

本章ではサービス化を前提にしないが、将来の移行を妨げない設計を取る。

## 24. モジュール間契約

### 24.1 `spec-frontend` -> `orchestrator`

- 完全なIR
- validation diagnostics
- source metadata

### 24.2 `orchestrator` -> `engine`

- RunPlan
- Property selection
- Limits
- backend options

### 24.3 `engine` -> `reporter`

- normalized result
- optional trace
- stats
- diagnostics

### 24.4 `testgen` -> `reporter`

- vector set
- generated files metadata
- oracle description

## 25. アーキテクチャ上の不変条件

- kernelはexternal processを知らない
- reporterは判定結果を変えない
- adapterはdomain IDを生成しない
- orchestratorは一次ソースを書き換えない
- testgenは証拠を破壊しない

## 26. 依存逆転の具体例

例えばKani連携を追加する場合、`engine-bmc-kani-adapter` が `BmcBackend` interface を実装する。Use CaseやCLIは `BmcBackend` のみを見る。この構造なら、将来別のSMT backendを追加しても、同じinterfaceに沿う限り上位は変更しない。

## 27. パフォーマンスとアーキテクチャ

性能最適化は境界を壊さない方法で行う。

- state hash最適化は engine内部で行う
- trace圧縮は reporter/evidence層で行う
- parser高速化は frontend内部で行う

これらを守ることで、性能改善がドメイン意味論変更へ波及しない。

## 28. DDDとモジュールのマッピング

- Modeling Context -> `spec-frontend`, model repository
- Verification Context -> `orchestrator`, `engine-*`
- Evidence Context -> `core-kernel`, `testgen`, evidence repository
- Integration Context -> `cli`, `ai-api`, CI wrappers, renderers

この対応を明確にすることで、「どこで何を変更すべきか」が分かりやすくなる。

## 29. STOの配線

アーキテクチャ上、STOは単なる規約ではなく配線設計として表現される。

- モデル定義からしかDocumentとContractを作れない
- reporterは派生物しか作れない
- 派生物からModelAggregateへ逆流する経路は持たない

この片方向性が、ドキュメントと実装の二重管理を防ぐ。

## 30. セルフホストを見据えた境界縮小

将来selfcheckで使うため、`core-kernel` には「自分自身で評価しやすい」インターフェースを持たせる。具体的には、純粋関数、有限入力、I/O分離、明示的エラーの4条件を守る。これにより、kernel一部の性質を将来自分で検証しやすくなる。

## 31. 章固有の補足結論

本章は、開発者がソースツリーをどう切るかを説明する文書ではない。むしろ、「何を混ぜてはいけないか」を明確化する文書である。検証基盤は、機能を足すことよりも、責務を混ぜないことの方が長期的に重要である。

## 32. 監視アーキテクチャ

アーキテクチャ上、監視も責務分離する。

- engineは生のメトリクスを出す
- orchestratorはrun単位で集約する
- reporterは可視化形式へ変換する

この分離により、監視のためにcoreロジックが汚染されることを防ぐ。

## 33. アーキテクチャ上のホットスポット

将来的に複雑化しやすい箇所を明記する。

- IR互換管理
- trace schema進化
- backend capability matrix
- contract hash生成
- testgen renderer

これらは設計上のホットスポットであり、変更時は局所実装ではなく全体整合を確認する。

## 34. データ流通の原則

同じ情報を複数形式で持つ時は、必ず正本と副本を決める。たとえばtraceはJSONが正本であり、Mermaidは副本である。coverage summaryはJSONが正本で、CIコメントは副本である。この原則により、表示用形式が一次判断に使われる事故を防ぐ。

## 35. 将来の分散実行を見据えた設計

現時点では単一プロセス中心だが、将来分散実行に進む場合でも、RunPlan、normalized result、EvidenceTraceの3点をネットワーク境界で渡せるよう設計する。これにより、分散化が必要になってもUse CaseやEntityを大きく変えずに済む。

## 36. 最終補足

本章は、ソース配置の趣味ではなく、責務境界の契約である。後の性能改善やsolver追加が成功するかどうかは、今ここでどれだけ境界を厳格に引けるかに依存する。

## 37. 章末追記

アーキテクチャは変更の受け皿ではなく、変更の制御機構である。本章で定義した境界は、将来の拡張を速くするためにこそ厳密である必要がある。
