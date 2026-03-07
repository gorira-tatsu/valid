use std::time::Instant;

use crate::engine::{CheckOutcome, RunStatus};
use crate::support::json::{parse_json, require_object, JsonValue};

const MIN_ELAPSED_BASELINE_FOR_REGRESSION_MS: u128 = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkIteration {
    pub iteration: usize,
    pub elapsed_ms: u128,
    pub status: String,
    pub exit_code: i32,
    pub explored_states: Option<usize>,
    pub explored_transitions: Option<usize>,
    pub trace_steps: Option<usize>,
    pub run_id: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkSummary {
    pub schema_version: String,
    pub request_id: String,
    pub model_id: String,
    pub backend: String,
    pub property_id: Option<String>,
    pub repeat: usize,
    pub total_elapsed_ms: u128,
    pub min_elapsed_ms: u128,
    pub max_elapsed_ms: u128,
    pub average_elapsed_ms: u128,
    pub pass_count: usize,
    pub fail_count: usize,
    pub unknown_count: usize,
    pub error_count: usize,
    pub iterations: Vec<BenchmarkIteration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkComparison {
    pub baseline_path: String,
    pub threshold_percent: u32,
    pub status: String,
    pub regressions: Vec<String>,
    pub baseline_average_elapsed_ms: u128,
    pub baseline_average_explored_states: Option<u128>,
    pub baseline_average_explored_transitions: Option<u128>,
}

pub fn benchmark_check_outcomes<F>(
    request_id: &str,
    model_id: &str,
    backend: &str,
    property_id: Option<&str>,
    repeat: usize,
    mut runner: F,
) -> BenchmarkSummary
where
    F: FnMut(usize) -> CheckOutcome,
{
    let repeat = repeat.max(1);
    let mut iterations = Vec::with_capacity(repeat);
    let mut total_elapsed_ms = 0u128;
    let mut min_elapsed_ms = u128::MAX;
    let mut max_elapsed_ms = 0u128;
    let mut pass_count = 0usize;
    let mut fail_count = 0usize;
    let mut unknown_count = 0usize;
    let mut error_count = 0usize;

    for iteration in 0..repeat {
        let started = Instant::now();
        let outcome = runner(iteration);
        let elapsed_ms = started.elapsed().as_millis();
        total_elapsed_ms += elapsed_ms;
        min_elapsed_ms = min_elapsed_ms.min(elapsed_ms);
        max_elapsed_ms = max_elapsed_ms.max(elapsed_ms);

        let record = match outcome {
            CheckOutcome::Completed(result) => {
                let (status, exit_code) = match result.status {
                    RunStatus::Pass => {
                        pass_count += 1;
                        ("PASS".to_string(), 0)
                    }
                    RunStatus::Fail => {
                        fail_count += 1;
                        ("FAIL".to_string(), 2)
                    }
                    RunStatus::Unknown => {
                        unknown_count += 1;
                        ("UNKNOWN".to_string(), 4)
                    }
                };
                BenchmarkIteration {
                    iteration,
                    elapsed_ms,
                    status,
                    exit_code,
                    explored_states: Some(result.explored_states),
                    explored_transitions: Some(result.explored_transitions),
                    trace_steps: result.trace.as_ref().map(|trace| trace.steps.len()),
                    run_id: Some(result.manifest.run_id),
                    summary: Some(result.property_result.summary),
                }
            }
            CheckOutcome::Errored(error) => {
                error_count += 1;
                BenchmarkIteration {
                    iteration,
                    elapsed_ms,
                    status: "ERROR".to_string(),
                    exit_code: 3,
                    explored_states: None,
                    explored_transitions: None,
                    trace_steps: None,
                    run_id: Some(error.manifest.run_id),
                    summary: error.diagnostics.first().map(|d| d.message.clone()),
                }
            }
        };
        iterations.push(record);
    }

    BenchmarkSummary {
        schema_version: "1.0.0".to_string(),
        request_id: request_id.to_string(),
        model_id: model_id.to_string(),
        backend: backend.to_string(),
        property_id: property_id.map(str::to_string),
        repeat,
        total_elapsed_ms,
        min_elapsed_ms: if iterations.is_empty() {
            0
        } else {
            min_elapsed_ms
        },
        max_elapsed_ms,
        average_elapsed_ms: total_elapsed_ms / repeat as u128,
        pass_count,
        fail_count,
        unknown_count,
        error_count,
        iterations,
    }
}

pub fn render_benchmark_json(summary: &BenchmarkSummary) -> String {
    let iterations = summary
        .iterations
        .iter()
        .map(|iteration| {
            format!(
                "{{\"iteration\":{},\"elapsed_ms\":{},\"status\":\"{}\",\"exit_code\":{},\"explored_states\":{},\"explored_transitions\":{},\"trace_steps\":{},\"run_id\":{},\"summary\":{}}}",
                iteration.iteration,
                iteration.elapsed_ms,
                iteration.status,
                iteration.exit_code,
                iteration
                    .explored_states
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "null".to_string()),
                iteration
                    .explored_transitions
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "null".to_string()),
                iteration
                    .trace_steps
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "null".to_string()),
                iteration
                    .run_id
                    .as_ref()
                    .map(|value| format!("\"{}\"", escape_json(value)))
                    .unwrap_or_else(|| "null".to_string()),
                iteration
                    .summary
                    .as_ref()
                    .map(|value| format!("\"{}\"", escape_json(value)))
                    .unwrap_or_else(|| "null".to_string()),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"schema_version\":\"{}\",\"request_id\":\"{}\",\"model_id\":\"{}\",\"backend\":\"{}\",\"property_id\":{},\"repeat\":{},\"total_elapsed_ms\":{},\"min_elapsed_ms\":{},\"max_elapsed_ms\":{},\"average_elapsed_ms\":{},\"pass_count\":{},\"fail_count\":{},\"unknown_count\":{},\"error_count\":{},\"iterations\":[{}]}}",
        escape_json(&summary.schema_version),
        escape_json(&summary.request_id),
        escape_json(&summary.model_id),
        escape_json(&summary.backend),
        summary
            .property_id
            .as_ref()
            .map(|value| format!("\"{}\"", escape_json(value)))
            .unwrap_or_else(|| "null".to_string()),
        summary.repeat,
        summary.total_elapsed_ms,
        summary.min_elapsed_ms,
        summary.max_elapsed_ms,
        summary.average_elapsed_ms,
        summary.pass_count,
        summary.fail_count,
        summary.unknown_count,
        summary.error_count,
        iterations
    )
}

pub fn render_benchmark_text(summary: &BenchmarkSummary) -> String {
    let mut out = String::new();
    out.push_str(&format!("model_id: {}\n", summary.model_id));
    out.push_str(&format!("backend: {}\n", summary.backend));
    if let Some(property_id) = &summary.property_id {
        out.push_str(&format!("property_id: {property_id}\n"));
    }
    out.push_str(&format!("repeat: {}\n", summary.repeat));
    out.push_str(&format!(
        "elapsed_ms: total={} avg={} min={} max={}\n",
        summary.total_elapsed_ms,
        summary.average_elapsed_ms,
        summary.min_elapsed_ms,
        summary.max_elapsed_ms
    ));
    out.push_str(&format!(
        "status_counts: pass={} fail={} unknown={} error={}\n",
        summary.pass_count, summary.fail_count, summary.unknown_count, summary.error_count
    ));
    out.push_str("iterations:\n");
    for iteration in &summary.iterations {
        out.push_str(&format!(
            "- #{} status={} exit_code={} elapsed_ms={}",
            iteration.iteration, iteration.status, iteration.exit_code, iteration.elapsed_ms
        ));
        if let Some(states) = iteration.explored_states {
            out.push_str(&format!(" explored_states={states}"));
        }
        if let Some(transitions) = iteration.explored_transitions {
            out.push_str(&format!(" explored_transitions={transitions}"));
        }
        if let Some(trace_steps) = iteration.trace_steps {
            out.push_str(&format!(" trace_steps={trace_steps}"));
        }
        if let Some(summary) = &iteration.summary {
            out.push_str(&format!(" summary={summary}"));
        }
        out.push('\n');
    }
    out
}

pub fn render_benchmark_comparison_json(comparison: &BenchmarkComparison) -> String {
    format!(
        "{{\"baseline_path\":\"{}\",\"threshold_percent\":{},\"status\":\"{}\",\"regressions\":[{}],\"baseline_average_elapsed_ms\":{},\"baseline_average_explored_states\":{},\"baseline_average_explored_transitions\":{}}}",
        escape_json(&comparison.baseline_path),
        comparison.threshold_percent,
        escape_json(&comparison.status),
        comparison
            .regressions
            .iter()
            .map(|item| format!("\"{}\"", escape_json(item)))
            .collect::<Vec<_>>()
            .join(","),
        comparison.baseline_average_elapsed_ms,
        comparison
            .baseline_average_explored_states
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string()),
        comparison
            .baseline_average_explored_transitions
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string()),
    )
}

