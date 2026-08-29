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
| Declaration shapes | `record`, `circuit`, `artifact`, `workflow`, `state` |
| Classification | `role`, `is`, `any` |
| Provenance | `build`, `buy` |
| Schemas and contracts | `declares`, `require`, `accept`, `across` |
| Control | `if`, `else`, `for`, `in`, `match`, `case`, `return` |
| Reactive control | `when`, `every`, `after`, `emit` |
| Boolean operators | `and`, `or`, `not` |

These are the mechanics. The *vocabulary* — `plasmid`, `strain`, and any domain word a package declares with `artifact` — is not in this table and is not in the parser, which is the point of [0022](decisions/0022-fixed-grammar-open-vocabulary.md). Circuits and workflows both declare a callable signature in their header. Laboratory verbs such as `synthesize`, `assemble`, `sequence`, `store`, and `dispose` are library operations, not keywords.

The core punctuation has one job each:

| Form | Meaning |
| --- | --- |
| `:` plus indentation | declaration or control block |
| `name = value` in a declaration body | declarative property |
| `name: Type` in a declaration body | a field, parameter, or annotation |
| `{ name: value }` | construct or destructure typed data |
| `(...)` | call or group |
| `[...]` | collection literal |
| `<...>` | type arguments |
| `Quantity<uL>` | a measurement in a stated unit |
| `name?:` in a schema | a field an instance may leave unstated |
| `Name: Role` inside `<...>` | introduce a type parameter |
| `any Role` inside `<...>` | a type argument deliberately forgotten |
| `is` after a declaration name | the roles that type plays |
| `=` | deterministic evaluation or state transition |
| `<-` | durable physical or external effect |
| `->` | circuit or workflow result declaration |
| `==`, `!=`, `<`, `<=`, `>`, `>=` | comparison |
| `+`, `-`, `*`, `/`, `..` | arithmetic, composition, or range operations selected by types |

## Roles and type parameters

A **role** is a name that types can play. It classifies types and has no values
of its own, which is why it may bound a type parameter and may never be the type
of anything.

```lab
role Inducer

record Arabinose is Inducer
record Tetracycline is Inducer
```

A role may name the ontology term it stands for. A role's whole content is its
identity, so the term is written after `=` rather than as a property:

```lab
role NucleicAcid = "SBO:0000251"
role EngineeredRegion = "SO:0000804"
```

A term is an absolute IRI or a compact identifier, and the two spellings of one
term mean the same thing. A type that plays a grounded role stands for its term,
which is how a design says what it is in a vocabulary other tools read. A role
that names no term classifies types and says nothing about any ontology.

`Signal` and `Protein` are roles the prelude already declares, so a module
declares its own rather than redeclaring those. A role takes no block. Its members are declared by the types that play it, so a
package can classify its own types against a role it imported, and a role stays
open to types that do not exist yet. Writing a role where a type belongs is an
error that names both ways forward:

```
error: 'Signal' is a role, not a type
  |
6 |   used: Signal
  |         ^^^^^^
  |
  = help: name it, and everything using that name must agree: <T: Signal>
  = help: or name a type that plays Signal: Arabinose, Tetracycline
```

A **type parameter** is introduced where it is first needed, inside the type of
the parameter that determines it. `Promoter<Trigger: Signal>` reads as "a
promoter for some signal, call it Trigger":

```lab
circuit regulated_expression(
  promoter: Promoter<Trigger: Signal>,
  coding: CDS<Product: Protein>,
) -> Circuit<Trigger, Product>:
  layout:
    promoter
    coding
```

Naming a parameter is what links its occurrences. A workflow that takes both a
design and the reagent it responds to says so by using one name twice, and the
compiler then refuses the wrong reagent:

```lab
workflow characterize(
  design: Circuit<S: Signal, GreenFluorescentProtein>,
  inducer: Material<S>,
) -> (
  design: Circuit<S, GreenFluorescentProtein>,
  inducer: Material<S>,
):
  return design, inducer
```

```
error: 'S' cannot be both Tetracycline and Arabinose
   |
22 |   design, inducer <- characterize tet_reporter arabinose_stock
   |                                   ^^^^^^^^^^^^ this fixes S = Tetracycline
   |                                                ^^^^^^^^^^^^^^^ this requires S = Arabinose
```

Reading order and binding order are the same. The first occurrence of a name
introduces it; introducing it twice, or using it before it is introduced, is an
error rather than something a later pass resolves. A data declaration keeps a
header instead — `record Sensor<T: Signal>:` — because its parameter appears in
field types, where there is no argument position to introduce it at.

## Forgetting a type argument

`any Role` is a type argument whose identity has been discarded. A concrete type
flows into it whenever it plays that role, and never back out:

```lab
panel: List<Circuit<any Signal, GreenFluorescentProtein>> = [tet_reporter, ara_reporter]
```

The asymmetry is the information. In a panel the trigger varies and the product
is pinned, because two readings only mean something side by side when they
measure the same thing. Varying the pinned position is refused:

```
error: binding has type List<Circuit<Arabinose, GreenFluorescentProtein> | Circuit<Arabinose, Luciferase>>,
       but annotation requires List<Circuit<any Signal, GreenFluorescentProtein>>
  = help: 'Luciferase' does not fit 'GreenFluorescentProtein'
```

`any Role` is legal only as a type argument. A value cannot *be* a signal, only
carry one, so `x: any Signal` is rejected and `Material<any Signal>` is not.

Forgetting happens only where an annotation asks for it. An unannotated list of
mixed circuits infers a union, which preserves the alternatives; turning that
into `any` is a deliberate act the author writes down. The two describe genuinely
different things — a union says "one of these specific things, and you may
`match` to find out which", an existential says "something that plays this role,
and the question is not answerable".

Which is why a forgotten argument cannot be recovered by naming it:

```
error: 'S' cannot be inferred from a forgotten type
  = help: 'any Signal' means some Signal, deliberately not recorded
  = help: there is nothing here for the other uses of 'S' to be matched against
```

That is not an awkwardness of the type system. A list of circuits with different
triggers has, by construction, no inducer that works for all of them.
[`semantics.md`](semantics.md) describes where the forgetting belongs instead.

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

## Callable signatures

Circuits and workflows are both called, so both declare a callable signature in
their header rather than inside their block. A circuit's block holds only what
it is built from; a workflow's holds its state and statements.

A declaration with one result names its type directly:

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

Parentheses are required for parameters, including for a zero-input workflow, and an explicit result declaration after `->` is mandatory. Parameters and named results are comma-separated typed fields and may span lines with a trailing comma. A multi-value `return` is comma-separated and must match the declared result arity and types. `-> None` with `return None` is the no-information form; `-> ()` is rejected. The indented body contains state and executable statements—not interface declarations.

Only workflows take named result lists. A circuit produces one value:

```lab
circuit regulated_expression(
  promoter: Promoter<Trigger: Signal>,
  coding: CDS<Product: Protein>,
) -> Circuit<Trigger, Product>:
  layout:
    promoter
    coding
```

## Laboratory declarations

```text
role      a part types can play, such as Signal or Reporter
record    structured data; roles say what it is for
circuit   reusable biological organization, declared as a callable
artifact  a kind of thing, and the schema its instances are checked against
build     a thing this laboratory makes: it has a recipe and acceptance criteria
buy       a thing a supplier lists: it has an identity to order against
workflow  durable orchestration definition
state     durable mutable memory owned by one workflow instance
```

A word is meaningful only if the compiler assigns a law to it. `record` plus role membership replaced six words that mostly did not — see [0020](decisions/0020-laws-are-declared-roles.md).

## Artifact kinds and their instances

A package declares a kind; the compiler knows only the shape. The schema block
uses `:` because it states types; the instance block uses `=` because it states
values, so a reader can tell at a glance which they are looking at.

```lab
/** A DNA design a laboratory can build. */
artifact Plasmid is NucleicAcid, EngineeredRegion:
  sequence?: DNA
  backbone?: Backbone
  cargo?: Circuit<any Signal, any Protein>

  declares sequence or (backbone and cargo)
```

A kind may play roles, written with the same `is` clause a record uses. The
roles classify the type the kind produces, so a kind grounded in an ontology
states what its instances are.

A kind names the type its instances have, which is what a workflow writes in
`Material<Plasmid>` and what `require` and `accept` read fields from. The word
those instances are written with is that type in snake_case — `Plasmid` gives
`plasmid`, `RestrictionEnzyme` gives `restriction_enzyme`, and an acronym stays
one word, so `DNA` gives `dna`. Neither name is written twice, and a word and
its type cannot disagree.

Every field is one an instance must state unless the schema marks it `?`. The
mark sits on the name rather than the type, because absence is a property of the
field: an optional `Backbone` field still holds a `Backbone` wherever it is
stated, and Lab already spells a value that may be nothing `Backbone | None`.

`declares` states which combinations of stated properties are complete. It is a
predicate over presence, not over values: its whole vocabulary is property names
combined with `and`, `or`, and `not`. It says what `?` cannot — that a plasmid
needs a sequence *or* a backbone and cargo — so it names only optional
properties. A kind whose properties are simply all required needs no `declares`,
and naming a required property in one is an error rather than a redundancy.

