# Lab source frontend

The frontend has two deliberately different entry points:

- `parse_module` builds a spanned source AST for the language under design;
- `compile_module` resolves and type-checks a complete module and lowers it to
  verified portable module IR;
- `parse` additionally lowers the currently executable standalone-plasmid
  subset into the original `ArtifactSpec` pipeline.

The executable subset is:

```lab
plasmid p_sensor:
  sequence = dna("ATGCGTACGTTAGCTA")
  require topology == circular

  accept sequence == design.sequence
  accept concentration >= 100 ng/uL
  accept volume >= 20 uL
```

The module compiler resolves the built-in standard-library modules exercised by
the representative specimens, checks circuit applications, data constructors,
workflow returns and control flow, timers, and typed durable action signatures.
It does not select a laboratory target or dispatch physical actions. Passing
these forms to the legacy `parse` artifact entry point still produces an
explicit `Unsupported` error.

The evolving language contract, decisions, support matrix, and larger syntax
specimens live in [`../../docs/language`](../../docs/language/README.md).
