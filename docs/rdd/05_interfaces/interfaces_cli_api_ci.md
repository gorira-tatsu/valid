# 06. Interfaces (CLI / API / CI)

- ドキュメントID: `RDD-0001-06`
- バージョン: `v0.3`
- 目的: CLI、AI API、CIゲートを同一操作モデルの外部インターフェースとして定義する。
- 関連ID:
  - [id_cross_reference.md](../09_reference/id_cross_reference.md)
  - 関連FR: `FR-030`〜`FR-032`, `FR-060`〜`FR-063`, `FR-070`〜`FR-073`
  - 関連Epic: `D-*`, `F-*`, `H-*`
  - 関連仕様: [RDD-0001-12](../08_specs/explicit_engine_and_evidence_specs.md), [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md), [RDD-0001-14](../08_specs/ai_solver_selfcheck_specs.md)

## 1. 本章の位置づけ

本システムの利用面は、CLI、将来のHTTP API、CIジョブの3つに分かれる。しかし内部的には、すべて同じUse Caseを起動しているべきである。CLIだけ独自実装、CIだけ特別ルール、APIだけ別の結果形式、という設計は長期的に破綻する。したがって本章では、入口ごとの差異を最小化し、外部表現だけを変える方針を固定する。

## 2. インターフェース原則

- 外部I/FはDomain語彙をそのまま使う。
- CLIとAPIで結果意味論を変えない。
- CIゲートはCLI/APIの上に載る運用規則であり、独自判定器を持たない。
- JSONはAI運用を前提に安定させる。

## 3. CLI設計

### 3.1 コマンド体系

関連ID: `FR-070`〜`FR-073`, `H-1`〜`H-5`, `PR-09`

- `inspect`
- `check`
- `capabilities`
- `trace`
- `explain`
- `minimize`
- `testgen`
- `coverage`
- `contract`
- `selfcheck`
- `orchestrate`
- 将来: `doc`

### 3.2 CLI責務

関連ID: `NFR-020`, `FR-032`, `FR-072`

CLIの責務は以下に限定する。

- 引数解析
- Use Case呼び出し
- 出力形式選択
- 終了コード決定
- backend adapter 設定の受け渡し

CLIがモデル解釈、独自の判定意味論、独自のバックエンド選択ロジックを持ってはならない。

### 3.3 出力モード

関連ID: `FR-031`, `FR-032`, `FR-063`, `D-2`, `D-3`

- human readable text
- machine readable json
- artifact file emission

### 3.4 終了コード

関連ID: `FR-072`, `NFR-020`, `FR-024`

- `0`: `PASS`
- `1`: `FAIL`
- `2`: `UNKNOWN`
- `3`: `ERROR`

終了コードはCIの最小契約であり、破壊変更を避ける。

## 4. CLIコマンド詳細

### 4.1 `inspect`

関連ID: `FR-070`, `H-1`

目的:

- モデル構造の取得
- 型、Action、Property、REQ対応の一覧

用途:

- AIが変更影響を把握する
- 人間がモデル全体を素早く理解する

### 4.2 `check`

関連ID: `FR-020`〜`FR-024`, `H-2`, `PR-03`

目的:

- Property評価
- Run生成
- 必要に応じてtrace生成

オプション例:

- `--backend`
- `--solver-exec`
- `--solver-arg`
- `--property`
- `--json`
- `--max-states`
- `--max-depth`
- `--time-limit`

`--backend` は `explicit | mock-bmc | sat-varisat | smt-cvc5 | command` を受け付ける。`sat-varisat` は組み込みの pure Rust SAT backend であり、現在は bool 中心の declarative subset を対象とする。`smt-cvc5` と `command` の場合は `--solver-exec` を必須とし、`--solver-arg` は複数回指定できる。

MVP では file path 入力も許可するが、長期的な正規経路は `crate + model_id` による Rust model discovery とする。

Rust で定義された現実的なモデル例は、CLI の組み込みコマンドではなく `examples/` と `tests/` に置く。システム本体は generic な modeling 契約と実行エンジンだけを持つ。

### 4.3 `trace`

関連ID: `FR-031`, `FR-032`, `D-2`, `D-3`, `PR-04`

目的:

- 既存Evidenceの可視化
- text/json/mermaidへの変換

### 4.4 `minimize`

関連ID: `FR-043`, `H-4`, `E-4`

目的:

- trace最小化
- 目的維持のまま短い再現に縮約

### 4.5 `testgen`

関連ID: `FR-040`〜`FR-042`, `H-5`, `E-1`〜`E-3`

目的:

- counterexample/witnessからvector生成
- Rust testコードへの変換

