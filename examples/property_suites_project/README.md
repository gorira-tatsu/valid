# Property Suites Example

This small project demonstrates project-level `critical_properties` and named
`property_suites`.

Run these commands from inside `examples/property_suites_project/`:

```sh
valid models
valid suite --critical --json
valid suite --suite=smoke --json
valid capabilities --backend=sat-varisat --json
valid selfcheck --json
```

What to look for:

- `valid models` shows the project-first surface that onboarding now teaches.
- `suite --critical` shows the project-level contract surface that CI would
  normally gate on.
- `capabilities --backend=sat-varisat` shows whether the preferred SAT path is
  compiled in and available for this project.
- `selfcheck --json` shows backend-aware readiness and parity information
  before you depend on solver-backed runs in automation.
