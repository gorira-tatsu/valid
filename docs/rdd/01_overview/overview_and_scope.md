# 01. Overview and Scope

- ドキュメントID: `RDD-0001-01`
- バージョン: `v0.3`
- 目的: 本プロジェクトの全体像、境界、上位設計原則、拘束条件を定義する。
- 関連ID:
  - 要求: `REQ-001`〜`REQ-010`
  - 機能: `FR-001`〜`FR-073`
  - 非機能: `NFR-001`〜`NFR-005`, `NFR-010`〜`NFR-042`
  - 参照索引: [id_cross_reference.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/id_cross_reference.md)
  - 要求カタログ: [requirements_catalog.md](/Users/tatsuhiko/code/valid/docs/rdd/11_requirements_catalog/requirements_catalog.md)
  - ガバナンス: [governance_and_operations.md](/Users/tatsuhiko/code/valid/docs/rdd/01_overview/governance_and_operations.md)

## 1. プロジェクト定義

本プロジェクトは、Rust 記法を中心とした形式検証基盤を構築し、モデル記述、検証実行、証拠生成、テスト生成、契約管理、CI ゲート、AI 操作を一体で扱う検証運用プラットフォームを提供する。

本プロジェクトは単体の solver や単体の model checker を作ることを目的としない。目的は、仕様と実装の乖離を縮小し、検証結果を replay 可能な証拠として保持し、その証拠を継続的なテスト資産と品質ゲートへ接続することである。

## 2. 解く課題

本プロジェクトが直接解く課題は次の通りである。

- 仕様レビューと実装レビューが分離し、整合維持コストが高い。
- 並行遷移、境界値、稀条件の欠陥が通常レビューで取りこぼされる。
- 形式検証結果が回帰テスト資産へ接続されない。
- AI が安全に反復実行できる安定した検証インターフェースがない。
- solver を差し替えるたびに上位運用契約が壊れる。

## 3. スコープ境界

### 3.1 In Scope

- Rust trait / macro によるモデル埋め込み
- 明示状態探索による MVP 検証
- 反例復元、証拠保存、trace replay
- 反例および witness の test vector 化
- contract hash / lock による drift 検知
- AI 向け安定 JSON API
- 将来の solver adapter による backend 差し替え

### 3.2 Out of Scope（MVP）

- Rust 全構文の完全意味論
- 完全定理証明を初期コアに据えること
- 1つの solver に固定した閉鎖設計
- ランタイム監視を主責務とすること
- 証拠の自動修復や自動承認

## 4. 上位原則

### 4.1 Correctness First

- `PASS` の誤判定は最も重い欠陥として扱わなければならない。
- `UNKNOWN` を `PASS` と同義に扱ってはならない。
- `FAIL` は replay 可能な証拠を伴わなければならない。

### 4.2 Evidence-Driven

- 判定結果だけでなく、証拠を保存しなければならない。
- 証拠は将来拡張を考慮し、`Evidence::Trace` と `Evidence::Certificate` の総称として扱う。
- v0.3 時点では、本番ゲートで使う `FAIL` 証拠は `Evidence::Trace` を必須とする。
- `Evidence::Certificate` は将来の演繹系 backend 追加時に導入する。

### 4.3 SSOT

- 各モデルは SSOT を1つだけ持たなければならない。
- SSOT はモデル単位またはリポジトリ単位で選択する。
- `Rust macro` と `spec block` の混在は原則禁止とし、必要な場合は ADR を必須とする。
- 生成物は SSOT ではなく派生成果物である。
- 派生成果物の手修正は許可しない。必要なら SSOT を更新し、再生成しなければならない。

### 4.4 Solver-Neutral Core

- コア意味論は統一 IR で保持しなければならない。
- solver 実装は adapter に閉じ込めなければならない。
- solver 固有の出力は上位へ直接公開してはならず、共通の結果形式へ正規化しなければならない。

### 4.5 AI-First Interface

- AI が扱うインターフェースは安定 JSON schema を返さなければならない。
- `schema_version`, `request_id`, `seed`, `diagnostics` を省略してはならない。
- explain は補助情報であり、意味論上の真実源として扱ってはならない。

## 5. DDD 方針

DDD は本プロジェクトに全面適用するのではなく、語彙と境界を安定させるために限定適用する。

### 5.1 MUST

- `Bounded Context` を明示すること
- `Ubiquitous Language` を固定すること

### 5.2 SHOULD

- `Aggregate` を整合性境界の説明に使うこと
- Domain Service で複雑な計算責務を分離すること

### 5.3 MAY

- `Domain Event` を将来の監査や通知用途で使ってよい
- ただし MVP の必須構成要素としては扱わない

