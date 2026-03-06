# 08 Specs

## このフォルダの役割

`08_specs` は実装前提の詳細仕様を置く。方針ではなく、入出力契約、schema、疑似コード、テストケースを定義する場所である。

## 前提として読む章

- [../03_architecture/architecture.md](/Users/tatsuhiko/code/valid/docs/rdd/03_architecture/architecture.md)
- [../04_domain/business_logic_and_data_model.md](/Users/tatsuhiko/code/valid/docs/rdd/04_domain/business_logic_and_data_model.md)
- [../07_planning/feature_breakdown.md](/Users/tatsuhiko/code/valid/docs/rdd/07_planning/feature_breakdown.md)

- [mvp_frontend_and_kernel_specs.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/mvp_frontend_and_kernel_specs.md)
- [explicit_engine_and_evidence_specs.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/explicit_engine_and_evidence_specs.md)
- [testgen_contract_coverage_specs.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/testgen_contract_coverage_specs.md)
- [ai_solver_selfcheck_specs.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/ai_solver_selfcheck_specs.md)
- [full_technology_usage_plan.md](/Users/tatsuhiko/code/valid/docs/rdd/08_specs/full_technology_usage_plan.md)
- [../09_reference](/Users/tatsuhiko/code/valid/docs/rdd/09_reference)

読むべきタイミング:

- 実装に着手する直前
- 技術を何に使うかを横断で確認したい時
- MVP詳細仕様を確認したい時
- `check` 実装を作る時
- 反例からテスト化、契約管理、AI APIへ進む時

読む順序:

1. `mvp_frontend_and_kernel_specs.md`
2. `explicit_engine_and_evidence_specs.md`
3. `testgen_contract_coverage_specs.md`
4. `ai_solver_selfcheck_specs.md`
5. `full_technology_usage_plan.md`
6. `../09_reference`

実装との接続先:

- 参照契約: [../09_reference/README.md](/Users/tatsuhiko/code/valid/docs/rdd/09_reference/README.md)
- PR 単位受け入れ: [../10_delivery/README.md](/Users/tatsuhiko/code/valid/docs/rdd/10_delivery/README.md)
