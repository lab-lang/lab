# 0005: Explicit durable workflow state

Status: accepted, initial implementation

Workflow-local mutable memory is declared with `state`:

```lab
state observations: List<PlateObservation> = []
```

The declared type is required because state is a durable schema, not merely an inference convenience. A later assignment to the same name is a deterministic state transition. Ordinary `=` bindings remain immutable; assigning to one is a compile-time error rather than an implicit promotion to durable state.

State declarations appear before executable workflow statements. Portable module IR records each state cell, its structured type and typed initializer, and represents updates separately from local bindings. This gives later durability, schema migration, and replay passes an explicit boundary.

The initial concurrency semantics process handlers atomically in journal order. Parallel handler execution, conflict detection, state versioning, and migration syntax remain later decisions.
