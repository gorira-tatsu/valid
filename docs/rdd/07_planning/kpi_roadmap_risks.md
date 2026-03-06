# 08. KPI, Roadmap, Risks

- ドキュメントID: `RDD-0001-08`
- バージョン: `v0.3`
- 目的: 目標指標、段階的実装計画、主要リスクと緩和策を定義する。
- ID参照:
  - [id_cross_reference.md](../09_reference/id_cross_reference.md)
  - 機能分解: [feature_breakdown.md](feature_breakdown.md)
  - 詳細仕様: [../08_specs/README.md](../08_specs/README.md)

## 1. 本章の位置づけ

本章はプロジェクト管理資料ではなく、設計と運用の優先順位を固定するための技術文書である。KPIは単に「測れると良い指標」ではなく、どの設計判断が成功かを判定する物差しである。ロードマップは作業一覧ではなく、依存関係のある能力獲得順序を定義する。リスクは悲観論ではなく、あらかじめアーキテクチャへ折り込む制約である。

## 2. KPI設計原則

- KPIは運用改善へ結びつくものだけを採用する。
- AIが自動収集できる指標を優先する。
- 単一数値で誤魔化されないよう、因果に近い指標を持つ。
- 結果指標と先行指標を分ける。

## 3. KPI体系

### 3.1 開発速度

- 反例発見から回帰テスト化までの時間
- 仕様変更PRのレビュー時間
- Contract mismatchの修正時間

### 3.2 品質

- 本番流出した状態遷移バグ件数
- 再発バグ件数
- FAIL without trace件数

### 3.3 検証運用

- UNKNOWN率
- transition coverage
- guard coverage
- trace再現率

### 3.4 AI運用

- 同seed再現一致率
- explain採用率
- AI提案の棄却率

### 3.5 アーキテクチャ健全性

- kernel変更頻度
- adapter追加時の既存層変更量
- schema破壊変更頻度

## 4. 主要KPI定義

### KPI-01 Fail-to-Test Time

関連ID: `FR-040`, `FR-041`, `E-1`, `E-2`, `PR-06`, `Phase 3`

定義:

`FAIL` が初めて観測された時刻から、その反例を再生する回帰テストがmainlineで利用可能になるまでの時間。

目的:

- 反例を資産化する運用が回っているかを測る。

目標:

- 平均30分以内

### KPI-02 Unknown Ratio

関連ID: `FR-024`, `C-6`, `I-1`, `Phase 4`, `Phase 5`

定義:

全Property評価数に対する `UNKNOWN` 件数の割合。

目的:

- モデル分割、抽象化、ソルバ選定の改善余地を測る。

目標:

- 3リリースで30%削減

### KPI-03 Transition Coverage

関連ID: `FR-050`, `G-1`, `G-4`, `PR-08`, `Phase 3`

定義:

実行されたAction数 / 総Action数

目的:

- テスト生成とモデル探索の偏り可視化

目標:

- 主要モデルで85%以上

### KPI-04 Replay Consistency

関連ID: `FR-021`, `NFR-002`, `B-5`, `J-1`, `PR-02`, `PR-11`

定義:

EvidenceTrace再生一致率

目的:

- 証拠運用の信頼性

目標:

- 100%

### KPI-05 Contract Drift Detection

関連ID: `FR-060`, `FR-061`, `F-1`, `F-2`, `F-3`, `PR-07`, `Phase 2`

定義:

意図しない契約変更をCIで検知できた割合

目的:

- 実装境界管理の強度測定

目標:

- 検知漏れ0件

## 5. KPIの算出元

KPIは手集計しない。以下のDomain Eventから計算する。

- `PropertyFailed`
- `EvidenceRecorded`
- `RegressionVectorGenerated`
- `ContractChanged`
- `CoverageThresholdBreached`
- `RunFinished`

この方針により、KPIが運用依存の曖昧な指標になることを防ぐ。

## 6. DDDから見たKPI

KPIはIntegrationのダッシュボード指標ではあるが、算出根拠はDomain Eventである。つまり、KPIはドメイン外のレポートではなく、ドメインの状態変化から導出される派生概念として扱う。これにより、イベント設計と監査設計が一致する。

## 7. クリーンアーキテクチャから見たKPI

KPIの集計ロジックはUse CaseまたはReporting Serviceで持ち、CIジョブや外部ダッシュボードに埋め込まない。外部ツールは結果を表示するだけで、計算意味論は内部で保持する。

## 8. SSOTから見たKPI

KPI算出元データは一次ソースと派生物の整合が取れていなければ意味がない。たとえば契約差分KPIは `contract_hash` が正しく更新されること、Unknown率はRunが完全保存されていることを前提にする。

