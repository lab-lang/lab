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

A `StandardModule` owns all of its exported types, values, pure functions, and
durable actions. Adding a bundled module means constructing another module
specification and returning it from the appropriate namespace registrar.
Catalog construction rejects duplicate module paths, duplicate exported names,
duplicate operation identities, and malformed action contracts.

The catalog is deliberately internal while `Ty` is an internal checker type.
Future compiled packages should provide equivalent checked public contracts,
then be merged into import scope through a provider boundary rather than by
adding parser productions or checker match arms.
