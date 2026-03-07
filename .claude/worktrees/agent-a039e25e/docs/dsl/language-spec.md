# valid Language Spec

この文書は、現在の `valid` Rust DSL の実装済み言語仕様をまとめるための
現行仕様書です。`docs/dsl/README.md` が利用ガイドであるのに対して、この文書は
「何が現在サポートされているか」を固定的に書きます。

## 位置づけ

`valid` は Rust-first の有限状態仕様記述 DSL です。

- 主用途は業務ルール、承認フロー、IAM/マルチテナント、料金、entitlement などの
  有限状態検証
- canonical path は declarative `transitions { ... }`
- `step` は explicit-first / migration-oriented な補助表現

## モデルの構成

現在の標準形は次です。

1. 状態型
2. action 型
3. `valid_model!`
4. registry (`valid_models!`)

## 状態型

状態は Rust struct として表現します。現時点で machine IR に落ちる field 種別は次です。

- `bool`
- `String`
  - `#[valid(range = "8..=64")]` または `password: String [range = "8..=64"]` は
    文字列長の制約として扱う
  - 現時点では explicit backend 向け
- bounded unsigned integers
  - `u8`
  - `u16`
  - `u32`
- finite enum
- `Option<FiniteEnum>`
- `FiniteEnumSet<T>`
- `FiniteRelation<A, B>`
- `FiniteMap<K, V>`

### 状態メタデータ

現在の field metadata は次です。

- `range = "..."`
  - 数値なら値域
  - `String` なら長さ範囲
- `enum`
- `set`
- `relation`
- `map`

## action

action は有限 enum です。各 variant は `action_id` を持ち、可能なら `reads` と
`writes` を宣言します。

この metadata は次に使われます。

- `inspect`
- `graph`
- `readiness`
- `explain`
- `coverage`
- `generate-tests`

## モデル定義

`valid_model!` の header は明示型必須です。

```rust
valid_model! {
    model PasswordPolicyModel<PasswordState, Action>;
    // ...
}
```

`model Name;` の shorthand は現行仕様では無効です。

## 初期状態

`init [ ... ];` で初期状態を与えます。

declarative IR lowering では、現在 1 つの初期状態を前提とします。

## 振る舞い記述

### 1. Declarative transitions

canonical path です。

```rust
transitions {
    on SetStrongPassword {
        [tags = ["password_policy_path"]]
        when |state| state.password_set == false
        => [PasswordState {
            password: "Str0ngPass!".to_string(),
            password_set: true,
        }];
    }
}
```

内部では flat な guarded transition IR に lower されます。

### 2. step

```rust
step |state, action| {
    match action {
        // ...
    }
}
```

これはまだサポートされていますが、canonical ではありません。

- explicit exploration には使える
- solver lowering は弱い
- `graph` / `coverage` / `testgen` の情報量は declarative より低い

## property

現在の `PropertyKind` は `Invariant` のみです。

```rust
properties {
    invariant P_EXPORT_REQUIRES_ENTERPRISE |state|
        state.export_enabled == false || state.plan == Plan::Enterprise;
}
```

property は「Rust の型」ではなく、「到達可能状態に対する意味的制約」です。

## 現在の式仕様

### bool / arithmetic

- `!`
- `&&`
- `||`
- `implies(a, b)`
- `iff(a, b)`
- `xor(a, b)`
- `==`, `!=`, `<`, `<=`, `>`, `>=`
- `+`, `-`, `%`

### finite collections

- `contains(set, item)`
- `insert(set, item)`
- `remove(set, item)`
- `is_empty(set)`
- `rel_contains(rel, left, right)`
- `rel_insert(rel, left, right)`
- `rel_remove(rel, left, right)`
- `rel_intersects(left, right)`
- `map_contains_key(map, key)`
- `map_contains_entry(map, key, value)`
- `map_put(map, key, value)`
- `map_remove(map, key)`

### string / password-oriented helpers

現在の explicit-first helper:

- `len(&state.password)`
- `str_contains(&state.password, "@")`
- `regex_match(&state.password, r"[A-Z]")`

補足:

- 文字列 helper は current explicit backend では評価できる
- current SAT/SMT backend では solver-ready ではない
- `readiness` / `lint` は `string_fields_require_explicit_backend`,
  `string_ops_require_explicit_backend`,
  `regex_match_requires_explicit_backend`
  を返す

### 文字列リテラル

現在の lowering で扱う文字列リテラルは次です。

- `"abc"`
- `r"[A-Z]"`
- `r#"... "#`
- `"abc".to_string()`
- `String::from("abc")`

## Capability / readiness

現在の capability matrix は次を返します。

- `parse_ready`
- `explicit_ready`
- `ir_ready`
- `solver_ready`
- `coverage_ready`
- `explain_ready`
- `testgen_ready`
- `reasons`

意味:

- `ir_ready=true` でも `solver_ready=false` はあり得る
- 文字列/regex モデルはその代表例

## graph

`graph` には 2 つの view があります。

- default `overview`
  - 設計レビュー向け
- `--view=logic`
  - guard / update の論理を深く見るための表示

## 現在の非目標

この仕様は次をまだ目標にしていません。

- 一般の `Vec`, `HashMap`, `HashSet`
- 無限文字列理論
- 一般正規表現理論の solver encoding
- 高階ロジック
- 汎用プログラム証明
