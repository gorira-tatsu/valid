# 03. Non-Functional Requirements

- ドキュメントID: `RDD-0001-03`
- バージョン: `v0.4`
- 目的: 本システムの品質特性を、実装・運用・監査まで含めて定義する。
- ID参照:
  - 文書索引: [../09_reference/id_cross_reference.md](../09_reference/id_cross_reference.md)
  - 関連仕様: [../08_specs/explicit_engine_and_evidence_specs.md](../08_specs/explicit_engine_and_evidence_specs.md), [../08_specs/testgen_contract_coverage_specs.md](../08_specs/testgen_contract_coverage_specs.md), [../08_specs/ai_solver_selfcheck_specs.md](../08_specs/ai_solver_selfcheck_specs.md)

## 1. 本章の位置づけ

本章は「何ができるか」ではなく、「どの品質でできなければならないか」を定義する。形式検証基盤では、機能要件を満たしていても、判定が不安定、再現不能、監査不能、遅すぎる、または誤PASSを返す設計であれば業務投入に耐えない。したがって、本章は実装の後から付け足す品質メモではなく、設計そのものを拘束する上位仕様として扱う。

## 2. 品質属性の優先順位

本システムでは、品質属性を以下の順序で優先する。

1. 正確性
2. 再現性
3. 監査性
4. 入力安全性 / 実行安全性
5. 運用性
6. 性能
7. 拡張性
8. 可搬性

## 3. 正確性

### 3.1 最上位原則

- `PASS` は最も重い主張であり、誤って返してはならない。
- `FAIL` は証拠を伴わなければならない。
- `UNKNOWN` は成功扱いしない。
- `ERROR` は内部不整合または環境不備として、`PASS/FAIL/UNKNOWN` と分離する。

### 3.2 判定意味論の固定

- `PASS`: 指定した探索条件と意味論の範囲で、対象プロパティが成立した。
- `FAIL`: 1つ以上の証拠によりプロパティ不成立が観測された。
- `UNKNOWN`: 上限到達、未対応構文、外部ソルバ制約などにより、成立・不成立のいずれも断定できなかった。
- `ERROR`: パーサ異常、型検査異常、内部例外、入出力障害、契約不整合など、検証行為そのものが成立していない。

### 3.3 誤判定防止策

- 最終出力は共通 result 形式へ正規化する。
- v0.4 では `FAIL` の本番証拠は `Trace` 必須とする。
- `PASS` は前提条件、探索境界、ソルバ能力、探索制限を必ず結果に同梱する。
- `UNKNOWN` の理由はカテゴリ別に構造化出力する。
- trace を出せない backend は、`Certificate` 対応後まで本番 `FAIL` ゲート対象外とする。

### 3.4 正確性SLO

- 反例 trace 再生一致率: 100%
- `FAIL` で証拠欠落率: 0%
- 互換スキーマ上での結果パース失敗率: release gate では 0%
- 結果パース失敗率は運用メトリクスとしても 0% を維持することを目標とする

## 4. 再現性

### 4.1 再現性の定義

同一の入力モデル、同一のソルバ能力宣言、同一の実行設定、同一の seed、同一のバージョン集合のもとで、同一のステータスと同一の証拠が得られることを再現性と定義する。

### 4.2 保存必須情報

再現性のため、以下をすべて保存する。

- `request_id`
- `run_id`
- `source_hash`
- `schema_version`
- `engine_version`
- `backend_name`
- `backend_version`
- `search_bounds`
- `resource_limits`
- `seed`
- `platform_metadata`
- `trace_hash` または `certificate_hash`
- `contract_hash`

### 4.3 再現不能時の扱い

再現不能は軽微障害ではない。以下の順序で扱う。

1. `trace_replay_mismatch`
2. `solver_output_instability`
3. `environmental_drift`
4. `schema_drift`

### 4.4 再現性目標

- 同一 seed 再現失敗率: 1%未満
- 同一 trace 再生不一致率: 0%
- 契約ハッシュ誤差検出漏れ: 0%

## 5. Diagnosability

AI と人間の双方が修復ループを回すために、診断は単なるエラーコードでは足りない。少なくとも次を返せることを非機能要件とする。

