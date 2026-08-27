# Core semantics

## Laboratory value kinds

- A **record** is immutable structured information. What a record is *for* is a role it plays, declared with `is`.
- A **design** is immutable biological intent and may be reused freely.
- A **material** has physical identity and is affine: it cannot be copied implicitly and must be consumed, retained, stored, transferred, or disposed.
- An **observation** is a record of an inspection or measurement; it plays `Evidential` so it may be offered in support of a claim.
- **Evidence** is immutable information shaped for evaluating a scientific claim. Evidence does not imply that the claim passes.
- An **event** is a record playing `Event`: an immutable occurrence stored in the durable workflow journal, which is what `emit` and `when` resolve against.
- An **outcome** is a record with `case` constructors, the tagged result of a workflow or scientific decision.

## Laws, kinds, and vocabulary

A **role** classifies types and has no values of its own. Membership travels
with the type that declares it, so a package may classify its own types against
a role it imported.

A **kind** is a word a package supplies together with the schema its declarations
are checked against. `plasmid` is not a keyword; it is `artifact
Plasmid:` in `std.bio.designs`. The compiler knows the shape — a word, a name, a
block of properties — and nothing about biology.

Structured data is declared with `record`, and what a record is *for* is a role
it plays: `Event` is what `emit` and `when` resolve against, and `Evidential` is
what may be offered in support of a claim. A declaration word the compiler reads
nothing from would assert nothing, so there is none.

## Roles, type parameters, and what a result is about

A **role** classifies types. It has no values, so a role may bound a type
parameter and may never be the type of anything. Membership is declared by the
type that plays the role rather than listed by the role, which keeps a role open
to types declared in packages that do not exist yet.

A **type parameter** names an unknown so that its occurrences can be linked. A
circuit generic over its trigger works for any signal; a workflow that takes both
a circuit and the reagent poured onto it uses one name twice, and that is what
makes inducing a tet-responsive strain with arabinose a compile error rather than
a wasted plate. A bare type parameter is not itself a material: `Material<S>`
transfers ownership, `S` alone carries none.

**`any Role` discards a type argument on purpose.** A concrete type flows into it
whenever it plays that role, and never back out. Discarding is available only
where an annotation asks for it; inference widens a mixed collection to a union
instead, which preserves the alternatives. The two say different things. A union
is one of these specific things, and a `match` can find out which. An existential
is something that plays this role, and the question is not answerable.

### Where forgetting belongs

A forgotten type argument cannot be recovered by naming it. That is not a
limitation to work around: a collection of circuits with different triggers has,
by construction, no inducer that works for all of them, so a type that let you
ask which one to use would be lying about the bench.

The resolution is to forget one step later than feels natural. Run each
characterization while its signal is still named, so the compiler enforces the
pairing, and then collect the **results** rather than the designs:

```lab
tet_reading <- characterize tet_reporter tetracycline_stock
ara_reading <- characterize ara_reporter arabinose_stock

panel: List<Reading<Fluorescence>> = [tet_reading, ara_reading]
```

Readings of the same kind of light are comparable across different triggers,
because what is being claimed is the outcome rather than the design that produced
it. Provenance is discarded after the type system has done its work, not before.

## Declarations, properties, and identities

A biological declaration is immutable intent. Its `name = value` entries are typed declarative properties: deterministic expressions evaluated once, never mutations and never durable effects. Portable module IR preserves them as named checked expressions without assigning target-specific meaning to every property name.

Inside a declaration body the two operators divide cleanly. `=` associates a name with a value; `:` gives a name a type. That is what lets a declaration's shape be read before its meaning is resolved, because one token after the name decides which is which without knowing the word that opened the block.

A declaration may carry an exact biological-design identity independently of how that design is obtained. `sbol_identity` is an absolute IRI naming the SBOL Component represented by either a `build` or `buy` declaration. A bought declaration may additionally state `supplier_identity`, the order identifier used to acquire it; that identifier defaults to the declared name. The source symbol, SBOL identity, and supplier identity are distinct, so renaming one does not silently rewrite either of the others. The typed symbol can appear in properties and expressions; using a bare string where a `Part`, `Backbone`, `Chassis`, or `Antibiotic` is required is a type error.

`Chassis` and `Strain` are different kinds of thing. A chassis is a catalogued host organism, declared with `buy`. A strain is a declared artifact: a chassis together with the plasmid designs it carries. One chassis appears in many strains, and one plasmid design may appear in strains built on different chassis.

Design identity is not availability. An `sbol_identity` does not establish a lot, quantity, location, provenance chain, or fitness for use. Those claims require exact MaterialLot resolution against a validated SBOLInventory document and runtime evidence.

