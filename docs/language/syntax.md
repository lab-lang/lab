# Surface syntax

## Layout

Lab uses newlines and indentation for statement blocks. A colon introduces an indented block. Tabs are not accepted for indentation.

Braces construct or destructure data; they never delimit control-flow blocks. Parentheses call pure functions and group expressions. Brackets construct collections. Angle brackets delimit type arguments.

## Comments and documentation

A comment begins with `//` and runs to the end of the line. It is the only comment form: `#` is not a comment character, and `/*` opens documentation rather than a block comment.

Documentation is a `/** ... */` block standing immediately above the declaration it describes. Unlike a comment, it is part of the declaration: the parser attaches it to the AST node, it travels in the module's checked interface and portable IR, and editors show it on hover and in completions.

```lab
/**
 * Assemble the GFP reporter plasmid.
 *
 * Returns the plasmid it produces and takes no material input, so the compiler
 * places this build ahead of everything that consumes it.
 */
workflow assemble_composite_plasmid_1() -> Material<Plasmid>:
  product <- realize composite_plasmid_1
  return product
```

A module documents itself with `/*! ... */`, which opens its file above the imports:

```lab
/*!
 * Four engineered organisms: each composite plasmid in each of two chassis.
 *
 * The same plasmid appearing in two strains is the point. A strain is its own
 * artifact, so DH5alpha carrying composite_plasmid_1 and BL21 carrying the same
 * plasmid are two distinct things to build, accept, and store.
 */

use golden_gate.designs.inventory
```

The two forms differ in what they describe, not in what they are: `/**` documents the declaration below it and `/*!` documents the file it opens. That is what keeps a block at the top of a file unambiguous — without the distinction, a reader and the compiler could disagree about whether it belongs to the module or to the first declaration.

