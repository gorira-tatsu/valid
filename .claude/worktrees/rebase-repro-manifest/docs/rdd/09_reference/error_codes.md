# Error Codes

## 0. 診断モデル

エラーコードは単独で返さない。失敗応答は次の診断構造へ埋め込む。

- `error_code`
- `segment`
- `severity`
- `message`
- `primary_span`
- `related_spans`
- `conflicts`
- `help`
- `best_practices`
- `related_diagnostics`

`message` は現在の失敗事実、`help` は直近の修復行動、`best_practices` は再発防止の規約提案とする。

## 0.1 セグメント分類

- `frontend.parse`
- `frontend.resolve`
- `frontend.typecheck`
- `frontend.lowering`
- `kernel.eval`
- `kernel.guard`
- `kernel.transition`
- `engine.init`
- `engine.search`
- `engine.reconstruct`
- `evidence.replay`
- `report.text`
- `report.json`
- `report.mermaid`
- `testgen.vector`
- `testgen.render`
- `contract.snapshot`
- `contract.lock`
- `coverage.compute`
- `solver.plan`
- `solver.exec`
- `solver.normalize`
- `api.inspect`
- `api.check`
- `api.explain`
- `api.minimize`
- `api.testgen`
- `selfcheck.run`

## 1. Frontend / Kernel

- `PARSE_ERROR`
- `NAME_RESOLUTION_ERROR`
- `TYPECHECK_ERROR`
- `UNSUPPORTED_EXPR`
- `INVALID_TRANSITION_UPDATE`

## 2. Explicit Engine

- `UNSAT_INIT`
- `INVALID_INIT_ASSIGNMENT`
- `INIT_ENUMERATION_LIMIT_EXCEEDED`
- `ERROR_PREDECESSOR_BROKEN`
- `UNKNOWN_STATE_LIMIT_REACHED`
- `UNKNOWN_DEPTH_LIMIT_REACHED`
- `UNKNOWN_TIME_LIMIT_REACHED`
- `UNKNOWN_INIT_ENUMERATION_LIMIT_REACHED`
- `UNKNOWN_UNSUPPORTED_PROPERTY_KIND`
- `UNKNOWN_ENGINE_ABORTED`

## 3. Evidence / Reporter

- `TRACE_SERIALIZATION_ERROR`
- `TRACE_REPLAY_ERROR`
- `TEXT_REPORT_ERROR`

## 4. Testgen / Contract / Coverage

- `VECTOR_BUILD_ERROR`
- `TEST_RENDER_ERROR`
- `CONTRACT_HASH_ERROR`
- `LOCK_MISMATCH`
- `DOC_DRIFT_DETECTED`
- `COVERAGE_COMPUTE_ERROR`

## 5. AI / Solver / Selfcheck

- `API_SCHEMA_ERROR`
- `UNSUPPORTED_BACKEND`
- `BACKEND_PLAN_ERROR`
- `BACKEND_EXECUTION_ERROR`
- `TRACE_NORMALIZATION_ERROR`
- `SELFCHECK_FAILURE`

## 6. 規約

- `UNKNOWN_*` は run 契約内停止を意味する。
- `ERROR_*` は内部破損または契約外失敗を意味する。
- 同じ意味に複数名を作らない。
- `message` は現在の失敗事実だけを書く。
- `help` は利用者が直近で実行できる行動を書く。
- `best_practices` は一般規約か、同種失敗の予防策だけを書く。

## 7. 代表的な help / best practice

### `PARSE_ERROR`

- help:
  - `unexpected token` の前後 3 行を確認する
  - 括弧、カンマ、区切り記号の対応を確認する
- best_practices:
  - action / property ごとに1ブロック1責務に保つ
  - 長い式は補助定義へ分割する

### `TYPECHECK_ERROR`

- help:
  - state field と action update の型一致を確認する
  - bool 条件と value 式の混在を見直す
- best_practices:
  - bounded int を明示し、暗黙拡張に依存しない
  - property 式は比較対象の型を揃えて書く

### `UNSAT_INIT`

- help:
  - init 条件と state schema の整合を確認する
  - 相互排他的な制約がないか調べる
- best_practices:
  - init は action や property の前提と独立にレビューする
  - 初期値は最小例から増やす

### `LOCK_MISMATCH`

- help:
  - contract snapshot を再生成し差分を確認する
  - lock 更新が意図的変更か確認する
- best_practices:
  - public trait 変更は dedicated PR で行う
  - snapshot と lock を同時レビューする
