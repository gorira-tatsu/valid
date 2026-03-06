# 10. MVP Frontend and Kernel Specs

- ドキュメントID: `RDD-0001-10`
- バージョン: `v0.1`
- 目的: 最初に実装する `A-1`〜`A-4`, `B-1`〜`B-3` の詳細仕様を定義する。
- 前提:
  - [../03_architecture/architecture.md](../03_architecture/architecture.md)
  - [../04_domain/business_logic_and_data_model.md](../04_domain/business_logic_and_data_model.md)
  - [../09_reference/glossary.md](../09_reference/glossary.md)
- 関連ID:
  - FR: `FR-001`〜`FR-014`
  - Epic: `A-1`〜`A-4`, `B-1`〜`B-3`
  - PR: `PR-01`, `PR-02`
  - 参照索引: [id_cross_reference.md](../09_reference/id_cross_reference.md)
- 次に読む:
  - [explicit_engine_and_evidence_specs.md](explicit_engine_and_evidence_specs.md)
  - [../10_delivery/pr_01_frontend_acceptance.md](../10_delivery/pr_01_frontend_acceptance.md)

## 1. 対象範囲

本章で扱うのは、以下の最初の中核機能である。

- `A-1`: モデルソース読込
- `A-2`: 名前解決
- `A-3`: 型付け
- `A-4`: IR生成
- `B-1`: 式評価器
- `B-2`: ガード評価
- `B-3`: 遷移適用

この範囲を先に固める理由は、以降の engine、trace、testgen、contract のすべてがこの土台に依存するためである。frontend は恒久的に独自 DSL を持つことを目標とせず、Rust model 定義から統一 IR を得ることを主目的とする。

## 2. 共通設計方針

- parser段階で意味を持ちすぎない。
- 名前解決と型付けを明確に分ける。
- IRは frontend表現に依存しない。
- kernelは純粋関数で構成する。
- エラーは構造化コードで返す。

## 3. A-1 モデルソース読込

### 3.1 責務

- 入力ソースを受け取り、構文木へ変換する。
- source locationを保持する。
- コメントや説明文をメタデータとして保持する。

### 3.2 入力

- Rust trait / Rust macro によるモデル定義
- 移行期のみ `.valid` 形式の fixture 入力を許可する

MVP では、Rust で書かれたモデル定義を正規経路とし、`.valid` は parser / engine / reporter の早期検証用ハーネスとしてのみ維持する。入力は内部共通の `RawSourceUnit` に正規化する。

### 3.3 出力

```text
RawModelAst {
  declarations: Vec<Decl>,
  spans: SpanTable,
  docs: DocTable,
  source_hash: HashValue,
}
```

### 3.4 エラー

- `PARSE_ERROR`
- `UNEXPECTED_TOKEN`
- `UNTERMINATED_BLOCK`
- `DUPLICATE_SECTION`

### 3.5 受け入れ条件

- state, action, property の最小構文を読める。
- spanが行/列と結びつく。
- source_hashが安定的に計算される。

### 3.6 実装メモ

- parserはrecoverable errorを集約できる形が望ましい。
- ただしMVPでは最初の致命的構文エラーで停止してもよい。

## 4. A-2 名前解決

### 4.1 責務

- 宣言済み識別子をsymbol tableへ登録する。
- state/action/property/typeの参照を解決する。
- 重複宣言を拒否する。

### 4.2 入力

- `RawModelAst`

### 4.3 出力

```text
ResolvedModelAst {
  states: Vec<ResolvedStateDecl>,
  actions: Vec<ResolvedActionDecl>,
  properties: Vec<ResolvedPropertyDecl>,
  symbols: SymbolTable,
}
```

### 4.4 解決対象

- state field参照
- action名参照
- property対象参照
- enum variant参照
- bounded type参照

### 4.5 エラー

- `UNRESOLVED_SYMBOL`
- `DUPLICATE_SYMBOL`
- `INVALID_SCOPE_REFERENCE`

### 4.6 受け入れ条件

- すべての識別子に一意なsymbol idが付与される。
- 未定義参照が構造化エラーとして返る。
- shadowingルールが文書化され、実装が一致する。

### 4.7 DDD/CA観点

