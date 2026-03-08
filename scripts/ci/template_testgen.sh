#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 3 || $# -gt 4 ]]; then
  echo "usage: $0 <project-manifest> <model> <strategy> [property-id]" >&2
  exit 2
fi

project_manifest="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
model="$2"
strategy="$3"
property_id="${4:-}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
artifact_dir="${repo_root}/template-artifacts/testgen"
project_dir="$(cd "$(dirname "${project_manifest}")" && pwd)"

rm -rf "${artifact_dir}"
mkdir -p "${artifact_dir}"
find "${project_dir}/generated-tests" -maxdepth 1 -type f -name '*.rs' -delete 2>/dev/null || true

cmd=(
  cargo run --quiet --manifest-path "${repo_root}/Cargo.toml" --bin cargo-valid --
  --manifest-path "${project_manifest}" generate-tests "${model}" --strategy="${strategy}" --json
)

if [[ -n "${property_id}" ]]; then
  cmd+=(--property="${property_id}")
fi

(
  cd "${project_dir}"
  "${cmd[@]}" | tee "${artifact_dir}/testgen.json"
)

generated_dir="${project_dir}/generated-tests"
generated_count="$(find "${generated_dir}" -maxdepth 1 -type f -name '*.rs' | wc -l | tr -d ' ')"
if [[ "${generated_count}" == "0" ]]; then
  echo "expected generated tests under ${generated_dir}" >&2
  exit 1
fi

mkdir -p "${artifact_dir}/generated-tests"
find "${generated_dir}" -maxdepth 1 -type f -name '*.rs' -exec cp {} "${artifact_dir}/generated-tests/" \;
