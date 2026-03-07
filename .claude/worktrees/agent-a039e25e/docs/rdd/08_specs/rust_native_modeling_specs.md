# 15. Rust-Native Modeling Specs

- ドキュメントID: `RDD-0001-15`
- バージョン: `v0.1`
- 目的: Rust で書かれたモデル定義を主要経路とするための最小仕様を定義する。
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

Rust model definition の公開契約は `StateSpec`, `ActionSpec`, `ModelSpec` とする。
`VerifiedMachine` は engine 内部の探索契約であり、利用者が直接実装対象として
意識しなくてよい。

```rust
pub trait StateSpec {
    fn snapshot(&self) -> BTreeMap<String, Value>;
}

pub trait ActionSpec {
    fn all() -> Vec<Self>;
    fn action_id(&self) -> String;
}

pub struct ModelProperty<S> {
    pub property_id: &'static str,
    pub property_kind: PropertyKind,
    pub holds: fn(&S) -> bool,
}

pub trait ModelSpec {
    type State: StateSpec;
    type Action: ActionSpec;

    fn model_id() -> &'static str;
    fn init_states() -> Vec<Self::State>;
    fn step(state: &Self::State, action: &Self::Action) -> Vec<Self::State>;
    fn properties() -> Vec<ModelProperty<Self::State>>;
}
```

`VerifiedMachine` は `ModelSpec` を満たす型に対して内部で与えられる探索契約とする。
`valid_state!` は field range metadata を保持できる。
`valid_actions!` は reads/writes metadata を保持できる。

## 2.1 表面 DSL

MVP の表面 DSL は次の macro 群で構成する。

- `valid_state!`
- `valid_actions!`
- `valid_model!`
- `valid_models!`

これらは「人間にとって書きやすい Rust 記法」を与えるが、内部では property 列、
action 列、state metadata へ正規化されることを前提とする。

## 3. 意味論

- `init_states()` は初期状態集合を返す。
- `step()` は 0 個以上の後続状態を返してよい。
- 0 件は disabled action を意味する。
- 複数件は非決定遷移を意味する。
- `properties()` は 1 件以上の property を返す。
- MVP の property kind は invariant / safety とする。
- `check` のデフォルト評価対象は properties の先頭要素とする。
- `orchestrate` は properties 全件を評価対象にできなければならない。

## 4. MVP 実装方針

- explicit BFS により最短反例を返す。
- 反例は action 列と state 列を含む trace に落とす。
- `cargo test` で Rust model をそのまま検証できる。
- declarative `transitions { transition ... }` を解析の正規経路とする。
- 表面 DSL の `step` は任意 Rust を許してよいが、explicit-first の補助表現とする。
- `inspect` は capability matrix を返し、少なくとも
  `explicit_ready`, `ir_ready`, `solver_ready`, `coverage_ready`,
  `explain_ready`, `testgen_ready` を示す。
- `lint` は capability matrix と metadata をもとに migration hint を返す。
- inspect / coverage / explain / testgen は shared decision/path tag
  vocabulary を使い、少なくとも `guard_path`, `allow_path`,
  `deny_path`, `boundary_path`, `write_path` を扱えること。
- declarative transition で与えた `tags = [...]` は lower 後の shared IR
  に保持され、inspect / coverage / explain / testgen / solver adapter が
  同じ path taxonomy を参照できること。
- `transitions { transition ... }` を使う場合は action / guard / effect を
  descriptor として保持し、solver-neutral IR に lower 可能でなければならない。
- 将来の solver-neutral 化のため、表面 DSL と engine 内部 trait は分離する。

## 4.1 最小 DSL 例

```rust
valid_state! {
    struct State {
        x: u8 [range = "0..=3"],
        locked: bool,
    }
}

valid_actions! {
    enum Action {
        Inc => "INC" [reads = ["x", "locked"], writes = ["x"]],
        Lock => "LOCK" [reads = ["locked"], writes = ["locked"]],
        Unlock => "UNLOCK" [reads = ["locked"], writes = ["locked"]],
    }
}

valid_model! {
    model CounterModel;

    init [State { x: 0, locked: false }];

    step |state, action| {
        match action {
            Action::Inc if !state.locked && state.x < 3 => [
                State { x: state.x + 1, locked: state.locked }
            ],
            Action::Lock => [
                State { x: state.x, locked: true }
            ],
            Action::Unlock => [
                State { x: state.x, locked: false }
            ],
            _ => [],
        }
    }

    properties {
        invariant P_RANGE |state| state.x <= 3;
        invariant P_LOCKED_RANGE |state| !state.locked || state.x <= 3;
    }
}
```

## 5. 現実的な Rust model 例

MVP 段階でも、教材的な counter だけではなく、現実の業務ドメインに近い例を repo 内に持つ。

- IAM-like authorization
  - deny precedence
  - permissions boundary / session / SCP 風の制約
  - policy diff による access widening 検出
- train fare calculation
  - child fare
  - day pass
  - transfer discount
  - distance monotonicity
- SaaS entitlements
  - free / pro / enterprise
  - member / admin / billing admin
  - feature gating

これらは `.valid` ではなく、利用者または repo の `examples/` / `tests/` に置かれた Rust モデルとして維持する。システム本体は generic な modeling 契約と engine だけを提供する。

## 6. 将来拡張

- proc-macro による `#[derive(ValidState)]`, `#[derive(ValidAction)]`
- field range metadata (`#[valid(range = ...)]`)
- `reads/writes` 抽出
- contract hash への統合
- IR 生成の自動化
- solver adapter への直接 lowering
- closure 形式の `step` と transition 宣言 DSL の併存

## 6.1 宣言的 transition 例

```rust
valid_model! {
    model IamAccessModel<AccessState, AccessAction>;
    init [AccessState {
        boundary_attached: false,
        session_active: false,
        billing_read_allowed: false,
    }];
    transitions {
        transition AttachBoundary [tags = ["boundary_path"]] when |state| !state.boundary_attached => [AccessState {
            boundary_attached: true,
            session_active: state.session_active,
            billing_read_allowed: state.billing_read_allowed,
        }];
        transition EvaluateBillingRead [tags = ["allow_path", "boundary_path", "session_path"]] when |state| state.boundary_attached && state.session_active => [AccessState {
            boundary_attached: state.boundary_attached,
            session_active: state.session_active,
            billing_read_allowed: true,
        }];
    }
    properties {
        invariant P_BILLING_READ_REQUIRES_BOUNDARY |state| !state.billing_read_allowed || state.boundary_attached;
    }
}
```

この形式では `guard`, `effect`, `tags` を shared IR に保持できるため、将来の
solver lowering, explain, coverage, testgen の強化に使える。

## 7. 受け入れ条件

- Rust struct / enum / impl だけで 1 モデルを記述できること
- `properties { ... }` で複数 invariant を宣言できること
- explicit engine 相当の探索で `PASS/FAIL` が返ること
- 反例が shortest trace として得られること
- `.valid` を使わなくても repo 内の検証例が成立すること