MVP の `strategy` は次を受け付ける。

- `counterexample`
- `transition`
- `witness`

### 4.6 `coverage`

関連ID: `FR-050`〜`FR-053`, `G-1`〜`G-5`

目的:

- モデル実行と生成テスト実行のCoverage集計
- `summary.transition_coverage_percent`
- `summary.guard_full_coverage_percent`
- `depth_histogram`
- 埋め込み `gate` 判定

### 4.7 `contract`

関連ID: `FR-060`, `FR-061`, `F-1`, `F-2`, `F-3`

目的:

- contract hashの確認
- lockとの整合確認
- 更新時の明示的再計算

### 4.8 `doc`

関連ID: `FR-030`, `FR-062`, `F-4`

目的:

- 一次ソースから仕様書/図/派生説明を生成
- CIでドリフトを検出

### 4.9 `orchestrate`

関連ID: `FR-023`, `I-1`, `I-4`

目的:

- 複数 property を `1 property = 1 run` に分解して実行する
- backend adapter を維持したまま property ごとの結果を返す
- explicit と non-explicit backend を同じ top-level 操作で扱う
- 実行で得た trace を集約し、`aggregate_coverage` を返してよい

### 4.10 `capabilities`

関連ID: `FR-071`, `FR-073`, `I-1`

目的:

- backend ごとの capability matrix を取得する
- AI と CI が backend 選択前に機能差分を確認できるようにする
- `command` backend では `solver_executable` と `solver_args[]` の整合を事前に検証する
- `sat-varisat` が現在の preferred SAT backend かどうか、selfcheck readiness、explicit parity readiness を機械可読に返す

Temporal reporting:

- `capabilities.temporal.status` は backend ごとの `complete | bounded | unavailable` を返す
- `capabilities.temporal.semantics` は temporal 判定の意味論を返す
  - `reachable_graph_fixpoint`: explicit backend。到達可能グラフ上で fixpoint として評価する
  - `depth_bounded_search`: BMC 系 backend。指定 horizon 内だけを対象にする
  - `unavailable`: その backend は temporal lowering を提供しない
- `capabilities.temporal.assurance_levels[]` はその backend が返しうる assurance を明示する
- inspect/readiness でも temporal property が存在する場合は backend-by-backend の temporal matrix を含め、`explicit` の complete semantics と `mock-bmc` の bounded-only semantics を混同しない

Backend readiness reporting:

- `capabilities.preferred` は現在の推奨 backend かどうかを返す
- `capabilities.selfcheck_status` は `verified | verifiable | unavailable | unsupported` などの短い状態語で selfcheck readiness を返す
- `capabilities.parity_status` は `reference | ready | experimental | unavailable | unsupported` などの短い状態語で parity の期待レベルを返す
- `sat-varisat` は finite declarative subset での main SAT path とし、`explicit` は broadest fallback かつ parity reference として扱う

## 5. API設計

### 5.1 原則

APIはCLIのラッパではなく、CLIと同じUse Caseを叩く別Adapterである。したがって、CLIで可能な操作はAPIでも可能であり、逆も同様であることが望ましい。

### 5.2 エンドポイント候補

関連ID: `H-1`〜`H-5`, `FR-070`〜`FR-073`

- `POST /inspect`
- `POST /check`
- `POST /capabilities`
- `POST /trace`
- `POST /explain`
- `POST /minimize`
- `POST /testgen`
- `POST /coverage`
- `POST /orchestrate`
- `POST /contract/check`
- `POST /doc/check`

### 5.3 API応答契約

関連ID: `FR-071`, `FR-072`, `NFR-020`

すべての応答に以下を含める。

- `schema_version`
- `request_id`
- `status`
- `error_code`（失敗時）
- `payload`

`check`, `capabilities`, `trace`, `explain`, `minimize`, `testgen`, `coverage`, `orchestrate` は backend 指定を受け付ける。backend が `command` の場合、`solver_executable` と `solver_args[]` を request に含める。

### 5.4 安定性

APIはAIが使う前提であるため、自然言語主体のレスポンスを返さない。説明文は補助情報とし、主情報は構造化データに置く。

## 6. JSONスキーマ設計

### 6.1 共通フィールド

- `schema_version`
- `generated_at`
- `tool_version`
- `correlation_id`

### 6.2 `check_result.json`

関連ID: `FR-032`, `D-2`, `schema.run_result`

- `run_id`
- `model_id`
- `backend`
- `status`
- `limits`
- `stats`
- `properties`

### 6.3 `trace.json`

関連ID: `FR-032`, `D-2`, `schema.evidence_trace`

