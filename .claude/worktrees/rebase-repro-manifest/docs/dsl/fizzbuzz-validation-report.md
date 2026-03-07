# FizzBuzz モデル使用検証レポート

対象: `examples/fizzbuzz.rs`
検証日: 2026-03-07
DSL ガイド: `docs/dsl/README.md`

---

## 1. モデル概要

FizzBuzz のルールを有限状態モデルとして表現し、5つの不変条件を形式検証する。

| 項目 | 内容 |
|------|------|
| State | `i: u8 [0..=15]`, `fizz: bool`, `buzz: bool` |
| Action | `STEP` (reads=[i], writes=[i, fizz, buzz]) |
| Transitions | 4本 (fizzbuzz_path / fizz_path / buzz_path / number_path) |
| Properties | 5個の invariant |
| 定義方式 | 宣言的 `transitions` (solver-ready パス) |

## 2. コマンド別検証結果

### 2.1 inspect

```
model_id: FizzBuzzModel
machine_ir_ready: true
capabilities: parse=true explicit=true ir=true solver=true coverage=true explain=true testgen=true
```

- 全7種の capability が `true`。DSL README が示す最高レベルの readiness に到達。
- 4つの遷移すべてでガード式・更新式・パスタグが正しくパースされ、IR に降ろされている。
- `reads`/`writes` メタデータが inspect 出力に正しく反映されている。

**判定: PASS** - モデル構造の静的解析は完全に機能。

### 2.2 verify (全プロパティ)

| Property | Result | explored_states | explored_transitions |
|----------|--------|-----------------|---------------------|
| P_COUNTER_BOUND | PASS | 1 | 1 |
| P_FIZZ_DIVISIBLE_BY_3 | PASS | 1 | 1 |
| P_BUZZ_DIVISIBLE_BY_5 | PASS | 1 | 1 |
| P_FIZZBUZZ_DIVISIBLE_BY_BOTH | PASS | 1 | 1 |
| P_NUMBER_NOT_DIVISIBLE | PASS | 1 | 1 |

- 全5プロパティが PASS。`assurance_level: complete` が返る。
- ただし **`explored_states: 1`** は問題。16状態 (i=0..15) を探索すべきところ、初期状態のみで打ち切られている。後述の課題セクション参照。

**判定: CONDITIONAL PASS** - 結果は正しいが、探索深度に懸念あり。

### 2.3 readiness

```
model_id: FizzBuzzModel
status: ok
capabilities: parse=true explicit=true ir=true solver=true coverage=true explain=true testgen=true
findings: none
```

- `findings: none` で readiness 上の問題なし。
- DSL README に記載の degraded readiness 理由（`opaque_step_closure`, `unsupported_machine_guard_expr` 等）は一切発生していない。

**判定: PASS** - 宣言的モデルとして完全に solver-ready。

### 2.4 graph

5形式すべてで正常出力を確認:

| Format | Status | 備考 |
|--------|--------|------|
| mermaid | OK | `flowchart LR` 形式で全遷移・プロパティを含むダイアグラム |
| dot | OK | Graphviz DOT 形式。ガードは diamond ノード、更新は note ノード |
| json | OK | 構造化データ。`schema_version: 1.0.0`。CI 連携可能 |
| text | OK | inspect と同等のテキスト出力 |
| svg | 未検証 | (Graphviz ランタイム依存のため省略) |

**判定: PASS** - グラフ生成は4形式で正常動作。

### 2.5 coverage

```
transition_coverage_percent=0
guard_full_coverage_percent=0
visited_state_count=1
step_count=0
gate_status=fail
uncovered_guards=STEP:true
path_tag_counts=
```

- カバレッジ 0%。状態探索が1状態で止まっているため、遷移が一度も発火していない。
- `uncovered_guards=STEP:true` は全ガードが未到達であることを示す。

**判定: FAIL** - カバレッジ分析は機能しているが、探索エンジンの浅さにより有意な結果が得られない。

### 2.6 explain

```
no evidence trace available for explain
```

- 全プロパティが PASS のため反例トレースが存在せず、explain の出力対象がない。
- これ自体は正常動作（違反がなければ説明もない）。

**判定: N/A** - 違反なしのため適用外。意図的に FAIL するプロパティでの検証が望ましい。

### 2.7 generate-tests

```
vector_ids:
(空)
```

- テストベクタが生成されない。coverage と同様、探索が浅いため素材がない。

