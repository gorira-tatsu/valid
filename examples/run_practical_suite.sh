#!/bin/sh
set -eu

registry="examples/practical_use_cases_registry.rs"

run_valid() {
  cargo run --bin cargo-valid -- --file "$registry" "$@"
}

echo "# practical use-case suite"
run_valid list --json

for model in prod-deploy-safe refund-control data-export-control; do
  echo "## $model"
  run_valid inspect "$model" --json
  run_valid lint "$model" --json
  run_valid check "$model" --json
  run_valid coverage "$model" --json
done

echo "## breakglass-access-regression"
run_valid inspect breakglass-access-regression --json
run_valid lint breakglass-access-regression --json
set +e
run_valid check breakglass-access-regression --json
status=$?
set -e
if [ "$status" -ne 2 ]; then
  echo "expected breakglass-access-regression to fail verification, got exit ${status}" >&2
  exit 1
fi
run_valid explain breakglass-access-regression --json
run_valid coverage breakglass-access-regression --json

echo "# optional generated regression assets"
echo "cargo run --bin cargo-valid -- --file $registry testgen breakglass-access-regression --strategy=counterexample --json"
echo "cargo run --bin cargo-valid -- --file $registry testgen refund-control --strategy=path --json"