- `trace_id`
- `kind`
- `steps`
- `hash`
- `source_run_id`

### 6.4 `vector.json`

関連ID: `FR-041`, `E-1`, `schema.test_vector`

- `vector_id`
- `evidence_id`
- `strategy`
- `seed`
- `actions`
- `expected`
- `oracle`

### 6.5 `explain.json`

関連ID: `FR-073`, `H-3`, `schema.ai.explain`

- `evidence_id`
- `primary_causes`
- `involved_vars`
- `repair_hints`
- `confidence`

## 7. エラーI/F

### 7.1 エラーコード体系

- `PARSE_ERROR`
- `TYPE_ERROR`
- `UNSUPPORTED_FEATURE`
- `CONTRACT_MISMATCH`
- `TRACE_REPLAY_ERROR`
- `SOLVER_TIMEOUT`
- `LIMIT_EXCEEDED`
- `UNKNOWN_RESULT`

### 7.2 診断構造

関連ID: `NFR-004`, `NFR-005`, `FR-072`, `FR-073`

すべての失敗応答は `diagnostics` 配列を返す。各要素は [diagnostic_bundle.schema.json](../09_reference/schemas/diagnostic_bundle.schema.json) に従う。

必須項目:

- `error_code`
- `segment`
- `severity`
- `message`

推奨項目:

- `primary_span`
- `conflicts`
- `help`
- `best_practices`

### 7.3 CLI 表示方針

CLI は Rust compiler 風の診断を目標にする。ただし装飾過多にはしない。

- 1行目に失敗要約
- 2行目以降に `segment` と `error_code`
- source span がある場合はその抜粋
- 最後に `help:` と `best practice:` を分けて出す

例:

```text
error: init constraints are unsatisfiable
  segment: engine.init
  code: UNSAT_INIT
  --> specs/counterlock.valid:12:5
   |
12 | init { locked = true && locked = false }
   |      ^ conflicting boolean constraints

help:
  - split init constraints and review each assignment independently
  - remove one side of the contradictory constraint

best practice:
  - keep init blocks minimal and review them before adding properties
```
- `INTERNAL_ERROR`

### 7.2 設計原則

- 例外文言に依存しない。
- 同一事象には同一コードを返す。
- backend固有エラーは共通コードへ正規化し、raw diagnosticsは別添する。

## 8. CIインターフェース設計

### 8.1 CIの位置づけ

CIは別のアプリケーションではない。CLIまたはAPIを用いてUse Caseを定期実行し、結果をマージ可否へ変換する運用層である。

### 8.2 必須ゲート

1. `cargo test`
2. `specforge check`
3. `specforge contract --check`
4. `specforge doc --check`

### 8.3 推奨ゲート

- `specforge coverage`
- `specforge testgen --strategy counterexample`
- nightly `check --backend bmc`

### 8.4 失敗時アーティファクト

CIは以下を必ず保存できるようにする。

- check result JSON
- trace JSON
- vector JSON
- coverage JSON
- generated markdown/mermaid

## 9. CIポリシー

### 9.1 ブランチ種別

- feature branch: 軽量検証
- main branch: 必須ゲート + coverage確認
- nightly: 深い探索 + BMC + extended testgen

### 9.2 マージポリシー

- `FAIL` はマージ不可
- `ERROR` はマージ不可
- `UNKNOWN` はリポジトリポリシーに応じて条件付き
- `CONTRACT_MISMATCH` はマージ不可

### 9.3 緊急時緩和

coverage thresholdなど条件付きゲートのみ緩和可。contract/doc/checkの必須ゲートは緩和しない。

## 10. DDDから見たI/F設計

CLI、API、CIはIntegration Contextに属する。ただし、表示語彙はModeling ContextのUbiquitous Languageを使う。Integration Contextが独自語彙を持つと、AIがモデル概念を直接扱えなくなるためである。

また、CLIコマンド名はDomain Service名やUse Case名と対応していることが望ましい。たとえば `testgen` は `GenerateRegressionTests` に、`contract --check` は `CheckContract` に対応する。

## 11. クリーンアーキテクチャから見たI/F設計

### 11.1 Input Adapter

- CLI argument parser
- JSON request parser
- CI wrapper script

### 11.2 Output Adapter

- Text presenter
- JSON presenter
- Mermaid presenter
- Exit code presenter

### 11.3 Rule

I/F層はDomainの意味を変更してはならない。たとえばCLIの都合で `UNKNOWN` を `0` にするような実装は禁止である。

## 12. SSOTとの接続

I/Fは一次ソースを直接変更しない。変更可能なのはモデル定義経由のみである。`doc` と `contract` は派生物を生成するコマンドであり、人間が派生物を修正して整合性を合わせることを許さない。

