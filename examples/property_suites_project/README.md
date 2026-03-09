# Property Suites Example

This small project demonstrates project-level `critical_properties` and named
`property_suites`.

First commands to try:

```sh
cargo valid --manifest-path examples/property_suites_project/Cargo.toml suite --critical --json
cargo valid --manifest-path examples/property_suites_project/Cargo.toml suite --suite=smoke --json
```
