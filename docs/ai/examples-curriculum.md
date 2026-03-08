# Examples Curriculum

Use these examples in this order when teaching an LLM how to work with
`valid`.

If you need the higher-level document order first, start with
[AI Docs Curriculum](./curriculum.md).

## 1. Counter basics

- Source: `examples/valid_models.rs`
- Focus:
  - smallest registry shape
  - bounded integer state
  - action metadata
  - explicit-first `step`
- Suggested commands:
  - `cargo valid --registry examples/valid_models.rs inspect counter`
  - `cargo valid --registry examples/valid_models.rs verify failing-counter`

## 2. Tenant relations and maps

- Source: `examples/tenant_relation_registry.rs`
- Focus:
  - `FiniteRelation`
  - `FiniteMap`
  - cross-tenant policies
  - declarative transitions
- Suggested commands:
  - `cargo valid --registry examples/tenant_relation_registry.rs inspect tenant-relation-safe`
  - `cargo valid --registry examples/tenant_relation_registry.rs verify tenant-relation-regression --property=P_NO_CROSS_TENANT_ACCESS`

## 3. SaaS grouped transitions

- Source: `examples/saas_multi_tenant_registry.rs`
- Focus:
  - grouped declarative transitions
  - entitlements and isolation policies
  - path tags
- Suggested commands:
  - `cargo valid --registry examples/saas_multi_tenant_registry.rs inspect tenant-isolation-safe`
  - `cargo valid --registry examples/saas_multi_tenant_registry.rs verify tenant-isolation-regression --property=P_NO_CROSS_TENANT_ACCESS`

## 4. Password and text constraints

- Source: `examples/password_policy.rs`
- Focus:
  - `String`
  - `len`, `regex_match`
  - explicit-ready vs solver-ready expectations
  - why the strong/weak action split is a bounded teaching stopgap rather than
    the desired long-term action shape
- Suggested commands:
  - `cargo valid --registry examples/password_policy.rs readiness password-policy-safe`
  - `cargo valid --registry examples/password_policy.rs verify password-policy-regression --property=P_PASSWORD_POLICY_MATCHES_FLAG`

## How to use this curriculum

For each example:

1. Inspect the model shape.
2. Check readiness/capability.
3. Verify one property.
4. Read one failure or explanation path if available.

Do not jump into larger relation or string models before learning the counter
and one declarative transition example.

## Where each example fits

- authoring:
  start with counter basics, then one declarative transition example
- review:
  use the declarative registry examples so `inspect`, `graph`, and `explain`
  stay readable
- migration:
  compare the counter registry shape with older step-heavy models
- conformance:
  prefer smaller registry examples before running implementation-facing flows

## Next read

- [Review Workflow](./review-workflow.md)
- [Migration Guide](./migration-guide.md)
- [Conformance Workflow](./conformance-workflow.md)
