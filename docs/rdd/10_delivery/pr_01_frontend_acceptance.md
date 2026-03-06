# PR-01 Frontend Acceptance

## 1. 範囲

- `src/frontend/`
- `src/ir/`

対象:

- parser
- resolver
- typechecker
- IR lowering

## 2. 目的

- 最小 spec source から `ModelIr` を生成できる状態にする。

## 3. 入力サンプル

```text
model CounterLock
state:
  x: u8[0..7]
  locked: bool
init:
  x = 0
  locked = false
action Inc:
  pre: !locked
  post:
    x = x + 1
property P_SAFE:
  invariant: x <= 7
```

## 4. 受け入れ条件

1. parse が成功する。
2. name resolution が成功する。
3. typecheck が成功する。
4. `ModelIr` が生成される。
5. 未定義参照で `NAME_RESOLUTION_ERROR` を返す。
6. 型不一致で `TYPECHECK_ERROR` を返す。
7. unsupported expr で `UNSUPPORTED_EXPR` を返す。

## 5. テスト

- parser golden
- resolver error
- typecheck error
- IR shape snapshot

## 6. DoD

- `frontend` と `ir` の公開型が作成済み。
- 最小モデルの unit test が通る。
- エラーコードが docs と一致する。
