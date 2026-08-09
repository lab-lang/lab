# 0019 — Properties are written with `=`

## Status

Accepted. Supersedes the property-syntax half of
[0009](0009-declaration-properties-and-workflow-signatures.md).

## Context

A property and a field were token-identical. These two lines differ only in what
the parser had already decided:

```lab
backbone: pSB1C3     // a property — the right side is a value
chassis: Chassis     // a field — the right side is a type
```

Both are `IDENT COLON IDENT`, and the parser told them apart only because it
knew which keyword opened the block. That is not a cosmetic difference, because
types and expressions overlap exactly where it hurts: `Promoter<Signal>` is a
type application and `a < b` is a comparison, so a parser that does not know
which side of that fence it is on cannot read either.

## Decision

A property associates a name with a value using `=`. `:` inside a declaration
body means one thing: the right side is a type.

```lab
artifact Plasmid:
  sequence: DNA              // the schema says what type

plasmid p_gfp:
  sequence = dna("ACGT")     // the instance says what value
```

Braces are unaffected. `Rejected{ material: retained }` in an expression and
`case Rejected{ material: retained }` in a pattern stay symmetric, and the
pattern side binds a name rather than a value, so `=` there would read as an
assignment happening during a match.

## Consequences

Declaration bodies parse from their shape rather than from the word that opened
them, which is what [0022](0022-fixed-grammar-open-vocabulary.md) rests on.

The claim in `semantics.md` that properties are "not executable assignments" is
rewritten rather than defended. Property values always were deterministic
expressions — `check_artifact` runs `infer_expr` over them — and `=` in Lab has
never meant mutation, which requires `state`. The contrast `=` carries is
against `<-`, and a property is definitively not a durable effect.

Every Lab program written before this breaks. At eight source files, this was
the cheapest the change would ever be.
