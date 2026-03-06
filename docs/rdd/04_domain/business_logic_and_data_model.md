# 05. Business Logic and Data Model

- ドキュメントID: `RDD-0001-05`
- バージョン: `v0.3`
- 目的: ドメインルール、用語、一貫性条件、永続化対象、集約境界、履歴管理を定義する。

## 1. 本章の位置づけ

本プロジェクトは検証基盤であるが、単なる技術ツールではない。そこには明確なビジネスロジックがある。ここでいうビジネスロジックとは、検証対象の業務機能ではなく、「要件をどうモデル化し、どう判定し、どう証拠として保存し、どう回帰テストへ変換するか」という本基盤自身のドメイン規則である。

このドメイン規則が曖昧だと、仕様と証拠の関係が壊れ、監査も自動化もできない。したがって、本章では検証プラットフォームそのものの業務ルールを定義する。

## 2. ドメインの中核概念

### 2.1 Requirement

自然言語で表現された要求。`REQ-XXX` 形式のIDを持つ。Requirementは単なるメモではなく、少なくとも1つのPropertyに写像されなければならない。

### 2.2 Model

状態、遷移、制約、性質の形式表現。Modelは一次ソースであり、他の成果物の根である。

### 2.3 Property

モデルに対して検証される命題。不変条件、到達性、デッドロック性、将来はライブネス等を含む。

### 2.4 Verification Run

特定のモデルに対し、特定の設定とバックエンドで行われた1回の検証実行。

### 2.5 Evidence

判定の根拠。主に反例トレース、満たす例トレース、診断断片を含む。

### 2.6 Test Vector

Evidenceから派生した、再生可能なテスト用中間表現。

### 2.7 Contract

Rust実装との境界仕様。trait形状、関連型、プロパティ対応、生成契約ハッシュを含む。

### 2.8 Coverage

モデルや生成テストがどこまで仕様空間を踏めているかを示す運用指標。

## 3. Ubiquitous Language

このプロジェクトでは、以下の語を全レイヤで同じ意味で使う。

- `model`
- `property`
- `run`
- `evidence`
- `trace`
- `vector`
- `contract`
- `coverage`
- `unknown`
- `policy`

CLI、JSONキー、ドキュメント、コードコメントで語の意味をずらさない。たとえば `counterexample` を `trace` と混同しない。`trace` は形式、`counterexample` は役割である。

## 4. ドメインルール

### BR-001 Requirement完全対応

すべての `REQ-ID` は少なくとも1つのPropertyへ対応づけられる。

### BR-002 Property実行可能性

すべてのPropertyは少なくとも1つの検証Runで評価可能であること。未評価のPropertyを放置しない。

### BR-003 FAIL証拠必須

`FAIL` はEvidenceなしに成立しない。エラーメッセージのみでの失敗は禁止する。

### BR-004 回帰固定

一度検出された反例は、修正後に回帰テストへ落とすことを原則とする。

### BR-005 UNKNOWN非成功

`UNKNOWN` は成功ではない。CIポリシー上の特例許可がある場合のみ条件付き通過可能とする。

### BR-006 契約破壊は明示

Contract変更はlock更新と理由記録を伴わなければならない。

### BR-007 一次ソース優先

Modelが一次ソースであり、生成ドキュメントや生成テストはそれを上書きしない。

### BR-008 監査追跡

Evidence、Vector、CoverageはRunへ逆参照可能でなければならない。

## 5. Bounded Contextごとの責務

### 5.1 Modeling Contextの業務ルール

- Requirementの語彙をModelへ落とす。
- Propertyに意味的な説明文を持たせる。
- 到達不能Actionや未参照Propertyを品質負債として可視化する。

### 5.2 Verification Contextの業務ルール

- 同一Runでは単一の実行設定を採用する。
- backend差異があっても最終判定形式は統一する。
- limits到達はUNKNOWNで保存する。

### 5.3 Evidence Contextの業務ルール

- Evidenceはappend-only運用を基本とする。
- 最小化Traceは派生物として保存し、元Traceを破壊しない。
- EvidenceからVectorを複数生成してよい。

### 5.4 Integration Contextの業務ルール

- 外部環境差異はRunメタデータとして保存する。
- 生成物の配置やファイル名はDomain IDで紐づける。
- CI判定はRun結果に基づく。