pub fn render_benchmark_comparison_text(comparison: &BenchmarkComparison) -> String {
    let mut out = String::new();
    out.push_str(&format!("baseline_path: {}\n", comparison.baseline_path));
    out.push_str(&format!(
        "baseline_threshold_percent: {}\n",
        comparison.threshold_percent
    ));
    out.push_str(&format!("baseline_status: {}\n", comparison.status));
    out.push_str(&format!(
        "baseline_average_elapsed_ms: {}\n",
        comparison.baseline_average_elapsed_ms
    ));
    if let Some(value) = comparison.baseline_average_explored_states {
        out.push_str(&format!("baseline_average_explored_states: {value}\n"));
    }
    if let Some(value) = comparison.baseline_average_explored_transitions {
        out.push_str(&format!("baseline_average_explored_transitions: {value}\n"));
    }
    if comparison.regressions.is_empty() {
        out.push_str("baseline_regressions: none\n");
    } else {
        out.push_str("baseline_regressions:\n");
        for regression in &comparison.regressions {
            out.push_str(&format!("- {regression}\n"));
        }
    }
    out
}

pub fn parse_benchmark_summary_json(body: &str) -> Result<BenchmarkSummary, String> {
    let root = parse_json(body)?;
    let object = require_object(&root, "benchmark summary")?;
    let iterations = match object.get("iterations") {
        Some(JsonValue::Array(items)) => items
            .iter()
            .map(parse_iteration_json)
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err("iterations must be an array".to_string()),
    };
    Ok(BenchmarkSummary {
        schema_version: string_field(object, "schema_version")?.to_string(),
        request_id: string_field(object, "request_id")?.to_string(),
        model_id: string_field(object, "model_id")?.to_string(),
        backend: string_field(object, "backend")?.to_string(),
        property_id: optional_string_field(object, "property_id")?,
        repeat: number_field(object, "repeat")? as usize,
        total_elapsed_ms: number_field(object, "total_elapsed_ms")? as u128,
        min_elapsed_ms: number_field(object, "min_elapsed_ms")? as u128,
        max_elapsed_ms: number_field(object, "max_elapsed_ms")? as u128,
        average_elapsed_ms: number_field(object, "average_elapsed_ms")? as u128,
        pass_count: number_field(object, "pass_count")? as usize,
        fail_count: number_field(object, "fail_count")? as usize,
        unknown_count: number_field(object, "unknown_count")? as usize,
        error_count: number_field(object, "error_count")? as usize,
        iterations,
    })
}

