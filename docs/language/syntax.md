# Surface syntax

## Layout

Lab uses newlines and indentation for statement blocks. A colon introduces an indented block. Tabs are not accepted for indentation.

Braces construct or destructure data; they never delimit control-flow blocks. Parentheses call pure functions and group expressions. Brackets construct collections. Angle brackets delimit type arguments.

Line comments begin with `#`.

## Current vocabulary

The kernel keeps orchestration mechanics distinct from domain operations:

| Role | Words or forms |
| --- | --- |
| Modules | `use` |
| Biological declarations | `part`, `circuit`, `plasmid` |
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
product, construct <- realize reporter from dependencies
```

The phrase after `<-` is resolved through an imported action contract. The contract—not a verb-specific parser rule—determines operand slots, ownership, result types, and required capability.

## Workflow signatures

Workflow inputs and results are part of the declaration signature. A workflow with one result names its type directly:

```lab
workflow preserve_sample(
  source: Material<Plasmid>,
) -> Material<Plasmid>:
  # durable workflow body
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
DH5alpha = strain("DH5alpha")
chloramphenicol = antibiotic("chloramphenicol")

plasmid p_gfp:
  sequence: dna("GCTAGCGGATCCATGACCATGATTACGCCAAGCTTGAATTC")
  backbone: pSB1C3
  components: [J23101, B0034, GFP, B0015]
  restriction_enzyme: BsaI
  host: DH5alpha
  selection: chloramphenicol
  assembly_replicates: 1
  transformation_replicates: 2
  plating_replicates: 2
  serial_dilutions: 2

  require topology == circular
  accept sequence == design.sequence
```

The string passed to an inventory constructor is an external inventory identity. Downstream declarations use the checked symbol, so renaming a source binding and changing an external identifier are distinct operations. Source symbols are values regardless of capitalization: `J23101`, `BsaI`, and `DH5alpha` do not become types because their names begin with capitals.

The OT-2 specialization interprets these properties after ordinary module checking. Another target may ignore them, interpret other metadata, or reject the module. Target-specific property names are not encoded in the core checker.

The component list above has type `List<Part>`. A list that refers to both a dependent plasmid and ordinary parts has the inferred type `List<Plasmid | Part>`:

```lab
components: [promoter_carrier, B0034, GFP, B0015]
```

The union preserves the nominal alternatives; it does not convert the symbols to strings or a universal metadata value.

Multiple property-bearing plasmids and their realization workflows may be compiled by a compatible target. Replicate and dilution settings are currently interpreted by the initial OT-2 specialization, not by the core language.

## Dependencies through workflow dataflow

Dependencies are expressed through workflow dataflow rather than string matching:

```lab
use std.bio.build

workflow realize_reporter_region(
  promoter_carrier: Material<Plasmid>,
) -> (
  product: Material<Plasmid>,
  plate: Material<Plate>,
):
  dependencies = [promoter_carrier]
  product, construct <- realize reporter_region from dependencies
  cells <- provision DH5alpha
  culture <- transform construct into cells
  plate <- plate culture on chloramphenicol
  return product, plate
```

`realize` is a bundled standard-library operation. The checker resolves its typed inputs, ownership, results, and capability. Target lowering reads that structured checked operation; it does not reinterpret the source phrase. Build ordering, graph depth, fixed-point retries, and roots derive from material flow rather than declaration names or compiler-defined assembly levels.

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