## 6. エンティティ設計

### 6.1 ModelDefinition

属性:

- `model_id`
- `model_version`
- `source_hash`
- `schema_version`
- `title`
- `description`
- `owned_req_ids`
- `created_at`
- `updated_at`

責務:

- 自身が一次ソースであることを示す。
- 自身に紐づくRequirementとProperty集合を表現する。

### 6.2 RequirementMap

属性:

- `req_id`
- `model_id`
- `property_ids`
- `status`
- `owner`
- `review_state`

責務:

- RequirementとPropertyの橋渡し。
- 要件レビューの進行管理。

### 6.3 PropertyDefinition

属性:

- `property_id`
- `model_id`
- `kind`
- `formal_expression_hash`
- `description`
- `severity`
- `enabled`

責務:

- 何を検証するかを固定する。
- 種別によってbackend適合性判断の材料を提供する。

### 6.4 VerificationRun

属性:

- `run_id`
- `model_id`
- `backend`
- `backend_version`
- `limits`
- `seed`
- `status`
- `started_at`
- `finished_at`
- `environment_fingerprint`

責務:

- 1回の検証実行を代表する。
- Run配下にPropertyResult、Evidence、Coverageをぶら下げる根になる。

### 6.5 PropertyResult

属性:

- `run_id`
- `property_id`
- `status`
- `message`
- `steps_examined`
- `evidence_id`

責務:

- 個別Propertyの結果を表す。
- 全体Run結果と個別Property結果の両方を保持する。

### 6.6 EvidenceTrace

属性:

- `evidence_id`
- `run_id`
- `property_id`
- `kind`
- `trace_hash`
- `steps_json`
- `derived_from_evidence_id`
- `minimized`

責務:

- 反例または満たす例の証拠を保持する。
- 派生関係を追跡可能にする。

### 6.7 TestVector

属性:

- `vector_id`
- `evidence_id`
- `strategy`
- `seed`
- `vector_hash`
- `vector_json`
- `oracle_kind`

責務:

- テスト生成と実行の中間資産。
- 生成コードと切り離して保持する。

### 6.8 CoverageReport

属性:

- `coverage_id`
- `run_id`
- `transition_coverage`
- `guard_coverage`
- `state_observed`
- `depth_stats_json`
- `threshold_result`

責務:

- 品質ゲート用集計を保持する。

### 6.9 ContractSnapshot

属性:

- `contract_id`
- `model_id`
- `contract_hash`
- `lock_version`
- `generated_at`
- `notes`

責務:

- 実装境界の整合性を固定する。

## 7. 集約設計

### 7.1 ModelAggregate

境界:

- ModelDefinition
- PropertyDefinition
- RequirementMap

不変条件:

- Property ID重複なし
- REQ-ID未解決なし
- 無効なProperty種別なし

### 7.2 RunAggregate

境界:

- VerificationRun
- PropertyResult
- CoverageReport

不変条件:

- `run_id` 一意
- `status` と `PropertyResult` の整合
- limitsとbackendが固定

### 7.3 EvidenceAggregate

境界:

- EvidenceTrace
- TestVector

不変条件:

- 派生関係が循環しない
- vectorは必ずevidenceへ紐づく
- minimizedフラグとderived relationが整合する

## 8. 値オブジェクト

- `ModelId`
- `RunId`
- `PropertyId`
- `EvidenceId`
- `VectorId`
- `HashValue`
- `LimitPolicy`
- `BackendCapability`
- `CoverageThreshold`

値オブジェクト化の理由は、単なる文字列の誤用を防ぐことと、境界でのValidationを局所化するためである。

## 9. Domain Service

### 9.1 PropertyEvaluationService

Propertyごとの結果を正規化する。

### 9.2 EvidenceReplayService

trace再生によりFAIL証拠を確認する。

### 9.3 TraceMinimizationService

目的を維持したままTraceを短縮する。

### 9.4 TestVectorGenerationService

EvidenceからVector集合を生成する。

### 9.5 ContractHashService

ContractSnapshotを計算する。

### 9.6 CoverageComputationService

Run結果とVector実行結果からCoverageを集計する。

## 10. ドメインイベント

- `ModelDefined`
- `ModelUpdated`
- `PropertyAdded`
- `VerificationStarted`
- `PropertyFailed`
- `EvidenceRecorded`
- `EvidenceMinimized`
- `RegressionVectorGenerated`
- `ContractChanged`
- `CoverageThresholdBreached`

