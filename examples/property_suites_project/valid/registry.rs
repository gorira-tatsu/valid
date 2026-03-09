include!("models/suite_policy.rs");

use valid::{registry::run_registry_cli, valid_models};

pub fn main() {
    run_registry_cli(valid_models![
        "suite-policy" => SuitePolicyModel,
    ]);
}
