#!/usr/bin/env bash

set -euo pipefail

usage() {
  cat <<'EOF' >&2
usage: ./scripts/benchmark-suite.sh [compare|record]

Environment:
  VALID_BENCHMARK_THRESHOLD_PERCENT  Regression threshold for compare mode (default: 25)
EOF
  exit 64
}

mode="${1:-compare}"
case "${mode}" in
  compare|record) ;;
  *) usage ;;
esac

threshold_percent="${VALID_BENCHMARK_THRESHOLD_PERCENT:-25}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
summary_file="${GITHUB_STEP_SUMMARY:-}"
overall_status=0

mkdir -p "${repo_root}/artifacts/benchmarks/ci"
cd "${repo_root}"

if [[ -n "${summary_file}" ]]; then
  {
    echo "## Benchmark Regression Gate"
    echo
    echo "- mode: \`${mode}\`"
    echo "- threshold_percent: \`${threshold_percent}\`"
    echo
  } >> "${summary_file}"
fi

aggregate_status() {
  local current="$1"
  local next="$2"

  if (( next == 0 )); then
    printf '%s\n' "${current}"
    return
  fi
  if (( current == 0 )); then
    printf '%s\n' "${next}"
    return
  fi
  if (( current == 3 || next == 3 )); then
    printf '3\n'
    return
  fi
  if (( current == 5 || next == 5 )); then
    printf '5\n'
    return
  fi
  if (( current == 4 || next == 4 )); then
    printf '4\n'
    return
  fi
  printf '2\n'
}

append_summary() {
  local label="$1"
  local command_string="$2"
  local status_code="$3"
  local baseline_status="$4"
  local log_path="$5"

  [[ -n "${summary_file}" ]] || return 0

  {
    echo "### ${label}"
    echo
    echo "- command: \`${command_string}\`"
    echo "- exit_code: \`${status_code}\`"
    echo "- baseline_status: \`${baseline_status}\`"
    echo "- log: \`${log_path}\`"
    echo
    echo '```text'
    cat "${log_path}"
    echo '```'
    echo
  } >> "${summary_file}"
}

run_case() {
  local label="$1"
  local log_slug="$2"
  shift 2

  local log_path="artifacts/benchmarks/ci/${log_slug}.log"
  local -a command=(cargo run --quiet --bin cargo-valid -- "$@" "--baseline=${mode}")
  if [[ "${mode}" == "compare" ]]; then
    command+=("--threshold-percent=${threshold_percent}")
  fi

  local command_string
  command_string="$(printf '%q ' "${command[@]}")"
  command_string="${command_string% }"

  echo "== ${label} =="
  echo "+ ${command_string}"

  set +e
  local output
  output="$("${command[@]}" 2>&1)"
  local status_code=$?
  set -e

  printf '%s\n' "${output}" | tee "${log_path}"

  local baseline_status
  baseline_status="$(awk -F': ' '/^baseline_status:/ { print $2; exit }' "${log_path}")"
  baseline_status="${baseline_status:-n/a}"

  if (( status_code == 5 )) || [[ "${baseline_status}" == "regressed" ]]; then
    echo "::warning title=Benchmark regression (${label})::Regression threshold ${threshold_percent}% exceeded. See ${log_path}."
  elif [[ "${baseline_status}" == "missing" || "${baseline_status}" == "invalid" ]]; then
    echo "::warning title=Benchmark baseline ${baseline_status} (${label})::Baseline comparison could not complete cleanly. See ${log_path}."
  elif (( status_code != 0 )); then
    echo "::error title=Benchmark command failed (${label})::Command exited with status ${status_code}. See ${log_path}."
  fi

  append_summary "${label}" "${command_string}" "${status_code}" "${baseline_status}" "${log_path}"
  overall_status="$(aggregate_status "${overall_status}" "${status_code}")"
}

run_case \
  "project counter" \
  "project-counter" \
  benchmark counter --repeat=5
run_case \
  "example failing-counter" \
  "example-failing-counter" \
  --registry examples/valid_models.rs benchmark failing-counter --repeat=1
run_case \
  "practical prod-deploy-safe" \
  "practical-prod-deploy-safe" \
  --registry benchmarks/registries/practical_use_cases_registry.rs benchmark prod-deploy-safe --repeat=5
run_case \
  "practical refund-control" \
  "practical-refund-control" \
  --registry benchmarks/registries/practical_use_cases_registry.rs benchmark refund-control --repeat=5
run_case \
  "practical data-export-control" \
  "practical-data-export-control" \
  --registry benchmarks/registries/practical_use_cases_registry.rs benchmark data-export-control --repeat=5
run_case \
  "enterprise access-review-scale" \
  "enterprise-access-review-scale" \
  --registry benchmarks/registries/enterprise_scale_registry.rs benchmark access-review-scale --repeat=5
run_case \
  "enterprise quota-guardrail-regression" \
  "enterprise-quota-guardrail-regression" \
  --registry benchmarks/registries/enterprise_scale_registry.rs benchmark quota-guardrail-regression --property=P_EXPORT_REQUIRES_BUDGET_DISCIPLINE --repeat=5

exit "${overall_status}"
