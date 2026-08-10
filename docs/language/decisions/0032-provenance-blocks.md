# 0032 — A provenance verb can open a block

## Status

Accepted. Extends [0027](0027-provenance-is-stated-per-thing.md), which states
provenance per thing.

## Context

A laboratory that buys six things writes `buy` six times. Repetition with no
counterpart is not a distinction: a page of `buy` lines reads as a drone, and
the pair the verbs were chosen to draw — what this laboratory makes against
what a supplier lists — is invisible in exactly the programs that buy the most.

The pair also disappears another way. The verb is optional and defaults to
`build`, so a program can show one half of the contrast many times and the
other half never.

## Decision

A provenance verb followed by `:` opens a block, and states one origin over
everything inside:

```lab
buy:
  part J23101
  part B0034
  backbone pSB1C3
  restriction_enzyme BsaI:
    digest_temperature = 37 C
```

Each line inside is the instance form without a verb — with its own block or
type ascription where it has one — and each lowers to its own declaration. The
block is surface grouping, not a node: nothing changes in the IR, in checking,
or in lineage analysis, and [0027](0027-provenance-is-stated-per-thing.md)
holds per thing exactly as before.

A verb on a line inside the block is refused, because the block has already
said where everything in it came from. Documentation is written per instance,
inside the block; a `/** */` above the block would describe several
declarations at once, and is refused.

Both verbs open blocks. `build:` earns its place the same way `buy:` does, and
a grammar where one provenance groups and the other does not would make the
pair unequal for no reason.

## Consequences

A program partitions visually into inventory and recipes: one `buy` block
above, `build` declarations below, each side of the pair written once where it
governs. A single bought thing is still one line — `buy backbone pSB1C3` — and
nothing forces a block on a program with nothing to group.

A diagnostic about one instance underlines that instance's own lines. The
block's verb belongs to no single declaration, so no declaration's span
includes it.

The words `build` and `buy` followed by `:` are claimed at the top level, so a
binding cannot be named after either verb and ascribed a type. Both words
already introduced declarations when followed by a word and a name, so nothing
that parsed before is lost.