## 9. ソルバから見たKPI

同じKPIでもbackendごとに補助指標を持つ。

- explicit: 状態数、枝刈り率、深さ
- bmc: k、SAT/UNSAT件数、solve time

ただし上位KPIはbackend非依存で比較できる形に正規化する。

## 10. ロードマップ原則

- 価値が早く出る順に進める。
- 依存関係を無視した並列開発をしない。
- セルフホストは後工程だが、最初から布石を打つ。

## 11. Phase 0: Foundations

関連ID: `A-1`〜`A-4`, `B-1`〜`B-3`, `PR-01`, `PR-02`, [RDD-0001-10](../08_specs/mvp_frontend_and_kernel_specs.md)

目的:

- IR定義
- schema versioning
- 最小CLI
- kernel骨格

完了条件:

- モデルを読み、IRへ変換できる
- 結果をJSONで出せる

## 12. Phase 1: Explicit MVP

関連ID: `C-1`〜`C-7`, `D-1`〜`D-3`, `PR-03`, `PR-04`, [RDD-0001-12](../08_specs/explicit_engine_and_evidence_specs.md)

目的:

- BFS/DFS
- invariant/reachability/deadlock
- 反例復元

完了条件:

- `check` がFAILでtraceを返す
- `trace` が可視化できる

## 13. Phase 2: Rust Integration

関連ID: `FR-010`〜`FR-014`, `F-1`, `F-2`, `H-1`, `H-2`, `PR-07`, `PR-09`

目的:

- `Finite`
- `VerifiedMachine`
- macro/contract hash
- debug assertion integration

完了条件:

- Rustコードからモデル埋め込み可能
- contract checkが動く

## 14. Phase 3: Test Automation

関連ID: `E-1`〜`E-4`, `G-1`, `G-2`, `G-4`, `PR-06`, `PR-08`, [RDD-0001-13](../08_specs/testgen_contract_coverage_specs.md)

目的:

- trace->vector
- vector->Rust test
- coverage計測

完了条件:

- FAILが回帰テストへ変換される
- coverageレポートがCIに乗る

## 15. Phase 4: BMC / Solver Expansion

関連ID: `FR-023`, `I-1`〜`I-4`, `PR-10`, [RDD-0001-14](../08_specs/ai_solver_selfcheck_specs.md)

目的:

- bounded model checking
- witness生成
- solver adapter追加

完了条件:

- backend切替で同一trace schemaが維持される

## 16. Phase 5: Concurrency and Reduction

関連ID: `FR-005`, `G-3`, `G-5`, `H-3`, `H-4`, `H-5`, `Phase 4`

目的:

- reads/writes活用
- POR
- 並行シナリオのexplore強化

完了条件:

- UNKNOWN削減と深い反例検出率向上

## 17. Phase 6: Self-Host Step 1

関連ID: `J-1`, `J-2`, `J-3`, `PR-11`, [RDD-0001-14](../08_specs/ai_solver_selfcheck_specs.md)

目的:

- kernel一部の自己検証
- selfcheck CI

完了条件:

- selfcheckがmainlineで継続実行される

## 18. 主要リスク

### R-01 状態爆発

影響:

- UNKNOWN増加
- CI時間超過

対策:

- limit control
- model slicing
- POR
- explicit/BMC使い分け

### R-02 モデル誤り

影響:

- 正しく検証しても価値が出ない

対策:

- REQ-IDレビュー
- property説明文の強制
- explain改善

### R-03 ソルバ不整合

影響:

- 同一入力で異なる結果

対策:

- trace正規化
- kernel replay
- capability宣言

### R-04 AI誤修正

影響:

- 自動変更でモデル破壊

対策:

- contract/doc/check mandatory
- AI mode制限
- 変更提案にtrace根拠必須

### R-05 ドキュメント陳腐化

影響:

- 設計と実装の乖離

対策:

- `doc --check`
- source_hash追跡

## 19. リスク優先順位

1. 誤PASS
2. trace再現不能
3. contract drift未検知
4. UNKNOWN増加
5. coverage停滞

この順序は対処優先順位でもある。

## 20. リスクの計測方法

- replay mismatch件数
- FAIL without trace件数
- contract mismatch漏れ件数
- backend別UNKNOWN率
- coverage threshold breach件数

## 21. 投資判断基準

### 21.1 explicit最適化へ投資する条件

- UNKNOWNの主因が状態数爆発
- trace品質は十分
- 主要モデルが明示探索に向いている

### 21.2 BMCへ投資する条件

