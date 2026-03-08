#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo "usage: $0 <model-file> <property-id> <actions> <runner>" >&2
  exit 2
fi

model_file="$1"
property_id="$2"
actions="$3"
runner="$4"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
artifact_dir="${repo_root}/template-artifacts/conformance"

rm -rf "${artifact_dir}"
mkdir -p "${artifact_dir}"

cargo run --quiet --manifest-path "${repo_root}/Cargo.toml" --features verification-runtime --bin valid -- \
  conformance "${model_file}" --property="${property_id}" --actions="${actions}" --runner="${runner}" --json \
  | tee "${artifact_dir}/conformance.json"
