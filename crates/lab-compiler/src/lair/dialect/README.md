# LAIR

LAIR—the Lab Automation Intermediate Representation—is the Lab ecosystem's multi-layer compiler IR. It is implemented as a family of Pliron dialects that preserve biological and physical meaning while programs are progressively lowered from scientific intent toward laboratory execution. [Decision 0045](../../../../../docs/language/decisions/0045-lair-method-refinement-and-facility-allocation.md) records how method alternatives, capability requirements, facility allocation, and public adapter boundaries extend this architecture.

LAIR is currently maintained inside `lab-compiler`. Its active consumer is the `lab-opt` textual IR tool, which parses, verifies, transforms, and reprints LAIR modules. It can be extracted into a crate later when an independent consumer requires a stable LAIR API.

Pliron is an implementation detail of this layer. Pliron contexts, modules, values, and pointers do not cross the public session boundary; callers exchange textual LAIR, stage and pipeline descriptions, and compiler-owned errors. Raw Pliron entities remain inside the dialect, analysis, pipeline, stage, and session modules.

## Current vertical slice

The first vertical slice contains:

- the `design` dialect for declarative plasmid artifact values;
- the `workflow` dialect for method-neutral realization, provision, transformation, recovery, dilution, and plating intent with typed material use-def edges;
- the `protocol` dialect for method-selected provision, synthesis, assembly, transformation, recovery, dilution, plating, selection, screening, growth, purification, sampling, sequencing, quantification, and acceptance;
- Protocol material-state types such as `CircularDna`, `ColonyPool`, `CloneCulture`, and `PurifiedPlasmid`;
- Protocol evidence types for sequence identity, concentration, and volume.

The dialects are layers within LAIR; no individual dialect is itself “the LAIR dialect.” Design and Workflow form the implemented portable source-lowering boundary. The current dialect conversion selects one plasmid-build Protocol and eliminates Workflow operations before facility planning. This is a vertical slice, not the accepted final method-selection boundary.

## Accepted stage architecture

The executable LAIR pipeline evolves toward three verifier-valid boundaries:

```text
design-intent
    -> refined-alternatives
    -> allocated-procedure
```

Design and generalized Intent operations preserve reachable source semantics and typed material flow. Method candidate regions contain Procedure tasks and first-class Capability requirements but no facility binding. A read-only analysis extracts a purpose-built global constraint problem; the solver selects unpinned methods together with exact offerings, Assets, adapters, MaterialLots, movements, and scheduling. An allocation pass applies that complete solution to the same LAIR identities, erases unselected candidates, and produces Allocated Procedure LAIR.

Pliron remains internal to `lab-compiler`. Immutable procedure, adapter-invocation, execution-plan, and run-document records are projections from verified stages rather than aliases for Pliron objects. External adapters consume versioned invocation records; a built-in adapter may use its own device dialect internally, but Pliron is not part of the adapter ABI.

## Physical-resource rule

Protocol values representing physical material are affine: they may have at most one consumer. Branching physical matter requires an explicit operation such as `protocol.sample`, which returns a retained sample and a separate assay aliquot. Design and evidence values are information rather than matter and may be reused.

Pliron operation verifiers check only operation-local material states. The separate `MaterialLinearityAnalysis` follows SSA use lists across the complete module, and `protocol-check-material-linearity` makes that analysis a required pipeline gate. This separation keeps non-local reasoning out of operation verifiers.
