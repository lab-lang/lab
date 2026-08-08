# 0016 — Circuits declare callable signatures with inline type parameters

## Status

Accepted. Supersedes the circuit-port half of
[0009](0009-declaration-properties-and-workflow-signatures.md).

## Context

A circuit declared its interface with `input` and `output` lines inside its
block, while a workflow declared a callable signature in its header. Both are
called, so the difference was unexplained. Worse, the port vocabulary was
actively misleading: `input promoter: Promoter<I>` names a *constructor
parameter*, in a language where a genetic circuit also has a real biological
input signal.

Type parameters were declared in a header, `circuit f<I: Signal, O: Protein>`,
and used further down. That reads as bureaucracy to anyone who has not met
generics before, and it permits declaring a parameter that appears nowhere in
the inputs and therefore can never be inferred.

## Decision

A circuit declares a callable signature, exactly as a workflow does, and its
block holds only what the circuit is built from:

```lab
circuit regulated_expression(
  promoter: Promoter<Trigger: Signal>,
  coding: CDS<Product: Protein>,
) -> Circuit<Trigger, Product>:
  layout:
    promoter
    B0034
    coding
    B0015
```

A type parameter is introduced where it is first needed. `Promoter<S: Signal>`
reads as "a promoter for some signal, call it S". Four rules keep reading order
and binding order identical:

1. the first textual occurrence of a name introduces it and carries `: Role`;
2. a second `: Role` on the same name is an error;
3. using a name before it is introduced is an error, not something a second pass
   resolves;
4. names are in scope for the whole signature, including the result type.

Data declarations keep the header form (`record Sensor<T: Inducer>:`). A data
type's parameter appears in field types rather than in a signature, so there is
no argument position to introduce it at. The syntax is deliberately not uniform,
because the two cases are not.

Introducing a parameter outside a signature is an error rather than something
that quietly means nothing.

## Consequences

An un-inferrable type parameter becomes unrepresentable: a parameter with
nowhere to be bound cannot be written, which is correct, because it could never
have been inferred.

The binder is local but its scope is the whole signature, including the result
type — the one real cost. It is paid for in diagnostics rather than syntax:

```
error: unknown type 'Product'
   |
5  | ) -> Circuit<Trigger, Product>:
   |                       ^^^^^^^
   |
   = help: this signature introduces 'Trigger'
```

`TypeExpr::Path`'s arguments become a `TypeArgument` enum, so a binding in a
nonsensical position is unrepresentable at the syntax-tree level rather than
policed at check time. That changes the source-AST JSON that `--emit source-ast`
produces.

Naming parameters with words rather than letters is a convention the specimens
carry, not a rule. `Promoter<Trigger: Signal>` is self-explaining where
`Promoter<I>` forces the reader to hold a symbol table.
