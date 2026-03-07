# valid Language Evolution

この文書は、`valid` DSL の検討中仕様と設計メモをまとめるための非規範文書です。
ここに書かれている内容は、現行実装ではなく「次にどう育てるか」です。

## 目的

`valid` は次の中間を狙います。

- SPIN 的な状態遷移・反例・deadlock
- Alloy 的な relation / set / map
- Dafny 的な contract / property / specification readability

つまり、単なる model checker でも theorem prover でもなく、
実務向け finite-state formal verification platform を目指します。

## 検討中 1: action の relational semantics

現在の canonical path は guarded update です。

将来的には、action をより pre/post relation として直接書ける形が候補です。

```rust
on Inc {
    require |pre| !pre.locked && pre.x < 3;
    ensure |pre, post| {
        post.x == pre.x + 1 &&
        post.locked == pre.locked
    };
}
```

ただし内部 IR は引き続き flat transition 列に lower する想定です。

## 検討中 2: property kind の拡張

現行は `Invariant` のみです。候補:

- `DeadlockFreedom`
- `Reachability`
- 将来的な contract/assertion 系

## 検討中 3: Decision / Path IR

`explain`, `coverage`, `generate-tests` を別々に進化させるのではなく、
次を共通抽象にしたいです。

- action
- guard
- guard outcome
- write-set
- path tags
- property branch

これにより:

- policy-path coverage
- explain の一貫性
- path-based testgen
- solver/exploration witness の統一

がやりやすくなります。

## 検討中 4: richer finite data model

現在:

- finite enum
- `Option<FiniteEnum>`
- `FiniteEnumSet`
- `FiniteRelation`
- `FiniteMap`
- `String` + explicit regex helpers

今後の候補:

- `FiniteTuple`
- relation / map の sugar 強化
- finite multiset 的表現
- text abstraction
  - raw string theoryではなく、policy/password向けの bounded abstraction

## 検討中 5: logic sugar

今後の候補:

- `all_of(...)`
- `any_of(...)`
- `none_of(...)`
- more ergonomic grouped transitions
- `otherwise` sugar

ただし core IR は小さく保つ方針です。

## 検討中 6: text / regex story

現在の text support は explicit-first です。

- `String`
- `len`
- `str_contains`
- `regex_match`

将来的な選択肢:

1. explicit-only のままにする
2. restricted regex fragment を SAT/SMT に落とす
3. password / token / identifier 用の higher-level predicate を導入する

今のところ 3 が現実的です。

例:

- `has_uppercase(password)`
- `has_digit(password)`
- `has_symbol(password)`
- `min_length(password, 12)`

これは backend-neutral にしやすい可能性があります。

## 検討中 7: step の位置づけ

`step` を完全に消す予定はありませんが、位置づけはかなり明確です。

- `step`
  - prototype
  - explicit-first
  - migration source
- `transitions`
  - canonical specification
  - solver-visible
  - graph/coverage/testgen/explain の正規入力

## 検討中 8: IDE / diagnostics

今後強めたいもの:

- `valid_model!` parser の span diagnostics
- `trybuild` UI tests
- rust-analyzer での誤診断低減
- `cargo valid readiness` / `cargo valid migrate` の提案精度向上

## 検討中 9: packaging

目標:

- Rust user 以外でも binary で使える
- Rust model author は Cargo で快適
- solver backend は plug-in 的に差し替えられる

方針:

- core は solver-neutral
- embedded backend は pure Rust を優先
- external solver backend は optional

## 近い将来の候補タスク

- `DeadlockFreedom` の first-class property 化
- `Reachability` の first-class property 化
- `FiniteRelation` / `FiniteMap` の solver encoding 拡張
- password / token / policy 向け text abstraction
- capability matrix の migration hint 強化
