# Documentation

- [Install Guide](./install.md)
  Installation modes, binary vs Cargo usage, Docker, and backend selection.
- [Architecture](./architecture.md)
  Clean-architecture view of the repository, package roles, DTO boundary, and
  solver-neutral layering.
- [Rust DSL Guide](./dsl/README.md)
  User-facing documentation for writing and operating models with the `valid`
  Rust DSL.
- [DSL Language Spec](./dsl/language-spec.md)
  Current implemented surface and semantic subset for the Rust DSL.
- [DSL Language Evolution](./dsl/language-evolution.md)
  Design notes for proposed and in-flight language features.
- [ADR-0001: `valid_model!` Frontend Decision](./adr/0001-valid-model-frontend.md)
  Decision record for keeping `valid_model!` on the `macro_rules!` track unless
  A1 fails to recover `rust-analyzer` compatibility.
- [RDD](./rdd/README.md)
  Requirements, architecture, planning, and delivery documents for the project
  itself.

If you want to install or distribute the tool, start with the install guide.
If you want to model and verify a system, start with the Rust DSL guide.
If you want to understand the repository's design and scope, read the
architecture note and the RDD.