Five lines, each with its own subject and its own moment:

| Line | Subject | Checked |
| --- | --- | --- |
| `backbone: Backbone` | which properties exist, and their types | when the schema is read |
| `declares sequence or (backbone and cargo)` | which combinations are complete | when a declaration is written |
| `require topology == circular` | the artifact, before it is built | before construction |
| `accept concentration >= 100 ng/uL` | the artifact, after it is built | against runtime evidence |
| `across 3 biological replicates` | how much independent evidence a claim needs | against the lineage of what was measured |

A word no imported package declares is an error that names what is in scope:

```
error: unknown declaration kind 'reagent'
  = help: kinds in scope: plasmid, strain
```

## Declaration properties

Inside an artifact declaration, `name = value` records a typed property:

```lab
reporter_sequence: DNA = dna("ACGT")

plasmid reporter:
  sequence = reporter_sequence
  backbone = pSB1C3
```

The sequence is a first-class typed value. Several designs can reference one
named sequence, and Design IR preserves that sharing as a use-def edge. Writing
`sequence = dna("ACGT")` inline remains valid shorthand and lowers to the same
independent sequence value with a synthetic name.

The value is a deterministic expression, evaluated once and never repeated: `=` contrasts with `<-`, which is the durable effect form, and a property is definitively not an effect. `:` is reserved for the other thing a declaration body can say — that a name has a type — so the two never collide:

```lab
record PlateObservation:
  image: Image          // a field: the right side is a type
  colonies: ColonyMap
```

Duplicate property names are rejected. Portable checked IR preserves the property name and typed value; method selection or an allocated adapter may consume a documented subset without the core AST growing one field per implementation.

## Plasmid requirements and acceptance

```lab
p_sensor_sequence: DNA = dna("ATGCGTACGTTAGCTA")

plasmid p_sensor:
  sequence = p_sensor_sequence

  require topology == circular

  accept sequence == design.sequence
  accept concentration >= 100 ng/uL
  accept volume >= 20 uL
```

`require` is checked before physical construction. `accept` describes a runtime claim that must be supported by evidence.

## Typed design identities and specialization properties

Biological designs enter through `build` and `buy` declarations against imported kinds, and plasmid properties refer to those typed symbols. An exact SBOL Component IRI may be attached to either provenance. These declarations and properties are not executable bindings or evidence that a material lot is physically available:

```lab
use std.lab.plasmid

buy:
  part J23101
  part B0034
  part GFP
  part B0015
  backbone pSB1C3
  restriction_enzyme BsaI
  chassis DH5alpha
  antibiotic chloramphenicol

plasmid p_gfp:
  sequence = dna("GCTAGCGGATCCATGACCATGATTACGCCAAGCTTGAATTC")
  backbone = pSB1C3
  components = [J23101, B0034, GFP, B0015]
  restriction_enzyme = BsaI
  assembly_replicates = 1

  require topology == circular
  accept sequence == design.sequence

strain reporter_host:
  chassis = DH5alpha
  plasmids = [p_gfp]
  selection = chloramphenicol
  transformation_replicates = 2
  plating_replicates = 2
  serial_dilutions = 2
```

An `sbol_identity` is an absolute IRI naming the SBOL Component represented by a built or bought declaration. A bought item's `supplier_identity` names what a supplier's order line calls it and defaults to the declared name; `identity` remains a legacy alias for `supplier_identity`. The source symbol, design identity, and supplier identity are distinct, so renaming one does not silently rewrite the others. Source symbols are values regardless of capitalization: `J23101`, `BsaI`, and `DH5alpha` do not become types because their names begin with capitals.

The current plasmid-build method interprets the scientific properties after ordinary module checking, and the OT-2 adapter interprets its documented operational subset only after facility allocation. Another method or compatible adapter may interpret other metadata or reject the module. Method- and adapter-specific property names are not encoded in the core checker.

The component list above has type `List<Part>`. A list that refers to both a dependent plasmid and ordinary parts has the inferred type `List<Plasmid | Part>`:

```lab
components: [promoter_carrier, B0034, GFP, B0015]
```

The union preserves the nominal alternatives; it does not convert the symbols to strings or a universal metadata value.

Multiple property-bearing artifacts and their realization workflows may be compiled by a supported method and lowered through a compatible facility adapter. Replicate and dilution settings are currently interpreted by the initial plasmid-build method and OT-2 adapter, not by the core language.

## Plasmids and strains

A `plasmid` and a `strain` share one declaration shape and differ in what they name. A plasmid is a DNA design: a sequence, or the backbone and components a sequence is built from. A strain is an organism: a chassis together with the plasmid designs it carries.

