# PR-04 Evidence / Reporter Acceptance

関連ID:

- FR: `FR-030`〜`FR-032`
- Epic: `D-1`〜`D-3`
- Specs: [RDD-0001-12](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md)
- 索引: [id_cross_reference.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/id_cross_reference.md)

## 1. 範囲

- `src/evidence/trace.rs`
- `src/evidence/reporter_json.rs`
- `src/evidence/reporter_text.rs`

## 2. 目的

- FAIL を replay 可能な trace として保存し、人間と機械の両方へ出せるようにする。

## 3. 受け入れ条件

1. `EvidenceTrace` を構築できる。
2. `check-result.json` を出力できる。
3. `*.trace.json` を出力できる。
4. PASS/FAIL/UNKNOWN の text summary を出力できる。
5. JSON が schema に一致する。

## 4. テスト

- PASS result JSON golden
- FAIL evidence JSON golden
- UNKNOWN result JSON golden
- text summary golden
- replay from evidence

## 5. DoD

- `EvidenceTrace`, `ExplicitRunResult`, `TraceStep` のシリアライズが安定。
- artifact path が naming rule に従う。
- text summary が docs の例と一致する。
