#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <project-manifest> <model> <property-id>" >&2
  exit 2
fi

project_manifest="$1"
model="$2"
property_id="$3"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
artifact_dir="${repo_root}/template-artifacts/inspect-check"

rm -rf "${artifact_dir}"
mkdir -p "${artifact_dir}"

cargo run --quiet --manifest-path "${repo_root}/Cargo.toml" --bin cargo-valid -- \
  --manifest-path "${project_manifest}" inspect "${model}" --json \
  | tee "${artifact_dir}/inspect.json"

cargo run --quiet --manifest-path "${repo_root}/Cargo.toml" --bin cargo-valid -- \
  --manifest-path "${project_manifest}" check "${model}" --property="${property_id}" --json \
  | tee "${artifact_dir}/check.json"