名前解決はModeling Contextの責務であり、Use Case層ではなくfrontend adapter寄りに置く。ただし、生成されるsymbol idは後段で長く使われるため、安定性が必要である。

## 5. A-3 型付け

### 5.1 責務

- 各式と宣言へ型を与える。
- bounded型の演算可能性を判定する。
- bool条件式と値式を区別する。

### 5.2 MVP型

- `Bool`
- `Enum(name)`
- `BoundedInt(min, max, signedness)`
- `Struct(name, fields)`
- `Unit`

### 5.3 型ルール例

- guardは `Bool` でなければならない。
- 比較演算は互換型同士に限る。
- enum variantは対応enumにのみ属する。
- 次状態代入の右辺型は左辺型へ代入可能であること。

### 5.4 出力

```text
TypedModelAst {
  typed_states: ...,
  typed_actions: ...,
  typed_properties: ...,
  type_env: TypeEnv,
}
```

### 5.5 エラー

- `TYPE_MISMATCH`
- `INVALID_GUARD_TYPE`
- `INVALID_ASSIGNMENT_TYPE`
- `UNKNOWN_TYPE`
- `UNSUPPORTED_TYPE_COMBINATION`

### 5.6 受け入れ条件

- 主要型エラーがsource spanつきで返る。
- typed ASTから後段のIR生成が可能。
- bounded型に対するmin/max制約が保持される。

### 5.7 将来拡張点

- set/relation
- generic bounded collection
- temporal property type checks

## 6. A-4 IR生成

### 6.1 責務

- frontend固有構文を捨て、backend中立のIRへ変換する。
- state/action/property/facts/initを統一表現にする。

### 6.2 IR設計原則

- backend中立
- trace生成に必要なラベルを保持
- source span参照を保持
- actionごとにreads/writesを持つ

### 6.3 主要IR型

```text
ModelIr {
  state_schema: StateSchema,
  init: InitSpec,
  actions: Vec<ActionIr>,
  properties: Vec<PropertyIr>,
  facts: Vec<ExprIr>,
}
```

```text
ActionIr {
  action_id: ActionId,
  label: String,
  reads: Vec<FieldId>,
  writes: Vec<FieldId>,
  guard: ExprIr,
  updates: Vec<UpdateIr>,
}
```

### 6.4 変換ルール

- 未更新state fieldは明示的にupdateへ入れない。
- フレーム条件はkernelまたはengine側で適用する。
- propertyの説明文はreporter用に保持する。

### 6.5 エラー

- `IR_LOWERING_ERROR`
- `MISSING_REQUIRED_COMPONENT`
- `UNSUPPORTED_IR_SHAPE`

### 6.6 受け入れ条件

- 同じtyped ASTから同じIRが決定的に生成される。
- ActionIrでguard/updates/reads/writesが揃う。
- source spanへ逆引きできる。

## 7. B-1 式評価器

### 7.1 責務

- `ExprIr` を `Value` へ評価する。
- 評価中の型整合はtyped AST前提だが、防御的検査も行う。

### 7.2 入力

- `ExprIr`
- `StateValue`

### 7.3 出力

- `Value`
- または `EvalError`

### 7.4 対応演算

- literal
- field access
- equality / inequality
- boolean and/or/not
- numeric compare
- numeric add/sub（bounded内）
- conditional expression（必要ならMVP最小限）

### 7.5 エラー

- `FIELD_NOT_FOUND`
- `TYPE_RUNTIME_MISMATCH`
- `BOUNDED_OVERFLOW_POLICY_VIOLATION`
- `UNSUPPORTED_EXPR`

### 7.6 受け入れ条件

- 主要式が決定的に評価される。
- 同一入力で同一出力を返す。
- panicではなく `EvalError` を返す。

## 8. B-2 ガード評価

### 8.1 責務

- `ActionIr.guard` を `bool` へ評価する。
- enabled/disabledを明確に返す。

### 8.2 入力

- `ActionIr`
- `StateValue`

### 8.3 出力

```text
GuardOutcome {
  enabled: bool,
  diagnostics: Option<GuardDiagnostics>,
}
```

### 8.4 受け入れ条件

