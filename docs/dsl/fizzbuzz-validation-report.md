# FizzBuzz Model Validation Report

Target: `examples/fizzbuzz.rs`  
Validation date: March 7, 2026  
DSL guide: `docs/dsl/README.md`

## 1. Model summary

The model encodes the FizzBuzz rules as a finite-state machine and validates
five invariants.

| Item | Content |
| --- | --- |
| State | `i`, `fizz`, `buzz` |
| Actions | 4 transitions |
| Transitions | 4 paths (`fizzbuzz_path`, `fizz_path`, `buzz_path`, `number_path`) |
| Properties | 5 invariants |
| Definition style | Declarative `transitions` (solver-ready path) |

## 2. Per-command results

### 2.1 inspect

- All 7 capability flags are `true`, reaching the highest readiness level
  described in the DSL guide.
- All four transitions parse correctly, including guards, updates, and path
  tags, and lower into IR correctly.
- `reads` / `writes` metadata is reflected correctly in the inspect output.

**Verdict: PASS** - static structural analysis works correctly.

### 2.2 verify (all properties)

- All five properties PASS with `assurance_level: complete`.
- However, **`explored_states: 1` is a problem**. The model should explore 16
  states (`i = 0..15`), but currently stops at the initial state only. See the
  issue section below.

**Verdict: CONDITIONAL PASS** - the reported result is correct, but exploration
depth is suspiciously shallow.

### 2.3 readiness

- `findings: none`, so there are no readiness issues.
- None of the degraded-readiness reasons listed in the DSL guide
  (`opaque_step_closure`, `unsupported_machine_guard_expr`, and similar) are
  triggered.

**Verdict: PASS** - the model is fully solver-ready as a declarative model.

### 2.4 graph

Confirmed working in four output formats:

| Format | Status | Notes |
| --- | --- | --- |
| `mermaid` | OK | `flowchart LR` diagram with all transitions and properties |
| `dot` | OK | Graphviz DOT; guards are diamond nodes, updates are note nodes |
| `json` | OK | Structured output with `schema_version: 1.0.0`; suitable for CI |
| `text` | OK | Text output comparable to `inspect` |
| `svg` | Not checked | skipped due to Graphviz runtime dependency |

**Verdict: PASS** - graph generation works correctly in four formats.

### 2.5 coverage

- Coverage is 0%. Because exploration stops after one state, no transitions are
  actually fired.
- `uncovered_guards=STEP:true` shows that every guard remains unreached.

**Verdict: FAIL** - the coverage feature itself runs, but shallow exploration
prevents meaningful output.

### 2.6 explain

- Since every property passes, there is no counterexample trace to explain.
- That is normal behavior: no violation means nothing to explain.

**Verdict: N/A** - not applicable because there is no failing property. A
deliberately failing property would be useful for validating `explain`.

### 2.7 generate-tests

- No test vectors are generated. As with coverage, shallow exploration leaves
  no material to emit.

**Verdict: FAIL** - the feature runs, but the output is empty because the
search depth is insufficient.

## 3. Detected issue and fix

### 3.1 Initial-state violation in `P_NUMBER_NOT_DIVISIBLE` (fixed)

**Problem**: in the initial state `{i: 0, fizz: false, buzz: false}`, the
invariant fails because `0 % 3 == 0`.

**Cause**: `i = 0` is a pre-start state and should not be treated as an actual
FizzBuzz classification state, but the invariant originally did not account for
that.

**Fix**: add `state.i == 0` as an exclusion condition.

**Lesson**: when the initial state uses zero values, modulo arithmetic can
create unintended divisibility matches. Invariants need to model the special
meaning of the initial state explicitly.

## 4. Conformance against the DSL guide

| DSL guide statement | Result in the FizzBuzz model |
| --- | --- |
| `valid_state!` can define state | OK - `[range = "0..=15"]` works |
| `#[derive(ValidAction)]` can expose reads/writes | OK - reflected in inspect/graph |
| Declarative `transitions` is the canonical path | OK - reaches solver-ready |
| Modulo arithmetic is supported | OK - `%` compiles in guards and updates |
| `tags` are used by coverage and graph | Partial - visible in graph; coverage blocked by shallow exploration |
| `invariant` properties are supported | OK - all five evaluate correctly |
| `valid_models!` supports registry registration | OK - CLI can access the model |

## 5. Assessment as a specification-validation tool

### Strengths

1. **DSL expressiveness**: a model with modulo arithmetic like FizzBuzz can be
   written declaratively and still reach solver-ready status. This confirms that
   the DSL guide’s claim about bounded arithmetic with `+`, `-`, and `%` is
   actually implemented.
2. **Strong structural analysis**: `inspect`, `readiness`, and `graph` are
   reliable. Fields, guards, updates, and tags are parsed and rendered
   accurately.
3. **Multi-format graph output**: Mermaid, DOT, JSON, and text are immediately
   usable. That is practical for CI pipelines and documentation generation.
4. **Readiness model**: staged capability reporting (`parse -> explicit -> ir
   -> solver -> coverage -> explain -> testgen`) gives a useful maturity signal
   for a model.
5. **Early bug detection**: the initial-state issue in
   `P_NUMBER_NOT_DIVISIBLE` was detected immediately, which demonstrates the
   core value of formal verification as a way to expose holes in the
   specification.

### Weaknesses

1. **Shallow exploration depth**: this is the biggest issue. Multiple commands
   report `explored_states: 1`, so a 16-state space is not actually explored.
   `assurance_level: complete` is therefore misleading in practice. Coverage
   and test generation become empty for the same reason.
2. **Coverage usefulness is limited**: because exploration is shallow,
   `transition_coverage_percent=0`, so path-based or tag-based coverage cannot
   be evaluated meaningfully.
3. **Empty test generation output**: `generate-tests --strategy=path` produces
   no vectors. This is likely secondary to the exploration-depth issue.
4. **Difficult to validate `explain` on all-pass models**: when every property
   passes, there is nothing for `explain` to analyze. A deliberately failing
   property or counterexample-injection path would help validate the feature.

### Overall score

| Dimension | Grade |
| --- | --- |
| DSL expressiveness | A - declarative modeling supports modulo arithmetic |
| Static analysis (`inspect` / `readiness` / `graph`) | A - accurate and multi-format |
| Dynamic verification (`verify`) | B- - correct result, but exploration is too shallow |
| Coverage and test generation | C - effectively blocked by limited exploration |
| Error detection power | A - catches real spec bugs quickly |
| CI / automation fit | B+ - JSON output and readiness checks are already useful |

**Overall conclusion**: as a specification-validation tool, `valid` is already
strong in structural analysis and declarative modeling. The main current
bottleneck is state-space exploration depth, and the practical value of
coverage and test generation depends on improving that behavior.
