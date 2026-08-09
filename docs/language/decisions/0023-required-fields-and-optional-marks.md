# 0023 — Schema fields are required unless marked optional

## Status

Accepted.

## Context

An artifact kind's schema said which properties an instance may state and what
each holds. It did not say which an instance must state. Every field was
effectively optional, so a schema described a vocabulary rather than an
obligation, and a misspelled property name was silently accepted as a new one.

## Decision

A schema field is one every instance states, unless the schema marks it `?`.

```lab
artifact Strain:
  chassis: Chassis
  plasmids: List<Plasmid>
  selection?: Antibiotic
```

The mark sits on the name rather than the type. Absence is a property of the
field: an optional `Antibiotic` field still holds an `Antibiotic` wherever it is
stated, and nothing about the type became nullable. Lab already spells a value
that may be nothing `Antibiotic | None`, and the two compose on separate axes.

Only an artifact kind's schema admits the mark. Every value of a record carries
every field, and every caller of a workflow supplies every parameter, so a
`?` in either place is rejected with the reason.

`declares` and requiredness are independent: a declaration must state every
required field **and** satisfy `declares`. A completeness rule may therefore name
only optional properties, because naming a required one asserts nothing. Under
this rule `strain` needs no `declares` at all, while `plasmid` keeps one for the
genuine alternative that `?` cannot express.

## Consequences

Absence inside a declaration is statically known, so an omitted optional leaves
scope entirely: a rule reading one is a diagnostic naming the property rather
than an unknown name.

Two namespaces remain distinct. A rule constrains the artifact that gets built,
so `require` and `accept` resolve a name against the produced type's fields
first: `accept sequence == design.sequence` compares a realized plasmid against
its design whether or not the declaration stated a sequence. Only a schema
property with no field of that name on the produced type is unreadable when
omitted.
