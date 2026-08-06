# 0010: Standard-library contracts and typed inventory identities

Status: accepted, bundled registry implemented

## Decision

Domain vocabulary enters the generic frontend through resolved module values, pure-function signatures, and action contracts. The parser and AST do not gain a production for each biological object or laboratory operation.

The initial bundled registry includes:

- `std.prelude` for the explicitly identified implicitly imported foundation;
- `std.bio.parts` and `std.bio.backbones` for fixed demonstration values;
- `std.bio.inventory` for typed external identities;
- `std.bio.build` for artifact-realization operations; and
- `std.lab.plasmid_actions` for laboratory workflow operations.

Inventory constructors are pure functions from an external identifier string to a nominally typed source value:

```lab
use std.bio.inventory

J23101 = part("J23101")
pSB1C3 = backbone("pSB1C3")
BsaI = restriction_enzyme("BsaI")
DH5alpha = chassis("DH5alpha")
kanamycin = antibiotic("kanamycin")
```

The left-hand source symbol and the external string have different identities and may be changed independently. Downstream source refers to the symbol, never to an untyped component-name string. Capitalization does not change whether a symbol is a value or a type.

Action contracts declare a stable operation identity, phrase operands, operand ownership (`copy`, `borrow`, or `take`), result types, and a dispatch capability. Checked IR stores the resolved operation and structured operands so later passes do not reinterpret source text.

Every standard module owns a single specification containing its types, values, pure functions, and durable actions. The bundled catalog indexes those specifications and validates duplicate modules, duplicate exports, duplicate operation identities, and malformed action contracts. Importing a module populates the checker scope from that specification; the checker does not independently dispatch standard functions or actions by spelling.

## Boundary

The bundled registry is an initial provider of a more general module interface, not the permanent home of changing catalogs or site-specific protocols. A typed inventory identity does not prove that a lot exists, is available, has sufficient quantity, or has trusted provenance. Live inventory lookup, SBOL resolution, catalog packages, aliases, visibility, and package-defined action contracts remain separate milestones.
