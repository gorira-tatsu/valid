# Examples

`examples/` is intentionally small. It only contains user-facing Rust DSL
registries that are easy to read and debug.

For broader project layout guidance, see
[`docs/project-organization.md`](../docs/project-organization.md).

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
  Small declarative integration model that demonstrates the shared-state
  pattern for tenant membership plus tenant plan checks.
- `password_policy.rs`
  Small declarative string/password-policy model using `len` and
  `regex_match`. Its strong/weak split is a bounded teaching fixture, not the
  recommended long-term pattern for arbitrary password payloads. Treat it as a
  temporary stand-in for the parameterized-action roadmap, not as a template
  for variant-per-input modeling.
- `iam_transition_registry.rs`
  Small declarative policy model with explicit path tags.
- `saas_multi_tenant_registry.rs`
  Medium-sized integration model that demonstrates the same shared-state
  pattern for tenant isolation and shared-service access.

Heavy or fixture-like inputs live elsewhere:

- `benchmarks/registries/`
  Larger practical and scale-oriented registries for performance and stress runs.
- `tests/fixtures/models/`
  Legacy `.valid` frontend fixtures.
- `tests/fixtures/solvers/`
  Mock solver scripts used by CLI and solver integration tests.
- `tests/fixtures/domain/`
  Shared domain helpers used only by integration tests.
- `src/models/` or another explicit module tree in real projects
  The actual model logic should usually live here, while `examples/` or other
  registry files stay thin and export-focused.

Authoring note:

- Do not treat these examples as a blanket endorsement of action explosion.
- If an example uses multiple action variants for what is conceptually one
  action plus a bounded choice, it is there to keep the teaching example small
  with today's enum-only action surface.
- The intended evolution path is documented in
  [docs/dsl/parameterized-action-roadmap.md](../docs/dsl/parameterized-action-roadmap.md).

Typical commands:

```sh
cargo valid --registry examples/valid_models.rs models
cargo valid --registry examples/fizzbuzz.rs verify fizzbuzz --property=P_FIZZBUZZ_DIVISIBLE_BY_BOTH
cargo valid --registry examples/tenant_relation_registry.rs inspect tenant-relation-safe
cargo valid --registry examples/tenant_relation_registry.rs verify tenant-relation-regression --property=P_NO_CROSS_TENANT_ACCESS
cargo valid --registry examples/password_policy.rs inspect password-policy-safe
cargo valid --registry examples/password_policy.rs verify password-policy-regression --property=P_PASSWORD_POLICY_MATCHES_FLAG
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
Registry files should stay small enough that a reviewer can tell which models
they export without reading pages of transition logic.

The two tenant-oriented registries are the canonical integration-model
examples. They show how to restate the minimum shared state for a cross-domain
check before full compose semantics exist:

- `tenant_relation_registry.rs`
  membership plus plan checks over shared relation/map state
- `saas_multi_tenant_registry.rs`
  entitlement plus isolation review checks over one shared service slice
