# Artifact Naming

## 1. ルート

- `artifacts/<run-id>/check-result.json`
- `artifacts/<run-id>/evidence/<evidence-id>.trace.json`
- `artifacts/<run-id>/vectors/<vector-id>.json`
- `artifacts/<run-id>/coverage/coverage-report.json`
- `artifacts/selfcheck/<suite-id>/<run-id>/report.json`

## 2. 命名規則

- 英小文字、数字、`-` のみ。
- 空白禁止。
- path traversal を防ぐため `/` や `..` を許容しない。
- `run-id`, `evidence-id`, `vector-id` は決定的生成かつ同一run内一意。

## 3. run-id

例:

- `run-20260306-0001`
- `run-pr1234-0007`

## 4. evidence-id

例:

- `ev-000001`
- `ev-deadlock-000001`

## 5. vector-id

例:

- `vec-000001`
- `vec-000001-min`

## 6. lock file

- `valid.lock.json`

## 7. generated tests

- `generated-tests/<vector-id>.rs`

## 8. 原則

- artifact 名に backend 固有情報を混ぜ込みすぎない。
- backend 差異は JSON 本文に書く。
