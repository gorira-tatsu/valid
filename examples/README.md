# Examples

`examples/` is intentionally small. It only contains user-facing Rust DSL
registries that are easy to read and debug.

Every example should start with a `/* ... */` block comment that explains:

- what business rule or finite-state contract is being modeled
- which properties are expected to pass or fail
- which command to run first
- why the example exists

Current examples:

- `valid_models.rs`
  Minimal step-first registry with `counter` and `failing-counter`.
- `fizzbuzz.rs`
  Small declarative arithmetic model using grouped `on Action { ... }`.
- `tenant_relation_registry.rs`
  Small declarative relation/map model for tenant membership and tenant plan checks.
- `iam_transition_registry.rs`
  Small declarative policy model with explicit path tags.
- `saas_multi_tenant_registry.rs`
  Medium-sized grouped example for tenant isolation and shared-service access.

Heavy or fixture-like inputs live elsewhere:

- `benchmarks/registries/`
  Larger practical and scale-oriented registries for performance and stress runs.
- `tests/fixtures/models/`
  Legacy `.valid` frontend fixtures.
- `tests/fixtures/solvers/`
  Mock solver scripts used by CLI and solver integration tests.
- `tests/fixtures/domain/`
  Shared domain helpers used only by integration tests.

Typical commands:

```sh
cargo valid --registry examples/valid_models.rs models
cargo valid --registry examples/fizzbuzz.rs verify fizzbuzz --property=P_FIZZBUZZ_DIVISIBLE_BY_BOTH
cargo valid --registry examples/tenant_relation_registry.rs inspect tenant-relation-safe
cargo valid --registry examples/tenant_relation_registry.rs verify tenant-relation-regression --property=P_NO_CROSS_TENANT_ACCESS
cargo valid --registry examples/iam_transition_registry.rs graph iam-access
cargo valid --registry examples/saas_multi_tenant_registry.rs verify tenant-isolation-regression --property=P_NO_CROSS_TENANT_ACCESS
```

Project-first flow still scaffolds `examples/valid_models.rs`:

```sh
cargo valid init
cargo valid models
cargo valid inspect counter
cargo valid verify failing-counter
```

Generated tests are written under `generated-tests/`, not under `tests/`.
