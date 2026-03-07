# Decision / Path IR

## 目的

`coverage` / `explain` / `testgen` が同じ経路語彙を共有できるように、
遷移の判断点を `Decision`、実行経路を `Path` で表現する。

既存の `path_tags` は互換用途として残すが、一次情報は `Decision` / `Path`
に寄せる。

## IR

- `DecisionPoint`
  - 遷移中の判断点を表すメタデータ
  - `decision_id`
  - `action_id`
  - `kind`
    - `Guard`
    - `StateUpdate`
  - `label`
  - `field`
  - `reads`
  - `writes`
  - `path_tags`
- `Decision`
  - `DecisionPoint` に実行時の結果を与えたもの
  - `outcome`
    - `GuardTrue`
    - `GuardFalse`
    - `UpdateApplied`
- `Path`
  - `Decision` の列
  - 互換用途として `legacy_path_tags()` を持つ

## 生成規則

- `ActionIr::decision_path()`
  - guard=true の遷移として `Path` を構築する
- `ActionIr::decision_path_for_guard(false)`
  - guard=false の判断点のみを構築する
- `TraceStep.path`
  - 実際に実行された step の `Path`
- `TestVector.expected_path`
  - ベクトルが期待する `Path`

## 利用方針

- `coverage`
  - `step.path` または `ActionIr::decision_path*()` から decision coverage を計算する
  - `path_tag_counts` は `Path::legacy_path_tags()` から集計する
- `testgen`
  - trace 由来ベクトルは `TraceStep.path` を束ねて `expected_path` を作る
  - model exploration 由来ベクトルは `ActionIr::decision_path*()` から `expected_path`
    を作る
- `explain`
  - failing step の `Path` を `decision_path` として返す
  - 既存の `failing_action_path_tags` は `decision_path.legacy_path_tags()` から導出する

## 互換性

- `path_tags` は残す
- `note = "path_tag:..."` も fallback として残す
- JSON/text 出力は既存の `path_tags` を維持しつつ、構造化された `path` /
  `decision_path` を追加する
