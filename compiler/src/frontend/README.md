# Lab source frontend

The frontend has two deliberately different entry points:

- `parse_module` builds a spanned source AST for the language under design;
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

The source parser also represents whole-module `use` declarations, circuits,
lab-specific data declarations, typed workflow inputs and bindings, durable
effects (`<-`), matches, loops, events, and reactive `when` clauses. Until
resolution, type checking, workflow lowering, and execution exist, passing
these forms to `parse` produces an explicit `Unsupported` error.

The evolving language contract, decisions, support matrix, and larger syntax
specimens live in [`../../docs/language`](../../docs/language/README.md).
