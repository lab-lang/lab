# 0026 — Replicate class is lineage, not a property

## Status

Accepted.

## Context

Three colonies picked from a plate are independent transformants, and measure
biological variance. One culture measured three times is a single organism, and
measures pipetting variance. Averaging the second and reporting `n = 3` is
pseudo-replication, and it inflates a result's significance.

Nothing about a sample says which it is. Two `Material<Clone>` values are
indistinguishable by type, by property, and by name. The answer is in where they
came from.

## Decision

An action's result either **begins** a biological lineage or **continues** the
one its operands carry. Transformation establishes an organism and picking
isolates independent transformants; diluting, recovering, plating, and measuring
carry on what went in.

A lineage beginning is per **event**, not per result. One transformation hands
back both a strain and its culture, and those are one organism with two handles.

Two materials are biological replicates when they trace to different beginnings
and technical replicates when they trace to the same one, computed by walking
dataflow the checked IR already holds. Nothing is recorded at runtime and nothing
is inferred from names.

An artifact states the evidence its claims are believed on:

```lab
plasmid p_gfp:
  sequence = dna("ACGT")

  across 3 biological replicates

  accept concentration >= 100 ng/uL
  accept volume >= 20 uL across 1 biological replicate
```

A declaration sets the standard every claim takes; a claim may state its own
instead, which replaces the declaration's rather than adding to it. The
declaration's standard is read before any claim, so one written below the claims
it governs still governs them. Zero replicates is refused: asking for no evidence
is a mistake rather than a way to opt out.

A judgement names the design it is judging, so `accepts(design, evidence)` is an
ordinary function rather than a method. Acceptance is a judgement *about* a
design given evidence, not something the design does, and putting the design in
an argument is what lets the compiler know whose criteria apply.

## Consequences

Where provenance is not known — a workflow parameter arrives from a caller, a
loop variable holds an unnamed member of a family — the analysis says so rather
than guessing, and anything reading it stays silent. A rule that fires on correct
science costs more than one that misses a case.

Pseudo-replication arises through repeated borrowing rather than duplicated
material: materials are affine and no action splits one into several, so
measuring one sample twice is the form it takes.

```
error: 'p_reporter' is accepted on 3 biological replicates, but this evidence spans 1
   |
14 |   if accepts(p_reporter, [first, second]):
   |      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
   |
   = help: measuring one sample repeatedly gives technical replicates, which measure handling rather than biology
   = help: independent colonies, or independent transformations, give biological replicates
```

A result that continues nothing begins a lineage: no material flowed in, so two
of them are as independent as two separate assemblies. Lineage follows the name
a program binds rather than the name a contract declares, because a binding
renames a result.
