# Lab source frontend

The frontend has one parsing entry point and one compilation entry point:

- `parse_module` builds a spanned source AST for the language under design;
- `compile_module` resolves and type-checks a complete module and lowers it to verified portable module IR.

For example:

```lab
plasmid p_sensor:
  sequence: dna("ATGCGTACGTTAGCTA")
  require topology == circular

  accept sequence == design.sequence
  accept concentration >= 100 ng/uL
  accept volume >= 20 uL
```

The module compiler resolves the built-in standard-library modules exercised by the representative specimens and emits structured typed expressions rather than copied source fragments. The [standard-library implementation](src/standard_library/README.md) is an explicit catalog of module specifications; each module owns its exported types, values, pure functions, and durable actions. It checks circuit applications, data constructors, explicit durable workflow state, returns and control flow, timers, and data-driven action contracts with capability and ownership modes. Before returning portable module IR it verifies affine material flow across actions, projections, branches, matches, returns, and reactive handlers. It does not select a laboratory target or dispatch physical actions.

The evolving language contract, decisions, support matrix, and larger syntax specimens live in [`../../docs/language`](../../docs/language/README.md).
