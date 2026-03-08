# CI Workflow Templates

This directory ships versioned CI patterns for the `valid` product surface.
The GitHub Actions workflows under [`.github/workflows/`](../../.github/workflows/)
are the executable templates, while the shell scripts under
[`scripts/ci/`](../../scripts/ci/) provide the same command sequences for
systems that are not GitHub Actions.

## Templates

- `template-cargo-valid-inspect-check.yml`
  Build `cargo-valid`, run `inspect`, then run `check` for one property.
  Artifacts land under `template-artifacts/inspect-check/`.
- `template-cargo-valid-testgen.yml`
  Run `cargo valid generate-tests` and upload both the JSON response and the
  generated Rust files from `generated-tests/`.
- `template-valid-conformance.yml`
  Build `valid`, run `valid conformance`, and upload the JSON report.
- `template-valid-doc-check.yml`
  Build `valid`, render a Markdown doc, rerun with `--check`, and upload both
  JSON responses plus the generated Markdown file.

## Fixture Coverage

The repository validates these templates through
[`ci-workflow-templates.yml`](../../.github/workflows/ci-workflow-templates.yml).
That workflow runs each template against the fixture project in
[`tests/fixtures/projects/ci_template_project/`](../../tests/fixtures/projects/ci_template_project/).

The fixture intentionally covers the issue acceptance surface:

- `cargo valid inspect`
- `cargo valid check`
- `cargo valid generate-tests`
- `valid conformance`
- `valid doc --check`

## Expected Artifacts

- `template-artifacts/inspect-check/inspect.json`
- `template-artifacts/inspect-check/check.json`
- `template-artifacts/testgen/testgen.json`
- `template-artifacts/testgen/generated-tests/*.rs`
- `template-artifacts/conformance/conformance.json`
- `template-artifacts/doc-check/doc-write.json`
- `template-artifacts/doc-check/doc-check.json`
- `template-artifacts/doc-check/*.md`

Consumers should treat these files as versioned templates: copy the workflow
shape into a repository, or copy the shell command sequences into another CI
system while preserving the same artifacts.