- 深い反例が欲しい
- witness生成の価値が高い
- testgenの多様性不足がある

### 21.3 self-hostへ投資する条件

- kernelが十分安定
- 通常運用の価値が出ている
- 監査要求が高い

## 22. マイルストーン判定

### M0

- JSON schema固定
- minimal run成功

### M1

- 反例復元
- replay一致

### M2

- contract lock
- doc drift detection

### M3

- regression test generation
- coverage report

### M4

- backend abstraction
- BMC prototype

### M5

- concurrency reduction
- unknown reduction evidence

### M6

- selfcheck pipeline

## 23. KPIとロードマップの接続

各PhaseがどのKPIへ寄与するかを定義する。

- Phase 1 -> Replay Consistency
- Phase 2 -> Contract Drift Detection
- Phase 3 -> Fail-to-Test Time, Coverage
- Phase 4 -> Unknown Ratio, Witness Diversity
- Phase 5 -> Unknown Ratio, Deep Bug Detection
- Phase 6 -> Self-host KPI

## 24. 失敗時の方針転換

### 24.1 explicitが伸びない場合

- モデル分割を優先
- BMC比重を上げる

### 24.2 AI運用が不安定な場合

- read-only assistant modeへ戻す
- explain品質改善を先行

### 24.3 self-hostが重すぎる場合

- kernel対象範囲を限定
- proof artifact supportを延期

## 25. 章固有の受け入れ基準

- KPIに定義、算出元、目標がある。
- ロードマップが依存順序を反映している。
- リスクに対して具体的緩和策がある。
- DDD/CA/SSOT/solver観点が章内にある。

## 26. 結論

本章の役割は、開発を測定可能にし、優先順位を固定し、失敗時の撤退線まで定義することである。検証基盤は理念だけでは継続しない。KPI、ロードマップ、リスクを設計文書として扱い、どの能力をどの順序で獲得し、どの兆候が危険かを明確にすることが、本プロジェクトの実行可能性を支える。

## 27. KPIの収集実装方針

KPIは別途スプレッドシートで手入力しない。収集は以下のイベントフローから自動化する。

- `RunFinished` で duration、status、backendを集計
- `EvidenceRecorded` で fail-to-evidence時間を記録
- `RegressionVectorGenerated` で fail-to-test時間を確定
- `CoverageComputed` で coverage推移を記録
- `ContractChanged` で drift件数を記録

これにより、KPIのための運用負荷が増えることを防ぐ。

## 28. フェーズごとの出口条件

### Phase 0 Exit

- IRとschemaが固定
- sample modelでinspect/checkが動く

### Phase 1 Exit

- critical propertyでFAIL traceを返せる
- replay consistency 100%

### Phase 2 Exit

- Rust埋め込みモデルでcontract checkが通る

### Phase 3 Exit

- counterexampleから回帰テストを自動生成できる

### Phase 4 Exit

- 2つ以上のbackendで同一trace schemaへ落とせる

### Phase 5 Exit

- UNKNOWN率が定量的に改善

### Phase 6 Exit

- selfcheckがCIに常設される

## 29. リスクの早期兆候

- UNKNOWN率が連続増加
- replay mismatchが1件でも出る
- contract mismatchが頻発
- AI提案の棄却率が高止まり
- coverageが停滞

これらは大事故の前兆として扱う。

## 30. DDDから見たリスク管理

リスクはIntegration問題だけではない。Modeling ContextにREQ未対応が溜まればモデルリスク、Verification ContextにUNKNOWNが溜まれば実行リスク、Evidence Contextにtrace欠損が出れば監査リスクとなる。したがって、リスク管理もContext別に持つべきである。

## 31. クリーンアーキテクチャから見た投資判断

どのPhaseへ投資するかは、どの層に負債が偏っているかで決める。たとえばAdapter負債が大きいのにEntityを複雑化する投資は誤りである。逆にkernelが小さく安定している段階でselfcheckへ進むのは妥当である。

## 32. SSOTから見たKPI運用

KPIの元データがdriftしていれば指標は無意味である。したがって、KPIダッシュボードよりも先に `contract --check` と `doc --check` を安定稼働させることが重要である。指標の信頼性は一次ソース整合の上にしか成立しない。

## 33. ソルバから見たロードマップ

ロードマップ上、solver投資は後半に置くが、これはsolver軽視ではない。先に共通IR、trace schema、testgen、contract lockを固めないと、solverを増やしても成果が運用資産にならないためである。逆に、これらが固まれば、solver追加のROIは高くなる。

## 34. 章固有の補足結論