- guardがtrue/falseで安定的に判定される。
- 失敗時は `EvalError` と区別される。
- explainやcoverageのため、どのguardを評価したか追跡可能。

## 9. B-3 遷移適用

### 9.1 責務

- enabled actionに対して、同時代入規則で次状態を生成する。
- writesに含まれないfieldは現状態を維持する。

### 9.2 入力

- `ActionIr`
- `StateValue`

### 9.3 出力

```text
TransitionOutcome {
  next_state: StateValue,
  changed_fields: Vec<FieldId>,
}
```

### 9.4 重要ルール

- 右辺評価は旧状態を参照する。
- 更新順序に依存してはならない。
- bounded型の制約は代入後に検証する。

### 9.5 エラー

- `GUARD_DISABLED`
- `INVALID_UPDATE_TARGET`
- `POST_STATE_INVALID`
- `RUNTIME_ASSIGNMENT_TYPE_ERROR`

### 9.6 受け入れ条件

- 同時代入の意味が保たれる。
- unchanged fieldが保持される。
- changed_fieldsが正確に算出される。

## 10. 例: 最小モデル

```text
state {
  x: BoundedU8<0, 7>
  locked: Bool
}

action Inc {
  reads: [x, locked]
  writes: [x]
  guard: !locked
  update: x = x + 1
}
```

この最小モデルで、A-1〜A-4 と B-1〜B-3 が一通り動くことをMVPの基本検証ケースとする。

## 11. API境界

### 11.1 frontend -> orchestrator

- `ModelIr`
- `FrontendDiagnostics`
- `SourceMetadata`

### 11.2 kernel public contract

- `eval_expr(expr, state) -> Result<Value, EvalError>`
- `evaluate_guard(action, state) -> Result<GuardOutcome, EvalError>`
- `apply_transition(action, state) -> Result<TransitionOutcome, TransitionError>`

これらの関数名は仮称だが、責務は固定する。

## 12. テスト戦略

### 12.1 Unit Test

- parser成功/失敗
- name resolution成功/失敗
- type check成功/失敗
- expr evaluation
- guard evaluation
- transition application

### 12.2 Golden Test

- 同じモデル入力から同じIR JSONが出る。

### 12.3 Property-style Test

- 同時代入の順序独立性
- unchanged field維持
- deterministic evaluation

## 13. DDD対応

本章で扱う機能は主にModeling ContextとVerification Contextの境界にある。

- A系はModeling Context
- B系はVerification Contextのkernel側

ここでIntegration ContextやEvidence Contextの責務を持ち込まないことが重要である。

## 14. クリーンアーキテクチャ対応

- A系はfrontend adapter + use case支援
- B系はentity/kernel層

特にB系は、外部I/Oと完全に分離しなければならない。

## 15. STO対応

- `source_hash` はA-1時点で確定する。
- `ModelIr` は一次ソースからのみ生成される。
- A/B系では派生物の逆流を許さない。

## 16. ソルバ対応

この段階ではexplicitもBMCもまだ完成していなくてよいが、A/B系の出力が両者で共有できることが重要である。したがって `ModelIr` と `ExprIr` はsolver中立性を前提に設計する。

## 17. 依存関係

- A-1 -> A-2 -> A-3 -> A-4
- A-4 -> B-1/B-2/B-3

これを崩して並列実装しない。特にA-4未確定のままB系実装を進めると、IR変更で手戻りが大きくなる。

## 18. 完了条件

本章の対象範囲が完了したと判断する条件は以下。

1. 最小モデルがparseからIR生成まで通る。
2. 式評価・guard評価・遷移適用が動く。
3. 主要エラーが構造化コードで返る。
4. unit testとgolden testが存在する。
5. 次フェーズのC系へ受け渡せる。

## 19. 次段

この章が固まったら、次は `C-1`〜`C-6` と `D-1`〜`D-3` を対象に、探索と証拠生成の詳細仕様へ進む。そこまで行くと、最初の動く `check` コマンドを実装できる。

## 20. 結論

最初に作るべきものは、派手なsolver統合ではなく、安定したfrontendと純粋なkernelである。ここが弱いと、後段のengine、trace、testgen、AI APIがすべて不安定になる。本章は、その最初の土台を実装可能な粒度で固定するための仕様である。