Domain Eventは監査と運用自動化の接点として使う。

## 11. 永続化戦略

### 11.1 保存原則

- 監査対象は消さない。
- 派生物は元IDを保持する。
- ハッシュで完全性を確認する。

### 11.2 更新原則

- ModelAggregateは更新可能だがバージョンを進める。
- RunAggregateは開始後ほぼ追記のみ。
- EvidenceAggregateはappend-onlyを原則。

### 11.3 削除原則

物理削除は原則禁止。論理削除またはアーカイブとする。

## 12. リポジトリ設計

- `ModelRepository`
- `RunRepository`
- `EvidenceRepository`
- `CoverageRepository`
- `ContractRepository`

RepositoryはUse Case層から見た契約であり、永続化方式を漏らさない。

## 13. クリーンアーキテクチャとの接続

本章のEntityとAggregateはEntities層に属する。RepositoryはInterfaceとしてUse Case層に公開され、ファイルシステムやDB実装は外側のAdapterに閉じる。つまり、ドメインは「どこに保存するか」ではなく「何を保存しなければならないか」だけを知る。

## 14. STO/SSOTとの接続

データモデルはSTOを前提に設計する。

- ModelDefinitionが一次ソースを表す。
- ContractSnapshot、Documentation、Vectorは派生物。
- `source_hash` が派生物の根拠になる。

これにより、生成物がどのモデルから生まれたかを追跡できる。

## 15. ソルバとの接続

Runにはsolver metadataを必ず持たせる。

- backend name
- backend version
- capability snapshot
- raw diagnostics reference

理由は、同じPropertyでもbackend差異でUNKNOWN理由やtrace形状が変わるためである。

## 16. AI運用との接続

AIが主に読むのは、Entityそのものではなく構造化ビューである。しかし、そのビューの根は本章で定義したデータモデルである。したがって以下を守る。

- IDは安定であること。
- 診断リンクは明示されること。
- explainの根拠がEntityへ辿れること。

## 17. データモデルの将来拡張

### 17.1 追加候補

- `LivenessResult`
- `UnsatCore`
- `ProofArtifact`
- `ModelSlice`

### 17.2 拡張制約

- 既存ID体系を壊さない。
- trace/vectorとの参照整合を保つ。
- 旧run閲覧互換を失わない。

## 18. 品質負債のモデル化

本章では品質負債もDomain概念として扱う。

- `UnmappedRequirementDebt`
- `DeadActionDebt`
- `PersistentUnknownDebt`
- `ContractDriftDebt`

これらはIssue化だけでなく、集計可能なデータとして残す。

## 19. 典型クエリ

このデータモデルで回答できるべき質問:

- このRequirementを裏付けるPropertyは何か。
- このFAILを再現する最新Traceはどれか。
- このContract差分はどのモデル更新に由来するか。
- UNKNOWNが多いPropertyは何か。
- どの反例がすでに回帰テスト化済みか。

## 20. 章固有の受け入れ基準

- エンティティごとの責務が明確である。
- Aggregate不変条件が定義されている。
- Repository境界がContext境界と整合している。
- STOとsolver metadataの取り込み方が定義されている。
- 監査用途の追跡がrun_id中心で成立する。

## 21. 結論

本章の目的は、「何を保存するか」だけでなく、「なぜその単位で保存するか」を定義することである。検証基盤は、アルゴリズムだけではなく証拠運用の質で価値が決まる。したがって、Requirement、Model、Run、Evidence、Vector、Contract、Coverageを明確に分け、それぞれの不変条件と関係を定義することが、実装の前提条件となる。

## 22. 履歴モデル

### 22.1 Model Version History

Modelは上書き更新ではなく、履歴を持つ。少なくとも次を記録する。

- version番号
- source hash
- author
- changed req ids
- summary

### 22.2 Run History

Runは完全な履歴を保持する。再現性確保のため、過去runの上書きは禁止する。

### 22.3 Evidence Lineage

Evidenceの派生関係を明示する。

- raw counterexample
- minimized counterexample
- derived witness
- test vector set

## 23. ビジネスルールの詳細

### 23.1 Property廃止