KPI、ロードマップ、リスクは別々の話ではない。どの能力をいつ作るかは、どの指標を改善し、どのリスクを下げるかと一体で決めるべきである。本章は、その連動関係を固定するための章である。

## 35. レビュー cadence

KPIとリスクは定例で見直す。

- weekly: fail/unknown/contract mismatch
- sprint end: coverage trend, fail-to-test time
- release: roadmap progress, risk reprioritization

## 36. 中止基準と成功基準

### 中止基準

- 主要KPIが複数四半期改善しない
- replay inconsistencyが解消しない
- モデル作成コストが価値を恒常的に上回る

### 成功基準

- 反例が自然に回帰資産化される
- モデルレビューがチームの標準になる
- backend追加が既存運用を壊さずにできる

## 37. リスクのオーナーシップ

- モデル誤り: Domain Lead
- solver instability: Verification Engineer
- CI drift: Platform Engineer
- AI misuse: AI Ops Engineer

オーナー不在のリスクは放置されるため、明示する。

## 38. 章末補足

本章は「やること一覧」ではない。どこに投資し、どこで止まり、何を危険信号とみなすかを決めることで、プロジェクトを技術的にも運用的にも前進可能にする章である。

## 39. KPIレビューの具体手順

1. 前スプリントとの比較差分を出す。
2. backend別にUnknown率を分解する。
3. coverage停滞の原因をAction/Property単位で見る。
4. fail-to-test時間が長い案件の共通原因を抽出する。
5. 次スプリントで改善するKPIを2つまでに絞る。

## 40. フェーズ間依存の明示

Phase 3以前にPhase 5へ進まない。理由は、反例を資産化する導線がない状態で並行最適化に投資しても、得られる価値が限定的だからである。同様に、Phase 2以前にself-hostへ進まない。契約とkernel境界が安定していない段階で自己検証を始めると、対象が揺れ続ける。

## 41. リスクとKPIの対応表

- 状態爆発 -> Unknown Ratio
- モデル誤り -> Review Time, Requirement Mapping Completeness
- ソルバ不整合 -> Replay Consistency
- AI誤修正 -> Contract Drift Detection, Explain Adoption

この対応表により、リスク管理が定量的になる。

## 42. DDDから見たロードマップ

各Phaseは主に強化するContextが異なる。

- Phase 0-1: Verification Context
- Phase 2: Modeling + Integration
- Phase 3: Evidence Context
- Phase 4: Verification Context拡張
- Phase 5: Verification + Modeling
- Phase 6: Kernel/Evidenceの再帰適用

## 43. クリーンアーキテクチャから見たロードマップ

初期Phaseでは内側のEntities/Use Casesを安定させ、後半でAdapterを増やす。これにより、後からHTTPや複数solverを追加してもコアが揺れない。

## 44. SSOTから見たロードマップ

SSOT整備は初期に完了させる。理由は、後から一次ソースと派生物の境界を直すと、過去run、過去trace、過去docの整合が壊れるためである。

## 45. ソルバ投資の判断材料

solverへ投資するかどうかは、理論的魅力ではなく次で決める。

- Unknownをどれだけ減らせるか
- witness生成でtestgen価値が増えるか
- trace正規化に無理がないか
- CIコストが許容範囲か

## 46. 章固有の章末補足2

KPI、ロードマップ、リスクの3点を別チームに分散させると、施策と結果が結びつかなくなる。本章ではこの3点を1つの判断系として扱い、プロジェクトの前進と中止判断の両方を支える。

## 47. 計画見直しトリガー

以下のいずれかが発生した場合、ロードマップを再評価する。

- 主要backendの継続利用が困難になった
- replay consistencyに継続障害が出た
- AI運用方針が大きく変わった
- self-hostの前提となるkernel境界が変わった

## 48. マイルストーンごとの証拠

各Phaseは、完了宣言だけでなく証拠を持つべきである。

- Phase 1: 実際の反例trace
- Phase 2: contract drift検知ログ
- Phase 3: 自動生成された回帰テスト
- Phase 4: backend比較結果
- Phase 5: unknown削減レポート
- Phase 6: selfcheck CI結果

## 49. リスク管理の失敗例

ありがちな失敗は、Unknown率が高いのにsolverを増やし続ける、契約差分が多いのにlock運用を軽視する、trace再現不能が出ているのにcoverage改善だけを追う、などである。本章はそうした誤投資を防ぐため、指標とリスクを一体で扱う。

## 50. 章末確認

- 指標は行動を変えられるものだけを残す。
- 計画は測定結果で更新する。
- リスクは責任者を明記する。
- 進捗報告では本章の指標とフェーズ出口条件を併記する。