Following [PEP 257](https://peps.python.org/pep-0257/), a docstring opens with a one-line summary that stands on its own; anything further follows a blank line. A leading `*` on continuation lines is decoration and is stripped, as is blank space at either end, so the text a tool renders is the prose the author wrote.

Documentation always has a subject, so it is an error rather than a comment that silently goes nowhere: a `/** */` that precedes an import, precedes nothing, or doubles up on one declaration is rejected, as is a `/*! */` anywhere but the start of a file. An aside inside a workflow body, or a note on an import, is a `//` comment.

## Current vocabulary

The kernel keeps orchestration mechanics distinct from domain operations:

| Role | Words or forms |
| --- | --- |
| Modules | `use` |
| Biological declarations | `part`, `circuit`, `plasmid`, `strain` |
| Laboratory data declarations | `record`, `material`, `observation`, `evidence`, `event`, `outcome` |
| Orchestration declarations | `workflow`, `state` |
| Signatures and contracts | `input`, `output`, `require`, `accept` |
| Control | `if`, `else`, `for`, `in`, `match`, `case`, `return` |
| Reactive control | `when`, `every`, `after`, `emit` |
| Boolean operators | `and`, `or`, `not` |

Most of these are contextual: for example, `input` and `output` describe circuit ports. Workflow parameters and results instead form a callable signature in the declaration header. Laboratory verbs such as `synthesize`, `assemble`, `sequence`, `store`, and `dispose` are library operations, not keywords.

The core punctuation has one job each:

| Form | Meaning |
| --- | --- |
| `:` plus indentation | declaration or control block |
| `name: value` in a biological declaration | declarative property |
| `{ name: value }` | construct or destructure typed data |
| `(...)` | call or group |
| `[...]` | collection literal |
| `<...>` | type arguments |
| `=` | deterministic evaluation or state transition |
| `<-` | durable physical or external effect |
| `->` | workflow result declaration |
| `==`, `!=`, `<`, `<=`, `>`, `>=` | comparison |
| `+`, `-`, `*`, `/`, `..` | arithmetic, composition, or range operations selected by types |

## Bindings

```lab
expected = design.sequence
state observations: List<PlateObservation> = []
observed <- sequence aliquot
<- dispose aliquot
```

An ordinary `=` binding is immutable. `state` declares durable workflow memory, and a later `=` to that name is a checked state transition. `<-` performs a durable effect and binds its result from the recorded completion event. It is intentionally not assignment: replay may reevaluate `=`, but must not repeat a completed physical action. A bare `<-` performs an effect whose result is not retained.

An effect may return more than one result:

```lab
strain, culture <- transform reporter_host from plasmids into cells
```

The phrase after `<-` is resolved through an imported action contract. The contract—not a verb-specific parser rule—determines operand slots, ownership, result types, and required capability.

## Workflow signatures

Workflow inputs and results are part of the declaration signature. A workflow with one result names its type directly:

```lab
workflow preserve_sample(
  source: Material<Plasmid>,
) -> Material<Plasmid>:
  // durable workflow body
  return source
```

Named results use a parenthesized typed field list and return their values directly:

```lab
workflow preserve_build(
  product: Material<Plasmid>,
  plate: Material<Plate>,
) -> (
  product: Material<Plasmid>,
  plate: Material<Plate>,
):
  return product, plate
```

Parentheses are required for workflow parameters, including for a zero-input workflow, and an explicit result declaration after `->` is mandatory. Parameters and named results are comma-separated typed fields and may span lines with a trailing comma. A multi-value `return` is comma-separated and must match the declared result arity and types. `-> None` with `return None` is the no-information form; `-> ()` is rejected. The indented body contains state and executable statements—not interface declarations.

## Laboratory declarations

```text
part         reusable biological part
circuit      reusable biological organization
plasmid      physical artifact design
strain       engineered organism: a chassis carrying named plasmids
record       ordinary immutable structured data
observation  recorded measurement or inspection
evidence     information evaluated against a claim
event        durable orchestration occurrence
outcome      tagged workflow result
workflow     durable orchestration definition
state        durable mutable memory owned by one workflow instance
```

The declaration words are meaningful only if the compiler assigns different laws to their values.

## Declaration properties

Inside a biological declaration, `name: value` records a typed property:

```lab
plasmid reporter:
  sequence: dna("ACGT")
  backbone: pSB1C3
```

A property is neither a type annotation nor a workflow assignment. Duplicate property names are rejected. Portable checked IR preserves the property name and typed value; a target may consume a documented subset without the core AST growing one field per backend.

## Plasmid requirements and acceptance

```lab
plasmid p_sensor:
  sequence: dna("ATGCGTACGTTAGCTA")

  require topology == circular

  accept sequence == design.sequence
  accept concentration >= 100 ng/uL
  accept volume >= 20 uL
```

`require` is checked before physical construction. `accept` describes a runtime claim that must be supported by evidence.

## Typed inventory identities and target properties

Inventory identities enter through typed standard-library constructors, and plasmid properties refer to those symbols. Properties are backend-neutral typed expressions—not executable bindings and not evidence that inventory is physically available:

```lab
use std.bio.inventory
use std.lab.plasmid_actions

J23101 = part("J23101")
B0034 = part("B0034")
GFP = part("GFP")
B0015 = part("B0015")
pSB1C3 = backbone("pSB1C3")
BsaI = restriction_enzyme("BsaI")
DH5alpha = chassis("DH5alpha")
chloramphenicol = antibiotic("chloramphenicol")

plasmid p_gfp:
  sequence: dna("GCTAGCGGATCCATGACCATGATTACGCCAAGCTTGAATTC")
  backbone: pSB1C3
  components: [J23101, B0034, GFP, B0015]
  restriction_enzyme: BsaI
  assembly_replicates: 1

  require topology == circular
  accept sequence == design.sequence

strain reporter_host:
  chassis: DH5alpha
  plasmids: [p_gfp]
  selection: chloramphenicol
  transformation_replicates: 2
  plating_replicates: 2
  serial_dilutions: 2
```

The string passed to an inventory constructor is an external inventory identity. Downstream declarations use the checked symbol, so renaming a source binding and changing an external identifier are distinct operations. Source symbols are values regardless of capitalization: `J23101`, `BsaI`, and `DH5alpha` do not become types because their names begin with capitals.

The OT-2 specialization interprets these properties after ordinary module checking. Another target may ignore them, interpret other metadata, or reject the module. Target-specific property names are not encoded in the core checker.

The component list above has type `List<Part>`. A list that refers to both a dependent plasmid and ordinary parts has the inferred type `List<Plasmid | Part>`:

```lab
components: [promoter_carrier, B0034, GFP, B0015]
```

The union preserves the nominal alternatives; it does not convert the symbols to strings or a universal metadata value.

Multiple property-bearing artifacts and their realization workflows may be compiled by a compatible target. Replicate and dilution settings are currently interpreted by the initial OT-2 specialization, not by the core language.

## Artifact kinds

A `plasmid` and a `strain` share one declaration shape and differ in what they name. A plasmid is a DNA design: a sequence, or the backbone and components a sequence is built from. A strain is an organism: a chassis together with the plasmid designs it carries.

The distinction matters because the same plasmid in two hosts is two artifacts, each with its own acceptance criteria and its own place in a build order:

```lab
strain reporter_dh5alpha:
  chassis: DH5alpha
  plasmids: [p_gfp]
  selection: chloramphenicol

strain reporter_bl21:
  chassis: BL21
  plasmids: [p_gfp]
  selection: chloramphenicol
```

## Reaction chemistry

Quantity-valued properties state the chemistry a design is built with. These are scientific choices, so they travel with the artifact rather than with the laboratory that runs it:

```lab
plasmid p_gfp:
  reaction_volume: 20 uL
  part_volume: 2 uL
  assembly_cycles: 75
  digest_temperature: 37 C
  ligate_duration: 5 min
```

Units are checked rather than assumed: `20 mL` where microlitres are expected is a diagnostic, not a thousandfold error on the bench. Water makes each reaction up to its stated volume, and reagents that over-subscribe that volume are rejected before any target sees the design.

## Dependencies through workflow dataflow

Dependencies are expressed through workflow dataflow rather than string matching:

```lab
use std.bio.build
use std.lab.plasmid_actions

workflow assemble_promoter_carrier() -> Material<Plasmid>:
  product <- realize promoter_carrier
  return product

workflow assemble_reporter_region(
  promoter_carrier: Material<Plasmid>,
) -> Material<Plasmid>:
  dependencies = [promoter_carrier]
  product <- realize reporter_region from dependencies
  return product

workflow build_reporter_host(
  reporter_region: Material<Plasmid>,
) -> (
  strain: Material<Strain>,
  plate: Material<Plate>,
):
  dependencies = [reporter_region]
  cells <- provision DH5alpha
  strain, culture <- transform reporter_host from dependencies into cells
  culture <- recover culture for 1 h
  culture <- dilute culture
  plate <- plate culture on chloramphenicol
  return strain, plate
```

`realize` assembles a plasmid and `transform` realizes a strain; both are bundled standard-library operations.

A contract may end with an optional clause. `realize`'s `from` clause is one, so a realization that consumes no artifact leaves it out and means the same thing as passing an empty list. Naming the clause commits to its operand: `realize x from` with nothing after it is an error, not an omission.

An optional clause may only carry collections. A list has an empty value to fall back to; a material does not, and silently conjuring one would be a lie about a physical thing.

For either operation the checker resolves its typed inputs, ownership, results, and capability. Target lowering reads that structured checked operation; it does not reinterpret the source phrase. Build ordering, graph depth, fixed-point retries, and roots derive from material flow rather than declaration names or compiler-defined assembly levels.

## Data construction

```lab
return Rejected{
  material: retained,
  reason: sequence_mismatch,
  evidence: evidence,
}
```

Record fields use `name: value`; multiline constructors conventionally retain a trailing comma.

## Reactive clauses

```lab
when every 30 min:
  image <- capture image of plate

when after 18 h:
  return TimedOut{
    plate: plate,
    observations: observations,
  }
```

`when` introduces an event pattern. `every` is a periodic timer pattern and `after` is a one-shot timer pattern.