Propertyを削除する場合、対応するREQ-IDへの影響を明示しなければならない。単純削除は禁止し、`deprecated` 状態を経る。

### 23.2 Requirement分割

REQ-IDを分割する場合、旧REQ-IDから新REQ-ID群へのマッピング履歴を残す。これにより、過去traceや過去レビューコメントが孤立しない。

### 23.3 Coverage Gate

Coverageはビジネスルールに紐づく。critical propertyに紐づくActionやGuardは、一般Actionより高いCoverage要件を持ちうる。

## 24. 読みモデル

永続化モデルと表示モデルを分ける。

### 24.1 Audit Read Model

- run timeline
- evidence lineage
- contract history

### 24.2 Review Read Model

- REQ to property matrix
- current unknown hotspots
- dead action list

### 24.3 AI Read Model

- normalized inspect payload
- compact explain payload
- recommended next actions

## 25. 不変条件の例

- `VerificationRun.status = FAIL` のとき、少なくとも1つのPropertyResultがFAILである。
- `PropertyResult.status = FAIL` のとき、`evidence_id` はnullでない。
- `TestVector.evidence_id` は有効なEvidenceを指す。
- `RequirementMap.property_ids` は同一model内のPropertyを指す。
- `ContractSnapshot.model_id` は有効なModelを指す。

## 26. 参照整合性ポリシー

### 26.1 強参照

- run -> model
- property result -> property
- evidence -> run/property

### 26.2 派生参照

- vector -> evidence
- coverage -> run
- contract -> model

### 26.3 参照破壊時

物理削除を禁止し、論理削除またはアーカイブで参照を保護する。

## 27. DDDから見たデータモデル判断

このデータモデルでは、モデル定義と検証実行を同じAggregateにしない。理由はライフサイクルが違うためである。モデルは長寿命でレビュー対象、Runは短寿命で観測対象、Evidenceは監査資産として中寿命である。これを分けることで、同一Modelに対する複数Run比較や、同一Evidenceに対する複数testgen戦略比較が簡潔になる。

## 28. クリーンアーキテクチャから見た永続化判断

Use CaseはRepository interfaceしか知らない。したがって、ファイル保存、SQLite、オブジェクトストアなどへの切替は外側の問題である。重要なのは、Entityが永続化制約に引きずられないことと、監査に必要な情報を落とさないことである。

## 29. STOから見たデータモデル判断

STOを守るために、ModelDefinitionは派生物から再構築しない。DocumentやVectorの内容がModelと矛盾した場合でも、正とするのはModelDefinitionである。矛盾はdriftとして検出し、派生物を再生成する。

## 30. ソルバから見たデータモデル判断

backend固有の細かい出力は `raw diagnostics` として分離保存し、共通Entityへは normalized value のみ入れる。こうすることで、backend差替え時に過去データとの比較可能性を保つ。

## 31. 章固有の補足結論

本章が定義しているのは、単なる保存項目一覧ではなく、検証プラットフォームの記憶装置である。記憶装置の設計を誤ると、要求、証拠、契約、回帰テストの関係が崩れ、基盤全体が使えなくなる。そのため、本章のデータモデルは機能追加より先に安定させる必要がある。

## 32. 監査クエリを前提にした設計

データモデルは保存だけでなく検索も前提とする。最低限、次の監査クエリへ即答できる構造が必要である。

- どのRequirementが未検証か
- どのPropertyが継続的にUNKNOWNか
- どのEvidenceがまだ回帰テスト化されていないか
- どのContract変更が最も頻繁か

この観点がないと、保存データはあっても運用に使えない。

## 33. データ保持期間

データは価値によって保持期間を変える。

- ModelDefinition/RequirementMap/ContractSnapshot: 長期保持
- VerificationRun/PropertyResult: 中長期保持
- raw diagnostics: 必要に応じた短中期保持
- generated text artifacts: 派生物として短期保持でもよい

ただし、traceとvectorは回帰資産であるため、重要度は高い。

## 34. 将来移行への備え

スキーマ拡張や保存先変更を前提に、Entity IDとhashを安定させる。これにより、たとえばファイル保存からDB保存へ移行しても、上位の参照関係が壊れない。

## 35. 最終補足

本章のデータモデルは、単に「正規化してきれい」にすることが目的ではない。証拠を失わず、履歴を辿れ、AIとCIが同じ事実を参照できることが目的である。
