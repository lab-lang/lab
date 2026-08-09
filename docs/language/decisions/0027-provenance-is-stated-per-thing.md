# 0027 — Provenance is a fact about a thing, not about its type

## Status

Accepted. Revises [0021](0021-typed-external-identities.md), which gave
catalogued items a declaration form of their own.

## Context

A plasmid may be assembled here or ordered from a supplier. `pSB1C3` is a
backbone a laboratory buys, and [0013](0013-strain-artifacts.md) already says a
circular assembled construct is a plasmid, so being built is not a property that
divides types. A form that split them — `artifact` for things you make,
`catalog` for things you order — asserted something false about the type and
made a thing that could arrive either way inexpressible.

The two forms were also unlike each other in ways nothing justified. An artifact
kind introduced a word; a catalogued item repeated its type on every line. What
made a type catalogued was a law tucked into an `is` clause rather than anything
a reader could see.

## Decision

A kind names a type, and the word its instances are written with is that type in
snake_case. Neither name is written twice.

```lab
artifact Plasmid:
  sequence?: DNA
  backbone?: Backbone
```

An instance states where it came from:

```lab
build plasmid composite_plasmid_1:
  backbone = pSB1C3
  components = [J23101, B0034, GFP, B0015]
  accept concentration >= 100 ng/uL

buy backbone pSB1C3
buy restriction_enzyme BsaI:
  digest_temperature = 37 C
```

`require` and `accept` and a build-graph node attach to `build`; an `identity` to
order against attaches to `buy`, and belongs to buying rather than to any one
kind's schema. Claiming to build something bought is refused.

A word whose kind takes no type arguments has already said what type its
instances have, so repeating it says nothing. Where a kind is generic the word
cannot say, and the instance names its own type:

```lab
buy promoter pTet: Promoter<Tetracycline>
```

An instance may fill in its kind's arguments; it may not name some other type.

The grammar does not grow. A verb followed by a word, a name, and a block is the
shape the parser already matched, and which kind the word names is resolved while
checking.

## Consequences

The `Catalogued` law retires: the declaration form says a thing is bought, so a
role no longer has to, and a law nothing checks would be decoration.

A word and its type are in bijection, so binding a target on the word and binding
it on the type are the same thing. `CheckedDeclaration::Catalog` remains as the
node a bought thing lowers to, which is what keeps a supplier identity distinct
in the IR from a recipe.

One type has exactly one word. A package that wants to speak of vectors declares
a `Vector` type rather than a second word for `Plasmid`, and a derived word that
collides with an existing name is a duplicate declaration.
