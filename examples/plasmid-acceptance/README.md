# Plasmid acceptance example

[`p_acceptance.lab`](p_acceptance.lab) exercises the prototype's full supported path: one circular plasmid with exact-sequence, minimum-concentration, and minimum-volume acceptance criteria.

Run it from the repository root:

```sh
labc examples/plasmid-acceptance/p_acceptance.lab
```

The compiler currently passes the input through these representations:

```text
Lab Lang source
  -> validated artifact specification
  -> target-neutral Design IR
  -> target-selected Design + Protocol IR
  -> executable plan
  -> human output or symbolic simulation
```

Each intermediate form can be inspected with `--emit`:

```sh
labc examples/plasmid-acceptance/p_acceptance.lab --emit specification-json
labc examples/plasmid-acceptance/p_acceptance.lab --emit design-ir
labc examples/plasmid-acceptance/p_acceptance.lab --emit target-ir
labc examples/plasmid-acceptance/p_acceptance.lab --emit plan-json
labc examples/plasmid-acceptance/p_acceptance.lab --emit simulation
```

The current compiler checks basic source validity, operation types, target capabilities, single-consumer use of physical material, and connections between requested acceptance criteria and evidence-producing steps. It deliberately rejects plasmid copy counts other than one.

This example does not demonstrate scheduling, reagent quantities, inventory, containers, staff, instrument selection, robot code generation, execution tracking, or successful physical construction. The compiler uses one built-in reference laboratory profile. Its IR and JSON outputs are debugging surfaces, not stable interchange formats.

The Rust and Python SDKs call the same compiler pipeline; see [`sdk/`](../../sdk/) for their small experimental APIs.
