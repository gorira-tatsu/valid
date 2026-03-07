/*
FizzBuzz 形式検証モデル

FizzBuzz のルールを有限状態モデルとして表現し、形式検証を行う。

ルール:
  - カウンタ i を 1 から 15 まで進める
  - i が 3 の倍数（5 の倍数でない）→ Fizz
  - i が 5 の倍数（3 の倍数でない）→ Buzz
  - i が 3 の倍数かつ 5 の倍数 → FizzBuzz
  - それ以外 → Number

検証する不変条件:
  - P_COUNTER_BOUND: カウンタは常に 0..=15 の範囲内
  - P_FIZZ_DIVISIBLE_BY_3: Fizz 出力なら i は 3 の倍数
  - P_BUZZ_DIVISIBLE_BY_5: Buzz 出力なら i は 5 の倍数
  - P_FIZZBUZZ_DIVISIBLE_BY_BOTH: FizzBuzz 出力なら i は 3 と 5 の両方の倍数
  - P_NUMBER_NOT_DIVISIBLE: Number 出力なら i は 3 の倍数でも 5 の倍数でもない（i=0 は開始前のため除外）

実行:
  cargo run --example fizzbuzz -- verify fizzbuzz
  cargo run --example fizzbuzz -- inspect fizzbuzz
  cargo run --example fizzbuzz -- graph fizzbuzz --format=mermaid
*/

use valid::{registry::run_registry_cli, valid_model, valid_models, valid_state, ValidAction};

valid_state! {
    struct FizzBuzzState {
        i: u8 [range = "0..=15"],
        fizz: bool,
        buzz: bool,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, ValidAction)]
enum Action {
    #[valid(action_id = "STEP", reads = ["i"], writes = ["i", "fizz", "buzz"])]
    Step,
}

valid_model! {
    model FizzBuzzModel<FizzBuzzState, Action>;

    init [FizzBuzzState { i: 0, fizz: false, buzz: false }];

    transitions {
        transition Step [tags = ["fizzbuzz_path"]]
            when |state| state.i < 15 && (state.i + 1) % 15 == 0
            => [FizzBuzzState { i: state.i + 1, fizz: true, buzz: true }];

        transition Step [tags = ["fizz_path"]]
            when |state| state.i < 15 && (state.i + 1) % 3 == 0 && (state.i + 1) % 5 != 0
            => [FizzBuzzState { i: state.i + 1, fizz: true, buzz: false }];

        transition Step [tags = ["buzz_path"]]
            when |state| state.i < 15 && (state.i + 1) % 5 == 0 && (state.i + 1) % 3 != 0
            => [FizzBuzzState { i: state.i + 1, fizz: false, buzz: true }];

        transition Step [tags = ["number_path"]]
            when |state| state.i < 15 && (state.i + 1) % 3 != 0 && (state.i + 1) % 5 != 0
            => [FizzBuzzState { i: state.i + 1, fizz: false, buzz: false }];
    }

    properties {
        invariant P_COUNTER_BOUND |state|
            state.i <= 15;

        invariant P_FIZZ_DIVISIBLE_BY_3 |state|
            state.fizz == false || state.i % 3 == 0;

        invariant P_BUZZ_DIVISIBLE_BY_5 |state|
            state.buzz == false || state.i % 5 == 0;

        invariant P_FIZZBUZZ_DIVISIBLE_BY_BOTH |state|
            (state.fizz == false || state.buzz == false) || (state.i % 3 == 0 && state.i % 5 == 0);

        invariant P_NUMBER_NOT_DIVISIBLE |state|
            state.fizz || state.buzz || state.i == 0 || (state.i % 3 != 0 && state.i % 5 != 0);
    }
}

fn main() {
    run_registry_cli(valid_models![
        "fizzbuzz" => FizzBuzzModel,
    ]);
}
