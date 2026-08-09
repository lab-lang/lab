# 0028 — Several packages describe one kind

## Status

Accepted.

## Context

What a plasmid *is* — a sequence, or a backbone and what goes into it — is a
fact about DNA. What building one needs — reaction volumes, cycle counts,
digest temperatures — is a fact about the method used to build it, and a
laboratory using another method needs none of it.

Holding both in one schema puts a thermocycler program in the definition of a
molecule. Holding the second nowhere leaves eighteen properties that every real
program states and no schema declares, which is why an undeclared property could
not be rejected: doing so would have rejected every real program.

## Decision

A kind's schema is the union of what every module in scope declares for it.

```lab
// std.bio.designs — what a plasmid is
artifact Plasmid:
  sequence?: DNA
  backbone?: Backbone
  components?: List<Part | Plasmid>

// std.bio.golden_gate — what this method needs to build one
artifact Plasmid:
  reaction_volume?: Quantity<uL>
  assembly_cycles?: Integer
  digest_temperature?: Quantity<C>
```

A design built by a method imports it, which is how a schema is chosen: sharing
is an ordinary import rather than a mechanism of its own. A completeness rule
comes from whichever module states one.

Every field a method contributes is optional, because a method's standard values
stand behind a design that states nothing, and because a design's own value wins
where a protocol departs from the datasheet.

## Consequences

An undeclared property is now a mistake everywhere, reported with the name it
most likely meant. That is the guarantee a schema was supposed to carry from the
start.

A quantity's unit is checked where it is written rather than when a target reads
the IR, because the field that declares it is typed.

A method is a module, so it needs no keyword, no export kind, and no resolution
machinery. Two backends running the same reaction import the same module.
