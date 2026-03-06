# PR-09 AI API Acceptance

関連ID:

- FR: `FR-070`〜`FR-073`
- Epic: `H-1`〜`H-5`
- Specs: [RDD-0001-14](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md)
- 索引: [id_cross_reference.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/id_cross_reference.md)

## 1. 目的

- inspect/check/explain/minimize/testgen の request/response 契約を固定する。

## 2. 受け入れ条件

1. request schema validation が通る。
2. response schema validation が通る。
3. `error_code` が失敗時に必須。
4. explain が構造化フィールドを返す。

## 3. DoD

- API schema JSON と golden payload を持つ。
