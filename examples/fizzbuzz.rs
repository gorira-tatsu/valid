/*
FizzBuzz verification example

Purpose:
  - model the classic FizzBuzz rules as a finite-state system
  - keep the arithmetic predicates small enough to inspect and verify directly
  - provide a compact declarative example that is not only auth or workflow

Rules:
  - advance counter `i` from 1 through 15
  - if `i` is divisible by 3 but not 5, mark Fizz
  - if `i` is divisible by 5 but not 3, mark Buzz
  - if `i` is divisible by both 3 and 5, mark FizzBuzz
  - otherwise treat the step as Number

Properties to inspect:
  - `P_COUNTER_BOUND`: the counter stays within `0..=15`
  - `P_FIZZ_DIVISIBLE_BY_3`: Fizz implies divisibility by 3
  - `P_BUZZ_DIVISIBLE_BY_5`: Buzz implies divisibility by 5
  - `P_FIZZBUZZ_DIVISIBLE_BY_BOTH`: FizzBuzz implies divisibility by both
  - `P_NUMBER_NOT_DIVISIBLE`: Number implies divisibility by neither 3 nor 5

First commands to try:
  cargo valid --registry examples/fizzbuzz.rs inspect fizzbuzz
  cargo valid --registry examples/fizzbuzz.rs verify fizzbuzz --property=P_FIZZBUZZ_DIVISIBLE_BY_BOTH
  cargo valid --registry examples/fizzbuzz.rs graph fizzbuzz --format=mermaid
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
