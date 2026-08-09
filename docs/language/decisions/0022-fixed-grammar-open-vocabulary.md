# 0022 — Fixed grammar, open vocabulary

## Status

Accepted, partially implemented.

## Context

`plasmid` is a synthetic-biology word, and it lived in the parser. So did
`strain`, along with hand-written shape checks for each. A chemistry or clinical
domain would have needed a compiler fork, and every new domain need added a
parser arm, an AST node, a checker branch, and an IR variant.

`syntax.md` already claimed the line — "the kernel keeps orchestration mechanics
distinct from domain operations" — and then did not honor it, because both
halves lived in the same place.

## Decision

The grammar is closed; the vocabulary is open.

A package declares a kind, and its instances use that word:

```lab
artifact Plasmid:
  sequence?: DNA
  backbone?: Backbone
  cargo?: Circuit<any Signal, any Protein>

  declares sequence or (backbone and cargo)
```

```lab
plasmid p_gfp:
  sequence = dna("ACGT")
  require topology == circular
```

The parser never learns a production. An unknown word followed by a name and a
block is always an artifact instance; which kind it names is resolved while
checking. A lone file still parses, `--emit source-ast` still works, and an
editor still recovers partial syntax from broken code.

**No package may introduce a grammar production.** That constraint is what buys
the extensibility for free, and it is not negotiable: the moment a production
can be added, single-file parsing dies and every tool that reads Lab has to
resolve packages first.

`declares` is a predicate over *presence*, not over values. Its whole vocabulary
is property names combined with `and`, `or`, and `not`, and it lowers to
`CheckedPresence` so the IR carries no source syntax. Keeping it this small is
what stops it becoming a second expression language.

A kind names the type its instances have, because a workflow writes
`Material<Plasmid>` and `require topology == circular` reads that type's fields.
The word those instances are written with is that type in snake_case, per
[0027](0027-provenance-is-stated-per-thing.md).
Four levels, each with its own subject: a schema field says what type a property
holds; `declares` says which combinations are complete; `require` constrains the
artifact before it is built; `accept` constrains it against runtime evidence.

## Consequences

`std.bio.designs` declares `plasmid` and `strain` in Lab.
`check_plasmid_shape` and `check_strain_shape` are deleted — two of the former's
three checks became schema field types, and only the genuine cross-field rule
needed a line of its own.

An unknown word now produces a better error than the parse failure it replaced:

```
error: unknown declaration kind 'reagent'
  = help: kinds in scope: plasmid, strain
```

**This removes biology from the frontend only.** The OT-2 backend still reads
`reaction_volume` and `digest_temperature` by name, so biology does not leave
the toolchain until target property contracts exist. What it gains is an honest
failure: a target asked to build a kind it does not know now says so, rather
than the checker refusing the word before a target ever sees it.