pub fn compare_benchmark_to_baseline(
    summary: &BenchmarkSummary,
    baseline_path: &str,
    baseline: &BenchmarkSummary,
    threshold_percent: u32,
) -> BenchmarkComparison {
    let mut regressions = Vec::new();
    if status_profile(summary) != status_profile(baseline) {
        regressions.push(format!(
            "status profile changed: baseline pass/fail/unknown/error = {:?}, current = {:?}",
            status_profile(baseline),
            status_profile(summary)
        ));
    }
    if baseline.average_elapsed_ms >= MIN_ELAPSED_BASELINE_FOR_REGRESSION_MS
        && exceeds_regression(
            summary.average_elapsed_ms,
            baseline.average_elapsed_ms,
            threshold_percent,
        )
    {
        regressions.push(format!(
            "average_elapsed_ms regressed from {} to {}",
            baseline.average_elapsed_ms, summary.average_elapsed_ms
        ));
    }

    let baseline_avg_states = average_optional_metric(&baseline.iterations, |iteration| {
        iteration.explored_states.map(|value| value as u128)
    });
    let current_avg_states = average_optional_metric(&summary.iterations, |iteration| {
        iteration.explored_states.map(|value| value as u128)
    });
    if let (Some(current), Some(reference)) = (current_avg_states, baseline_avg_states) {
        if exceeds_regression(current, reference, threshold_percent) {
            regressions.push(format!(
                "average_explored_states regressed from {} to {}",
                reference, current
            ));
        }
    }

    let baseline_avg_transitions = average_optional_metric(&baseline.iterations, |iteration| {
        iteration.explored_transitions.map(|value| value as u128)
    });
    let current_avg_transitions = average_optional_metric(&summary.iterations, |iteration| {
        iteration.explored_transitions.map(|value| value as u128)
    });
    if let (Some(current), Some(reference)) = (current_avg_transitions, baseline_avg_transitions) {
        if exceeds_regression(current, reference, threshold_percent) {
            regressions.push(format!(
                "average_explored_transitions regressed from {} to {}",
                reference, current
            ));
        }
    }

    BenchmarkComparison {
        baseline_path: baseline_path.to_string(),
        threshold_percent,
        status: if regressions.is_empty() {
            "ok".to_string()
        } else {
            "regressed".to_string()
        },
        regressions,
        baseline_average_elapsed_ms: baseline.average_elapsed_ms,
        baseline_average_explored_states: baseline_avg_states,
        baseline_average_explored_transitions: baseline_avg_transitions,
    }
}

