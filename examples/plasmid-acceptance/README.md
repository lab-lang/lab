# Plasmid acceptance example

[`p_acceptance.lab`](p_acceptance.lab) exercises portable frontend checking for one circular plasmid with exact-sequence, minimum-concentration, and minimum-volume acceptance criteria.

Run it from the repository root:

```sh
labc examples/plasmid-acceptance/p_acceptance.lab
```

The compiler passes the input through one source frontend:

```text
Lab Lang source
  -> spanned source AST
  -> resolved and type-checked portable module IR
  -> human summary or a separately selected backend
```

The frontend forms can be inspected with `--emit`:

```sh
labc examples/plasmid-acceptance/p_acceptance.lab --emit source-ast
labc examples/plasmid-acceptance/p_acceptance.lab --emit module-ir
```

The frontend checks source validity, names, types, requirements, acceptance expressions, action contracts, and affine material flow where workflows manipulate physical materials.

This example does not select a laboratory target or demonstrate scheduling, reagent quantities, inventory, containers, staff, instrument selection, robot code generation, execution tracking, or successful physical construction. Its JSON output is a compiler-development surface, not a stable interchange format.

Rust callers use the public `lab-compiler` API directly. The [`crates/lab-python`](../../crates/lab-python/) binding exposes that same portable frontend to Python.