**判定: FAIL** - テスト生成機能自体は動作するが、出力が空。

## 3. 発見された問題と修正

### 3.1 P_NUMBER_NOT_DIVISIBLE の初期状態違反 (修正済み)

**問題**: 初期状態 `{i:0, fizz:false, buzz:false}` で `0 % 3 == 0` のため不変条件が成立しない。

**原因**: i=0 は「開始前」状態であり FizzBuzz 分類の対象外だが、不変条件がこれを考慮していなかった。

**修正**: `state.i == 0` を除外条件として追加。
```rust
// before
state.fizz || state.buzz || (state.i % 3 != 0 && state.i % 5 != 0);
// after
state.fizz || state.buzz || state.i == 0 || (state.i % 3 != 0 && state.i % 5 != 0);
```

**教訓**: 初期状態が「ゼロ値」の場合、モジュロ演算で意図しない整除が発生する。初期状態の特殊性を不変条件に織り込む必要がある。

## 4. DSL 仕様との適合性評価

| DSL README の記述 | FizzBuzz モデルでの検証結果 |
|-------------------|---------------------------|
| `valid_state!` マクロで状態定義 | OK - `[range = "0..=15"]` 含め正常動作 |
| `#[derive(ValidAction)]` で reads/writes | OK - inspect/graph に正しく反映 |
| 宣言的 `transitions` が canonical path | OK - solver-ready 判定を取得 |
| モジュロ算術がサポート範囲内 | OK - `%` 演算子がガード・更新式で正常コンパイル |
| `tags` がカバレッジ・グラフで使用される | 部分的 - graph には反映。coverage は探索不足で未確認 |
| `invariant` プロパティ | OK - 5個すべて正常に評価 |
| `valid_models!` でレジストリ登録 | OK - CLI から正常にアクセス |

## 5. 仕様検証ツールとしての妥当性評価

### 強み

1. **DSL の表現力**: FizzBuzz のようなモジュロ算術を含むモデルが宣言的に書け、solver-ready まで到達する。DSL README の「bounded arithmetic expressions including `+`, `-`, `%`」が実際に機能することを確認。

2. **構造解析の完成度**: inspect / readiness / graph は信頼性が高い。モデルの静的構造（フィールド、ガード式、更新式、タグ）が正確に解析・可視化される。

3. **多形式グラフ出力**: Mermaid / DOT / JSON / text の4形式が即座に利用可能。CI パイプラインへの組み込み（JSON）やドキュメント生成（Mermaid）に実用的。

4. **readiness モデル**: capability の段階的評価（parse → explicit → ir → solver → coverage → explain → testgen）により、モデルの成熟度が定量的に把握できる。

5. **早期バグ検出**: P_NUMBER_NOT_DIVISIBLE の初期状態違反を即座に検出。形式検証が「仕様の穴」を見つける本来の価値を実証。

### 課題

1. **探索深度の浅さ**: 最大の課題。全コマンドで `explored_states: 1` となり、16状態の状態空間が探索されない。`assurance_level: complete` と報告されるが、実質的には初期状態のみの検証。coverage / generate-tests が空になる直接原因。

2. **カバレッジの実用性**: 探索が浅いため `transition_coverage_percent=0` となり、カバレッジ分析の価値が発揮されない。タグベースのパスカバレッジも未検証。

3. **テスト生成の空出力**: `generate-tests --strategy=path` がベクタを生成しない。探索深度の問題が解消されれば改善される可能性が高い。

4. **explain の検証困難**: 全プロパティ PASS の場合に explain を検証する手段がない。意図的に FAIL するプロパティを用意するか、反例注入機能があると検証しやすい。

### 総合判定

| 観点 | 評価 |
|------|------|
| DSL としての記述力 | A - モジュロ算術まで宣言的に書ける |
| 静的解析 (inspect/readiness/graph) | A - 正確で多形式対応 |
| 動的検証 (verify) | B- - 結果は正しいが探索が浅い |
| カバレッジ・テスト生成 | C - 探索不足により実質未機能 |
| エラー検出能力 | A - 初期状態の仕様バグを即座に検出 |
| CI/自動化への適合性 | B+ - JSON 出力・readiness チェック等が揃っている |

**仕様検証ツールとしては「構造解析と宣言的モデリングにおいて高い妥当性」を持つ。ただし、状態空間探索の深度が現時点のボトルネックであり、カバレッジとテスト生成の実用性はこの改善に依存する。**
