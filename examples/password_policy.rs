/*
パスワード構成要件モデル

目的:
  - 文字列状態、長さ制約、正規表現ベースの構成要件を `valid` DSL で表現する
  - 現在の text support が explicit-ready で使えることを確認する

含まれるモデル:
  - password-policy-safe
    強いパスワードを設定し、compliant フラグも正しく true になる
  - password-policy-regression
    弱いパスワードなのに compliant=true にしてしまう回帰

主な性質:
  - P_PASSWORD_POLICY_MATCHES_FLAG
    compliant は実際の構成要件判定と一致しなければならない
  - P_PASSWORD_LENGTH_BOUND
    パスワード長は常に 64 文字以下でなければならない

最初に試すコマンド:
  cargo valid --registry examples/password_policy.rs inspect password-policy-safe
  cargo valid --registry examples/password_policy.rs readiness password-policy-safe
  cargo valid --registry examples/password_policy.rs verify password-policy-regression --property=P_PASSWORD_POLICY_MATCHES_FLAG
*/

use valid::{
    iff, len, regex_match, registry::run_registry_cli, valid_actions, valid_model, valid_models,
    valid_state,
};

valid_state! {
    struct PasswordState {
        password: String [range = "0..=64"],
        password_set: bool,
        compliant: bool,
    }
}

valid_actions! {
    enum PasswordAction {
        SetStrongPassword => "SET_STRONG_PASSWORD" [reads = ["password_set"], writes = ["password", "password_set", "compliant"]],
        SetWeakPassword => "SET_WEAK_PASSWORD" [reads = ["password_set"], writes = ["password", "password_set", "compliant"]],
    }
}

valid_model! {
    model PasswordPolicySafeModel<PasswordState, PasswordAction>;
    init [PasswordState {
        password: "".to_string(),
        password_set: false,
        compliant: false,
    }];
    transitions {
        transition SetStrongPassword [tags = ["password_policy_path", "allow_path"]]
        when |state| state.password_set == false
        => [PasswordState {
            password: "Str0ngPass!".to_string(),
            password_set: true,
            compliant: true,
        }];
    }
    properties {
        invariant P_PASSWORD_POLICY_MATCHES_FLAG |state|
            iff(
                state.compliant,
                state.password_set
                    && len(&state.password) >= 10
                    && regex_match(&state.password, r"[A-Z]")
                    && regex_match(&state.password, r"[a-z]")
                    && regex_match(&state.password, r"[0-9]")
                    && regex_match(&state.password, r"[^A-Za-z0-9]")
            );
        invariant P_PASSWORD_LENGTH_BOUND |state|
            len(&state.password) <= 64;
    }
}

valid_model! {
    model PasswordPolicyRegressionModel<PasswordState, PasswordAction>;
    init [PasswordState {
        password: "".to_string(),
        password_set: false,
        compliant: false,
    }];
    transitions {
        transition SetWeakPassword [tags = ["password_policy_path", "regression_path"]]
        when |state| state.password_set == false
        => [PasswordState {
            password: "password".to_string(),
            password_set: true,
            compliant: true,
        }];
    }
    properties {
        invariant P_PASSWORD_POLICY_MATCHES_FLAG |state|
            iff(
                state.compliant,
                state.password_set
                    && len(&state.password) >= 10
                    && regex_match(&state.password, r"[A-Z]")
                    && regex_match(&state.password, r"[a-z]")
                    && regex_match(&state.password, r"[0-9]")
                    && regex_match(&state.password, r"[^A-Za-z0-9]")
            );
        invariant P_PASSWORD_LENGTH_BOUND |state|
            len(&state.password) <= 64;
    }
}

fn main() {
    run_registry_cli(valid_models![
        "password-policy-safe" => PasswordPolicySafeModel,
        "password-policy-regression" => PasswordPolicyRegressionModel,
    ]);
}
