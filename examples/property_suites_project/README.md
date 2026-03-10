# Property Suites Example

This small project demonstrates project-level `critical_properties` and named
`property_suites`.

First commands to try:

```sh
cargo valid --manifest-path examples/property_suites_project/Cargo.toml suite --critical --json
cargo valid --manifest-path examples/property_suites_project/Cargo.toml suite --suite=smoke --json
cargo valid --manifest-path examples/property_suites_project/Cargo.toml capabilities --backend=sat-varisat --json
cargo valid --manifest-path examples/property_suites_project/Cargo.toml selfcheck --json
```

What to look for:

- `suite --critical` shows the project-level contract surface that CI would
  normally gate on.
- `capabilities --backend=sat-varisat` shows whether the preferred SAT path is
  compiled in and available for this project.
- `selfcheck --json` shows backend-aware readiness and parity information
  before you depend on solver-backed runs in automation.
