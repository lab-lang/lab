# 0012: Named workflow results and direct returns

Status: accepted, frontend implemented

## Decision

A workflow declares either one result type or a parenthesized list of named, typed results. Both forms remain explicit parts of the callable interface:

```lab
workflow preserve_sample(
  source: Material<Plasmid>,
) -> Material<Plasmid>:
  return source

workflow preserve_build(
  product: Material<Plasmid>,
  plate: Material<Plate>,
) -> (
  product: Material<Plasmid>,
  plate: Material<Plate>,
):
  return product, plate
```

Named results are not an implicit record or tuple value. They are the workflow operation's ordered result fields. A durable call binds them directly:

```lab
product, plate <- preserve_build product plate
```

`return` takes a comma-separated value list. The checker requires its arity to match the declared result list and checks each value against the corresponding result type. Result names and types are preserved in checked module interfaces. Returning several physical materials transfers all of them out of the terminating workflow path, subject to the same affine ownership analysis as a single return.

The existing `-> T` syntax remains the single-result form. Its checked result field is named `outcome`. `-> ()` is not a no-result spelling; workflows that return no information continue to declare `-> None` and `return None`.

## Interface meaning

Workflow inputs describe requirements and caller-controlled variability. Named results describe guarantees and the values or physical ownership transferred back to the caller. The body describes the durable method. Tagged outcomes remain the way to represent scientific success, rejection, timeout, or other alternative terminal states.

Configuration may therefore be explicit typed input data:

```lab
record RealizationPolicy:
  host: Strain
  selection: Antibiotic
  recovery: Duration

workflow realize_reporter_region(
  promoter_carrier: Material<Plasmid>,
  policy: RealizationPolicy,
) -> (
  product: Material<Plasmid>,
  plate: Material<Plate>,
):
  # Durable realization method omitted.
```

This distinction does not make configuration records or result records implicit. A named record remains appropriate when a group of fields has independent domain identity and should travel as one value. Named workflow results are appropriate when a record would exist only to work around a single-result restriction.

## Consequences

All workflow result interfaces remain annotated; return-type inference and a universal implicit workflow result are rejected. Public workflow guarantees do not become dependent on implementation details, and workflows may still return materials, observations, tagged outcomes, records, `None`, or named combinations of those values.

The source AST distinguishes the single-result and named-result spellings. Portable checked IR and module interfaces normalize both to named result fields, and workflow calls use the same multi-result action machinery as bundled durable operations.
