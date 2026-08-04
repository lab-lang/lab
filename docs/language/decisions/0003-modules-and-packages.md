# 0003: Whole-module imports and source organization

Status: provisional, built-in standard-library resolution implemented

Lab uses a compact `use dotted.module.path` form. Rust-style brace selectors are not part of the language. The compiler parses imports as source-level module dependencies; resolution must be deterministic and diagnose missing modules, cycles, and ambiguous public names.

Projects conventionally organize source into `designs`, `policies`, `workflows`, and `programs`. Those directories communicate intent to readers but do not add new language semantics. Runtime histories live outside the source tree under `.lab/runs`.

The standard library is deliberately narrower than the package ecosystem. Core types, units, orchestration interfaces, and broadly stable biological concepts may live under `std`; changing part catalogs and laboratory-specific actions belong in independently versioned packages.