During inventory-backed planning, a checked `sbol_identity` is joined only to active MaterialLots in the selected facility whose `sbol:built` names that exact Component IRI. A unique candidate is frozen in the dependency plan together with the facility IRI and source-document hash. No candidate is a missing input; several candidates are an allocation ambiguity requiring policy or review. The compiler never treats candidate ordering as allocation.

## Commands and events

An effect binding records a command and durably waits for the corresponding event. Dispatching a command does not establish that a physical action happened. Only the recorded completion event establishes the result observed by the workflow.

Workflow replay must not repeat completed physical actions. Time, randomness, inventory queries, device interaction, network access, and human decisions are effects rather than ambient language operations.

Every resolved action contract names the capability required to dispatch it as an absolute SBOLInventory capability-kind IRI, the type of each operand and result, and how each operand participates in physical ownership. `copy` is for freely reusable information, `borrow` permits observation without consuming a material, and `take` transfers a material into the action. Capability matching is exact IRI equality; source actions that describe composite biological work retain an explicit refinement boundary rather than pretending to name one instrument operation. The complete standard-action audit is in [`capabilities.md`](capabilities.md).

`=` and `<-` therefore have different replay laws. `=` evaluates a deterministic expression or commits an explicit state transition. `<-` creates a durable command boundary and obtains its value from a recorded completion event. The result may look like a local binding, but the physical action must not be repeated merely because a workflow is replayed.

The portable material-flow verifier follows these modes through each workflow. It tracks material places, including projections such as `colony_result.plate`, and rejects copying, use after `take`, hidden loss at a terminating path, and incompatible ownership at control-flow joins. A returned, stored, transferred, or disposed material leaves the workflow's ownership; an action result establishes fresh ownership. `borrow` never changes ownership.

Reactive handlers are checked from their shared captured state. A handler that does not terminate the workflow must preserve captured material ownership so a later event cannot observe a material consumed by an earlier invocation. Iteration over material collections remains unavailable until the language has an explicit consuming iterator contract.

## Workflow interfaces and artifact dependencies

A workflow's parameters and results are part of its declaration signature. A parameter of type `Material<T>` transfers an owned physical input into a workflow instance; it is not documentation attached to the first lines of the body. Inputs express requirements and caller-controlled variability. Results express guarantees and the values or physical ownership transferred back to the caller.

A workflow may declare one result type or an ordered parenthesized list of named typed results. A direct comma-separated `return` must match that arity and each corresponding type. Named results are operation results rather than an implicit record value; a workflow call binds them with the same multi-result effect syntax used by other durable actions. Returning several materials transfers each of them out of the terminating path before the affine checker requires the workflow to own nothing else.

Artifact dependencies are expressed with these ordinary typed interfaces. When a realization workflow consumes `List<Material<Plasmid>>`, the checked operand values identify which artifacts must already exist. A later planner may derive edges, roots, build waves, cycles, and inventory blockers from that dataflow. The language does not encode biological assembly levels or infer dependencies by matching component-name strings.

Heterogeneous reusable design values may acquire a union element type. A component list containing a plasmid symbol and part symbols is checked as `List<Plasmid | Part>`; this does not weaken either symbol into an untyped name.

## Chemistry and site configuration

A declaration's quantity-valued properties state reaction chemistry: reagent volumes, cycle counts, and thermal holds. These are claims about the science, so they belong to the artifact and travel with it into every target.

Which labware sits in which deck slot, which pipette is on which mount, and how many plates a bench holds are claims about a laboratory. A target specialization reads them from its own configuration, not from source. The same program compiled against two benches produces two different robot plans and one unchanged set of designs.

## Acceptance

The compiler may establish that a workflow can produce the kinds of evidence required by a plasmid's acceptance predicates. It cannot establish the values of future measurements. Acceptance therefore has three distinct judgments:

1. the design and claims are well-typed;
2. the selected workflow covers every evidence obligation;
3. runtime evidence satisfies the predicates.

Only the third judgment produces an accepted physical material.

## Portable semantics and target specialization

Portable module checking resolves module-provided contracts, types expressions, verifies workflow returns, and checks affine material ownership. It does not choose a robot, a deck, a reaction chemistry, or a laboratory schedule.

A target specialization may interpret a documented set of checked properties and resolved operations. It must fail explicitly when required properties, capabilities, value shapes, capacities, or operation sequences are unsupported. Target diagnostics should describe generic constraints where possible; experiment names and tutorial-specific sequences do not belong in the core language checker.

## Reactive execution

A workflow is a deterministic durable state machine over recorded events. Durable mutable memory must be declared explicitly with `state`. Ordinary bindings are immutable, so assigning to an existing non-state name is an error. State initializers and transitions are deterministic typed expressions; a runtime must journal their committed values together with the event transition. Handlers for one workflow instance are processed atomically and in journal order. Returning an outcome terminates the workflow and closes its remaining logical subscriptions; it does not implicitly cancel physical actions already in progress.
