# Migration Guide

Use this page when a model needs to move from an older or weaker form toward
the current canonical path.

Typical migrations:

- `step` to declarative `transitions`
- repeated guard logic to named `predicates`
- fixture ladders to `scenarios`
- large property dumps to `critical_properties` and named suites

## Migration order

1. Inspect the current model.
2. Run lint/readiness to see capability or maintainability constraints.
3. Preserve public identifiers where possible:
   - model id
   - action ids
   - property ids
4. Move one concern at a time.
5. Re-run inspect, lint, and explain before verify claims.

## Common migration targets

### `step` to `transitions`

- keep the same action vocabulary
- convert hidden frame conditions into explicit `..state`
- preserve property intent before optimizing syntax

### Repeated conditions to `predicates`

- extract repeated guards/property clauses into one name
- keep predicates state-based and side-effect free
- prefer domain vocabulary over low-level arithmetic names

### Setup-heavy models to `scenarios`

- prefer scenario-restricted checks over long setup-only action ladders
- keep `role = setup` for compatibility/reporting when needed
- separate state-space restriction from business transition semantics

## Commands

```sh
cargo valid inspect <model>
cargo valid lint <model>
cargo valid migrate <model>
```

In MCP-driven flows, start with:

- `migrate_step_to_transitions`
- `valid_inspect`
- `valid_lint`

## Migration review questions

- Did the migration preserve action ids and property ids?
- Did the new form improve `inspect`, `graph`, and `explain` output?
- Did the migration reduce readiness or lint findings?
- Did the model become easier to review, not only shorter?

## Next read

- [AI Authoring Guide](./authoring-guide.md)
- [Examples Curriculum](./examples-curriculum.md)
- [Review Workflow](./review-workflow.md)