- どのセグメントで止まったか
- どの入力要素が原因か
- どの制約や定義が競合しているか
- 次に確認すべき候補
- 既知のベストプラクティスのうち何が適用可能か

### 5.1 NFR-004 診断の局所性

- すべての `ERROR` と多くの `UNKNOWN` は `segment` を持つ。
- `segment` は最低でも `frontend.parse`, `frontend.resolve`, `frontend.typecheck`, `kernel.eval`, `engine.search`, `solver.normalize`, `report.render`, `contract.check`, `selfcheck.run` のいずれかに分類される。
- 1つの診断は 1つの主原因を持ち、二次原因は `related_diagnostics` へ分離する。

### 5.2 NFR-005 修復可能性

- 診断は `help` と `best_practices` を返す。
- `help` は直近の修復行動を1〜3件返す。
- `best_practices` は一般論ではなく、現在の失敗カテゴリに紐づく規約を返す。
- 修復ヒントは `error_code` と矛盾してはならない。

## 6. 監査性

### 6.1 監査対象

- モデル定義
- 実行設定
- 検証結果
- 証拠
- 生成テスト
- 契約差分
- ドキュメント生成差分

### 6.2 監査証跡

監査証跡は人間が読むログではなく、構造化データとして保存する。最小単位は `run_id` であり、`run_id` から以下を辿れる必要がある。

- 対象モデル
- 対象プロパティ
- 利用バックエンド
- 実行パラメータ
- 結果
- 生成された証拠
- その後の最小化処理
- 生成されたテスト

## 7. Input Safety / Execution Safety

### 7.1 入力安全性

- パース失敗で panic しない。
- 巨大入力で無制限メモリ消費しない。
- Mermaid や JSON 出力に危険文字列をそのまま埋め込まない。

### 7.2 実行安全性

- 外部ソルバは直接シェル文字列連結で起動しない。
- 一時ファイルは衝突しないディレクトリへ隔離する。
- 実行上限を超えた場合は強制中断可能である。

### 7.3 生成コード安全性

- testgen 結果は安全なエスケープを行う。
- 生成コードが追加で任意コード実行経路を作らないこと。
- `include!` を使う場合も固定出力先とハッシュ管理を行う。

## 8. 性能

### 8.1 基本方針

性能は「速いこと」よりも「制御できること」を重視する。状態空間爆発は避けられないため、速度目標に加え、停止性と説明可能性を品質要件として持つ。

### 8.2 MVP性能要件

- 明示探索で `10^5` 状態は開発機で現実的時間内に処理できる。
- 適切なハッシュ戦略により `10^6` 状態まで段階的に対応可能である。
- 反例復元コストは探索コストの範囲内に抑える。

### 8.3 ベンチマーク基準

性能判定は benchmark suite に基づいて行う。少なくとも以下を固定する。

- benchmark suite 名
- 開発機 profile
- CI machine profile

詳細な suite 名と環境条件は別紙 benchmark 設定へ委譲するが、性能 SLO はこの固定前提の上でのみ評価してよい。

### 8.4 性能SLO

- 主要ユースケースで PR 時検証は10分以内を目標。
- nightly 深掘り検証は1時間以内を目標。
- coverage 集計は検証時間の20%以内を目安。

## 9. 可用性

### 9.1 CLI可用性

- 失敗時でも意味のある終了コードを返す。
- 標準出力と標準エラーの責務を分離する。
- `--json` 指定時は機械可読出力を壊さない。

### 9.2 API可用性

- スキーマバージョンを必須化。
- 不完全応答を返さない。
- 失敗分類は常にコード化する。

### 9.3 CI可用性

- 環境差異で判定が変わる場合は明示警告。
- optional backend 不在時の振る舞いを定義する。
- 必須 backend 不在時は即失敗する。

## 10. 保守性

- kernel を最小化する。
- solver ごとの複雑性を adapter へ閉じ込める。
- parser / typechecker / evaluator / reporter / testgen を分離する。
- `unsafe` は kernel で禁止。
- public JSON schema 変更には移行ノート必須。
- deprecated option には期限付き廃止計画を付ける。