また、すべての生成物出力には `source_hash` またはそれを参照するIDを含める。

## 13. ソルバとの接続

I/F層はソルバ固有の挙動を最小限しか見せない。利用者へ見せるのは、共通のstatus、共通のerror code、共通trace形式である。詳細diagnosticsが必要な場合でも、`diagnostics.raw` のような隔離領域へ入れる。

これにより、新しいソルバを追加してもCLI/API/CIの契約を変えずに済む。

## 14. データモデルとの接続

I/Fは以下のIDを外部へ露出する。

- `model_id`
- `run_id`
- `property_id`
- `evidence_id`
- `vector_id`
- `coverage_id`
- `contract_id`

この方針により、外部システムが文字列検索ではなくID参照で追跡できる。

## 15. AIインターフェース特化設計

### 15.1 AIが必要とする情報

- モデル一覧
- 変更影響範囲
- 失敗分類
- 根拠trace
- 修復候補
- 再現seed

### 15.2 AI向け禁止事項

- 不定形ログのみを根拠にした自動修正
- schema_versionなし応答
- request_idなし応答

### 15.3 Explain設計

`explain` は人間向け作文機能ではなく、失敗原因候補の構造化抽出として設計する。自然言語の詳細は補助であり、主役は `primary_causes[]` と `involved_vars[]` である。

## 16. セキュリティとインターフェース

- CLI引数はシェル注入前提で検証する。
- APIは巨大payload制限を持つ。
- JSON生成はエスケープを保証する。
- Mermaidは表示用途であり、コードとして実行されないことを前提に安全化する。

## 17. 可観測性

### 17.1 correlation

CLI、API、CIのいずれでも `correlation_id` を採番する。これにより、1回の検証連鎖をログとアーティファクトで追える。

### 17.2 メトリクス

- command duration
- response size
- failure rate by command
- unknown rate by backend
- artifact generation rate

## 18. 将来拡張

### 18.1 HTTPサービス化

将来、長時間ジョブや分散実行のためにHTTP APIが必要になる可能性がある。その際も、現行のJSON schemaとUse Case対応を維持する。

### 18.2 IDE統合

LSPやエディタ統合を追加しても、実体は `inspect/check/explain` の再利用とする。

### 18.3 GitHub App統合

CI結果をPRコメントへ反映する場合でも、ソースはJSON artifactであるべきで、コメント本文を一次判定に使ってはならない。

## 19. 章固有の受け入れ基準

- CLI/API/CIが同一Use Case群に接続されている。
- 終了コード、error code、JSON schemaが定義済みである。
- AI向け必須情報が構造化されている。
- contract/doc/checkがCI必須ゲートとして定義されている。

## 20. 章固有の運用規約

- 新コマンド追加時はUse Case対応を明記する。
- 新JSONレスポンス追加時はschema versionの影響を明記する。
- CIゲート追加時は緊急時緩和方針も同時に定義する。

## 21. 結論

本章が守ろうとしているのは、入口の多様化と意味論の単一化である。人間がCLIで使う時も、AIがAPIで使う時も、CIがゲートとして使う時も、システムの中で起きていることは同じでなければならない。したがって、CLI/API/CIを別仕様にせず、Use Case駆動の同一契約として維持することが本章の核心である。

## 22. 入出力設計の詳細

### 22.1 Human Readable Output

人間向け出力は短く、決定事項を先に書く。

- status
- target property
- key stats
- evidence availability
- next actionable command

### 22.2 Machine Readable Output

機械向け出力は、途中失敗でもJSON外形を保つ。`payload` が空でも `status` と `error_code` は常に存在する。

### 22.3 Artifact Naming

artifact名はランダムではなく、`run_id` や `evidence_id` に基づく決定的命名を採用する。

## 23. CLI UXポリシー

- 長い説明よりも、次に実行すべきコマンド例を出す。
- `FAIL` 時はtrace locationを明示する。
- `UNKNOWN` 時は主因カテゴリを明示する。
- `ERROR` 時は再試行しても無駄かどうかを示す。

## 24. API信頼性ポリシー

- タイムアウト時も構造化エラーを返す。
- 冪等性がある操作にはrequest hashを持たせる。
- partial successは明示する。

## 25. CIジョブ設計例

### 25.1 Pull Request Job

- parser/typecheck
- explicit check on critical properties
- contract check
- doc drift check

### 25.2 Main Branch Job

- full explicit check
- counterexample testgen validation
- coverage summary

### 25.3 Nightly Job

- deep limits
- BMC backend
- hotspot reports

