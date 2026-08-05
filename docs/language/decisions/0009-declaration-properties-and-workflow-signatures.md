# 0009: Declarative properties and workflow signatures

Status: accepted, frontend implemented

## Decision

Biological declarations use `name: value` for declarative properties. Executable `=` bindings remain deterministic evaluations or explicit workflow-state transitions; declaration metadata is not modeled as a sequence of assignments.

```lab
plasmid reporter:
  sequence: dna("ACGT")
  backbone: pSB1C3
```

The source AST and portable module IR preserve this distinction as properties rather than translating properties into ordinary bindings. Property values are typed expressions. Their names remain backend-neutral: target-specific names such as `restriction_enzyme` or `serial_dilutions` are interpreted by a target lowerer, not by parser productions or dedicated core AST fields.

Workflow parameters and results form a mandatory callable signature in the declaration header:

```lab
workflow realize_reporter(
  carrier: Material<Plasmid>,
) -> Material<Plasmid>:
```

Parentheses are required even when a workflow has no parameters, and every workflow declares its results after `->`. A result may be one type or, as accepted in [0012](0012-named-workflow-results.md), a parenthesized list of named typed fields. `input` and `output` remain circuit-port syntax; they are not statements at the beginning of a workflow body.

## Consequences

The former `property = value` plasmid syntax and body-level workflow `input`/`output` syntax are rejected. This makes interfaces visible at call sites and prevents declaration data from acquiring workflow evaluation semantics accidentally.

`<-` remains distinct from `=`. It records and awaits a durable physical or external effect. A future typed effect-expression system may enrich action composition, but it must preserve this replay boundary rather than making physical effects look like ordinary deterministic evaluation.