fn exceeds_regression(current: u128, baseline: u128, threshold_percent: u32) -> bool {
    if baseline == 0 {
        return current > 0;
    }
    current > baseline.saturating_mul(100 + threshold_percent as u128) / 100
}

fn average_optional_metric<F>(items: &[BenchmarkIteration], metric: F) -> Option<u128>
where
    F: Fn(&BenchmarkIteration) -> Option<u128>,
{
    let mut total = 0u128;
    let mut count = 0u128;
    for value in items.iter().filter_map(metric) {
        total += value;
        count += 1;
    }
    if count == 0 {
        None
    } else {
        Some(total / count)
    }
}

fn status_profile(summary: &BenchmarkSummary) -> (bool, bool, bool, bool) {
    (
        summary.pass_count > 0,
        summary.fail_count > 0,
        summary.unknown_count > 0,
        summary.error_count > 0,
    )
}

fn parse_iteration_json(value: &JsonValue) -> Result<BenchmarkIteration, String> {
    let object = require_object(value, "benchmark iteration")?;
    Ok(BenchmarkIteration {
        iteration: number_field(object, "iteration")? as usize,
        elapsed_ms: number_field(object, "elapsed_ms")? as u128,
        status: string_field(object, "status")?.to_string(),
        exit_code: number_field(object, "exit_code")? as i32,
        explored_states: optional_number_field(object, "explored_states")?
            .map(|value| value as usize),
        explored_transitions: optional_number_field(object, "explored_transitions")?
            .map(|value| value as usize),
        trace_steps: optional_number_field(object, "trace_steps")?.map(|value| value as usize),
        run_id: optional_string_field(object, "run_id")?,
        summary: optional_string_field(object, "summary")?,
    })
}

fn string_field<'a>(
    object: &'a std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<&'a str, String> {
    match object.get(field) {
        Some(JsonValue::String(value)) => Ok(value.as_str()),
        _ => Err(format!("{field} must be a string")),
    }
}