The distinction matters because the same plasmid in two hosts is two artifacts, each with its own acceptance criteria and its own place in a build order:

```lab
strain reporter_dh5alpha:
  chassis = DH5alpha
  plasmids = [p_gfp]
  selection = chloramphenicol

strain reporter_bl21:
  chassis = BL21
  plasmids = [p_gfp]
  selection = chloramphenicol
```

## Reaction chemistry

Quantity-valued properties state the chemistry a design is built with. These are scientific choices, so they travel with the artifact rather than with the laboratory that runs it:

```lab
plasmid p_gfp:
  backbone = pSB1C3
  cargo = [gfp_circuit]
  reaction_volume = 20 uL
  part_volume = 2 uL
  assembly_cycles = 75
  digest_temperature = 37 C
  ligate_duration = 5 min
```

Units are checked rather than assumed: `20 mL` where microlitres are expected is a diagnostic, not a thousandfold error on the bench. Water makes each reaction up to its stated volume, and reagents that over-subscribe that volume are rejected before facility allocation or adapter lowering.

## Evidence a claim is believed on

Three colonies picked from a plate are independent transformants; one culture
measured three times is a single organism. The first measures biological
variance and the second measures pipetting variance, so reporting the second as
`n = 3` claims more than the experiment supports.

An artifact says what its claims are believed on:

```lab
plasmid p_gfp:
  sequence = dna("ACGT")

  across 3 biological replicates

  accept concentration >= 100 ng/uL
  accept volume >= 20 uL across 1 biological replicate
```

A declaration sets the standard every claim in it takes. A claim may state its
own instead, which replaces the declaration's rather than adding to it, so what
any claim is believed on is written in one place. The declaration's standard is
read before any claim, so one written below the claims it governs still governs
them, and stating it twice is an error rather than something position resolves.

`across 0 biological replicates` is refused: asking for no evidence is a mistake,
not a way to opt out. Omitting `across` is how a claim accepts whatever evidence
is offered.

Which replicates are which is not a property of a sample — it is where the
sample came from. Transformation establishes an organism and picking isolates
independent transformants, while diluting, recovering, plating, and measuring
carry on the lineage that went in. Two materials are biological replicates when
they trace to different beginnings and technical replicates when they trace to
the same one. See [`0026`](decisions/0026-lineage-and-replicates.md).

## Where a thing came from

A kind names a type; the word its instances are written with is that type in
snake_case, so neither name is written twice. An instance states its provenance,
because being built is a fact about a particular thing rather than about its
type — a plasmid may be assembled here or ordered from a supplier.

```lab
artifact Plasmid:
  sequence?: DNA
  backbone?: Backbone

build plasmid composite_plasmid_1:
  backbone = pSB1C3
  accept concentration >= 100 ng/uL

buy backbone pSB1C3
buy restriction_enzyme BsaI:
  digest_temperature = 37 C
```

`require`, `accept`, and a place in the build order attach to `build`. An
`identity` to order against attaches to `buy`, and belongs to buying rather than
to any kind's schema. Claiming to build something bought is refused.

A provenance verb followed by `:` opens a block, and states one origin over
everything inside. Each line is the instance form without a verb — with its own
block or type ascription where it has one — and each is its own declaration, so
a program reads as its inventory and its recipes rather than as a verb repeated
per line:

```lab
buy:
  part J23101
  part B0034
  backbone pSB1C3
  restriction_enzyme BsaI:
    digest_temperature = 37 C
```

A verb on a line inside the block is refused: the block has already said where
everything in it came from.

A word whose kind takes no type arguments has already said what type its
instances have. Where a kind is generic it cannot, and the instance names its
own type:

```lab
buy promoter pTet: Promoter<Tetracycline>
```

## A schema several packages describe

A kind's schema is everything every module in scope declares for it. What a
plasmid *is* comes from one package; what a method needs to build one comes from
another, and a design built by that method imports it.

```lab
use std.bio.designs      // sequence, backbone, components
use std.bio.golden_gate  // reaction_volume, assembly_cycles, digest_temperature
```

Every field a method contributes is optional, because its standard values stand
behind a design that states nothing, and a design's own value wins where a
protocol departs from the datasheet. A digest runs at its enzyme's temperature
unless the design says otherwise.

A property no module in scope declares is a mistake rather than an extension:

```
error: Plasmid has no property 'reaction_volme'
  |
6 |   reaction_volme = 20 uL
  |   ^^^^^^^^^^^^^^
  |
  = help: did you mean 'reaction_volume'?
```

## Dependencies through workflow dataflow

Dependencies are expressed through workflow dataflow rather than string matching:

```lab
use std.bio.build
use std.lab.plasmid

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
