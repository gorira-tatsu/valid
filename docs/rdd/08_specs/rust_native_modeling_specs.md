# 15. Rust-Native Modeling Specs

- ドキュメントID: `RDD-0001-15`
- バージョン: `v0.1`
- 目的: Rust 記法を主要なモデル定義経路とするための最小仕様を定義する。
- 前提:
  - [../01_overview/overview_and_scope.md](../01_overview/overview_and_scope.md)
  - [mvp_frontend_and_kernel_specs.md](mvp_frontend_and_kernel_specs.md)
  - [full_technology_usage_plan.md](full_technology_usage_plan.md)

## 1. 目標

本仕様の目標は、実装・モデル・性質を Rust 記法の中に同居させ、1つの定義から次を得ることである。

- 実行可能な Rust コード
- 検証用 IR
- 反例・witness
- 回帰テスト

`.valid` 形式や spec block は最終形ではなく、移行期または fixture 用の補助入力として扱う。

## 2. 最小契約

Rust-native modeling の最小契約は `Finite` と `VerifiedMachine` とする。

```rust
pub trait Finite {
    fn all() -> Vec<Self>;
}

pub trait VerifiedMachine {
    type State: Clone + Debug + Eq + Hash;
    type Action: Clone + Debug + Eq + Hash + Finite;

    fn model_id() -> &'static str;
    fn property_id() -> &'static str;
    fn init_states() -> Vec<Self::State>;
    fn step(state: &Self::State, action: &Self::Action) -> Vec<Self::State>;
    fn holds(state: &Self::State) -> bool;
}
```

## 3. 意味論

- `init_states()` は初期状態集合を返す。
- `step()` は 0 個以上の後続状態を返してよい。
- 0 件は disabled action を意味する。
- 複数件は非決定遷移を意味する。
- `holds()` は invariant / safety property の MVP 形である。

## 4. MVP 実装方針

- explicit BFS により最短反例を返す。
- 反例は action 列と state 列を含む trace に落とす。
- `cargo test` で Rust-native model をそのまま検証できる。

## 5. 将来拡張

- proc-macro による `#[model]`, `#[action]`, `#[invariant]`
- `reads/writes` 抽出
- contract hash への統合
- IR 生成の自動化
- solver adapter への直接 lowering

## 6. 受け入れ条件

- Rust struct / enum / impl だけで 1 モデルを記述できること
- explicit engine 相当の探索で `PASS/FAIL` が返ること
- 反例が shortest trace として得られること
- `.valid` を使わなくても repo 内の検証例が成立すること
