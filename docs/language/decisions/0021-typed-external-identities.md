# 0021 — Typed external identities are declarations

## Status

Accepted. Supersedes the constructor half of
[0010](0010-standard-library-contracts-and-inventory-identities.md).

## Context

An external identity was a function call: `J23101 = part("J23101")`. Five
constructors — `part`, `backbone`, `restriction_enzyme`, `chassis`,
`antibiotic` — each encoded in its name the type it returned.

Nothing was computed. The "function" could not even determine its own result
type, which is why `pTet: Promoter<Tetracycline>` was inexpressible and why
`std.bio.parts` could not be written in Lab.

The cost showed up in the backend, which recovered the symbol-to-identity map by
recognizing a call shape: an operation prefix plus exactly one string literal.
That is what modelling data as a call buys you.

## Decision

```lab
catalog J23101, J23106, B0034, B0015: Part
catalog pSB1C3: Backbone
catalog BsaI_HF: RestrictionEnzyme = "BsaI-HF-v2"
```

Several names share one type, because a catalog reads as a listing. The
supplier's identifier defaults to the declared name and is written only where
they differ — every occurrence in this repository had them identical, so the
explicit form was redundant in every real case.

The declared type's head must play the `Catalogued` law. This check reads the
head name only, and is deliberately **not** `plays_role`, which requires a bare
name. `Promoter<Tetracycline>` is catalogued because `Promoter` is; widening it
into `any Signal` must still depend on the whole type. Relaxing one of these
must not relax the other, and keeping them separate functions is how that is
enforced rather than remembered.

## Consequences

`std.bio.parts` and `std.bio.backbones` are written in Lab, and
`std.bio.inventory` is deleted because nothing was left in it.

`CheckedDeclaration::Catalog` carries the identity as a field, so
`source_lowering` reads it instead of pattern-matching call shapes.

Defaulting the identifier couples the symbol to the supplier's name, which
`semantics.md` had deliberately kept apart. The mitigation assumed during design
— that `lab.toml`'s `[inventory]` list would catch a drifted identity — did not
exist: that list was only read to decide what was already available, so a rename
silently changed the build plan from "use stock" to "build it".

The check that works is narrower than the one first proposed. Several inventory
entries are backend reagents that no source declares, so "every listed material
must match a catalogued identity" would reject a correct manifest. Instead,
every listed material must be one the build actually requires. That catches the
rename, a typo, and a stale entry alike:

```
error: the manifest declares material 'BsaI_HF', which this build never uses;
       a catalogued name that was renamed leaves its old identity here
```
