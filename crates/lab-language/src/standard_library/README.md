# Lab standard library implementation

This directory is the bundled implementation of the language-owned `std`
namespace. It is a module catalog, not a second parser and not a collection of
checker special cases.

## Structure

- `catalog.rs` defines immutable standard modules, their export kinds, catalog
  validation, and lookup.
- `contract.rs` defines the typed phrase, ownership, capability, and result
  contract for durable actions.
- `prelude.rs` contains the explicitly identified implicit prelude. Names in
  this module are available without a source-level `use`.
- `bio/` mirrors the `std.bio.*` namespace, with one registration file per
  standard module.
- `lab/` mirrors the `std.lab.*` namespace, with one registration file per
  standard module.
- `authored/` holds standard modules written in Lab rather than in Rust.

## Modules written in Lab

A standard module whose whole surface is expressible in Lab lives in
`authored/` as ordinary source. `AUTHORED_SOURCES` lists them in the order they
compile: each may import the ones before it and nothing after, so the bootstrap
is a straight line rather than a graph to resolve. They compile once for the
life of the process behind a `OnceLock`, against a `StandardLibrary` holding
only the Rust modules and their already-compiled predecessors — which is what
keeps building a checker from re-entering the bootstrap. Every later
`StandardLibrary::bundled()` pays one atomic refcount for them.

An importer resolves such a module through its `ModuleInterface`, exactly as it
resolves a module from a package, so nothing in the checker distinguishes them.

What a module must stay in Rust for: pure functions, durable action contracts,
and inventory constructors have no source declaration form. That is why
`std.bio.parts` has not moved — three of its five values are expressible as
`part("B0034")`, but `pTet: Promoter<Tetracycline>` needs a typed constructor
Lab cannot yet declare.

A `StandardModule` owns all of its exported type specifications, values, pure
functions, constructors, and durable actions. Type specifications carry
generic arity, fields, biological conformance relationships, and documentation.
Adding a bundled module means constructing another module specification and
returning it from the appropriate namespace registrar.
Catalog construction rejects duplicate module paths, duplicate exported names,
duplicate operation identities, and malformed action contracts.

`lab_language::standard_library_markdown` renders reference documentation from
this same catalog, so documentation cannot silently invent a second export
surface. Compiled packages provide equivalent `ModuleInterface` contracts and
the project host merges those contracts into import scope through
`SemanticEnvironment`, without adding parser productions or checker match arms.
