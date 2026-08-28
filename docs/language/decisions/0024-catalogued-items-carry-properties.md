# 0024 — A catalogued item states the fields of its type

## Status

Accepted.

## Context

A supplier's item has a datasheet. `digest_temperature = 37 C` is a fact about
BsaI, not about any plasmid built with it, but a catalogued name could carry
nothing, so every design that used the enzyme restated its working temperature.
In the golden-gate example the same twenty-one chemistry lines appear on every
design, identical each time.

## Decision

A bought item may open an indented block, and the fields of the type its kind
names are the schema those properties are checked against.

```lab
record Enzyme:
  digest_temperature: Quantity<C>
  supplier: String

artifact Enzyme

buy enzyme BsaI:
  digest_temperature = 37 C
  supplier = "NEB"
```

No declaration form is introduced. An artifact kind introduces a word and
declares a schema of its own; a catalogued item names a type it already has and
fills that type's fields in. Every field must be stated, and a property the type
does not declare is a mistake rather than an extension, reported with the name it
most likely meant.

Each item is its own declaration, because two enzymes do not share one working
temperature.

## Consequences

`CheckedDeclaration::Catalog` carries the properties, so method selection and adapter planning read an item's datasheet from the IR rather than from private lookup tables.