fn optional_string_field(
    object: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<Option<String>, String> {
    match object.get(field) {
        Some(JsonValue::String(value)) => Ok(Some(value.clone())),
        Some(JsonValue::Null) | None => Ok(None),
        _ => Err(format!("{field} must be a string or null")),
    }
}

fn number_field(
    object: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<u64, String> {
    match object.get(field) {
        Some(JsonValue::Number(value)) => Ok(*value),
        _ => Err(format!("{field} must be an unsigned integer")),
    }
}

fn optional_number_field(
    object: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<Option<u64>, String> {
    match object.get(field) {
        Some(JsonValue::Number(value)) => Ok(Some(*value)),
        Some(JsonValue::Null) | None => Ok(None),
        _ => Err(format!("{field} must be an unsigned integer or null")),
    }
}

fn escape_json(input: &str) -> String {
    input
        .chars()
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{
        engine::{
            AssuranceLevel, BackendKind, CheckOutcome, ExplicitRunResult, PropertyResult,
            RunManifest, RunStatus,
        },
        evidence::EvidenceTrace,
    };

    use super::{
        benchmark_check_outcomes, compare_benchmark_to_baseline, parse_benchmark_summary_json,
        render_benchmark_comparison_json, render_benchmark_comparison_text, render_benchmark_json,
        render_benchmark_text,
    };

    #[test]
    fn summarizes_completed_benchmark_runs() {
        let summary = benchmark_check_outcomes(
            "req-bench",
            "BenchModel",
            "explicit",
            Some("P_SAFE"),
            2,
            |_| {
                CheckOutcome::Completed(ExplicitRunResult {
                    manifest: RunManifest {
                        request_id: "req".to_string(),
                        run_id: "run".to_string(),
                        schema_version: "1.0.0".to_string(),
                        source_hash: "sha256:src".to_string(),
                        contract_hash: "sha256:contract".to_string(),
                        engine_version: "0.1.0".to_string(),
                        backend_name: BackendKind::Explicit,
                        backend_version: "0.1.0".to_string(),
                        seed: None,
                    },
                    status: RunStatus::Pass,
                    assurance_level: AssuranceLevel::Complete,
                    explored_states: 3,
                    explored_transitions: 5,
                    property_result: PropertyResult {
                        property_id: "P_SAFE".to_string(),
                        property_kind: crate::ir::PropertyKind::Invariant,
                        status: RunStatus::Pass,
                        assurance_level: AssuranceLevel::Complete,
                        reason_code: Some("PASS".to_string()),
                        unknown_reason: None,
                        terminal_state_id: None,
                        evidence_id: None,
                        summary: "safe".to_string(),
                    },
                    trace: None::<EvidenceTrace>,
                })
            },
        );
        assert_eq!(summary.pass_count, 2);
        assert_eq!(summary.repeat, 2);
        assert!(render_benchmark_json(&summary).contains("\"pass_count\":2"));
        assert!(render_benchmark_text(&summary).contains("status_counts: pass=2"));
    }

    #[test]
    fn parses_and_compares_benchmark_summaries() {
        let baseline = benchmark_check_outcomes(
            "req-bench",
            "Counter",
            "explicit",
            Some("P_SAFE"),
            2,
            |_| completed_benchmark_outcome("run-baseline", RunStatus::Pass, 4, 8),
        );
        let current = benchmark_check_outcomes(
            "req-bench",
            "Counter",
            "explicit",
            Some("P_SAFE"),
            2,
            |_| completed_benchmark_outcome("run-current", RunStatus::Pass, 6, 12),
        );
        let parsed = parse_benchmark_summary_json(&render_benchmark_json(&baseline)).unwrap();
        let comparison = compare_benchmark_to_baseline(
            &current,
            "artifacts/benchmarks/counter.json",
            &parsed,
            0,
        );
        assert_eq!(comparison.status, "regressed");
        assert!(comparison
            .regressions
            .iter()
            .any(|item| item.contains("average_explored_states")));
        assert!(render_benchmark_comparison_json(&comparison).contains("\"status\":\"regressed\""));
        assert!(render_benchmark_comparison_text(&comparison).contains("baseline_regressions"));
    }

    #[test]
    fn ignores_elapsed_regression_for_tiny_baselines() {
        let baseline = benchmark_check_outcomes(
            "req-bench",
            "Counter",
            "explicit",
            Some("P_SAFE"),
            1,
            |_| completed_benchmark_outcome("run-baseline", RunStatus::Pass, 4, 8),
        );
        let mut current = baseline.clone();
        current.total_elapsed_ms = 50;
        current.min_elapsed_ms = 50;
        current.max_elapsed_ms = 50;
        current.average_elapsed_ms = 50;
        current.iterations[0].elapsed_ms = 50;
        let comparison = compare_benchmark_to_baseline(
            &current,
            "benchmarks/baselines/counter.json",
            &baseline,
            25,
        );
        assert_eq!(comparison.status, "ok");
        assert!(comparison.regressions.is_empty());
    }

    fn completed_benchmark_outcome(
        run_id: &str,
        status: RunStatus,
        explored_states: usize,
        explored_transitions: usize,
    ) -> CheckOutcome {
        CheckOutcome::Completed(ExplicitRunResult {
            manifest: RunManifest {
                request_id: "req".to_string(),
                run_id: run_id.to_string(),
                schema_version: "1.0.0".to_string(),
                source_hash: "sha256:src".to_string(),
                contract_hash: "sha256:contract".to_string(),
                engine_version: "0.1.0".to_string(),
                backend_name: BackendKind::Explicit,
                backend_version: "0.1.0".to_string(),
                seed: None,
            },
            status,
            assurance_level: AssuranceLevel::Complete,
            explored_states,
            explored_transitions,
            property_result: PropertyResult {
                property_id: "P_SAFE".to_string(),
                property_kind: crate::ir::PropertyKind::Invariant,
                status,
                assurance_level: AssuranceLevel::Complete,
                reason_code: Some("PASS".to_string()),
                unknown_reason: None,
                terminal_state_id: None,
                evidence_id: None,
                summary: "safe".to_string(),
            },
            trace: None::<EvidenceTrace>,
        })
    }
}