現時点での主要 Context は次の4つとする。

- `Modeling Context`: Requirement, Model, Property の定義
- `Verification Context`: RunPlan, exploration, solver execution
- `Evidence Context`: trace, vector, report, replay
- `Integration Context`: CLI, AI API, CI, contract, lock

## 6. クリーンアーキテクチャ方針

- Entities は State, Action, Property, Evidence, Contract を持つ。
- Use Cases は Check, Minimize, TestGen, Coverage, ContractCheck, Selfcheck を持つ。
- Interface Adapters は CLI, JSON API, Renderer, Solver Adapter を持つ。
- Framework/Drivers は filesystem, process execution, CI runtime を持つ。

依存方向は内向きでなければならない。特に次を固定する。

- kernel は I/O を持ってはならない。
- Use Case は solver 実装詳細を知らない。
- Reporter は Domain Model を破壊してはならない。
- solver 追加は adapter 増設で完結するべきである。

## 7. Solver Strategy

### 7.1 フェーズ戦略

- Phase 1: explicit exploration を主とする
- Phase 2: BMC を追加する
- Phase 3: 必要箇所で symbolic / deductive backend を追加する

### 7.2 選定基準

solver または backend を採用する際は、少なくとも次を評価しなければならない。

- 決定性
- 診断品質
- replay 可能性
- CI 適合性
- 能力宣言の明示性
- 保守性

### 7.3 証拠正規化

- backend の出力は共通の Evidence 形式へ正規化しなければならない。
- trace を出せる backend は `Evidence::Trace` を返す。
- trace を直接出せない backend は将来 `Evidence::Certificate` を返してよい。
- 本番ゲートに使う `FAIL` は、v0.3 では trace replay により再確認できることを必須とする。

## 8. Assurance Model

上位章として、検証結果は保証水準を必ず持つ。

- `complete`: 探索条件の範囲で完全性を主張できる
- `bounded`: 明示された境界内でのみ成立を主張する
- `incomplete`: 結果は参考情報であり、完全性を主張しない

この `assurance_level` は status とは別軸で保持する。

- `PASS + complete`
- `PASS + bounded`
- `UNKNOWN + incomplete`
- `FAIL + bounded` など

status と assurance を分離することで、AI と CI が「成立したか」と「どの強さで成立したか」を混同しないようにする。

## 9. モデル発見方式

MVP では registry-based discovery を正とする。

- CLI は `crate + model_id` もしくは model registry から対象を解決する。
- Rust macro により登録されたモデルが主要経路である。
- `spec block` からの直接入力は拡張フェーズの対象とする。

この優先順位を固定することで、実装初期の discovery 方式を曖昧にしない。

## 10. レビュー方針

- ドメインロジック変更の主レビュー単位は `ModelDelta` とする。
- `ModelDelta` は State, Action, Property, Constraint, Requirement mapping の差分から構成される。
- ただし kernel, adapter, CI, unsafe, 性能境界に関わる変更は従来のコードレビューを継続しなければならない。
- レビューコメントは可能な限り `REQ-*` または `Property-ID` に紐づける。

## 11. AI Guardrails

- AI が生成した変更は contract check を通さなければならない。
- SSOT に影響する変更で、必要な doc / lock / artifact 差分が欠けている場合はマージしてはならない。
- `UNKNOWN` を `PASS` とみなす提案は拒否しなければならない。
- explain は修正候補を返してよいが、自動承認の根拠にしてはならない。

## 12. 上位受け入れ条件

この章が満たすべき条件は次の通りとする。

- プロジェクト境界、原則、依存方向、SSOT、solver strategy、assurance model を定義していること
- 下位章と矛盾しないこと
- 運用メモや導入手引きではなく、拘束力のある上位方針に限定されていること
- 改訂時は [governance_and_operations.md](/Users/tatsuhiko/code/valid/docs/rdd/01_overview/governance_and_operations.md) と整合していること

## 13. 参照先

- アーキテクチャ詳細: [architecture.md](/Users/tatsuhiko/code/valid/docs/rdd/03_architecture/architecture.md)
- ドメイン詳細: [business_logic_and_data_model.md](/Users/tatsuhiko/code/valid/docs/rdd/04_domain/business_logic_and_data_model.md)
- 機能要件: [functional_requirements.md](/Users/tatsuhiko/code/valid/docs/rdd/02_requirements/functional_requirements.md)
- 非機能要件: [non_functional_requirements.md](/Users/tatsuhiko/code/valid/docs/rdd/02_requirements/non_functional_requirements.md)
- ガバナンスと運用: [governance_and_operations.md](/Users/tatsuhiko/code/valid/docs/rdd/01_overview/governance_and_operations.md)
