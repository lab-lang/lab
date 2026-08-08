# 0018 — Standard modules may be written in Lab

## Status

Accepted, partially implemented.

## Context

Every bundled `std` module was a Rust value: a `StandardModule` listing type
specifications, values, pure functions, and action contracts. That is the right
shape for operations the compiler implements, and the wrong shape for a
biological catalog, which is the half that changes as biology changes and the
half a user would most want to fork.

Roles made the missing piece expressible. A classification and its members are
now ordinary Lab declarations.

## Decision

A standard module whose whole surface is expressible in Lab is written in Lab,
under `standard_library/authored/`, and compiled into the binary.

`AUTHORED_SOURCES` lists them in the order they compile: each may import the
ones before it and nothing after, so the bootstrap is a straight line rather
than a graph to resolve. They compile once for the life of the process behind a
`OnceLock`, against a library holding only the Rust modules and their
already-compiled predecessors — which is what keeps building a checker from
re-entering the bootstrap.

An importer resolves such a module through its `ModuleInterface`, exactly as it
resolves a module from a package. Nothing in the checker distinguishes them.

`std.bio.reporters` is the first, and is entirely Lab:

```lab
role Reporter

/** Light emitted after excitation, read by a plate reader or a microscope. */
record Fluorescence is Reporter
```

## Boundary

Three things have no source declaration form, and a module needing any of them
stays in Rust:

- **pure functions** — `dna`, `sites`, `detect_colonies`, and every inventory
  constructor;
- **durable action contracts** — operand ownership, capability, and phrase slots;
- **typed external identities** — `part("pTet")` returns `Part`, so a catalogue
  entry such as `pTet: Promoter<Tetracycline>` cannot be written.

That last one is why `std.bio.parts` has not moved. Three of its five values are
expressible as ordinary constructor calls; the two typed promoters and coding
sequences are not.

## Consequences

A module written in Lab documents itself from its own source. Its `/*! */` and
`/** */` text reaches the generated reference, so the reference cannot drift
from a second description of the same exports.

Every later `StandardLibrary::bundled()` — one per module checked, which is the
editor's per-keystroke path — pays one atomic reference count for the authored
modules rather than recompiling them.

A malformed bundled module panics during the bootstrap. It is a build-time
invariant, caught by the test suite, not a condition a user can reach.
