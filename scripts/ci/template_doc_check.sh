#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <model-file> <output-path>" >&2
  exit 2
fi

model_file="$1"
output_path="$2"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
artifact_dir="${repo_root}/template-artifacts/doc-check"

rm -rf "${artifact_dir}"
mkdir -p "${artifact_dir}"
mkdir -p "$(dirname "${output_path}")"

cargo run --quiet --manifest-path "${repo_root}/Cargo.toml" --features verification-runtime --bin valid -- \
  doc "${model_file}" --write="${output_path}" --json \
  | tee "${artifact_dir}/doc-write.json"

cargo run --quiet --manifest-path "${repo_root}/Cargo.toml" --features verification-runtime --bin valid -- \
  doc "${model_file}" --write="${output_path}" --check --json \
  | tee "${artifact_dir}/doc-check.json"

cp "${output_path}" "${artifact_dir}/"