## 26. DDDから見たI/F運用

InterfaceはIntegration Contextだが、運用上はModeling ContextのレビューとVerification Contextの判定を橋渡しする。たとえばCLIの `inspect` はモデル理解を助けるが、モデルを書き換える責務までは持たない。この役割境界を崩さないことが、AI運用でも重要である。

## 27. クリーンアーキテクチャから見たPresenter設計

Presenterは同じUse Case結果を複数の表示へ変換する層である。Text presenter、JSON presenter、Mermaid presenterは共通のDomain結果を読むだけで、再計算や再判定を行わない。ここで再計算を始めると、表示ごとに意味論が分裂する。

## 28. SSOTから見たI/F運用

I/F層は派生物の生成しか行わない。たとえば `doc` は一次ソースを書き換えず、`contract --update` であっても一次ソースではなくlock派生物のみを更新する。この制約を守ることで、入口ごとの差異がSSOT違反に繋がることを防ぐ。

## 29. ソルバから見たI/F運用

backend固有オプションをCLI/APIで見せる場合でも、共通のRunPlanへ吸収できる形にする。例えば `--solver-extra` のような無秩序なescape hatchは、長期的に契約を壊すため避ける。必要ならbackend別サブ設定を構造化して受け取る。

## 30. 章固有の補足結論

本章は、単なるコマンド一覧やAPI一覧ではなく、外部との約束を定義する章である。ここが揺れるとAIもCIも人間運用も同時に壊れるため、I/Fは利便性よりも安定性を優先する。

## 31. 互換性管理

I/F互換性は段階的に管理する。

- CLI: フラグ変更は非推奨期間を設ける
- API: schema_versionを更新する
- CI: required gate変更時はmigration noteを出す

互換性を軽視すると、AI運用と既存CI設定が同時に壊れる。

## 32. 入力バリデーション戦略

- CLIは型と必須引数を即時検査する
- APIはJSON schema validationを通す
- CI wrapperは環境変数の妥当性を検査する

入力不備の責任境界を入口で止めることが、後段のError減少に直結する。

## 33. ユーザー別I/F最適化

### 人間

- 短い要約
- 次アクションの提示
- Mermaidやtext中心

### AI

- JSON
- error code
- request/correlation id
- explain payload

### CI

- exit code
- artifact path
- threshold evaluation

## 34. 章固有の章末補足

インターフェースは見た目の問題ではない。どの利用者が、どの事実を、どの再現性で取得できるかを決める設計そのものである。

## 35. 実運用例

### 35.1 開発者の典型フロー

1. `inspect` でモデル構造を確認する。
2. `check --property <id>` で対象Propertyだけを実行する。
3. `trace` で反例を読む。
4. `minimize` で短縮する。
5. `testgen --strategy counterexample` で回帰化する。

この流れは人間向けだが、AIにもそのまま適用できる。

### 35.2 AIの典型フロー

1. `inspect` でAction/Property一覧を読む。
2. `check --json` で結果を得る。
3. `explain` で根拠候補を取得する。
4. `minimize` と `testgen` を順に実行する。
5. contract/doc driftの有無を確認して変更提案を作る。

### 35.3 CIの典型フロー

1. `check` 実行
2. `contract --check`
3. `doc --check`
4. `coverage`
5. 失敗時artifact upload

## 36. API進化方針

APIは新機能追加よりも破壊的変更回避を優先する。新情報は既存フィールドの意味変更ではなく、新フィールド追加または新エンドポイント追加で表現する。これにより、AIエージェントの実装を長寿命化する。

## 37. CIガバナンス

CI設定自体も管理対象とする。

- required jobs一覧を文書化する
- job timeoutを管理する
- artifact retentionを管理する
- 緊急緩和時は期限を定める

## 38. Adapter責務の再確認

CLI/API/CI wrapperは「どのUse Caseをどう呼ぶか」を決めるだけで、ドメイン判断をしない。たとえば、coverage thresholdのしきい値計算はUse Caseにあり、CI wrapperはその結果に従って失敗させるだけである。

## 39. 互換性テスト

本章のI/Fは専用の互換性試験を持つ。

- golden CLI output test
- JSON schema validation test
- exit code contract test
- artifact naming test

## 40. 章固有の補足結論2

外部I/Fはしばしば軽視されるが、本システムではここがAI運用とCI運用の境界である。したがって、表示やラッパの都合で意味論を揺らさないことが最重要である。

## 41. 章末確認

- 本章は利用者境界の仕様書として維持する。
- 互換性を壊す変更には移行手順を必須とする。
- CLI、API、CI は同一意味論に従う。
